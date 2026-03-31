# AD-6.3 — Updated Mobile Settings Page

**Story:** 6.3 — Updated mobile settings page
**Branch:** `feature/phase6-mobile-settings`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 6

---

## Summary

Replace the two placeholder sections in `MobileSettings.tsx` with real content: platform-specific PWA install instructions (iOS vs Android), a three-state notification status section with a deferred "Enable Notifications" button, and a QR code section ported from the old `components/settings/MobileSettings.tsx`. After this story, the Settings tab is fully functional for helping users install the PWA and understand notification status.

---

## Current State

`web/src/layouts/mobile/MobileSettings.tsx` is a shell with two placeholder sections:
- **"Install as App"** — single row reading "Setup instructions coming soon."
- **"Notifications"** — single row reading "Push notification setup coming soon."

`web/src/layouts/mobile/MobileSettings.module.css` already has all structural styles needed: `.section`, `.sectionLabel`, `.sectionCard`, `.row`, `.rowTitle`, `.rowDesc`. A few new classes will be needed for step rows, a copy-to-clipboard button, and notification status badges.

The QR code generation logic (canvas renderer, `pairingApi.generate()` call, loading/error states, refresh button) lives in `web/src/components/settings/MobileSettings.tsx`. That component will not be deleted — it is still referenced by the desktop SettingsModal — but the QR logic will be ported into the new layout component.

`web/src/api/client.ts` exports `pairingApi.generate()` which POSTs to `/api/pairing/generate` and returns `{ data: { pairing_payload: { server_url: string; token: string }, hint: string } }`.

---

## Implementation Plan

### Task 1 — Platform detection utility

- **File:** `web/src/hooks/usePlatform.ts` (new)
- **Change:** Export a `usePlatform()` hook that reads `navigator.userAgent` once on mount and returns `"ios" | "android" | "other"`. iOS is detected by `/iphone|ipad|ipod/i`. Android by `/android/i`. Used only by `MobileSettings` at this stage but written as a reusable hook.

```ts
export function usePlatform(): "ios" | "android" | "other" {
  const ua = navigator.userAgent;
  if (/iphone|ipad|ipod/i.test(ua)) return "ios";
  if (/android/i.test(ua)) return "android";
  return "other";
}
```

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

Add `.stepRow`, `.stepNum`, `.stepText` classes to the CSS module for the numbered step layout. Steps use the same `.sectionCard` + `.row` wrapper as the rest of the page.

### Task 3 — QR code section (port from old component)

- **Files:** `web/src/layouts/mobile/MobileSettings.tsx`, `web/src/layouts/mobile/MobileSettings.module.css`
- **Change:** Add a "Connect a Device" section below the install section. Port the `QrCanvas` component and generate/refresh logic from `web/src/components/settings/MobileSettings.tsx`, adapting the visual style to the new card layout.

The section contains:
- A `.sectionCard` with a single `.row` that holds:
  - The 220×220 QR canvas (centred), or a placeholder box when not yet generated
  - A "Generate QR Code" primary button below the canvas (becomes "⟳ Refresh" after first generation)
  - Error message display if generation fails
- A second `.sectionCard` with step rows for "How to connect":
  1. Make sure **Tailscale** is running on both devices and signed in to the same account
  2. Tap "Generate QR Code" above
  3. On your phone, open the server URL shown in the QR code in Safari (iOS) or Chrome (Android)

The `QrCanvas` sub-component is defined locally in `MobileSettings.tsx` — no separate file needed. QR colours stay consistent with the existing implementation: dots `#F0EDE4`, background `#242422`.

Add `.qrWrap`, `.qrPlaceholder`, `.qrActions` CSS classes. The generate/refresh button uses the existing `.btn` pattern or inline styles matching the rest of the page — prefer a small reusable `<ActionBtn>` element defined locally using `var(--accent-primary)` for the primary style.

### Task 4 — Notification status section (replace placeholder)

- **Files:** `web/src/layouts/mobile/MobileSettings.tsx`, `web/src/layouts/mobile/MobileSettings.module.css`
- **Change:** Replace the "Notifications" placeholder with a three-state status display.

Determine notification permission state on mount using `Notification.permission` (`"granted"` | `"denied"` | `"default"`). Also check whether a service worker push subscription exists: call `navigator.serviceWorker.ready` then `registration.pushManager.getSubscription()` — if non-null, status is "subscribed".

Derive a display state:
- **Enabled** (`permission === "granted"` and subscription exists): green dot + "Notifications enabled"
- **Not enabled** (`permission !== "denied"` and no subscription): yellow dot + "Notifications not enabled" + "Enable Notifications" button
- **Blocked** (`permission === "denied"`): red dot + "Notifications blocked — enable in your browser settings"

The "Enable Notifications" button renders but is **disabled** with tooltip text "Available after setup is complete" — the VAPID endpoint (`GET /api/push/vapid-public-key`) does not exist yet and will be wired up in Story 7.3. The button must be a real `<button>` element (not hidden) so the UI shape is correct from day one. Add a small `(coming soon)` label in `var(--text-tertiary)` beneath it.

If `Notification` or `navigator.serviceWorker` is not available (non-HTTPS, old browser), show a neutral grey dot + "Notifications unavailable in this browser."

Add `.statusRow`, `.statusDot`, `.statusDotGreen`, `.statusDotYellow`, `.statusDotRed`, `.statusDotGrey` CSS classes. Dots are `8px` circles with a subtle glow (`box-shadow: 0 0 4px <colour>`).

---

## Schema Changes

None. No server changes in this story.

---

## Parallelisation Note

All four tasks touch the same two files (`MobileSettings.tsx` and `MobileSettings.module.css`), so they must run sequentially. The logical order is Task 1 → Task 2 → Task 3 → Task 4. Task 1 is a separate new file and could technically be written in parallel with Tasks 2–4, but since it is trivial (< 15 lines), sequential execution is fine.

---

## Acceptance Criteria

*(copied verbatim from PLAN_3)*

- [ ] iOS and Android show different install instructions based on user agent
- [ ] Notification status section renders correctly in all three states
- [ ] "Enable Notifications" button is present but clearly marked as coming in a future update (or hidden, TBD in Phase 7)
- [ ] QR code generation still works

---

## Human Review Instructions

*Written after implementation is complete — see Step 5. Leave blank until then.*

---

## Approval

- [ ] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off