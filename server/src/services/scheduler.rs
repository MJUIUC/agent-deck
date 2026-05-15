//! Routine scheduler service — Story 5.3
//!
//! Uses a channel-based design to avoid a circular dependency:
//!
//! - `AppState` stores a `Sender<SchedulerCommand>` (only the channel end).
//! - `SchedulerService` runs as a background tokio task and holds an
//!   `Arc<AppState>` so it can call `state.get_run_state` and spawn the
//!   agent run-loop the same way `routes/notify.rs` does.
//!
//! This is valid in Rust because both types live in the same crate — there
//! are no cross-crate circular dependencies, only cross-module references,
//! which the compiler handles fine.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use chrono_tz::Tz;
use dashmap::DashMap;
use sqlx::SqlitePool;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::models::routine::Routine;
use crate::routes::AppState;

// ─── Public command type ──────────────────────────────────────────────────────

/// Commands sent from route handlers to the scheduler background task.
///
/// Route handlers send on `state.scheduler_tx`; the scheduler task receives
/// on its `Receiver` end and reacts accordingly.
#[derive(Debug)]
pub enum SchedulerCommand {
    /// Load the routine from DB and register (or re-register) its cron job.
    /// Send this after a routine is created or updated.
    Upsert(String),
    /// Remove the routine's cron job entirely.
    /// Send this after a routine is deleted.
    Remove(String),
    /// Disable all running cron jobs for this thread (thread archived).
    PauseThread(String),
    /// Re-enable all `enabled = true` routines for this thread (thread
    /// unarchived).
    ResumeThread(String),
}

// ─── SchedulerService ────────────────────────────────────────────────────────

/// Channel-based cron scheduler.
///
/// Runs as a background tokio task.  `AppState` holds only the
/// `Sender<SchedulerCommand>` half; this struct holds the `Receiver` half
/// plus the live `JobScheduler` and a map of currently-registered jobs.
pub struct SchedulerService {
    pool: SqlitePool,
    state: Arc<AppState>,
    inner: JobScheduler,
    /// Maps `routine_id` → scheduler-assigned job UUID.
    jobs: DashMap<String, Uuid>,
}

impl SchedulerService {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a new scheduler.  Does **not** start the cron engine yet.
    pub async fn new(pool: SqlitePool, state: Arc<AppState>) -> Result<Arc<Self>> {
        let inner = JobScheduler::new().await?;
        Ok(Arc::new(Self {
            pool,
            state,
            inner,
            jobs: DashMap::new(),
        }))
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    /// Load all enabled, active-thread routines, register them as cron jobs,
    /// start the underlying scheduler, then process `SchedulerCommand`s
    /// until the sender end of the channel is dropped.
    ///
    /// Call this exactly once at server startup via `tokio::spawn`.
    pub async fn start(self: Arc<Self>, mut rx: tokio::sync::mpsc::Receiver<SchedulerCommand>) {
        // ── Load initial routines ─────────────────────────────────────────────
        let routines: Vec<Routine> = match sqlx::query_as(
            "SELECT r.id, r.thread_id, r.name, r.prompt, r.cron_expr, r.timezone, r.enabled,
                    r.run_count, r.last_run_at, r.next_run_at, r.created_at, r.updated_at
             FROM routines r
             JOIN threads t ON r.thread_id = t.id
             WHERE r.enabled = 1 AND t.status = 'active'",
        )
        .fetch_all(&self.pool)
        .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("Scheduler: failed to load routines on startup: {}", e);
                vec![]
            }
        };

        for routine in &routines {
            if let Err(e) = self.register_routine(routine).await {
                warn!(
                    "Scheduler: failed to register routine '{}' ({}): {}",
                    routine.name, routine.id, e
                );
            }
        }

        // ── Start the cron engine ─────────────────────────────────────────────
        if let Err(e) = self.inner.start().await {
            error!("Scheduler: failed to start cron engine: {}", e);
            return;
        }

        info!(
            "Scheduler started — {} routine(s) registered",
            self.jobs.len()
        );

        // ── Command loop ──────────────────────────────────────────────────────
        while let Some(cmd) = rx.recv().await {
            match cmd {
                SchedulerCommand::Upsert(routine_id) => {
                    self.handle_upsert(&routine_id).await;
                }
                SchedulerCommand::Remove(routine_id) => {
                    if let Err(e) = self.unregister_routine(&routine_id).await {
                        warn!(
                            "Scheduler: failed to remove routine '{}': {}",
                            routine_id, e
                        );
                    }
                }
                SchedulerCommand::PauseThread(thread_id) => {
                    self.handle_pause_thread(&thread_id).await;
                }
                SchedulerCommand::ResumeThread(thread_id) => {
                    self.handle_resume_thread(&thread_id).await;
                }
            }
        }

