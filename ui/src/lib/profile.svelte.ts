// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// The reader's profile: a name, an avatar and reading goals. Stored on this
// device only (settings table, key "profile"); there are no accounts.

import { api } from "./ipc/api";

export interface Profile {
  name: string;
  /** Avatar: an emoji, or null for the name's initials. */
  emoji: string | null;
  /** Avatar background (one of AVATAR_COLORS). */
  color: string;
  /** Books to finish this year (0 = no goal). */
  yearGoal: number;
  /** Minutes to read each day (0 = no goal). */
  dailyGoal: number;
}

export const AVATAR_COLORS = ["#204946", "#2b5b8a", "#7a3e8e", "#a0412d", "#8a6d1f", "#3f6f2e"];
export const AVATAR_EMOJI = ["📚", "📖", "🦉", "🐱", "🦊", "🐢", "🌙", "☕", "🌿", "⭐"];
export const DAILY_GOALS = [0, 10, 15, 20, 30, 45, 60, 90];

export const DEFAULT_PROFILE: Profile = { name: "", emoji: null, color: AVATAR_COLORS[0]!, yearGoal: 0, dailyGoal: 0 };

export const profile: Profile = $state({ ...DEFAULT_PROFILE });

/** Keep only known, sensible values (the stored JSON could be anything). */
export function sanitizeProfile(raw: unknown): Profile {
  const p = { ...DEFAULT_PROFILE, ...(typeof raw === "object" && raw ? (raw as Partial<Profile>) : {}) };
  p.name = typeof p.name === "string" ? p.name.replace(/\s+/g, " ").trim().slice(0, 40) : "";
  p.emoji = AVATAR_EMOJI.includes(p.emoji as string) ? p.emoji : null;
  p.color = AVATAR_COLORS.includes(p.color) ? p.color : DEFAULT_PROFILE.color;
  p.yearGoal = Number.isInteger(p.yearGoal) ? Math.min(365, Math.max(0, p.yearGoal)) : 0;
  p.dailyGoal = DAILY_GOALS.includes(p.dailyGoal) ? p.dailyGoal : 0;
  return p;
}

/** Up to two initials ("Ada Lovelace" → "AL"); a book icon stands in when there is no name. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return "";
  const first = [...parts[0]!][0] ?? "";
  const last = parts.length > 1 ? ([...parts[parts.length - 1]!][0] ?? "") : "";
  return (first + last).toUpperCase();
}

export async function loadProfile(): Promise<void> {
  const raw = await api.getSetting("profile").catch(() => null);
  Object.assign(profile, sanitizeProfile(raw));
}

export function saveProfile(): void {
  Object.assign(profile, sanitizeProfile($state.snapshot(profile)));
  void api.setSetting("profile", $state.snapshot(profile)).catch(() => {});
}
