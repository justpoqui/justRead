// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { computeGeometry, screenCount, screenOf, useSpread } from "../src/lib/reader/geometry";

describe("paginated geometry", () => {
  it("obeys the reader's choice regardless of window shape", () => {
    for (const [w, h] of [[1400, 900], [700, 1200], [480, 900], [2000, 600]]) {
      expect(useSpread({ hostWidth: w!, hostHeight: h!, margin: 40, spread: "double" })).toBe(true);
      expect(useSpread({ hostWidth: w!, hostHeight: h!, margin: 40, spread: "single" })).toBe(false);
    }
  });

  it("produces integer columns whose stride tiles the document", () => {
    for (const [w, h, spread] of [[1400, 900, "double"], [1001, 700, "double"], [640, 900, "single"]] as const) {
      const g = computeGeometry({ hostWidth: w, hostHeight: h, margin: 48, spread });
      expect(Number.isInteger(g.columnWidth)).toBe(true);
      expect(g.frameWidth).toBe(g.columnWidth * g.columns + (g.columns - 1) * g.gap);
      expect(g.stride).toBe(g.columns * (g.columnWidth + g.gap));
      expect(g.left * 2 + g.frameWidth).toBeLessThanOrEqual(w);
    }
  });

  it("counts screens and locates x positions", () => {
    const g = computeGeometry({ hostWidth: 1000, hostHeight: 800, margin: 50, spread: "single" });
    // n columns occupy n*cw + (n-1)*gap pixels.
    const width = (n: number) => n * g.columnWidth + (n - 1) * g.gap;
    expect(screenCount(width(1), g)).toBe(1);
    expect(screenCount(width(5), g)).toBe(5);
    expect(screenOf(0, g)).toBe(0);
    expect(screenOf(g.stride * 3 + 10, g)).toBe(3);
    const s = computeGeometry({ hostWidth: 1400, hostHeight: 800, margin: 50, spread: "double" });
    const sw = (n: number) => n * s.columnWidth + (n - 1) * s.gap;
    expect(screenCount(sw(3), s)).toBe(2); // 3 pages → 2 spreads
    expect(screenCount(sw(4), s)).toBe(2);
  });
});