        info!("Scheduler: command channel closed, task exiting");
    }

    // ── Command handlers ──────────────────────────────────────────────────────

    async fn handle_upsert(&self, routine_id: &str) {
        match self.load_routine_from_db(routine_id).await {
            Ok(Some(routine)) => {
                if !routine.enabled {
                    // Routine was disabled — remove its job if present.
                    if let Err(e) = self.unregister_routine(routine_id).await {
                        warn!(
                            "Scheduler: failed to unregister disabled routine '{}': {}",
                            routine_id, e
                        );
                    }
                    return;
                }

                // Check whether the thread is still active.
                match self.thread_is_active(&routine.thread_id).await {
                    Ok(true) => {
                        if let Err(e) = self.register_routine(&routine).await {
                            warn!(
                                "Scheduler: failed to (re)register routine '{}': {}",
                                routine_id, e
                            );
                        }
                    }
                    Ok(false) => {
                        // Thread is archived — do not register; remove if present.
                        if let Err(e) = self.unregister_routine(routine_id).await {
                            warn!(
                                "Scheduler: failed to unregister archived-thread routine '{}': {}",
                                routine_id, e
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Scheduler: failed to check thread status for routine '{}': {}",
                            routine_id, e
                        );
                    }
                }
            }
            Ok(None) => {
                // Routine was deleted — remove its job if present.
                if let Err(e) = self.unregister_routine(routine_id).await {
                    warn!(
                        "Scheduler: failed to unregister deleted routine '{}': {}",
                        routine_id, e
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Scheduler: failed to load routine '{}' from DB: {}",
                    routine_id, e
                );
            }
        }
    }

    async fn handle_pause_thread(&self, thread_id: &str) {
        // Query all routines that belong to this thread.
        let ids: Vec<String> =
            match sqlx::query_as::<_, (String,)>("SELECT id FROM routines WHERE thread_id = ?")
                .bind(thread_id)
                .fetch_all(&self.pool)
                .await
            {
                Ok(rows) => rows.into_iter().map(|(id,)| id).collect(),
                Err(e) => {
                    warn!(
                        "Scheduler: failed to query routines for thread '{}': {}",
                        thread_id, e
                    );
                    return;
                }
            };

        // Only unregister routines that are currently active in the scheduler.
        for routine_id in &ids {
            if self.jobs.contains_key(routine_id.as_str()) {
                if let Err(e) = self.unregister_routine(routine_id).await {
                    warn!("Scheduler: failed to pause routine '{}': {}", routine_id, e);
                }
            }
        }
    }

    async fn handle_resume_thread(&self, thread_id: &str) {
        let routines: Vec<Routine> = match sqlx::query_as(
            "SELECT id, thread_id, name, prompt, cron_expr, timezone, enabled,
                    run_count, last_run_at, next_run_at, created_at, updated_at
             FROM routines
             WHERE thread_id = ? AND enabled = 1",
        )
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "Scheduler: failed to query routines for thread '{}': {}",
                    thread_id, e
                );
                return;
            }
        };

        for routine in &routines {
            if let Err(e) = self.register_routine(routine).await {
                warn!(
                    "Scheduler: failed to resume routine '{}': {}",
                    routine.id, e
                );
            }
        }
    }

    // ── Core job management ───────────────────────────────────────────────────

    /// Register (or re-register) a cron job for `routine`.
    ///
    /// Calls `unregister_routine` first so this is always idempotent.
    async fn register_routine(&self, routine: &Routine) -> Result<()> {
        // Idempotent: remove any existing job for this routine first.
        self.unregister_routine(&routine.id).await?;

        // tokio-cron-scheduler expects a 6-field expression with seconds as
        // the first field.  The DB stores 5-field (standard cron) expressions,
        // so we prepend "0 " to fire at second 0 of each matching minute.
        let cron_6 = format!("0 {}", routine.cron_expr);

        // Clone all captured values — the closure may fire many times.
        let routine_id = routine.id.clone();
        let thread_id = routine.thread_id.clone();
        let routine_name = routine.name.clone();
        let routine_prompt = routine.prompt.clone();
        let state = self.state.clone();
        let pool = self.pool.clone();

        // Look up the user's timezone preference; fall back to UTC.
        let tz: Tz = routine.timezone.parse::<Tz>().unwrap_or_else(|_| {
            warn!(
                "Scheduler: unknown timezone '{}' for routine '{}', falling back to UTC",
                routine.timezone, routine.id
            );
            Tz::UTC
        });

        let job = Job::new_async_tz(cron_6.as_str(), tz, move |_uuid, _lock| {
            // Inner clones so each invocation gets its own owned copies.
            let routine_id = routine_id.clone();
            let thread_id = thread_id.clone();
            let routine_name = routine_name.clone();
            let routine_prompt = routine_prompt.clone();
            let state = state.clone();
            let pool = pool.clone();

            Box::pin(async move {
                let fired_at = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

                info!(
                    "Scheduler: routine '{}' ({}) firing at {}",
                    routine_name, routine_id, fired_at
                );

                // Build the trigger-content payload (matches routine_invocation
                // schema from the notify route).
                let trigger_content = serde_json::json!({
                    "type": "routine_invocation",
                    "routine_id": routine_id,
                    "routine_name": routine_name,
                    "instructions": routine_prompt,
                    "fired_at": fired_at,
                })
                .to_string();

                // ── Fire the agent run-loop ────────────────────────────────────
                // Mirrors the pattern in routes/notify.rs exactly.
                let run_state = state.get_run_state(&thread_id);

                // Routine runs are not depth-limited — they are server-initiated.
                run_state.depth.fetch_add(1, Ordering::SeqCst);

                let (cancellation_tx, cancellation_rx) = tokio::sync::watch::channel(false);
                let turn_id = Uuid::new_v4();

                let handle = tokio::spawn({
                    let run_state = run_state.clone();
                    let state = state.clone();
                    let tid = thread_id.clone();
                    async move {
                        let _permit = run_state.semaphore.acquire().await.unwrap();
                        crate::services::agent::run(
                            state,
                            tid,
                            trigger_content,
                            cancellation_rx,
                            run_state.clone(),
                            turn_id,
                            true,
                        )
                        .await;
                        run_state.depth.fetch_sub(1, Ordering::SeqCst);
                    }
                });

                {
                    let mut slot = run_state.running_turn.lock().await;
                    *slot = Some(crate::routes::RunningTurn {
                        task: handle,
                        cancellation_tx,
                        thread_id: thread_id.clone(),
                        turn_id,
                    });
                }

                // ── Update run statistics in DB ───────────────────────────────
                let now = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

                if let Err(e) = sqlx::query(
                    "UPDATE routines
                     SET run_count = run_count + 1,
                         last_run_at = ?,
                         updated_at  = ?
                     WHERE id = ?",
                )
                .bind(&now)
                .bind(&now)
                .bind(&routine_id)
                .execute(&pool)
                .await
                {
                    warn!(
                        "Scheduler: failed to update run stats for routine '{}': {}",
                        routine_id, e
                    );
                }
            })
        })?;

        let job_uuid = self.inner.add(job).await?;
        self.jobs.insert(routine.id.clone(), job_uuid);

        info!(
            "Scheduler: registered routine '{}' ({}) with cron '{}'",
            routine.name, routine.id, cron_6
        );

        Ok(())
    }

    /// Remove the cron job for `routine_id` from the scheduler and from the
    /// internal map.  A no-op (returns `Ok`) if the routine is not registered.
    async fn unregister_routine(&self, routine_id: &str) -> Result<()> {
        if let Some((_, job_uuid)) = self.jobs.remove(routine_id) {
            if let Err(e) = self.inner.remove(&job_uuid).await {
                // Log but do not propagate — the map entry is already gone.
                warn!(
                    "Scheduler: failed to remove cron job for routine '{}': {}",
                    routine_id, e
                );
            } else {
                info!("Scheduler: unregistered routine '{}'", routine_id);
            }
        }
        Ok(())
    }

    // ── DB helpers ────────────────────────────────────────────────────────────

    /// Load a single routine from the DB by its ID.
    async fn load_routine_from_db(&self, routine_id: &str) -> Result<Option<Routine>> {
        let routine: Option<Routine> = sqlx::query_as(
            "SELECT id, thread_id, name, prompt, cron_expr, timezone, enabled,
                    run_count, last_run_at, next_run_at, created_at, updated_at
             FROM routines
             WHERE id = ?",
        )
        .bind(routine_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(routine)
    }

    /// Returns `true` iff the thread exists and its status is `'active'`.
    async fn thread_is_active(&self, thread_id: &str) -> Result<bool> {
        let row: Option<(String,)> = sqlx::query_as("SELECT status FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map_or(false, |(status,)| status == "active"))
    }
}
