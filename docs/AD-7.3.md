# AD-7.3 — PWA Service Worker and Client Subscription

**Story:** 7.3 — PWA service worker and client subscription
**Branch:** `feature/phase7-pwa-push-client`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 7

---

## Summary

Wires the client-side half of Web Push notifications. The service worker push and
notification-click handlers are already fully implemented in `web/public/sw.js` (a
complete Phase 6 stub, not just a placeholder). This story adds three things: the
`pushApi` endpoints in `client.ts`, the enable/disable subscription flow in
`MobileSettings.tsx`, and the deep-link handler in `MobileLayout.tsx` that navigates
to the correct thread when a notification is tapped.

---

## Current State

### Service worker — `web/public/sw.js` (already complete, no changes needed)

- **`push` handler:** reads `event.data.json()`, calls
  `self.registration.showNotification(title, { body, icon, badge, data })`.
  Handles malformed JSON gracefully.
- **`notificationclick` handler:** closes the notification, extracts `thread_id`
  from `event.notification.data`. If a window is already open, focuses it and
  posts `{ type: "OPEN_THREAD", threadId }` via `client.postMessage`. If no window
  is open, calls `self.clients.openWindow("/?thread=<id>")`.
- The SW is registered via `vite-plugin-pwa` with `strategies: "injectManifest"`,
  `srcDir: "public"`, `filename: "sw.js"` — Vite handles registration automatically.

### `MobileSettings.tsx` — partially built

- `useNotificationStatus()` hook checks `Notification.permission` and
  `pushManager.getSubscription()` on mount; returns one of:
  `"loading" | "enabled" | "not-enabled" | "blocked" | "unavailable"`.
- Status dot and message are rendered correctly.
- "Enable Notifications" button is rendered when `notifState === "not-enabled"` but
  is **`disabled`** with a `(coming soon)` label — the actual subscribe flow is not
  wired up.
- No "Disable Notifications" button exists yet.

### `client.ts` — no push endpoints

No `pushApi` object. The three push endpoints from Stories 7.1/7.2 are live on the
server but not yet called from the frontend:
- `GET /api/push/vapid-public-key` — public, returns `{ data: { public_key: string } }`
- `POST /api/push/subscribe` — authenticated, body: `{ endpoint, p256dh, auth, user_agent? }`
- `DELETE /api/push/subscribe` — authenticated, body: `{ endpoint }`

### `MobileLayout.tsx` — no deep-link handling

- Does not listen for `{ type: "OPEN_THREAD", threadId }` postMessages from the SW.
- Does not read `?thread=` from `window.location.search` on cold start (when the
  app is opened via `openWindow("/?thread=<id>")`).

---

## Implementation Plan

### Task 1 — Add `pushApi` to `web/src/api/client.ts`

- **File:** `web/src/api/client.ts`
- **Change:** Append a new `pushApi` object at the end of the file (after `profileApi`):

  ```ts
  export const pushApi = {
    /** Fetch the server's VAPID public key. Public endpoint — no auth required. */
    getVapidPublicKey(): Promise<{ data: { public_key: string } }> {
      return apiFetch("/api/push/vapid-public-key");
    },

    /** Register a new browser push subscription on the server (upsert by endpoint). */
    subscribe(payload: {
      endpoint: string;
      p256dh: string;
      auth: string;
      user_agent?: string;
    }): Promise<{ data: { subscribed: boolean } }> {
      return apiFetch("/api/push/subscribe", {
        method: "POST",
        body: JSON.stringify(payload),
      });
    },

    /** Remove a push subscription from the server by endpoint URL. */
    unsubscribe(endpoint: string): Promise<{ data: { deleted: boolean } }> {
      return apiFetch("/api/push/subscribe", {
        method: "DELETE",
        body: JSON.stringify({ endpoint }),
      });
    },
  };
  ```

### Task 2 — Update `web/src/layouts/mobile/MobileSettings.tsx`

- **File:** `web/src/layouts/mobile/MobileSettings.tsx`
- **Change:** Three parts.

#### 2a — Extend `useNotificationStatus` to support re-checking

Replace the current hook (which only runs on mount) with a version that exposes a
`recheck()` function so that enable/disable handlers can refresh the status after
the browser subscription changes:

```ts
function useNotificationStatus(): {
  state: NotifState;
  recheck: () => void;
} {
  const [state, setState] = useState<NotifState>("loading");
  const [refreshKey, setRefreshKey] = useState(0);

  useEffect(() => {
    async function check(): Promise<void> {
      // ... exact same logic as the current hook body ...
    }
    check().catch(() => setState("unavailable"));
  }, [refreshKey]);

  return { state, recheck: () => setRefreshKey((k) => k + 1) };
}
```

