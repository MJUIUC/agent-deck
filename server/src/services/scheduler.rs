//! Routine scheduler service.
//!
//! Responsible for:
//! - Loading all enabled routines from the database on startup
//! - Scheduling cron jobs via tokio-cron-scheduler
//! - Firing routines by injecting a user message into the agent run-loop
//! - Updating run_count, last_run_at, next_run_at after each fire
//! - Pausing routines when their thread is archived
//! - Reloading schedule when routines are created/updated/deleted via API

// Full implementation lands in Phase 4.

use anyhow::Result;
use sqlx::SqlitePool;
use tracing::info;

/// Placeholder scheduler state.
/// Will hold the tokio-cron-scheduler instance and a reference to the pool.
#[allow(dead_code)]
pub struct SchedulerService {
    pool: SqlitePool,
}

impl SchedulerService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Start the scheduler — loads all enabled routines and registers cron jobs.
    /// Called once at server startup.
    pub async fn start(&self) -> Result<()> {
        info!("Scheduler starting (stub — full implementation in Phase 4)");
        Ok(())
    }

    /// Add or update a routine's cron job at runtime (called when a routine is
    /// created or modified via the API).
    #[allow(dead_code)]
    pub async fn upsert_routine(&self, _routine_id: &str) -> Result<()> {
        Ok(())
    }

    /// Remove a routine's cron job (called when a routine is deleted or its
    /// thread is archived).
    #[allow(dead_code)]
    pub async fn remove_routine(&self, _routine_id: &str) -> Result<()> {
        Ok(())
    }
}
