// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { availableModes, fullPosition, nextMode, progressLabel, type ProgressInput } from "../src/lib/reader/progress";
import { sanitizeSettings, DEFAULT_TOOLBAR } from "../src/lib/settings/reader-settings";

const text: ProgressInput = {
  fixed: false, paginated: true, page: 3, pages: 12, visiblePages: null,
  chapterFraction: 0.25, bookFraction: 0.1, chapterChars: 12000, charsAfterChapter: 48000,
};
const comic: ProgressInput = { ...text, fixed: true, page: 4, pages: 9, visiblePages: [3, 4], chapterFraction: null };

describe("page indicator", () => {
  it("cycles chapter pages → book pages → chapter % → book % for paginated text", () => {
    expect(availableModes(text)).toEqual(["chapterPages", "bookPages", "chapterPercent", "bookPercent"]);
    let m: import("../src/lib/settings/reader-settings").ProgressDisplay = "chapterPages";
    const seen = [];
    for (let k = 0; k < 4; k++) {
      seen.push(progressLabel(m, text));
      m = nextMode(m, text);
    }
    expect(seen).toEqual(["9 pages left in chapter", "about 57 pages left in book", "25% of chapter", "10% of book"]);
    expect(m).toBe("chapterPages"); // wraps around
  });

  it("handles the last page and singulars", () => {
    expect(progressLabel("chapterPages", { ...text, page: 12 })).toBe("Last page in chapter");
    expect(progressLabel("chapterPages", { ...text, page: 11 })).toBe("1 page left in chapter");
  });

  it("scroll mode has no pages: only percentages", () => {
    const s = { ...text, paginated: false, pages: 0 };
    expect(availableModes(s)).toEqual(["chapterPercent", "bookPercent"]);
    expect(progressLabel("chapterPages", s)).toBe("25% of chapter"); // falls back
  });

  it("fixed books count real pages (spreads use the last visible page)", () => {
    expect(availableModes(comic)).toEqual(["bookPages", "bookPercent"]);
    expect(progressLabel("bookPages", comic)).toBe("4 pages left");
    expect(progressLabel("chapterPercent", comic)).toBe("4 pages left"); // not available → first mode
    expect(fullPosition(comic)).toBe("Pages 4–5 of 9");
    expect(fullPosition(text)).toBe("Page 3 of 12");
  });
});

describe("toolbar settings", () => {
  it("keeps known items once, in the user's order", () => {
    expect(sanitizeSettings({ toolbar: ["layout", "toc", "layout", "bogus"] }).toolbar).toEqual(["layout", "toc"]);
    expect(sanitizeSettings({ toolbar: [] }).toolbar).toEqual([]);
    expect(sanitizeSettings({}).toolbar).toEqual(DEFAULT_TOOLBAR);
    expect(sanitizeSettings({ toolbarPosition: "left" }).toolbarPosition).toBe("bottom");
  });
});
