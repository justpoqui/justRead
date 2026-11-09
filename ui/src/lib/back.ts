// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// The Android back button (hardware key or back gesture).
//
// By default Tauri lets Android handle it, which closes the whole app even
// when a panel or a book is open. Registering `onBackButtonPress` (Tauri 2.9+,
// https://v2.tauri.app/reference/javascript/api/namespaceapp/#onbackbuttonpress)
// takes that over: screens push a handler that closes their topmost layer and
// returns true; only when nothing is left to close does the app exit.

import { exit, onBackButtonPress } from "@tauri-apps/api/app";

/** Returns true when it closed something (the press is consumed). */
type BackHandler = () => boolean;
const stack: BackHandler[] = [];

/** Register a handler; the most recently registered runs first. Returns an unregister function. */
export function onBack(handler: BackHandler): () => void {
  stack.push(handler);
  return () => {
    const i = stack.lastIndexOf(handler);
    if (i >= 0) stack.splice(i, 1);
  };
}

/** Run the handlers top-down until one consumes the press. */
export function handleBack(): boolean {
  for (let i = stack.length - 1; i >= 0; i--) if (stack[i]!()) return true;
  return false;
}

/** STEP: hook the Android back button once at startup (no-op elsewhere). */
export async function installBackButton(): Promise<void> {
  // Exposed for the phone e2e test, which has no hardware back key.
  (window as unknown as { __justreadBack?: () => boolean }).__justreadBack = handleBack;
  if (!/Android/i.test(navigator.userAgent)) return;
  try {
    await onBackButtonPress(() => {
      if (!handleBack()) void exit(0);
    });
  } catch {
    // Not running inside Tauri (browser tests) or an older runtime: keep the default.
  }
}
