// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { buildSrcdoc } from "../src/lib/reader/chapter-doc";
import { computeGeometry } from "../src/lib/reader/geometry";
import { readerCss } from "../src/lib/reader/styles";
import { DEFAULT_SETTINGS, sanitizeSettings } from "../src/lib/settings/reader-settings";
import { bookmarksInRange } from "../src/lib/reader/bookmarks";
import type { Annotation } from "../src/lib/ipc/types";

describe("reader CSS", () => {
  const g = computeGeometry({ hostWidth: 1200, hostHeight: 800, margin: 40, spread: "never" });

  it("paginates with columns matching the geometry", () => {
    const css = readerCss(DEFAULT_SETTINGS, { kind: "paginated", geometry: g });
    expect(css).toContain(`column-width: ${g.columnWidth}px`);
    expect(css).toContain(`column-gap: ${g.gap}px`);
    expect(css).toContain(`font-size: ${DEFAULT_SETTINGS.fontSize}px`);
    expect(css).toContain("hyphens: auto");
  });

  it("forces colors only on dark themes", () => {
    expect(readerCss({ ...DEFAULT_SETTINGS, theme: "light" }, { kind: "scrolled", width: 600 })).not.toContain("background-color: transparent");
    expect(readerCss({ ...DEFAULT_SETTINGS, theme: "dark" }, { kind: "scrolled", width: 600 })).toContain("background-color: transparent");
  });

  it("injects the CSP first, then <base>, and reader styles last", () => {
    const doc = buildSrcdoc('<!DOCTYPE html><html><head>\n<link rel="stylesheet" href="../resources/a.css" /></head><body data-jrb="0"></body></html>', "justbook://localhost/id/content/", "p{}");
    const head = doc.slice(doc.indexOf("<head>") + 6).trimStart();
    expect(head.startsWith(`<meta http-equiv="Content-Security-Policy" content="script-src 'none';`)).toBe(true);
    expect(doc.indexOf("<base")).toBeLessThan(doc.indexOf("<link"));
    expect(doc.indexOf('id="jr-reader-style"')).toBeGreaterThan(doc.indexOf("<link"));
  });
});

describe("settings sanitizing", () => {
  it("clamps, repairs and strips unsafe font names", () => {
    const s = sanitizeSettings({ fontSize: 500, theme: "neon", customFont: 'X"; } body { display:none', customColors: { background: "red" } });
    expect(s.fontSize).toBe(48);
    expect(s.theme).toBe("light");
    expect(s.customFont).not.toMatch(/[";{}:]/);
    expect(s.customColors.background).toBe(DEFAULT_SETTINGS.customColors.background);
    expect(sanitizeSettings(null)).toEqual(DEFAULT_SETTINGS);
  });
});

describe("bookmarks on the visible page", () => {
  const bm = (start: string): Annotation => ({
    id: start, bookId: "b", kind: "bookmark", start, end: null, quote: "", note: "", color: "yellow", chapter: null, createdAt: 0, updatedAt: 0,
  });
  const list = [bm("f:1/5:0"), bm("f:1/9:0"), bm("f:2/0:0")];
  it("selects bookmarks inside [start, end)", () => {
    const range = { start: { kind: "flow", spine: 1, block: 4, offset: 0 }, end: { kind: "flow", spine: 1, block: 9, offset: 0 } } as const;
    expect(bookmarksInRange(list, range).map((a) => a.start)).toEqual(["f:1/5:0"]);
    const open = { start: range.start, end: null };
    expect(bookmarksInRange(list, open).map((a) => a.start)).toEqual(["f:1/5:0", "f:1/9:0"]);
  });
});
