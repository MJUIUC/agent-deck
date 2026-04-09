# AD-9.0 — Timezone-Aware Cron Scheduling

**Story:** 9.0 — Timezone-Aware Cron (Phase 5 retrospective fix)  
**Branch:** `feature/phase9-timezone-cron`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9  
**Related story:** `docs/deprecated/AD-5.3.md` (original routine scheduler — now in deprecated/)

---

## Summary

The routine scheduler introduced in Story 5.3 fires cron expressions in UTC. A user who sets a "9 PM daily" routine gets a 2 PM PDT fire. This story adds a `timezone` column to the `routines` table, surfaces a timezone picker in the routine creation UI (desktop `ConfigPane`, mobile `MobileConfigSheet`), and converts cron scheduling to fire relative to the stored timezone using `chrono-tz`. The user should never need to think about UTC — they pick a time in their local zone and the scheduler handles the rest.

---

## Current State

### What already exists

- `server/src/db/migrations/` — sequential migrations up to 014 (after AD-8.6 lands); the next available number is 015
- `server/src/services/scheduler.rs` — `tokio-cron-scheduler` based service; `register_routine` and `remove_routine` functions; cron expressions passed directly to the scheduler with no timezone conversion
- `server/src/models/routine.rs` — `Routine` struct: `id`, `thread_id`, `name`, `prompt`, `cron_expr`, `enabled`, `last_run_at`, `run_count`, `created_at`; no `timezone` field
- `server/src/routes/routines.rs` — `CreateRoutine` and `UpdateRoutine` request structs; `GET/POST/PUT/DELETE/PATCH /api/threads/:id/routines`
- `web/src/components/ConfigPane.tsx` — desktop routine add/edit form; `CronPicker` component with `cronstrue` human-readable description; no timezone picker
- `web/src/layouts/mobile/MobileConfigSheet.tsx` — mobile routine list; no add/edit form (currently view-only — creating routines requires the desktop UI or the `create-routine` skill)
- `web/src/types/index.ts` — `Routine` TypeScript interface; no `timezone` field
- `Cargo.toml` — `chrono = "0.4"` already present; `chrono-tz` is **not** yet a dependency

### What does not exist yet

- Migration 015 adding `timezone TEXT` to `routines`
- `chrono-tz` Cargo dependency
- Timezone-aware cron conversion logic in `scheduler.rs`
- `timezone` field on `Routine` model and request structs
- Timezone picker in `ConfigPane.tsx` routine form
- Timezone picker in `MobileConfigSheet.tsx` routine add/edit form (if mobile add/edit is added)
- User profile timezone pre-fill for new routines

---

## Implementation Plan

### Task 1 — Migration

**New file:** `server/src/db/migrations/015_routine_timezone.sql`

```sql
ALTER TABLE routines ADD COLUMN timezone TEXT NOT NULL DEFAULT 'UTC';
```

`DEFAULT 'UTC'` means existing routines continue to fire on UTC schedule — no behavioral regression for routines that were intentionally set in UTC. Users who want local-time firing must edit their existing routines after upgrading.

### Task 2 — `chrono-tz` dependency and timezone conversion

**Modified file:** `Cargo.toml` (workspace)

Add:
```toml
chrono-tz = "0.9"
```

**Modified file:** `server/src/services/scheduler.rs`

The core change: when registering a routine, convert the user's wall-clock cron expression + timezone into the equivalent UTC cron expression before handing it to `tokio-cron-scheduler`.

**Conversion approach:**

`tokio-cron-scheduler` accepts POSIX cron syntax and schedules in UTC. We cannot pass a timezone-aware string to it directly. The solution is to compute the UTC offset for the target timezone at the next scheduled fire time, then shift the cron fields accordingly.

```rust
use chrono::Utc;
use chrono_tz::Tz;

fn to_utc_cron(cron_expr: &str, timezone: &str) -> anyhow::Result<String> {
    // 1. Parse the timezone string into a chrono_tz::Tz
    let tz: Tz = timezone.parse().map_err(|_| anyhow::anyhow!("Unknown timezone: {}", timezone))?;
    
    // 2. Find the next scheduled fire time by evaluating the cron expression
    //    against the current local time in the target timezone.
    //    Use `cron` crate (already a transitive dep of tokio-cron-scheduler) to parse.
    let schedule = cron::Schedule::from_str(cron_expr)?;
    let now_local = Utc::now().with_timezone(&tz);
    let next_local = schedule.after(&now_local).next()
        .ok_or_else(|| anyhow::anyhow!("Cron expression has no future occurrences"))?;
    
    // 3. Get the UTC offset at that moment (handles DST correctly).
    let utc_offset_seconds = next_local.offset().fix().local_minus_utc();
    let offset_minutes = utc_offset_seconds / 60;
    
    // 4. Shift the minute and hour fields of the cron expression by the offset.
    //    Only simple expressions (numeric minute + hour fields, no ranges/steps in those fields)
    //    are shifted. Complex expressions fall back to UTC with a warning.
    shift_cron_by_minutes(cron_expr, -offset_minutes)
}
```

