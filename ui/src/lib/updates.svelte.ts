// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// In-app updates (Android). The check and download run on the Rust/Android
// side against the justRead GitHub Releases; this module keeps the UI state
// and the "check automatically about once a day" preference.

import { api } from "./ipc/api";
import type { UpdateInfo } from "./ipc/types";

/** Automatic checks at most this often. */
const AUTO_EVERY_MS = 20 * 60 * 60 * 1000;

interface UpdatePrefs {
  auto: boolean;
  lastCheck: number;
}

export const updates = $state({
  prefs: { auto: true, lastCheck: 0 } as UpdatePrefs,
  info: null as UpdateInfo | null,
  checking: false,
  installing: false,
  error: null as string | null,
  /** Hidden for this session after "Later". */
  dismissed: false,
});

export const isAndroid = typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);

async function loadPrefs() {
  const p = await api.getSetting<Partial<UpdatePrefs>>("updates").catch(() => null);
  if (p) updates.prefs = { auto: p.auto !== false, lastCheck: typeof p.lastCheck === "number" ? p.lastCheck : 0 };
}

export function savePrefs() {
  void api.setSetting("updates", $state.snapshot(updates.prefs)).catch(() => {});
}

/** Ask GitHub now (Settings → Updates → Check now, or the daily automatic check). */
export async function checkNow(): Promise<void> {
  if (updates.checking) return;
  updates.checking = true;
  updates.error = null;
  try {
    updates.info = await api.checkUpdate();
    updates.prefs.lastCheck = Date.now();
    savePrefs();
  } catch (e) {
    updates.error = String(e);
  } finally {
    updates.checking = false;
  }
}

/** At start-up: check if automatic checks are on and the last one was a while ago. */
export async function autoCheck(): Promise<void> {
  if (!isAndroid) return;
  await loadPrefs();
  if (updates.prefs.auto && Date.now() - updates.prefs.lastCheck >= AUTO_EVERY_MS) await checkNow();
}

/** Download, verify and open the installer. */
export async function install(): Promise<"started" | "permission" | "error"> {
  updates.installing = true;
  updates.error = null;
  try {
    await api.installUpdate();
    return "started";
  } catch (e) {
    const msg = String(e);
    if (/permission/.test(msg)) return "permission";
    updates.error = msg;
    return "error";
  } finally {
    updates.installing = false;
  }
}
