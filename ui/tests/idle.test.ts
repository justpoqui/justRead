// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { IdleGuard, RELEASE_MS, doubleTapDetector, type ScreenState } from "../src/lib/screen/idle";

function setup(dimAfter = 120, lockAfter = 30) {
  let t = 0;
  const seen: ScreenState[] = [];
  const g = new IdleGuard({ dimAfter, lockAfter }, (s) => seen.push(s), () => t);
  const at = (sec: number) => {
    t = sec * 1000;
    g.tick();
  };
  return { g, seen, at, set: (sec: number) => (t = sec * 1000) };
}

describe("reader screen timeout", () => {
  it("dims after 2 minutes, soft-locks 30 seconds later, then lets the device sleep", () => {
    const { g, seen, at } = setup();
    at(119);
    expect(g.current).toBe("awake");
    at(120);
    expect(g.current).toBe("dim");
    at(149);
    expect(g.current).toBe("dim");
    at(150);
    expect(g.current).toBe("locked");
    at(150 + RELEASE_MS / 1000);
    expect(seen).toEqual(["dim", "locked", "released"]);
  });

  it("activity restarts the clock; the first touch on a dim screen only brightens it", () => {
    const { g, at, set } = setup();
    at(100);
    set(100);
    expect(g.activity()).toBe(false); // awake: the tap does its normal job
    at(219);
    expect(g.current).toBe("awake");
    at(220);
    expect(g.current).toBe("dim");
    expect(g.activity()).toBe(true); // swallowed
    expect(g.current).toBe("awake");
  });

  it("only a double-tap unlocks", () => {
    const { g, at, set } = setup();
    at(150);
    expect(g.current).toBe("locked");
    expect(g.activity()).toBe(true);
    expect(g.current).toBe("locked");
    set(151);
    g.unlock();
    expect(g.current).toBe("awake");
    at(270);
    expect(g.current).toBe("awake");
    at(271); // 2 minutes after unlocking
    expect(g.current).toBe("dim");
  });

  it("lockAfter 0 never locks", () => {
    const { g, at } = setup(60, 0);
    at(60);
    at(10_000);
    expect(g.current).toBe("dim");
  });

  it("detects double-taps", () => {
    let t = 0;
    const tap = doubleTapDetector(() => t);
    expect(tap()).toBe(false);
    t = 300;
    expect(tap()).toBe(true);
    t = 1000;
    expect(tap()).toBe(false);
    t = 1600; // too slow
    expect(tap()).toBe(false);
    t = 1700;
    expect(tap()).toBe(true);
    t = 1800; // a third quick tap starts over
    expect(tap()).toBe(false);
  });
});