`shift_cron_by_minutes` parses the minute and hour fields, applies the signed offset with wrapping (0–59 for minutes, 0–23 for hours, carrying overflow into the day-of-week/month if needed), and returns the modified expression. Field values that contain `/`, `-`, or `*` (other than `*` meaning "any") are left unmodified with a WARN log — edge cases like `*/15` (every 15 minutes) don't need shifting since they fire relative to the hour regardless.

This approach is pragmatic and correct for the most common use cases (daily/weekly routines at a specific hour). A fully general timezone-aware cron implementation would require replacing `tokio-cron-scheduler` entirely — out of scope for this story.

`register_routine` is updated to call `to_utc_cron` before passing the expression to the scheduler. The original `cron_expr` and `timezone` are stored in the DB unchanged; only the scheduler sees the shifted expression.

### Task 3 — Model and API

**Modified file:** `server/src/models/routine.rs`

Add `timezone: String` to `Routine` struct. Update all `SELECT` queries in `routes/routines.rs` to include `timezone`. Add `timezone: Option<String>` to `CreateRoutine` (defaults to `"UTC"` when absent) and `UpdateRoutine`. Validate the timezone string in the route handler using `timezone.parse::<chrono_tz::Tz>()` — return `400 Bad Request` on an unknown timezone name.

### Task 4 — Frontend: timezone picker in ConfigPane

**Modified file:** `web/src/components/ConfigPane.tsx`  
**Modified file:** `web/src/types/index.ts`

Add `timezone: string` to the `Routine` TypeScript interface.

In the routine add/edit inline form in `ConfigPane.tsx`, add a timezone selector below the `CronPicker`:

**Implementation:**

Use a `<select>` element populated with IANA timezone names. The full IANA list (~600 entries) is large — use `Intl.supportedValuesOf('timeZone')` (available in all modern browsers) to get the browser's supported list at runtime. No npm package needed.

Group the `<select>` options by continent prefix for usability (America/..., Europe/..., Asia/..., etc.).

**Default value logic:**
1. Pre-fill with the user's profile `timezone` field (already fetched in `ConfigPane` via `profileApi`)
2. Fall back to `Intl.DateTimeFormat().resolvedOptions().timeZone`
3. Fall back to `"UTC"`

Show a one-line hint below the picker: `"Routine fires at the time shown in <timezone>"`.

**Modified file:** `web/src/layouts/mobile/MobileConfigSheet.tsx`

The mobile config sheet currently lists routines but has no add/edit form. Adding one is out of scope for this story — users create/edit routines on desktop or via the `create-routine` skill. However, when displaying a routine's details on mobile, show the timezone alongside the cron expression human-readable string: `"Daily at 9:00 PM (America/Los_Angeles)"`.

### Parallelisation note

Task 1 (migration) must land first. Tasks 2 and 3 (Rust: scheduler + model/API) can be built in parallel — they touch different files. Task 4 (frontend) depends on Task 3 (the `timezone` field must be in the API response) but not on Task 2.

---

## Acceptance Criteria

- [ ] Migration 015 adds `timezone TEXT NOT NULL DEFAULT 'UTC'` to `routines`
- [ ] Existing routines fire on their UTC schedule after migration (no behavioral regression)
- [ ] `POST /api/threads/:id/routines` accepts a `timezone` field; defaults to `"UTC"` when absent
- [ ] `PUT /api/threads/:id/routines/:id` accepts a `timezone` field
- [ ] An unknown timezone name returns `400 Bad Request` from the API
- [ ] `GET /api/threads/:id/routines` returns `timezone` on each routine object
- [ ] A routine with `timezone: "America/Los_Angeles"` and `cron_expr: "0 21 * * *"` fires at 9 PM Pacific (2 AM UTC in winter, 4 AM UTC in summer)
- [ ] A routine with `timezone: "UTC"` fires at the same UTC time as before this story
- [ ] Timezone picker appears in the ConfigPane routine add/edit form
- [ ] Timezone picker is pre-filled with the user's profile timezone (when set)
- [ ] Timezone picker falls back to browser-detected timezone when profile timezone is unset
- [ ] Human-readable cron description in ConfigPane reflects the chosen timezone: `"Daily at 9:00 PM (America/Los_Angeles)"`
- [ ] Mobile routine display shows timezone alongside the human-readable schedule
- [ ] `cargo build` passes, all existing tests pass
- [ ] Unit tests: `to_utc_cron` correctly shifts a `"0 21 * * *"` expression by -7 hours (PDT) to `"0 4 * * *"`
- [ ] Unit tests: `to_utc_cron` returns an error for an unrecognized timezone string

---

## Human Review Instructions

*To be filled in after coding is complete (Step 5 of AGENT_WORKFLOW).*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**
