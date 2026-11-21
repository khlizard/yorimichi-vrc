import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
  build: { outDir: "../dist", emptyOutDir: true },
});
