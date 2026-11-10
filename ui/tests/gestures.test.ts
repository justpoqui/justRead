// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { GestureTracker } from "../src/lib/reader/gestures";

const p = (x: number, y: number) => ({ x, y });

describe("gestures", () => {
  it("recognizes quick horizontal swipes", () => {
    const g = new GestureTracker();
    g.touch([p(300, 400)], 0, true);
    g.touch([p(200, 410)], 100, false);
    g.touch([p(120, 415)], 200, false);
    expect(g.end(250)).toEqual({ kind: "swipe", direction: "left" });
    g.touch([p(100, 400)], 0, true);
    g.touch([p(260, 380)], 150, false);
    expect(g.end(200)).toEqual({ kind: "swipe", direction: "right" });
  });

  it("turns on a long drag at any speed, or a quick flick", () => {
    const g = new GestureTracker();
    g.touch([p(300, 100)], 0, true);
    g.touch([p(250, 102)], 600, false);
    expect(g.dragOffset()).toBe(-50); // the page follows the finger
    g.touch([p(200, 104)], 1200, false);
    expect(g.end(1300, 400)).toEqual({ kind: "swipe", direction: "left" }); // 100 px of 400: slow but far enough
    g.touch([p(100, 100)], 0, true);
    g.touch([p(140, 100)], 120, false);
    expect(g.end(150, 400)).toEqual({ kind: "swipe", direction: "right" }); // short, quick flick
    g.touch([p(100, 100)], 0, true);
    g.touch([p(150, 100)], 900, false);
    expect(g.end(1000, 400)).toBeNull(); // short and slow: changed their mind
  });

  it("ignores taps and vertical scrolls", () => {
    const g = new GestureTracker();
    g.touch([p(100, 100)], 0, true);
    expect(g.end(80)).toBeNull(); // tap
    expect(g.dragOffset()).toBeNull();
    g.touch([p(100, 100)], 0, true);
    g.touch([p(104, 130)], 50, false); // commits to vertical first…
    g.touch([p(260, 140)], 150, false); // …so a later sideways move doesn't turn the page
    expect(g.dragOffset()).toBeNull();
    expect(g.end(160, 400)).toBeNull();
  });

  it("measures pinches as distance ratios", () => {
    const g = new GestureTracker();
    g.touch([p(200, 300)], 0, true);
    g.touch([p(200, 300), p(300, 300)], 10, true); // second finger
    g.touch([p(150, 300), p(350, 300)], 200, false);
    expect(g.end(260)).toEqual({ kind: "pinch", scale: 2 });
    g.touch([p(100, 300), p(300, 300)], 0, true);
    g.touch([p(195, 300), p(205, 300)], 100, false);
    const r = g.end(150);
    expect(r?.kind).toBe("pinch");
    expect((r as { scale: number }).scale).toBeCloseTo(0.05, 2);
  });
});
