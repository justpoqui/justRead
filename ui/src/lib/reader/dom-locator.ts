// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Mapping between DOM positions and flow locators.
//
// Every block element in a justBook chapter carries `data-jrb="N"` (stamped
// at import). A position is (innermost enclosing block, UTF-16 offset into
// that block's textContent). Because offsets are counted over *text only*,
// they don't change when highlights wrap text in <mark> elements, and they
// don't depend on layout at all.

import type { FlowLocator } from "./locator";

export const BLOCK_ATTR = "data-jrb";

/** Innermost element with a block id containing `node` (or itself). */
export function blockOf(node: Node): Element | null {
  const el = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  return el?.closest(`[${BLOCK_ATTR}]`) ?? null;
}

/** All text nodes under `root`, in document order. */
export function textNodes(root: Node): Text[] {
  const doc = root.ownerDocument ?? (root as Document);
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const out: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) out.push(n as Text);
  return out;
}

/** UTF-16 offset of DOM boundary point (node, offset) inside `block`. */
export function offsetInBlock(block: Element, node: Node, offset: number): number {
  // STEP 1: a point inside a text node — all text before that node, plus offset.
  if (node.nodeType === Node.TEXT_NODE) {
    let acc = 0;
    for (const t of textNodes(block)) {
      if (t === node) return acc + offset;
      acc += t.data.length;
    }
    return acc;
  }
  // STEP 2: a point between children of an element — it sits just before
  // child[offset] (or after the element's last descendant when offset is past the end).
  const ref = node.childNodes[offset] ?? null;
  let acc = 0;
  for (const t of textNodes(block)) {
    const before = ref
      ? ref !== t && !ref.contains(t) && !!(t.compareDocumentPosition(ref) & Node.DOCUMENT_POSITION_FOLLOWING)
      : node.contains(t) || !!(t.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING);
    if (!before) break;
    acc += t.data.length;
  }
  return acc;
}

/** Inverse of `offsetInBlock`: the text position at `offset` in `block`. */
export function positionInBlock(block: Element, offset: number): { node: Node; offset: number } {
  // STEP 1: walk text nodes until the offset falls inside one.
  const nodes = textNodes(block);
  let acc = 0;
  for (const t of nodes) {
    if (offset <= acc + t.data.length) return { node: t, offset: Math.max(0, offset - acc) };
    acc += t.data.length;
  }
  // STEP 2: past the end → end of the last text node, or the block start.
  const last = nodes[nodes.length - 1];
  return last ? { node: last, offset: last.data.length } : { node: block, offset: 0 };
}

/** Locator for a DOM boundary point inside chapter `spine`. */
export function locatorAt(spine: number, node: Node, offset: number): FlowLocator | null {
  const block = blockOf(node);
  if (!block) return null;
  const id = Number(block.getAttribute(BLOCK_ATTR));
  if (!Number.isInteger(id) || id < 0) return null;
  return { kind: "flow", spine, block: id, offset: offsetInBlock(block, node, offset) };
}

/** Find the block element for a locator (falls back to <body>). */
export function findBlock(doc: Document, loc: FlowLocator): Element | null {
  return doc.querySelector(`[${BLOCK_ATTR}="${loc.block}"]`) ?? doc.body;
}

/** Collapsed range at a locator. */
export function rangeAt(doc: Document, loc: FlowLocator): Range | null {
  const block = findBlock(doc, loc);
  if (!block) return null;
  const pos = positionInBlock(block, loc.offset);
  const r = doc.createRange();
  r.setStart(pos.node, pos.offset);
  r.collapse(true);
  return r;
}

/** Range between two locators of the same chapter. */
export function rangeBetween(doc: Document, start: FlowLocator, end: FlowLocator): Range | null {
  const a = findBlock(doc, start);
  const b = findBlock(doc, end);
  if (!a || !b) return null;
  const s = positionInBlock(a, start.offset);
  const e = positionInBlock(b, end.offset);
  const r = doc.createRange();
  r.setStart(s.node, s.offset);
  r.setEnd(e.node, e.offset);
  return r;
}

/** Start/end locators of a (non-collapsed) range. */
export function locatorsOfRange(spine: number, range: Range): { start: FlowLocator; end: FlowLocator } | null {
  const start = locatorAt(spine, range.startContainer, range.startOffset);
  const end = locatorAt(spine, range.endContainer, range.endOffset);
  return start && end ? { start, end } : null;
}

/**
 * First text position for which `test(rect)` is true, assuming `test` is
 * monotone (false … false true … true) in document order — e.g. "is not
 * before the viewport". Used to find the first/last visible character.
 */
export function firstPositionWhere(doc: Document, test: (r: DOMRect) => boolean): { node: Text; offset: number } | null {
  const range = doc.createRange();
  const rectOf = (node: Text, i: number): DOMRect | null => {
    range.setStart(node, i);
    range.setEnd(node, Math.min(node.data.length, i + 1));
    const rects = range.getClientRects();
    return rects.length ? rects[0]! : null;
  };

  for (const node of textNodes(doc.body)) {
    // STEP 1: skip nodes that are only whitespace or not rendered.
    if (!node.data.trim()) continue;
    range.selectNodeContents(node);
    const rects = range.getClientRects();
    if (rects.length === 0) continue;

    // STEP 2: does this node reach the boundary? (its last box passes the test)
    if (!test(rects[rects.length - 1]!)) continue;

    // STEP 3: binary search the first character that passes.
    let lo = 0;
    let hi = node.data.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      const r = rectOf(node, mid);
      if (r && test(r)) hi = mid;
      else lo = mid + 1;
    }
    return { node, offset: lo };
  }
  return null;
}

/** Bounding rect of a locator's position (for scrolling/page math). */
export function rectAt(doc: Document, loc: FlowLocator): DOMRect | null {
  const block = findBlock(doc, loc);
  if (!block) return null;
  const pos = positionInBlock(block, loc.offset);
  const r = doc.createRange();
  // STEP 1: a one-character range has a real box in every engine
  // (collapsed ranges report empty rects in some).
  if (pos.node.nodeType === Node.TEXT_NODE) {
    const len = (pos.node as Text).data.length;
    const start = Math.min(pos.offset, Math.max(0, len - 1));
    r.setStart(pos.node, start);
    r.setEnd(pos.node, Math.min(len, start + 1));
    const rects = r.getClientRects();
    if (rects.length) return rects[0]!;
  }
  // STEP 2: fall back to the block's own box.
  return block.getBoundingClientRect();
}

/** Letters, digits and in-word apostrophes/hyphens count as word characters. */
const WORD_CHAR = /[\p{L}\p{N}\p{M}'’-]/u;

/**
 * Expand [start, end) outward to whole words, so a drag that begins or ends
 * mid-word highlights complete words (what readers expect).
 */
export function snapToWords(doc: Document, start: FlowLocator, end: FlowLocator): { start: FlowLocator; end: FlowLocator } {
  // STEP 1: move the start left while the previous character is part of a word.
  const sText = findBlock(doc, start)?.textContent ?? "";
  let s = Math.min(start.offset, sText.length);
  if (WORD_CHAR.test(sText.charAt(s))) while (s > 0 && WORD_CHAR.test(sText.charAt(s - 1))) s--;

  // STEP 2: move the end right while the next character is part of a word.
  const eText = findBlock(doc, end)?.textContent ?? "";
  let e = Math.min(end.offset, eText.length);
  if (e > 0 && WORD_CHAR.test(eText.charAt(e - 1))) while (e < eText.length && WORD_CHAR.test(eText.charAt(e))) e++;

  return { start: { ...start, offset: s }, end: { ...end, offset: e } };
}
