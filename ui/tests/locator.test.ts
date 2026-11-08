// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// The same vectors as crates/justread-locator/tests/vectors.rs.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { compareLocators, formatLocator, locateProgression, parseLocator, progression } from "../src/lib/reader/locator";

// Tests run with the ui/ directory as working directory.
const vectors = JSON.parse(readFileSync(resolve(process.cwd(), "../tests/fixtures/locators.json"), "utf8"));

describe("locator grammar (shared vectors)", () => {
  it("round-trips every valid vector", () => {
    for (const v of vectors.valid) {
      const loc = parseLocator(v.text);
      expect(loc, v.text).not.toBeNull();
      expect(formatLocator(loc!)).toBe(v.text);
      if (loc!.kind === "flow") expect([loc!.spine, loc!.block, loc!.offset]).toEqual([v.spine, v.block, v.offset]);
      else expect([loc!.page, loc!.offset]).toEqual([v.page, v.offset]);
    }
  });

  it("rejects every invalid vector", () => {
    for (const text of vectors.invalid) expect(parseLocator(text), JSON.stringify(text)).toBeNull();
  });

  it("orders the order vector", () => {
    const list = vectors.order.map((t: string) => parseLocator(t)!);
    for (let i = 1; i < list.length; i++) expect(compareLocators(list[i - 1], list[i])!).toBeLessThan(0);
    expect(compareLocators(parseLocator("p:1@0.5")!, parseLocator("f:0/0:0")!)).toBeNull();
  });
});

describe("progression (mirrors the Rust tests)", () => {
  const spine = [
    { chars: 100, blocks: 10 },
    { chars: 300, blocks: 4 },
  ];
  const at = (s: number, b: number) => progression(spine, { kind: "flow", spine: s, block: b, offset: 0 });

  it("weights chapters by size", () => {
    expect(at(0, 0)).toBe(0);
    expect(at(0, 5)).toBe(50 / 400);
    expect(at(1, 0)).toBe(0.25);
    expect(at(1, 2)).toBe(250 / 400);
    expect(at(9, 99)).toBe(1);
  });

  it("inverts", () => {
    expect(locateProgression(spine, 0.125)).toEqual({ kind: "flow", spine: 0, block: 5, offset: 0 });
    expect(locateProgression(spine, 0.625)).toEqual({ kind: "flow", spine: 1, block: 2, offset: 0 });
    expect(locateProgression(spine, 1)).toEqual({ kind: "flow", spine: 1, block: 3, offset: 0 });
  });
});
