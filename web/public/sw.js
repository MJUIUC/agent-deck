// agent-deck service worker
// Phase 6 stub — push handling will be wired in Phase 7.

const CACHE_NAME = "agent-deck-v1";

// Install: activate immediately without waiting for existing tabs to close.
self.addEventListener("install", (event) => {
  self.skipWaiting();
});

// Activate: claim all clients immediately.
self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

// Push stub — full implementation in Phase 7.
self.addEventListener("push", (event) => {
  let data = { title: "agent-deck", body: "" };
  if (event.data) {
    try {
      data = event.data.json();
    } catch {
      data.body = event.data.text();
    }
  }

  event.waitUntil(
    self.registration.showNotification(data.title ?? "agent-deck", {
      body: data.body ?? "",
      icon: "/icon-192.png",
      badge: "/icon-192.png",
      data: data.data ?? {},
    }),
  );
});

// Notification click: focus or open the app.
self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const threadId = event.notification.data?.thread_id;
  const url = threadId ? `/?thread=${threadId}` : "/";

  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then((clientList) => {
        for (const client of clientList) {
          if ("focus" in client) {
            client.focus();
            if (threadId) client.postMessage({ type: "OPEN_THREAD", threadId });
            return;
          }
        }
        return self.clients.openWindow(url);
      }),
  );
});
