// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { formatDuration, shortDuration } from "../src/lib/stats/format";

describe("durations", () => {
  it("formats reading time", () => {
    expect(formatDuration(0)).toBe("0 min");
    expect(formatDuration(30)).toBe("<1 min");
    expect(formatDuration(45 * 60)).toBe("45 min");
    expect(formatDuration(125 * 60)).toBe("2 h 5 min");
    expect(formatDuration(3 * 3600)).toBe("3 h");
    expect(shortDuration(40)).toBe("<1m");
    expect(shortDuration(20 * 60)).toBe("20m");
    expect(shortDuration(90 * 60)).toBe("1.5h");
    expect(shortDuration(12 * 3600)).toBe("12h");
  });
});
