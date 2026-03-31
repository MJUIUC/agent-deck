# AD-6.2 — Mobile-first layout

**Story:** 6.2 — Mobile-first layout
**Branch:** `feature/phase6-mobile-layout`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 6

---

## Summary

Implement a dedicated mobile UI using a layout-level split. A `useIsMobile()` hook selects between `DesktopLayout` (the existing shell, unchanged) and a new `MobileLayout` at the `App.tsx` level. All business logic — stores, hooks, API calls — is shared. Only the shell and navigation components differ. The mobile UI is built from the mockups in `mockups/mobile-*.html` and consists of a bottom tab bar (Threads / Settings), a full-screen thread list, a full-screen chat view, a slide-up config sheet, and a mobile settings view.

---

## Current State

The vast majority of this story is already implemented on the branch. The following files exist and are complete:

- `web/src/hooks/useIsMobile.ts` — viewport + touch detection, reactive on resize
- `web/src/styles/mobile.css` — safe-area variables, tap-highlight removal, `.tap-target`, `.scroll-momentum`, `.mobile-fill`, `.safe-top`, `.safe-bottom`
- `web/src/App.tsx` — already wired; renders `<MobileLayout />` or `<DesktopLayout />` based on `useIsMobile()`
- `web/src/layouts/DesktopLayout.tsx` — extracted from the old `App.tsx`; no behaviour change
- `web/src/layouts/MobileLayout.tsx` — bootstraps stores/SSE, 2-tab bottom bar (Threads + Settings), `PersonaPickerModal` wired for multi-persona thread creation
- `web/src/layouts/mobile/MobileThreadList.tsx` — nav bar with title + compose icon, thread rows with avatar / name / timestamp / preview / left-edge active dot
- `web/src/layouts/mobile/MobileThreadList.module.css`
- `web/src/layouts/mobile/MobileChatView.tsx` — back arrow, config icon, scrollable message area, `visualViewport` keyboard-lift, auto-growing textarea, send/cancel buttons, paginated load-more via IntersectionObserver
- `web/src/layouts/mobile/MobileChatView.module.css`
- `web/src/layouts/mobile/MobileConfigSheet.tsx` — slide-up sheet, drag-to-dismiss, provider/model picker, routines toggle list, Full Settings deep-link
- `web/src/layouts/mobile/MobileConfigSheet.module.css`
- `web/src/layouts/mobile/MobileSettings.tsx` — shell with Install as App, Notifications (placeholders), Full Settings button
- `web/src/layouts/mobile/MobileSettings.module.css`

**Two gaps remain before every acceptance criterion is met:**

1. `web/src/styles/mobile.css` is never imported. `web/src/main.tsx` only imports `./styles.css`. The utility classes (`.tap-target`, etc.) and safe-area CSS variables are therefore dead code.
2. No further implementation gaps remain. The compose icon in the nav bar is the intended affordance for new-thread creation; no FAB is required.

**Deliberate deviations from the spec (accepted):**

- The spec described 3 tabs (Threads, Chat, Settings). The implementation uses 2 (Threads, Settings); Chat is a view within the Threads tab, navigated to by selecting a thread or creating one. This is simpler and equally usable.
- Tab state is not URL-driven. The spec mentioned `?tab=` query params; this was not implemented and is not called out in any acceptance criterion.
- The unread badge and routine tag on thread rows were not implemented. Neither `unread_count` nor a routine association exists on the `Thread` type, so these would require schema + API work beyond this story's scope. The left-edge accent dot is rendered for the active thread only (via `.threadItemActive` + `::before`), which matches the mockup intent for the selected state.

---

## Implementation Plan

### Task 1 — Import `mobile.css` in `main.tsx`

- **File:** `web/src/main.tsx`
- **Change:** Add `import "./styles/mobile.css";` directly after `import "./styles.css";`
- **Why:** Without this import the safe-area CSS custom properties (`--safe-area-inset-*`) and utility classes (`.tap-target`, `.scroll-momentum`, etc.) are never applied. This is a one-line fix.

### Schema changes

None. No migrations, no new columns, no `cargo sqlx prepare` needed.

### Parallelisation note

Only one task remains — a single-line edit to `main.tsx`. No parallelisation needed.

---

## Acceptance Criteria

Copied verbatim from `docs/PLAN/PLAN_3.md` §Story 6.2:

- [ ] `useIsMobile()` correctly detects phone viewports and touch devices
- [ ] Desktop layout is pixel-identical to pre-6.2 at viewports > 768px
- [ ] Mobile layout renders at 375px, 390px, and 430px with no horizontal scroll
- [ ] Bottom tab bar is always visible and above the iOS home indicator
- [ ] Chat input stays above the keyboard when it opens on iOS and Android
- [ ] Config sheet slides up/down smoothly and dismisses on drag-down or backdrop tap
- [ ] All interactive elements meet 44×44px minimum tap target size
- [ ] Back arrow in chat nav returns to Threads tab
- [ ] No regressions on desktop (sidebar, ConfigPane, SettingsModal all unchanged)

---

## Human Review Instructions

**Prerequisites:** Server running on port 7474. Open the app in a browser devtools device emulator (Chrome → DevTools → Toggle device toolbar) at 375px, 390px, and 430px width. Also test on a real iOS or Android device if available.

**Steps:**

1. **Desktop regression check**
   - Open the app at a viewport wider than 768px.
   - **Expected:** Sidebar + main area render exactly as before. No layout shift, no missing components.
   - **Failure sign:** Missing sidebar, broken ConfigPane, or SettingsModal not opening.

2. **Mobile layout at 375px**
   - Set devtools to 375px width and reload.
   - **Expected:** `MobileLayout` renders — bottom tab bar (Threads, Settings), full-screen thread list, no horizontal scrollbar.
   - **Failure sign:** Desktop sidebar visible, or content overflows horizontally.

3. **Repeat at 390px and 430px** — same expectation as above.

4. **Thread selection → chat**
   - Tap any thread row.
   - **Expected:** Chat view fills the screen with back arrow (←) in the header.
   - Tap the back arrow.
   - **Expected:** Returns to thread list.

5. **New thread creation**
   - Tap the compose icon (✏) in the thread list nav bar.
   - **Expected:** PersonaPickerModal appears (if multiple personas) or chat view opens directly (if single persona). Draft chat is usable.

6. **Config sheet**
   - Open a thread, tap the config icon in the chat header.
   - **Expected:** Sheet slides up smoothly. Provider/model picker and routines list are visible.
   - Drag the sheet down or tap the backdrop.
   - **Expected:** Sheet dismisses.

7. **Keyboard lift (iOS/Android or devtools mobile emulation)**
   - Tap the message input in the chat view.
   - **Expected:** Input bar lifts above the software keyboard; message area scrolls up. Input is not obscured.

8. **Settings tab**
   - Tap the Settings tab in the bottom tab bar.
   - **Expected:** Mobile settings view renders with "Install as App", "Notifications", and "Full Settings" sections.

9. **Safe-area / home indicator (real device)**
   - On an iPhone with a home indicator, confirm the bottom tab bar sits above it and is not clipped.

**Server log check (optional):**
No specific server-side behaviour changed in this story.

---

## Approval

- [X] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [x] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [x] **Human review approved** — human has tested the changes live and signed off