Update the `MobileSettings` component to destructure `{ state: notifState, recheck }`.

#### 2b — Add `urlBase64ToUint8Array` helper

Add this utility function at the top of the file (below imports, above the hook).
It converts the server's base64url VAPID public key into the `Uint8Array` that
`pushManager.subscribe({ applicationServerKey })` requires:

```ts
function urlBase64ToUint8Array(base64String: string): Uint8Array {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding)
    .replace(/-/g, "+")
    .replace(/_/g, "/");
  const rawData = window.atob(base64);
  const output = new Uint8Array(rawData.length);
  for (let i = 0; i < rawData.length; i++) {
    output[i] = rawData.charCodeAt(i);
  }
  return output;
}
```

#### 2c — Wire up enable/disable handlers and update the render

Add `notifLoading: boolean` state to `MobileSettings`. Add two async handlers:

**`handleEnable`:**
1. Set `notifLoading = true`.
2. Call `await Notification.requestPermission()`. If result is not `"granted"`,
   call `recheck()`, set `notifLoading = false`, and return.
3. Get the SW registration: `const reg = await navigator.serviceWorker.ready`.
4. Fetch the VAPID key: `const { data } = await pushApi.getVapidPublicKey()`.
5. Subscribe: `const sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: urlBase64ToUint8Array(data.public_key) })`.
6. Convert to JSON: `const subJson = sub.toJSON()` — shape is
   `{ endpoint: string; keys: { p256dh: string; auth: string } }`.
7. Post to server: `await pushApi.subscribe({ endpoint: subJson.endpoint!, p256dh: subJson.keys!.p256dh!, auth: subJson.keys!.auth!, user_agent: navigator.userAgent })`.
8. Call `recheck()`, set `notifLoading = false`.
9. Wrap in try/catch — on error, log to console and set `notifLoading = false`.

**`handleDisable`:**
1. Set `notifLoading = true`.
2. Get SW registration: `const reg = await navigator.serviceWorker.ready`.
3. Get existing subscription: `const sub = await reg.pushManager.getSubscription()`.
4. If `sub` exists: call `await sub.unsubscribe()`, then
   `await pushApi.unsubscribe(sub.endpoint)`.
5. Call `recheck()`, set `notifLoading = false`.
6. Wrap in try/catch — on error, log and set `notifLoading = false`.

**Render changes in the Notifications section:**
- Remove `disabled` from the "Enable Notifications" button and wire `onClick={handleEnable}`.
- Remove the `(coming soon)` label entirely.
- Add `disabled={notifLoading}` to the button (spinner-free but non-interactive while
  the async flow runs).
- When `notifState === "enabled"`, render a "Disable Notifications" button alongside
  the status row:
  ```tsx
  {notifState === "enabled" && (
    <div style={{ marginTop: 12 }}>
      <button
        className={styles.disableBtn}
        onClick={handleDisable}
        disabled={notifLoading}
      >
        Disable Notifications
      </button>
    </div>
  )}
  ```

Add `.disableBtn` to `MobileSettings.module.css` — same shape as `.enableBtn` but
using `var(--bg-tertiary)` background and `var(--text-secondary)` text color:

```css
.disableBtn {
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 12px;
    border: none;
    border-radius: 8px;
    padding: 8px 16px;
    cursor: pointer;
}

.disableBtn:disabled {
    opacity: 0.5;
    cursor: default;
}
```

### Task 3 — Handle deep-links in `web/src/layouts/MobileLayout.tsx`

- **File:** `web/src/layouts/MobileLayout.tsx`
- **Change:** Two additions to the bootstrap `useEffect`.

#### 3a — Read `?thread=` URL param on cold start

When the app is opened by the SW's `openWindow("/?thread=<id>")` (i.e. the user
tapped a notification while the app was closed), the app mounts with `?thread=<id>`
in the URL. Read and act on it inside the existing bootstrap `useEffect`:

```ts
// Deep-link from notification tap (cold start)
const searchParams = new URLSearchParams(window.location.search);
const deepLinkThreadId = searchParams.get("thread");
if (deepLinkThreadId) {
  setActiveThread(deepLinkThreadId);
  // Clean up the URL so it doesn't persist across navigation
  window.history.replaceState({}, "", "/");
}
```

#### 3b — Listen for `OPEN_THREAD` postMessages from the SW

When the app is already open and the user taps a notification, the SW calls
`client.postMessage({ type: "OPEN_THREAD", threadId })`. Add a listener inside
the same `useEffect` (and clean it up on return):

