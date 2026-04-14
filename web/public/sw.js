// agent-deck service worker

const CACHE_NAME = "agent-deck-v1";

// Install: activate immediately without waiting for existing tabs to close.
self.addEventListener("install", (event) => {
  self.skipWaiting();
});

// Activate: claim all clients immediately.
self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("push", (event) => {
  console.log("[sw] push event received", event);

  let data = { title: "agent-deck", body: "" };
  if (event.data) {
    try {
      data = event.data.json();
      console.log("[sw] push payload parsed:", data);
    } catch (err) {
      console.warn("[sw] push payload JSON parse failed, using raw text:", err);
      data.body = event.data.text();
    }
  } else {
    console.warn("[sw] push event had no data");
  }

  const notifTitle = data.title ?? "agent-deck";
  const notifOptions = {
    body: data.body ?? "",
    icon: "/icon-192.png",
    badge: "/icon-192.png",
    data: data.data ?? {},
  };

  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then((clients) => {
        // If any window is currently visible (app is in the foreground), suppress
        // the notification — the user will already see the message via the live
        // SSE stream.
        const anyVisible = clients.some(
          (client) => client.visibilityState === "visible",
        );
        if (anyVisible) {
          console.log(
            "[sw] app is in foreground — suppressing push notification",
          );
          return Promise.resolve();
        }

        console.log("[sw] calling showNotification:", notifTitle, notifOptions);
        return self.registration
          .showNotification(notifTitle, notifOptions)
          .then(() => console.log("[sw] showNotification resolved OK"))
          .catch((err) =>
            console.error("[sw] showNotification rejected:", err),
          );
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
