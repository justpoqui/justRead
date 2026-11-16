// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { DEFAULT_PROFILE, initials, sanitizeProfile } from "../src/lib/profile.svelte";

describe("profile", () => {
  it("makes initials", () => {
    expect(initials("Ada Lovelace")).toBe("AL");
    expect(initials("  marcus  ")).toBe("M");
    expect(initials("Jean Paul Sartre")).toBe("JS");
    expect(initials("")).toBe("");
  });

  it("keeps only sensible stored values", () => {
    expect(sanitizeProfile(null)).toEqual(DEFAULT_PROFILE);
    const p = sanitizeProfile({ name: "  A   very long name that keeps going and going on  ", emoji: "<img>", color: "red", yearGoal: 9999, dailyGoal: 7 });
    expect(p.name.length).toBeLessThanOrEqual(40);
    expect(p.name.startsWith("A very long")).toBe(true);
    expect(p.emoji).toBeNull();
    expect(p.color).toBe(DEFAULT_PROFILE.color);
    expect(p.yearGoal).toBe(365);
    expect(p.dailyGoal).toBe(0);
    expect(sanitizeProfile({ emoji: "🦉", yearGoal: 24, dailyGoal: 20 })).toMatchObject({ emoji: "🦉", yearGoal: 24, dailyGoal: 20 });
  });
});
