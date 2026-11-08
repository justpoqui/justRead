// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed dev port and must not have Vite clear its output.
// Docs: https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2022",
    sourcemap: false,
    // Two pages: the reader (index.html) and the justBrowse toolbar (browse.html).
    rollupOptions: { input: { main: "index.html", browse: "browse.html" } },
  },
  test: {
    environment: "jsdom",
    include: ["tests/**/*.test.ts"],
  },
});