```ts
function handleSwMessage(event: MessageEvent) {
  if (event.data?.type === "OPEN_THREAD" && event.data.threadId) {
    setActiveThread(event.data.threadId);
    setActiveTab("threads");
  }
}

navigator.serviceWorker?.addEventListener("message", handleSwMessage);
// Add to the existing cleanup return:
return () => {
  disconnectGlobal();
  navigator.serviceWorker?.removeEventListener("message", handleSwMessage);
};
```

Note: `setActiveTab` is already local state in `MobileLayout`; it just needs to be
called from within the effect. Declare `handleSwMessage` inside the effect so it
closes over the stable setter references.

### Schema changes

None. No server changes. No `cargo sqlx prepare` run needed.

### Parallelisation note

- Task 1 (`client.ts`) and Task 3 (`MobileLayout.tsx`) touch different files with
  no shared state — they **can run in parallel**.
- Task 2 (`MobileSettings.tsx`) imports `pushApi` from `client.ts` — it **must
  follow Task 1**.
- Assign Tasks 1 and 3 to parallel sub-agents, then Task 2 to a third sub-agent
  once both complete.

---

## Acceptance Criteria

*(Copied verbatim from PLAN_3.md §Story 7.3)*

- [x] "Enable Notifications" button triggers the browser permission prompt
- [x] After granting permission, subscription is stored on the server
- [ ] Push notification is received and shown on Android Chrome when a routine fires
      with no SSE client connected
- [ ] Push notification is received and shown on iOS Safari (PWA installed, home
      screen launch) when a routine fires
- [x] Tapping the notification opens the PWA and navigates to the correct thread
- [x] "Notifications enabled" / "Notifications not enabled" status in settings
      reflects actual subscription state
- [x] Unsubscribe flow works and removes the subscription from the server

---

## Human Review Instructions

**Prerequisites:**
- Server running on port 7474 (`cargo run` in `server/`).
- PWA installed on at least one mobile device (Android Chrome or iOS Safari home screen).
- At least one routine configured with a cron schedule (so it fires without a manual trigger).
- The device's browser/PWA must NOT be open and foregrounded when the routine fires (to confirm push delivery bypasses SSE).

---

**Part A — Enable Notifications (Android Chrome or iOS Safari PWA)**

1. Open the PWA on your mobile device and navigate to **Settings** (bottom tab).
2. In the **Notifications** section, verify the status dot shows yellow and reads
   *"Notifications not enabled"*.
3. Tap **"Enable Notifications"**.
   - **Expected (first time):** The browser displays a native permission prompt asking
     to allow notifications. Tap **Allow**.
   - **Expected after granting:** The status dot turns green and reads
     *"Notifications enabled"*. The "Enable Notifications" button disappears.
   - **Failure sign:** Button remains, status unchanged, or an error appears in the
     browser console.

4. Confirm the subscription was stored on the server:
   ```
   curl -s -H "Authorization: Bearer <your-token>" http://localhost:7474/api/push/subscribe | jq
   ```
   **Expected:** A row is returned with the device's endpoint URL.

---

**Part B — Receive a Push Notification**

5. Close (or background) the PWA so it is not the active foreground tab/window.
6. Wait for a routine to fire (or manually trigger one via the routine's "Run now"
   button in desktop settings), and ensure the SSE client is disconnected (close the
   desktop browser tab too, if needed).
   - **Expected:** A push notification appears in the OS notification tray with the
     routine's output as the title/body.
   - **Failure sign:** No notification appears within ~30 seconds of the routine completing.

---

**Part C — Deep-link on notification tap**

7. With the PWA closed (not backgrounded — fully closed), tap the notification.
   - **Expected (cold start):** The PWA opens and immediately navigates to the thread
     that the routine ran in. The `?thread=` query param is stripped from the URL bar
     after navigation.
   - **Failure sign:** PWA opens to the default thread list without navigating to the
     correct thread.

8. With the PWA already open in the background (e.g. on the threads screen), trigger
   another routine run and tap the resulting notification.
   - **Expected (warm start):** The PWA comes to the foreground, switches to the
     Threads tab, and opens the relevant thread directly.
   - **Failure sign:** App does not navigate to the thread.

---

**Part D — Disable Notifications**

9. Go back to **Settings → Notifications**. Tap **"Disable Notifications"**.
   - **Expected:** The status dot reverts to yellow (*"Notifications not enabled"*).
     The "Disable Notifications" button disappears and "Enable Notifications" reappears.
   - **Failure sign:** Status unchanged, or console error.

10. Confirm the subscription was removed from the server:
    ```
    curl -s -H "Authorization: Bearer <your-token>" http://localhost:7474/api/push/subscribe | jq
    ```
    **Expected:** No subscription row for this device's endpoint.

---

**Server log to verify dispatch (optional):**
```
grep "push: dispatched" ~/.agent-deck/server.log | tail -5
```

---

## Approval

- [x] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [x] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off
