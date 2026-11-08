// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { backingScale, buildSpreads, fitScreen, screenOfPage, scrollPageSize } from "../src/lib/fixed/layout";

const P = { width: 600, height: 900 }; // portrait
const L = { width: 1200, height: 900 }; // landscape (double-page scan)

describe("spreads", () => {
  it("keeps the cover alone and pairs the rest", () => {
    expect(buildSpreads([P, P, P, P, P], true)).toEqual([[0], [1, 2], [3, 4]]);
    expect(buildSpreads([P, P, P, P], true)).toEqual([[0], [1, 2], [3]]);
  });
  it("never pairs a landscape page", () => {
    expect(buildSpreads([P, P, L, P, P], true)).toEqual([[0], [1], [2], [3, 4]]);
  });
  it("is one page per screen without spreads", () => {
    expect(buildSpreads([P, P, P], false)).toEqual([[0], [1], [2]]);
    expect(screenOfPage([[0], [1, 2], [3, 4]], 4)).toBe(2);
  });
});

describe("fitting", () => {
  const vp = { width: 1000, height: 800 };
  it("fit-page keeps the whole page visible", () => {
    const [s] = fitScreen([P], vp, "page", 1);
    expect(s!.height).toBe(800);
    expect(s!.width).toBeLessThanOrEqual(1000);
  });
  it("fit-width fills the width", () => {
    const [s] = fitScreen([P], vp, "width", 1);
    expect(s!.width).toBe(1000);
    expect(s!.height).toBe(1500);
  });
  it("spread pages share a height and fit together", () => {
    const [a, b] = fitScreen([P, { width: 500, height: 900 }], vp, "page", 1, 0);
    expect(a!.height).toBe(b!.height);
    expect(a!.width + b!.width).toBeLessThanOrEqual(1001);
  });
  it("zoom multiplies", () => {
    const [s] = fitScreen([P], vp, "page", 2);
    expect(s!.height).toBe(1600);
  });
  it("scroll pages keep aspect ratio", () => {
    const s = scrollPageSize(P, vp, "width", 1);
    expect(s).toEqual({ width: 1000, height: 1500 });
    const f = scrollPageSize(P, vp, "page", 1);
    expect(f.height).toBe(800);
  });
  it("caps canvas memory", () => {
    expect(backingScale({ width: 1000, height: 1500 }, 2)).toBe(2);
    expect(backingScale({ width: 4000, height: 6000 }, 2)).toBeLessThan(1);
  });
});
