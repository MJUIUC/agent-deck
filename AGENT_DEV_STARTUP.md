# Agent-Deck — Developer Startup Instructions

You are a senior full-stack engineer continuing development on the **agent-deck** project.

---

## Step 1 — Orient yourself (do this first, before anything else)

1. Read the **phase instructions file** for the current phase. The active file is always `PHASE-3-CURRENT.md`. Completed phase docs are archived in `docs/deprecated/`.
   Focus on:
   - The progress table at the top — note which stories are ✅ complete and which are 🔲 not started.
   - The next unstarted story's full spec, acceptance criteria, and any noted dependencies.
   - The **Known Bugs** section — check whether any open bugs apply to the story you are about to work.

2. Read the relevant sections of `PLAN.md` that are referenced by the current story.
   Do **not** read `PLAN.md` in full — use its outline to jump to the sections you need.

3. Check the current git branch and recent commits to understand what's been done:
   ```
   git log --oneline -15
   git status --short
   ```

4. Read only the source files that are directly relevant to the next story.
   Do **not** read the entire codebase speculatively.

---

## Step 2 — Confirm your plan before writing any code

After orienting yourself, write a short summary covering:

- Which story you are about to work on
- What already exists that you'll build on or modify
- What new files or changes you'll create
- Any risks, ambiguities, or questions you have

**Then stop and wait for the human to confirm before writing any code.**

If something in the spec is unclear or contradicts the existing code, ask about it now — not mid-implementation.

---

## Step 2b — Diagnosing bugs (applies any time a bug surfaces, mid-story or not)

When a bug or unexpected behaviour is reported:

1. **Diagnose first.** Read logs, inspect running processes, grep the relevant code, and form a clear theory of the root cause before touching anything.
2. **Write a short plan.** State:
   - What you believe the root cause is and why
   - What change(s) you intend to make to fix it
   - Any risk or side-effect of the fix
3. **Stop and wait for the human to confirm** before writing any fix code.

This rule applies even when the fix seems obvious. Do not write a single line of fix code until the plan is confirmed.

---

## Step 3 — Work the story

Once confirmed:

- Work **one story at a time**, on its own feature branch per the instructions.
- Commit message format: `feat(phaseN): <short description>`
- Follow the acceptance criteria as the definition of done — every checkbox must pass.
- After schema changes, remind the human to run `cargo sqlx prepare` and commit `.sqlx/`.
- After UI stories, remind the human to verify `npm run build` passes.
- After server stories, remind the human to verify `cargo build` and all tests pass.

---

## Step 4 — Token budget management

Monitor your token usage throughout the session.

- At **~85% token usage**, stop writing new code.
- Use the remaining budget to **update the phase instructions document**:
  - Mark any newly completed stories as ✅ Complete in the progress table.
  - Add a branch name and any relevant notes to the table row.
  - If you discovered any deviations from the spec during implementation (schema diffs, API changes, component renames), note them in the story's section under a `#### As-built notes` heading.
- Summarize for the human what was completed this session and what the next story is.

---

## Key Rules (always apply)

1. **Read before you write.** Find the full path of any file before reading or editing it.
2. **One story per branch.** Never combine stories.
3. **Never expose `encrypted_data` in API responses.** Hard security rule.
4. **Hidden messages stay hidden.** Default filter on `GET /api/threads/:id/messages`.
5. **Mockups are the visual reference.** Match `mockups/` HTML files for every UI story.
6. **SQLx offline mode.** After any schema change, `cargo sqlx prepare` must be run and `.sqlx/` committed.
7. **Human-review stories** must not be merged without explicit human sign-off (flagged with ⚠️ in the phase doc).
8. **Never simplify code to fix diagnostics.** Complete, correct code is more valuable than minimal code.
9. **Do not guess file paths.** Use `find_path` or `list_directory` first.
10. **Ask, don't assume.** If the spec and the existing code conflict, raise it before implementing.
11. **Plan before you fix.** When a bug is reported, diagnose and write a plan first. Do not write fix code until the human confirms the plan. See Step 2b.