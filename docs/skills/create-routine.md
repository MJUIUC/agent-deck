# Skill: Create a Routine

You can create scheduled routines on behalf of the user by calling the agent-deck REST API directly. A routine is a prompt that runs automatically on a cron schedule inside a specific thread — the response appears as a normal assistant message in that thread, and a push notification is sent if no SSE client is connected.

---

## When to use this skill

Use this skill when the user asks you to:
- Create a routine, scheduled task, or recurring reminder
- Set something up to run automatically at a given time or interval
- Schedule a daily briefing, summary, check-in, or notification

---

## API contract

**Endpoint:** `POST /api/threads/:thread_id/routines`

**Required fields:**

| Field | Type | Description |
|---|---|---|
| `name` | string | Short human-readable label shown in the UI (e.g. "Morning Briefing") |
| `prompt` | string | The full prompt that will be sent to the agent when the routine fires |
| `cron_expr` | string | Standard 5-field cron expression (e.g. `0 8 * * 1-5`) |

**Optional fields:**

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | boolean | `true` | Whether the routine starts active immediately |

**Response envelope:**
```json
{
  "data": {
    "id": "<uuid>",
    "thread_id": "<uuid>",
    "name": "Morning Briefing",
    "prompt": "...",
    "cron_expr": "0 8 * * 1-5",
    "enabled": true,
    "run_count": 0,
    "last_run_at": null,
    "next_run_at": "2026-04-04T08:00:00Z",
    "created_at": "...",
    "updated_at": "..."
  }
}
```

---

## Cron expression reference

All times are interpreted in the server's local timezone.

| Expression | Meaning |
|---|---|
| `0 8 * * *` | Every day at 8:00 AM |
| `0 8 * * 1-5` | Weekdays at 8:00 AM |
| `0 9,17 * * *` | Every day at 9:00 AM and 5:00 PM |
| `0 */2 * * *` | Every 2 hours |
| `30 7 * * 1` | Every Monday at 7:30 AM |
| `0 12 1 * *` | First day of every month at noon |

Fields (left to right): `minute hour day-of-month month day-of-week`

---

## Step-by-step

1. **Confirm the thread ID.** The routine must be created on the thread where the user is asking. You already know the current `thread_id` from your context — use it.

2. **Clarify any missing details before calling the API.** You need at minimum:
   - A name for the routine
   - The prompt it should run
   - When it should fire (translate the user's natural language into a cron expression)

   If the user said "every morning at 8" that is `0 8 * * *`. If they said "weekday mornings" that is `0 8 * * 1-5`. Confirm your interpretation before proceeding if there is any ambiguity.

3. **Call the API.**

4. **Confirm success to the user.** Tell them:
   - The routine name
   - When it will next fire (`next_run_at` from the response)
   - That it will appear as a message in this thread and trigger a push notification if they have notifications enabled

---

## Example

User: *"Can you set up a daily standup summary for me at 9am every weekday?"*

You would call:
```
POST /api/threads/bf1f3bc5-d7ae-4b25-bdcd-6dd052a35339/routines
{
  "name": "Daily Standup Summary",
  "prompt": "It's time for the daily standup. Please provide a brief summary of what I should focus on today, any blockers to be aware of, and one priority to tackle first.",
  "cron_expr": "0 9 * * 1-5"
}
```

Then confirm: *"Done — I've set up 'Daily Standup Summary' to run every weekday at 9:00 AM. You'll get a push notification each time it fires."*

---

## Error cases

| Error | Cause | Resolution |
|---|---|---|
| `400 name must not be empty` | Empty name field | Ask the user for a name |
| `400 cron_expr must be a standard 5-field cron expression` | Malformed cron | Fix the expression |
| `404 Thread not found` | Wrong thread ID | Use the current thread's ID |