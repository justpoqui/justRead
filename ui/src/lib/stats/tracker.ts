// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Counts *active* reading time for the open book.
//
// Time counts only while the app is visible and the reader did something
// (turned a page, scrolled, selected) in the last IDLE_MS — so a book left
// open on the nightstand doesn't pile up hours. Forward movement through
// the book is summed too (for the reader's pace); big jumps (contents,
// position popup) are ignored. Reports go out about once a minute and when
// the book is closed or the app is hidden. Pure logic with an injectable
// clock, unit-tested in tests/tracker.test.ts.

/** No activity for this long → the reader stepped away; stop counting. */
export const IDLE_MS = 2 * 60_000;
/** A forward move bigger than this in one step is a jump, not reading. */
export const MAX_STEP = 0.02;
/** How often to send what was counted. */
export const FLUSH_MS = 60_000;

export interface Report {
  /** Reader's local date, YYYY-MM-DD. */
  day: string;
  seconds: number;
  /** Share of the book read forward (0..1). */
  progress: number;
}

/** Local calendar date (not UTC): "today" means the reader's today. */
export function localDay(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export class ReadingTracker {
  private lastActivity = -Infinity;
  private lastTick: number;
  private lastFlush: number;
  private ms = 0;
  private progress = 0;
  private position: number | null = null;
  private day: string;
  private visible = true;

  constructor(
    private send: (r: Report) => void,
    private now: () => number = () => Date.now(),
  ) {
    this.lastTick = this.lastFlush = now();
    this.day = localDay(this.lastTick);
  }

  /** The reader moved (page turn, scroll); `progression` is 0..1 through the book. */
  moved(progression: number) {
    this.tick();
    if (this.position !== null) {
      const step = progression - this.position;
      if (step > 0 && step <= MAX_STEP) this.progress += step;
    }
    this.position = progression;
    this.lastActivity = this.now();
  }

  /** Any other sign of reading (selection, taps). */
  active() {
    this.tick();
    this.lastActivity = this.now();
  }

  /** The app was hidden or shown (visibilitychange). Hiding flushes. */
  setVisible(visible: boolean) {
    this.tick();
    this.visible = visible;
    if (!visible) this.flush();
    else this.lastTick = this.now();
  }

  /** Add the active time since the last call (up to the idle cut-off). */
  private count() {
    const t = this.now();
    if (this.visible) {
      const until = Math.min(t, this.lastActivity + IDLE_MS);
      if (until > this.lastTick) this.ms += until - this.lastTick;
    }
    this.lastTick = t;
    return t;
  }

  /** Call regularly (every few seconds): counts time and sends a report each minute. */
  tick() {
    const t = this.count();
    // Midnight: what was counted so far is sent for the day it was counted on.
    const today = localDay(t);
    if (today !== this.day) {
      this.flush();
      this.day = today;
    }
    if (t - this.lastFlush >= FLUSH_MS) this.flush();
  }

  /** Send whatever was counted (whole seconds; the remainder carries over). */
  flush() {
    this.lastFlush = this.count();
    const seconds = Math.floor(this.ms / 1000);
    if (seconds < 1) return;
    // The backend accepts at most 600 s per report.
    const sent = Math.min(seconds, 600);
    this.ms -= sent * 1000;
    const progress = this.progress;
    this.progress = 0;
    this.send({ day: this.day, seconds: sent, progress });
  }
}
