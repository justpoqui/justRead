// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// E2E entry: install mocks for Tauri IPC, then boot the real app.
// Packages are real justBook output (produced by the Rust importer) served
// by the dev server under /__books/. Docs: https://v2.tauri.app/develop/tests/mocking/

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { installBackend } from "./mock-backend";

mockWindows("main");
const backend = await installBackend();
mockIPC((cmd, args) => backend.handle(cmd, args as Record<string, unknown>));

// Resources resolve to the dev server instead of the justbook:// scheme.
const internals = (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__;
internals.convertFileSrc = (path: string) => `${location.origin}/__books/${path}`;

await import("../src/main");
