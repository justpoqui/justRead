// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Touch gesture recognition shared by both reading engines (pure state
// machine, unit-tested): horizontal swipes and two-finger pinches.

export type Gesture = { kind: "swipe"; direction: "left" | "right" } | { kind: "pinch"; scale: number } | null;

interface Point {
  x: number;
  y: number;
}

/** Minimum horizontal travel (CSS px) and maximum duration of a swipe. */
const SWIPE_MIN = 50;
const SWIPE_MAX_MS = 700;

export class GestureTracker {
  private start: Point[] = [];
  private last: Point[] = [];
  private t0 = 0;
  private maxFingers = 0;

  /** touchstart / touchmove: current finger positions. */
  touch(points: Point[], now: number, isStart: boolean): void {
    if (isStart && this.start.length === 0) {
      this.t0 = now;
      this.maxFingers = 0;
    }
    // A finger added mid-gesture restarts the measurement (e.g. pinch start).
    if (isStart || this.start.length !== points.length) this.start = points.map((p) => ({ ...p }));
    this.last = points.map((p) => ({ ...p }));
    this.maxFingers = Math.max(this.maxFingers, points.length);
  }

  /** touchend with no fingers left: what happened? */
  end(now: number): Gesture {
    const { start, last, maxFingers } = this;
    this.start = [];
    // STEP 1: two fingers → pinch, as the ratio of finger distances.
    if (maxFingers >= 2 && start.length >= 2 && last.length >= 2) {
      const d = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);
      const d0 = d(start[0]!, start[1]!);
      const d1 = d(last[0]!, last[1]!);
      if (d0 > 10 && Math.abs(d1 / d0 - 1) > 0.08) return { kind: "pinch", scale: d1 / d0 };
      return null;
    }
    // STEP 2: one finger → a quick, mostly horizontal swipe.
    if (maxFingers !== 1 || !start[0] || !last[0]) return null;
    const dx = last[0].x - start[0].x;
    const dy = last[0].y - start[0].y;
    if (now - this.t0 > SWIPE_MAX_MS || Math.abs(dx) < SWIPE_MIN || Math.abs(dx) < Math.abs(dy) * 1.5) return null;
    return { kind: "swipe", direction: dx < 0 ? "left" : "right" };
  }
}

/** Positions of a TouchList as plain points. */
export function points(list: TouchList): Point[] {
  return Array.from(list, (t) => ({ x: t.clientX, y: t.clientY }));
}
