// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Reactive settings shared by the whole UI (Svelte 5 runes in a module).
// Persisted to SQLite through the `set_setting` command, debounced.

import { api } from "../ipc/api";
import { DEFAULT_SETTINGS, sanitizeSettings, type ReaderSettings } from "./reader-settings";

export const settings: ReaderSettings = $state({ ...DEFAULT_SETTINGS });

export interface LibraryPrefs {
  view: "grid" | "list";
  sort: "title" | "author" | "series" | "recent" | "progress" | "added";
  descending: boolean;
}

export const libraryPrefs: LibraryPrefs = $state({ view: "grid", sort: "recent", descending: true });

/** Load persisted settings once at startup. */
export async function loadSettings(): Promise<void> {
  try {
    Object.assign(settings, sanitizeSettings(await api.getSetting("reader")));
    const lib = await api.getSetting<Partial<LibraryPrefs>>("library");
    if (lib) Object.assign(libraryPrefs, lib);
  } catch (e) {
    console.warn("settings unavailable, using defaults", e);
  }
}

let readerTimer: ReturnType<typeof setTimeout> | undefined;
export function saveSettings(): void {
  clearTimeout(readerTimer);
  readerTimer = setTimeout(() => void api.setSetting("reader", $state.snapshot(settings)), 300);
}

export function saveLibraryPrefs(): void {
  void api.setSetting("library", $state.snapshot(libraryPrefs));
}
