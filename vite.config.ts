import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // Pre-bundle the demo mode's IPC mock at startup. Discovering it later makes Vite swap
  // node_modules/.vite/deps while running, which Dropbox can lock (EBUSY).
  optimizeDeps: { include: ["@tauri-apps/api/mocks"] },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
  build: { outDir: "../dist", emptyOutDir: true },
});
