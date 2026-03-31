# AD-6.3 — Updated Mobile Settings Page

**Story:** 6.3 — Updated mobile settings page
**Branch:** `feature/phase6-mobile-settings`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 6

---

## Summary

Replace the two placeholder sections in `MobileSettings.tsx` with real content: platform-specific PWA install instructions (iOS vs Android), a simplified client-side QR code showing the server URL for first-time phone setup, and a three-state notification status section with a deferred "Enable Notifications" button. Also removes all dead pairing infrastructure (server endpoints, API client code, desktop settings tab) that was built for a native app that no longer exists in the plan.

---

## Current State

`web/src/layouts/mobile/MobileSettings.tsx` is a shell with two placeholder sections:
- **"Install as App"** — single row reading "Setup instructions coming soon."
- **"Notifications"** — single row reading "Push notification setup coming soon."

`web/src/layouts/mobile/MobileSettings.module.css` already has all structural styles needed: `.section`, `.sectionLabel`, `.sectionCard`, `.row`, `.rowTitle`, `.rowDesc`. A few new classes will be needed for step rows and notification status badges.

**Dead code to remove:**
- `web/src/components/settings/MobileSettings.tsx` — the old QR pairing component, built for a native Android app that is no longer in the plan. It calls `pairingApi.generate()` which exchanges an auth token via a QR scan. The PWA model does not need this.
- `web/src/api/client.ts` — `pairingApi` export (the two pairing endpoints it calls are being removed).
- `web/src/components/settings/SettingsNav.tsx` — "Mobile Pairing" nav tab that renders the old component.
- `server/src/routes/tokens.rs` — `generate_pairing` and `complete_pairing` handlers.
- `server/src/routes/mod.rs` — route registrations for `/api/pairing/generate` and `/api/pairing/complete`.
- `server/src/models/app_config.rs` — `PAIRING_TOKEN` and `PAIRING_TOKEN_EXPIRES_AT` key constants.

The `qrcode` npm package stays — it is still used to render the simplified URL QR.

---

## Implementation Plan

### Task 1 — Platform detection utility

- **File:** `web/src/hooks/usePlatform.ts` (new)
- **Change:** Export a `usePlatform()` hook that reads `navigator.userAgent` once and returns `"ios" | "android" | "other"`. iOS is detected by `/iphone|ipad|ipod/i`, Android by `/android/i`. Written as a reusable hook for future use.

No reactive updates needed — the UA does not change during a session.

### Task 2 — PWA install section (replace placeholder)

- **Files:** `web/src/layouts/mobile/MobileSettings.tsx`, `web/src/layouts/mobile/MobileSettings.module.css`
- **Change:** Replace the "Install as App" placeholder section with real platform-specific install steps.

Call `usePlatform()` at the top of the component. Render a numbered step list inside the existing `.sectionCard`. Steps differ by platform:

**iOS:**
1. Open this page in **Safari** (not Chrome or Firefox)
2. Tap the **Share** button (box with arrow) at the bottom of the screen
3. Scroll down and tap **"Add to Home Screen"**
4. Tap **"Add"** — agent-deck will appear on your home screen

**Android:**
1. Open this page in **Chrome**
2. Tap the **⋮ menu** in the top-right corner
3. Tap **"Add to Home Screen"** or **"Install app"**
4. Tap **"Add"** — agent-deck will appear in your app drawer

**Other (desktop or unknown):**
A single row: "Visit this page on your iOS or Android device to install agent-deck as an app."

Add `.stepRow`, `.stepNum`, `.stepText` CSS classes for the numbered step layout.

### Task 3 — Simplified URL QR code section

- **Files:** `web/src/layouts/mobile/MobileSettings.tsx`, `web/src/layouts/mobile/MobileSettings.module.css`
- **Change:** Add an "Open on Your Phone" section. The QR encodes `window.location.origin` as a plain URL string — no API call, no auth token, no server involvement. When scanned with a standard phone camera, it opens agent-deck directly in the phone browser.

The section contains a single `.sectionCard` with:
- A 220×220 `<canvas>` rendered immediately on mount (no button press needed — the URL is always known)
- The URL printed as copyable text below the canvas
- A brief caption: "Scan with your phone camera to open agent-deck. Make sure Tailscale is running on both devices first."

