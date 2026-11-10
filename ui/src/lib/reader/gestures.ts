// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Touch gesture recognition shared by both reading engines (pure state
// machine, unit-tested): horizontal swipes (with a live drag offset so the
// page can follow the finger) and two-finger pinches.

export type Gesture = { kind: "swipe"; direction: "left" | "right" } | { kind: "pinch"; scale: number } | null;

interface Point {
  x: number;
  y: number;
}

/** Movement (CSS px) before a one-finger gesture commits to horizontal or vertical. */
const AXIS_LOCK = 10;
/** A drag this far (share of the page width, at least SWIPE_MIN px) turns the page, however slow. */
const SWIPE_SHARE = 0.2;
const SWIPE_MIN = 50;
/** A quick flick turns the page with less travel. */
const FLICK_MIN = 30;
const FLICK_MAX_MS = 300;

export class GestureTracker {
  private start: Point[] = [];
  private last: Point[] = [];
  private t0 = 0;
  private maxFingers = 0;
  private axis: "x" | "y" | null = null;

  /** touchstart / touchmove: current finger positions. */
  touch(points: Point[], now: number, isStart: boolean): void {
    if (isStart && this.start.length === 0) {
      this.t0 = now;
      this.maxFingers = 0;
      this.axis = null;
    }
    // A finger added mid-gesture restarts the measurement (e.g. pinch start).
    if (isStart || this.start.length !== points.length) this.start = points.map((p) => ({ ...p }));
    this.last = points.map((p) => ({ ...p }));
    this.maxFingers = Math.max(this.maxFingers, points.length);
    // One finger: decide once whether this is a horizontal or a vertical gesture.
    if (this.axis === null && this.maxFingers === 1 && this.start[0] && this.last[0]) {
      const dx = this.last[0].x - this.start[0].x;
      const dy = this.last[0].y - this.start[0].y;
      if (Math.hypot(dx, dy) >= AXIS_LOCK) this.axis = Math.abs(dx) > Math.abs(dy) ? "x" : "y";
    }
  }

  /** While one finger drags horizontally: how far (px) it has moved; otherwise null. */
  dragOffset(): number | null {
    if (this.axis !== "x" || this.maxFingers !== 1 || !this.start[0] || !this.last[0]) return null;
    return this.last[0].x - this.start[0].x;
  }

  /**
   * touchend / touchcancel with no fingers left: what happened?
   * `width` is the page width, so a long enough drag turns the page at any speed.
   */
  end(now: number, width = 0): Gesture {
    const { start, last, maxFingers, axis } = this;
    this.start = [];
    this.axis = null;
    // STEP 1: two fingers → pinch, as the ratio of finger distances.
    if (maxFingers >= 2 && start.length >= 2 && last.length >= 2) {
      const d = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);
      const d0 = d(start[0]!, start[1]!);
      const d1 = d(last[0]!, last[1]!);
      if (d0 > 10 && Math.abs(d1 / d0 - 1) > 0.08) return { kind: "pinch", scale: d1 / d0 };
      return null;
    }
    // STEP 2: one finger, horizontal: a long drag or a quick flick.
    if (maxFingers !== 1 || axis !== "x" || !start[0] || !last[0]) return null;
    const dx = last[0].x - start[0].x;
    const far = Math.abs(dx) >= Math.max(SWIPE_MIN, width * SWIPE_SHARE);
    const flick = Math.abs(dx) >= FLICK_MIN && now - this.t0 <= FLICK_MAX_MS;
    if (!far && !flick) return null;
    return { kind: "swipe", direction: dx < 0 ? "left" : "right" };
  }
}

/**
 * Positions of a TouchList as plain points. `screen` uses screen coordinates:
 * needed inside an iframe that moves with the finger (its own coordinates
 * would move along with it).
 */
export function points(list: TouchList, screen = false): Point[] {
  return Array.from(list, (t) => (screen ? { x: t.screenX, y: t.screenY } : { x: t.clientX, y: t.clientY }));
}
