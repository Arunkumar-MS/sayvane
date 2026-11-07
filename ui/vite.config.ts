/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const uiRoot = fileURLToPath(new URL(".", import.meta.url));
const mockDir = resolve(uiRoot, "src/ipc/mock");

// `vite --mode mock` serves the UI in a plain browser: the Tauri API modules are
// redirected to an in-memory mock core so commands.ts stays untouched.
export default defineConfig(({ mode }) => ({
  root: uiRoot,
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias:
      mode === "mock"
        ? [
            { find: /^@tauri-apps\/api\/core$/, replacement: resolve(mockDir, "core.ts") },
            { find: /^@tauri-apps\/api\/event$/, replacement: resolve(mockDir, "event.ts") },
            { find: /^@tauri-apps\/api\/window$/, replacement: resolve(mockDir, "window.ts") },
          ]
        : [],
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      input: {
        main: resolve(uiRoot, "index.html"),
        bar: resolve(uiRoot, "bar.html"),
      },
    },
  },
  test: {
    root: uiRoot,
    include: ["src/**/*.test.ts"],
    environment: "node",
    cache: false,
  },
}));
