// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Which bookmarks fall on the currently visible page/screen.

import type { Annotation } from "../ipc/types";
import { compareLocators, parseLocator } from "./locator";
import type { VisibleRange } from "./view";

/** Bookmarks on the visible pages of a fixed-layout book. */
export function bookmarksOnPages(list: Annotation[], pages: number[] | null): Annotation[] {
  if (!pages) return [];
  return list.filter((a) => {
    const loc = a.kind === "bookmark" ? parseLocator(a.start) : null;
    return loc?.kind === "fixed" && pages.includes(loc.page);
  });
}

/** Bookmarks whose position lies in [range.start, range.end). */
export function bookmarksInRange(list: Annotation[], range: VisibleRange | null): Annotation[] {
  if (!range) return [];
  return list.filter((a) => {
    if (a.kind !== "bookmark") return false;
    const loc = parseLocator(a.start);
    if (!loc || loc.kind !== "flow") return false;
    const afterStart = (compareLocators(loc, range.start) ?? -1) >= 0;
    // With no end, the chapter end is visible: anything later in this chapter counts.
    const beforeEnd = range.end ? (compareLocators(loc, range.end) ?? 1) < 0 : loc.spine === range.start.spine;
    return afterStart && beforeEnd;
  });
}
