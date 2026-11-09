// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Human-friendly durations for the stats screens.

/** 0 → "0 min", 59 s → "<1 min", 125 min → "2 h 5 min", exact hours → "3 h". */
export function formatDuration(seconds: number): string {
  if (seconds <= 0) return "0 min";
  if (seconds < 60) return "<1 min";
  const min = Math.round(seconds / 60);
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m ? `${h} h ${m} min` : `${h} h`;
}

/** Short form for chart labels: "45m", "2h", "1.5h". */
export function shortDuration(seconds: number): string {
  if (seconds > 0 && seconds < 60) return "<1m";
  const min = seconds / 60;
  if (min < 60) return `${Math.round(min)}m`;
  const h = min / 60;
  return `${h < 10 ? Math.round(h * 10) / 10 : Math.round(h)}h`;
}