QR canvas colours stay consistent: dots `#F0EDE4`, background `#242422`.

The `QrCanvas` sub-component is defined locally in `MobileSettings.tsx`. Add `.qrWrap`, `.qrUrl`, `.qrCaption` CSS classes.

**Note:** This section is only meaningful before the PWA is installed. Once installed, the user already has the URL pinned. No need to hide it after install — it is low-noise and harmless to leave visible.

### Task 4 — Notification status section (replace placeholder)

- **Files:** `web/src/layouts/mobile/MobileSettings.tsx`, `web/src/layouts/mobile/MobileSettings.module.css`
- **Change:** Replace the "Notifications" placeholder with a three-state status display.

Determine notification permission state on mount using `Notification.permission` (`"granted"` | `"denied"` | `"default"`). Also check whether a service worker push subscription exists: call `navigator.serviceWorker.ready` then `registration.pushManager.getSubscription()` — if non-null, status is "subscribed".

Derive a display state:
- **Enabled** (`permission === "granted"` and subscription exists): green dot + "Notifications enabled"
- **Not enabled** (`permission !== "denied"` and no subscription): yellow dot + "Notifications not enabled" + "Enable Notifications" button
- **Blocked** (`permission === "denied"`): red dot + "Notifications blocked — enable in your browser settings"

The "Enable Notifications" button renders but is **disabled** with a `(coming soon)` label in `var(--text-tertiary)` beneath it — the VAPID endpoint does not exist until Story 7.3. It must be a real `<button>` element so the UI shape is correct from day one.

If `Notification` or `navigator.serviceWorker` is unavailable, show a grey dot + "Notifications unavailable in this browser."

Add `.statusRow`, `.statusDot`, `.statusDotGreen`, `.statusDotYellow`, `.statusDotRed`, `.statusDotGrey` CSS classes. Dots are `8px` circles with a subtle glow (`box-shadow: 0 0 4px <colour>`).

### Task 5 — Remove dead pairing infrastructure

**Frontend:**
- Delete `web/src/components/settings/MobileSettings.tsx`
- In `web/src/components/settings/SettingsNav.tsx`: remove the "Mobile Pairing" `<NavItem>` entry and its `"mobile"` tab case. Also remove it from wherever the desktop `SettingsModal` switches on tab names.
- In `web/src/api/client.ts`: delete the `pairingApi` export block.

**Server:**
- In `server/src/routes/tokens.rs`: delete `generate_pairing` and `complete_pairing` functions.
- In `server/src/routes/mod.rs`: delete the two `/api/pairing/*` route registrations.
- In `server/src/models/app_config.rs`: delete the `PAIRING_TOKEN` and `PAIRING_TOKEN_EXPIRES_AT` constants from the `keys` module.

Run `cargo build` after server edits to confirm no remaining references.

---

## Schema Changes

None. The pairing token was stored in `app_config` (key-value table), not a dedicated migration, so no migration is needed to remove it.

---

## Parallelisation Note

Tasks 1–4 all touch `MobileSettings.tsx` / `MobileSettings.module.css` and must run sequentially. Task 5 (cleanup) touches different files and could run in parallel with Tasks 1–4 since there is no overlap, but sequential is fine given the small scope.

Logical order: Task 1 → Task 2 → Task 3 → Task 4 → Task 5.

---

## Acceptance Criteria

- [ ] iOS and Android show different PWA install instructions based on user agent
- [ ] QR code renders automatically on page load and encodes `window.location.origin` as a plain URL — scanning it with a phone camera opens agent-deck in the browser
- [ ] No API call is made to generate the QR code
- [ ] Notification status section renders correctly in all three states (enabled, not enabled, blocked)
- [ ] "Enable Notifications" button is present but disabled with a "coming soon" label
- [ ] Old `components/settings/MobileSettings.tsx` is deleted
- [ ] "Mobile Pairing" tab is removed from desktop Settings nav
- [ ] `pairingApi` is removed from `client.ts`
- [ ] `/api/pairing/generate` and `/api/pairing/complete` routes are removed from the server
- [ ] `cargo build` passes with no errors after server cleanup

---

## Human Review Instructions

*Written after implementation is complete — see Step 5. Leave blank until then.*

---

## Approval

- [x] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off
