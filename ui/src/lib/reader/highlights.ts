// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Painting highlights by wrapping text in <mark> elements.
//
// Wrapping changes the DOM tree but not the text, and locators count text
// only, so every existing locator stays valid after painting/clearing.

import { textNodes } from "./dom-locator";

export const MARK_CLASS = "jr-hl";

/** Wrap the text inside `range` in marks tagged with `id`. */
export function paintHighlight(doc: Document, range: Range, id: string, color: string, hasNote = false): void {
  // STEP 1: collect the text nodes the range touches (before mutating anything).
  const root = range.commonAncestorContainer;
  const nodes = root.nodeType === Node.TEXT_NODE ? [root as Text] : textNodes(root);
  const targets = nodes.filter((t) => range.intersectsNode(t) && t.data.length > 0);

  for (const node of targets) {
    // STEP 2: clip to the range at both ends.
    const start = node === range.startContainer ? range.startOffset : 0;
    const end = node === range.endContainer ? range.endOffset : node.data.length;
    if (end <= start) continue;
    // Whitespace between blocks (e.g. between two <p>s) is not worth wrapping.
    if (!node.data.slice(start, end).trim() && (isBlock(node.previousSibling) || isBlock(node.nextSibling))) continue;

    // STEP 3: isolate [start, end) into its own text node and wrap it.
    let target = node;
    if (start > 0) target = target.splitText(start);
    if (end - start < target.data.length) target.splitText(end - start);
    const mark = doc.createElement("mark");
    mark.className = `${MARK_CLASS} jr-c-${color}${hasNote ? " jr-note" : ""}`;
    mark.dataset.hl = id;
    target.parentNode!.insertBefore(mark, target);
    mark.appendChild(target);
  }
}

/** Remove the marks of highlight `id`, merging text back together. */
export function clearHighlight(doc: Document, id: string): void {
  doc.querySelectorAll(`mark.${MARK_CLASS}[data-hl="${CSS.escape(id)}"]`).forEach((mark) => {
    const parent = mark.parentNode!;
    while (mark.firstChild) parent.insertBefore(mark.firstChild, mark);
    parent.removeChild(mark);
    parent.normalize();
  });
}

/** Remove every highlight mark from a document. */
export function clearAll(doc: Document): void {
  const ids = new Set<string>();
  doc.querySelectorAll(`mark.${MARK_CLASS}`).forEach((m) => ids.add((m as HTMLElement).dataset.hl ?? ""));
  ids.forEach((id) => clearHighlight(doc, id));
}

/** Block elements carry a data-jrb id (stamped at import). */
function isBlock(n: Node | null): boolean {
  return !!n && n.nodeType === Node.ELEMENT_NODE && (n as Element).hasAttribute("data-jrb");
}
