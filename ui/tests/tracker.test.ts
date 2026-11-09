// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { IDLE_MS, ReadingTracker, localDay, type Report } from "../src/lib/stats/tracker";

function setup(start = new Date(2026, 10, 9, 20, 0, 0).getTime()) {
  let t = start;
  const reports: Report[] = [];
  const tr = new ReadingTracker((r) => reports.push(r), () => t);
  return { tr, reports, advance: (ms: number) => (t += ms), at: () => t };
}

describe("reading tracker", () => {
  it("counts active time and sends it each minute", () => {
    const { tr, reports, advance } = setup();
    tr.moved(0.1);
    for (let i = 0; i < 12; i++) {
      advance(5000);
      tr.tick();
    }
    expect(reports).toEqual([{ day: "2026-11-09", seconds: 60, progress: 0 }]);
  });

  it("stops counting two minutes after the last page turn", () => {
    const { tr, reports, advance } = setup();
    tr.moved(0.1);
    advance(10 * 60_000); // left the book open for 10 minutes
    tr.tick();
    tr.flush();
    expect(reports.reduce((s, r) => s + r.seconds, 0)).toBe(IDLE_MS / 1000);
  });

  it("sums forward reading but not jumps or going back", () => {
    const { tr, reports, advance } = setup();
    tr.moved(0.1);
    advance(20_000);
    tr.moved(0.11); // a page
    advance(20_000);
    tr.moved(0.5); // jumped via contents
    advance(10_000);
    tr.moved(0.49); // back a page
    advance(10_000);
    tr.moved(0.5);
    advance(1_000); // progress travels with the next second of reading
    tr.flush();
    expect(reports.reduce((n, r) => n + r.seconds, 0)).toBe(61);
    expect(reports.reduce((n, r) => n + r.progress, 0)).toBeCloseTo(0.02, 10);
  });

  it("doesn't count while hidden, and flushes on hide", () => {
    const { tr, reports, advance } = setup();
    tr.moved(0.1);
    advance(30_000);
    tr.setVisible(false);
    expect(reports).toEqual([{ day: "2026-11-09", seconds: 30, progress: 0 }]);
    advance(60_000);
    tr.setVisible(true);
    tr.moved(0.1);
    advance(5_000);
    tr.flush();
    expect(reports[1]!.seconds).toBe(5);
  });

  it("splits at local midnight", () => {
    const { tr, reports, advance } = setup(new Date(2026, 10, 9, 23, 59, 30).getTime());
    tr.moved(0.1);
    advance(20_000);
    tr.tick();
    advance(20_000); // now 00:00:10 on the 10th: the 40 s so far go to the 9th
    tr.active();
    advance(10_000);
    tr.flush();
    expect(reports).toEqual([
      { day: "2026-11-09", seconds: 40, progress: 0 },
      { day: "2026-11-10", seconds: 10, progress: 0 },
    ]);
    expect(localDay(new Date(2026, 0, 5).getTime())).toBe("2026-01-05");
  });
});
