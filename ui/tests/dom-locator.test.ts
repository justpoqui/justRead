// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

import { beforeEach, describe, expect, it } from "vitest";
import { blockOf, locatorAt, locatorsOfRange, offsetInBlock, positionInBlock, rangeAt, rangeBetween, snapToWords } from "../src/lib/reader/dom-locator";
import { clearHighlight, paintHighlight } from "../src/lib/reader/highlights";

// A chapter shaped exactly like ChapterWriter output.
const CHAPTER = `<body data-jrb="0"><h2 data-jrb="1">Chapter One</h2>
<p data-jrb="2">Hello <em>brave <b>new</b></em> world.</p>
<blockquote data-jrb="3"><p data-jrb="4">Quoted 😀 text.</p></blockquote>
<p data-jrb="5"></p></body>`;

let doc: Document;
beforeEach(() => {
  doc = document.implementation.createHTMLDocument("t");
  doc.documentElement.innerHTML = `<head></head>${CHAPTER}`;
});

const block = (n: number) => doc.querySelector(`[data-jrb="${n}"]`)!;

describe("DOM ↔ locator", () => {
  it("finds the innermost block", () => {
    const b = doc.querySelector("b")!;
    expect(blockOf(b.firstChild!)!.getAttribute("data-jrb")).toBe("2");
    expect(blockOf(doc.querySelector("blockquote p")!.firstChild!)!.getAttribute("data-jrb")).toBe("4");
  });

  it("counts offsets across inline elements", () => {
    const p = block(2);
    const newText = doc.querySelector("b")!.firstChild!;
    // "Hello " (6) + "brave " (6) = 12, then 1 into "new"
    expect(offsetInBlock(p, newText, 1)).toBe(13);
    expect(locatorAt(7, newText, 1)).toEqual({ kind: "flow", spine: 7, block: 2, offset: 13 });
    // Element boundary point: before the <em> child (index 1) = after "Hello ".
    expect(offsetInBlock(p, p, 1)).toBe(6);
  });

  it("counts UTF-16 units like the Rust side", () => {
    const q = block(4).firstChild!; // "Quoted 😀 text."
    expect(offsetInBlock(block(4), q, (q as Text).data.indexOf("text"))).toBe(10);
  });

  it("round-trips positions", () => {
    for (const off of [0, 5, 6, 12, 13, 16, 22]) {
      const pos = positionInBlock(block(2), off);
      expect(offsetInBlock(block(2), pos.node, pos.offset)).toBe(off);
    }
  });

  it("handles empty blocks and missing blocks", () => {
    const r = rangeAt(doc, { kind: "flow", spine: 0, block: 5, offset: 3 })!;
    expect(r.startContainer).toBe(block(5));
    // An unknown block id falls back to <body>.
    expect(rangeAt(doc, { kind: "flow", spine: 0, block: 999, offset: 0 })!.startContainer).toBeTruthy();
  });

  it("builds ranges between locators", () => {
    const r = rangeBetween(doc, { kind: "flow", spine: 0, block: 2, offset: 6 }, { kind: "flow", spine: 0, block: 2, offset: 15 })!;
    expect(r.toString()).toBe("brave new");
    expect(locatorsOfRange(0, r)).toEqual({
      start: { kind: "flow", spine: 0, block: 2, offset: 6 },
      end: { kind: "flow", spine: 0, block: 2, offset: 15 },
    });
  });
});

describe("word snapping", () => {
  const f = (block: number, offset: number) => ({ kind: "flow" as const, spine: 0, block, offset });
  it("expands partial words at both ends", () => {
    // "Hello brave new world." — select "llo bra" (2..9)
    const r = snapToWords(doc, f(2, 2), f(2, 9));
    expect([r.start.offset, r.end.offset]).toEqual([0, 11]);
    expect(rangeBetween(doc, r.start, r.end)!.toString()).toBe("Hello brave");
  });
  it("leaves word-aligned selections and punctuation alone", () => {
    const r = snapToWords(doc, f(2, 6), f(2, 11)); // "brave"
    expect([r.start.offset, r.end.offset]).toEqual([6, 11]);
    const p = snapToWords(doc, f(2, 16), f(2, 22)); // "world."
    expect([p.start.offset, p.end.offset]).toEqual([16, 22]);
  });
});

describe("highlights keep locators stable", () => {
  it("paints and clears without changing text or offsets", () => {
    // STEP 1: paint a highlight spanning two blocks.
    const before = doc.body.textContent;
    const r = rangeBetween(doc, { kind: "flow", spine: 0, block: 2, offset: 12 }, { kind: "flow", spine: 0, block: 4, offset: 6 })!;
    paintHighlight(doc, r, "h1", "yellow");
    const marks = doc.querySelectorAll('mark[data-hl="h1"]');
    expect(marks.length).toBeGreaterThan(1);
    expect([...marks].map((m) => m.textContent).join("")).toContain("new world.");
    expect(doc.body.textContent).toBe(before);

    // STEP 2: locators computed on the painted DOM are the same as before.
    // The mark holding " world." starts right after "Hello brave new" (15 units).
    const world = [...marks].find((m) => m.textContent!.includes("world"))!.firstChild!;
    expect(locatorAt(0, world, 0)).toEqual({ kind: "flow", spine: 0, block: 2, offset: 15 });
    const again = rangeBetween(doc, { kind: "flow", spine: 0, block: 2, offset: 6 }, { kind: "flow", spine: 0, block: 2, offset: 15 })!;
    expect(again.toString()).toBe("brave new");

    // STEP 3: clearing restores the original structure.
    clearHighlight(doc, "h1");
    expect(doc.querySelectorAll("mark").length).toBe(0);
    expect(doc.body.textContent).toBe(before);
    expect(block(2).innerHTML).toBe("Hello <em>brave <b>new</b></em> world.");
  });
});
