import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "path";
import http from "node:http";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
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
    proxy: {
      "/api": {
        target: "http://localhost:7474",
        changeOrigin: true,
        // Use a no-keepalive agent so every proxied request gets its own TCP
        // connection to the Rust server. Without this, the persistent SSE
        // stream (/api/threads/:id/stream) holds a keep-alive connection open,
        // and subsequent requests (e.g. POST /messages) can queue behind it,
        // making the POST appear to block until the SSE stream closes.
        agent: new http.Agent({ keepAlive: false }),
      },
      "/health": {
        target: "http://localhost:7474",
        changeOrigin: true,
      },
    },
  },
});
