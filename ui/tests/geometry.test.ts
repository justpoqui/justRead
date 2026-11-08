// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { computeGeometry, screenCount, screenOf, useSpread } from "../src/lib/reader/geometry";

describe("paginated geometry", () => {
  it("uses a spread only on wide landscape screens in auto mode", () => {
    expect(useSpread({ hostWidth: 1400, hostHeight: 900, margin: 40, spread: "auto" })).toBe(true);
    expect(useSpread({ hostWidth: 800, hostHeight: 900, margin: 40, spread: "auto" })).toBe(false);
    // Landscape but too narrow for two comfortable columns at this font size.
    expect(useSpread({ hostWidth: 920, hostHeight: 700, margin: 48, spread: "auto", fontSize: 19 })).toBe(false);
    expect(useSpread({ hostWidth: 1420, hostHeight: 800, margin: 48, spread: "auto", fontSize: 19 })).toBe(true);
    expect(useSpread({ hostWidth: 1000, hostHeight: 1200, margin: 40, spread: "auto" })).toBe(false);
    expect(useSpread({ hostWidth: 700, hostHeight: 900, margin: 40, spread: "always" })).toBe(true);
    expect(useSpread({ hostWidth: 1400, hostHeight: 900, margin: 40, spread: "never" })).toBe(false);
  });

  it("produces integer columns whose stride tiles the document", () => {
    for (const [w, h, spread] of [[1400, 900, "auto"], [1001, 700, "always"], [640, 900, "never"]] as const) {
      const g = computeGeometry({ hostWidth: w, hostHeight: h, margin: 48, spread });
      expect(Number.isInteger(g.columnWidth)).toBe(true);
      expect(g.frameWidth).toBe(g.columnWidth * g.columns + (g.columns - 1) * g.gap);
      expect(g.stride).toBe(g.columns * (g.columnWidth + g.gap));
      expect(g.left * 2 + g.frameWidth).toBeLessThanOrEqual(w);
    }
  });

  it("counts screens and locates x positions", () => {
    const g = computeGeometry({ hostWidth: 1000, hostHeight: 800, margin: 50, spread: "never" });
    // n columns occupy n*cw + (n-1)*gap pixels.
    const width = (n: number) => n * g.columnWidth + (n - 1) * g.gap;
    expect(screenCount(width(1), g)).toBe(1);
    expect(screenCount(width(5), g)).toBe(5);
    expect(screenOf(0, g)).toBe(0);
    expect(screenOf(g.stride * 3 + 10, g)).toBe(3);
    const s = computeGeometry({ hostWidth: 1400, hostHeight: 800, margin: 50, spread: "always" });
    const sw = (n: number) => n * s.columnWidth + (n - 1) * s.gap;
    expect(screenCount(sw(3), s)).toBe(2); // 3 pages → 2 spreads
    expect(screenCount(sw(4), s)).toBe(2);
  });
});
