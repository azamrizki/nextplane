import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// `make dev`: Vite on :15173 proxies the API to the server on :12313. The Host header is kept
// (no changeOrigin) so the server's Origin check sees the same host the browser used.
const hub = process.env.NEXTPLANE_DEV_HUB ?? "http://127.0.0.1:12313";

export default defineConfig({
  plugins: [react()],
  server: {
    port: Number(process.env.WEB_PORT ?? 15173),
    strictPort: true,
    proxy: {
      "/api": { target: hub, changeOrigin: false, ws: true },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 1500,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
