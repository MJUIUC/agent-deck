import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { VitePWA } from "vite-plugin-pwa";
import path from "path";
import http from "node:http";
import type { IncomingMessage, ServerResponse } from "node:http";

const apiTarget = process.env.API_TARGET || "http://localhost:7474";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      // Use our hand-written service worker so Phase 7 push handling slots
      // straight in. injectionPoint: undefined skips workbox precache
      // injection (not needed for a self-hosted, always-online app).
      strategies: "injectManifest",
      srcDir: "public",
      filename: "sw.js",
      injectManifest: {
        injectionPoint: undefined,
      },
      // manifest.json already lives in public/ and is linked from index.html;
      // tell the plugin not to generate or inject a second one.
      manifest: false,
      devOptions: {
        enabled: true,
        type: "classic",
      },
    }),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    globals: true,
    environment: "happy-dom",
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    pool: "vmThreads",
  },
  build: {
    outDir: path.resolve(__dirname, "../server/public"),
    emptyOutDir: true,
  },
  server: {
    port: 5173,
    // Allow access via Tailscale hostnames (*.ts.net) and any other
    // external host — needed when using `tailscale serve` to proxy the
    // dev server over HTTPS to a phone or other device on the tailnet.
    allowedHosts: true,
    proxy: {
      "/api": {
        target: apiTarget,
        changeOrigin: true,
        // Use a no-keepalive agent so every proxied request gets its own TCP
        // connection to the Rust server. Without this, the persistent SSE
        // stream (/api/threads/:id/stream) holds a keep-alive connection open,
        // and subsequent requests (e.g. POST /messages) can queue behind it,
        // making the POST appear to block until the SSE stream closes.
        agent: new http.Agent({ keepAlive: false }),
        // Disable response buffering for SSE streams. Vite's proxy (http-proxy)
        // buffers the response body by default, which causes tokens to
        // accumulate in Node's buffer instead of being forwarded to the browser
        // the instant the Rust server emits them. Calling res.flush() on every
        // proxyRes data chunk forces Node to forward each chunk immediately.
        configure(proxy) {
          proxy.on(
            "proxyRes",
            (
              proxyRes: IncomingMessage,
              _req: IncomingMessage,
              res: ServerResponse,
            ) => {
              const contentType = proxyRes.headers["content-type"] ?? "";
              if (contentType.includes("text/event-stream")) {
                proxyRes.on("data", () => {
                  // flush() is available on the ServerResponse when the
                  // underlying socket has been upgraded to streaming mode.
                  if (
                    typeof (res as unknown as { flush?: () => void }).flush ===
                    "function"
                  ) {
                    (res as unknown as { flush: () => void }).flush();
                  }
                });
              }
            },
          );
        },
      },
      "/health": {
        target: apiTarget,
        changeOrigin: true,
      },
    },
  },
});
