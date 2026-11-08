// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// The page indicator: which progress views are possible for the current book
// and layout, and their labels (pure, unit-tested).

import type { ProgressDisplay } from "../settings/reader-settings";

export interface ProgressInput {
  /** PDF / comic (pages are real pages of the whole book). */
  fixed: boolean;
  /** Paginated layout (scroll mode has no pages). */
  paginated: boolean;
  /** Current page/screen (1-based) and the total: chapter pages for text, book pages for fixed. */
  page: number;
  pages: number;
  /** Fixed books: 0-based pages on screen (spreads show two). */
  visiblePages: number[] | null;
  /** 0..1 through the current chapter (text only). */
  chapterFraction: number | null;
  /** 0..1 through the whole book. */
  bookFraction: number;
  /** Text only: characters in this chapter and in all chapters after it. */
  chapterChars: number;
  charsAfterChapter: number;
}

/** Views that make sense here, in tap-cycle order. */
export function availableModes(i: ProgressInput): ProgressDisplay[] {
  if (i.fixed) return i.paginated ? ["bookPages", "bookPercent"] : ["bookPercent"];
  if (!i.paginated || i.pages <= 0) return ["chapterPercent", "bookPercent"];
  return ["chapterPages", "bookPages", "chapterPercent", "bookPercent"];
}

/** The chosen view if possible here, otherwise the first available one. */
export function effectiveMode(chosen: ProgressDisplay, i: ProgressInput): ProgressDisplay {
  const modes = availableModes(i);
  return modes.includes(chosen) ? chosen : modes[0]!;
}

/** The next view when the indicator is tapped. */
export function nextMode(current: ProgressDisplay, i: ProgressInput): ProgressDisplay {
  const modes = availableModes(i);
  const at = modes.indexOf(effectiveMode(current, i));
  return modes[(at + 1) % modes.length]!;
}

const pct = (f: number) => `${Math.round(Math.min(1, Math.max(0, f)) * 100)}%`;
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export function progressLabel(mode: ProgressDisplay, i: ProgressInput): string {
  switch (effectiveMode(mode, i)) {
    case "chapterPages": {
      const left = Math.max(0, i.pages - i.page);
      return left === 0 ? "Last page in chapter" : `${plural(left, "page")} left in chapter`;
    }
    case "bookPages": {
      if (i.fixed) {
        const last = Math.max(i.page - 1, ...(i.visiblePages ?? []));
        const left = Math.max(0, i.pages - 1 - last);
        return left === 0 ? "Last page" : `${plural(left, "page")} left`;
      }
      // Text: pages left here, plus the rest of the book at this chapter's page density.
      const here = Math.max(0, i.pages - i.page);
      const perChar = i.pages / Math.max(1, i.chapterChars);
      const left = here + Math.round(i.charsAfterChapter * perChar);
      return left === 0 ? "Last page" : `about ${plural(left, "page")} left in book`;
    }
    case "chapterPercent":
      return `${pct(i.chapterFraction ?? 0)} of chapter`;
    case "bookPercent":
      return `${pct(i.bookFraction)} of book`;
  }
}

/** Full position for screen readers and tests ("Page 3 of 12", "Pages 4–5 of 8"). */
export function fullPosition(i: ProgressInput): string {
  const v = i.visiblePages;
  if (i.fixed && i.paginated && v && v.length > 1) return `Pages ${v[0]! + 1}–${v[v.length - 1]! + 1} of ${i.pages}`;
  if (i.pages > 0) return `Page ${i.page} of ${i.pages}`;
  return pct(i.bookFraction);
}
