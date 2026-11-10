// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// The reader's own screen timeout, replacing the device's while a book is open:
//
//   awake ──(no activity for dimAfter)──▶ dim ──(lockAfter more)──▶ locked
//     ▲                                     │                          │
//     └──────────── any activity ───────────┘      double-tap ─────────┘
//
// While awake or dim the screen is kept on. Once locked for RELEASE_MS the
// keep-on is released ("released"), so the device's own sleep timer takes
// over and an abandoned phone doesn't drain its battery. Pure logic with an
// injectable clock; tested in tests/idle.test.ts.

export type ScreenState = "awake" | "dim" | "locked" | "released";

/** How long the lock screen stays lit before the device may sleep. */
export const RELEASE_MS = 5 * 60_000;

export interface IdleOptions {
  /** Seconds without activity before dimming. */
  dimAfter: number;
  /** Seconds after dimming before the soft lock (0 = never lock). */
  lockAfter: number;
}

export class IdleGuard {
  private state: ScreenState = "awake";
  private last: number;

  constructor(
    private opts: IdleOptions,
    private onChange: (s: ScreenState) => void,
    private now: () => number = () => Date.now(),
  ) {
    this.last = now();
  }

  get current(): ScreenState {
    return this.state;
  }

  setOptions(opts: IdleOptions) {
    this.opts = opts;
    this.tick();
  }

  /**
   * The reader did something (tap, key, page turn, scroll).
   * Returns true when the event should be swallowed: the first touch on a
   * dimmed screen only brightens it, like the device's own dimming.
   */
  activity(): boolean {
    if (this.state === "locked" || this.state === "released") return true; // only a double-tap unlocks
    const wasDim = this.state === "dim";
    this.last = this.now();
    this.set("awake");
    return wasDim;
  }

  /** Double-tap on the lock screen. */
  unlock() {
    this.last = this.now();
    this.set("awake");
  }

  /** Call every second or so. */
  tick() {
    const idle = this.now() - this.last;
    const dimAt = this.opts.dimAfter * 1000;
    const lockAt = dimAt + this.opts.lockAfter * 1000;
    if (this.state === "awake" && idle >= dimAt) this.set("dim");
    if (this.state === "dim" && this.opts.lockAfter > 0 && idle >= lockAt) this.set("locked");
    if (this.state === "locked" && idle >= lockAt + RELEASE_MS) this.set("released");
  }

  private set(s: ScreenState) {
    if (s === this.state) return;
    this.state = s;
    this.onChange(s);
  }
}

/** Two taps within this window count as a double-tap. */
export const DOUBLE_TAP_MS = 400;

/** Tracks taps on the lock screen; returns true on the second tap of a double-tap. */
export function doubleTapDetector(now: () => number = () => Date.now()) {
  let lastTap = -Infinity;
  return (): boolean => {
    const t = now();
    const double = t - lastTap <= DOUBLE_TAP_MS;
    lastTap = double ? -Infinity : t;
    return double;
  };
}
