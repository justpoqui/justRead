// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Paginated view: one chapter at a time, laid out in CSS columns; one or
// two columns ("spread") per screen. See geometry.ts for the math.

import { applyCss } from "./chapter-doc";
import { firstPositionWhere, locatorAt, rectAt } from "./dom-locator";
import { createFrame, loadFrame, nextLayout, wireFrame } from "./frame";
import { computeGeometry, screenCount, screenOf, type Geometry } from "./geometry";
import type { FlowLocator } from "./locator";
import { readerCss } from "./styles";
import type { View, ViewContext, VisibleRange } from "./view";

export class PaginatedView implements View {
  private frame = createFrame();
  private doc: Document | null = null;
  private unwire: (() => void) | null = null;
  private spine = -1;
  private screen = 0;
  private screens = 1;
  private geometry: Geometry;
  private resizeObs: ResizeObserver;
  private resizeTimer = 0;

  constructor(private ctx: ViewContext) {
    // STEP 1: the iframe is absolutely positioned inside the host.
    this.geometry = this.computeGeometry();
    this.frame.style.position = "absolute";
    ctx.host.appendChild(this.frame);
    this.placeFrame();

    // STEP 2: re-layout (debounced) when the window/host size changes.
    this.resizeObs = new ResizeObserver(() => {
      clearTimeout(this.resizeTimer);
      this.resizeTimer = window.setTimeout(() => void this.relayout(), 120);
    });
    this.resizeObs.observe(ctx.host);
  }

  private computeGeometry(): Geometry {
    const s = this.ctx.settings();
    return computeGeometry({
      hostWidth: this.ctx.host.clientWidth,
      hostHeight: this.ctx.host.clientHeight,
      margin: s.margin,
      spread: s.spread,
    });
  }

  private placeFrame() {
    const g = this.geometry;
    Object.assign(this.frame.style, {
      left: `${g.left}px`,
      top: `${g.top}px`,
      width: `${g.frameWidth}px`,
      height: `${g.frameHeight}px`,
    });
  }

  private css(): string {
    return readerCss(this.ctx.settings(), { kind: "paginated", geometry: this.geometry });
  }

  /** Load spine item `i` into the frame. */
  private async load(i: number): Promise<void> {
    // STEP 1: build and load the document.
    this.unwire?.();
    const html = await this.ctx.srcdoc(i, this.css());
    const doc = await loadFrame(this.frame, html);
    this.doc = doc;
    this.spine = i;

    // STEP 2: events, highlights, and our scroll lock.
    this.unwire = wireFrame(doc, i, this.frame, this.ctx.hooks);
    this.ctx.paint(doc, i);
    doc.addEventListener("scroll", () => this.snap(), { passive: true });
    await nextLayout();
    this.measure();
  }

  private measure() {
    const el = this.doc?.scrollingElement;
    this.screens = el ? screenCount(el.scrollWidth, this.geometry) : 1;
    this.screen = Math.min(this.screen, this.screens - 1);
  }

  private scrollToScreen(n: number) {
    this.screen = Math.max(0, Math.min(n, this.screens - 1));
    const el = this.doc?.scrollingElement;
    if (el) el.scrollLeft = this.screen * this.geometry.stride;
  }

  /** Selection drags can scroll the hidden overflow; snap back to the page. */
  private snap() {
    const el = this.doc?.scrollingElement;
    if (!el) return;
    const want = this.screen * this.geometry.stride;
    if (Math.abs(el.scrollLeft - want) > 1) requestAnimationFrame(() => (el.scrollLeft = want));
  }

  async display(loc: FlowLocator): Promise<void> {
    // STEP 1: make sure the right chapter is loaded.
    const spine = Math.max(0, Math.min(loc.spine, this.ctx.manifest.spine.length - 1));
    if (spine !== this.spine || !this.doc) await this.load(spine);

    // STEP 2: find the screen containing the locator's text.
    const rect = rectAt(this.doc!, { ...loc, spine });
    const x = (rect?.left ?? 0) + (this.doc!.scrollingElement?.scrollLeft ?? 0);
    this.scrollToScreen(screenOf(x, this.geometry));
    this.ctx.relocated();
  }

  async next(): Promise<void> {
    if (this.screen < this.screens - 1) {
      this.scrollToScreen(this.screen + 1);
    } else if (this.spine < this.ctx.manifest.spine.length - 1) {
      this.screen = 0;
      await this.load(this.spine + 1);
      this.scrollToScreen(0);
    } else return;
    this.ctx.relocated();
  }

  async prev(): Promise<void> {
    if (this.screen > 0) {
      this.scrollToScreen(this.screen - 1);
    } else if (this.spine > 0) {
      await this.load(this.spine - 1);
      this.scrollToScreen(this.screens - 1); // last page of the previous chapter
    } else return;
    this.ctx.relocated();
  }

  visibleRange(): VisibleRange | null {
    const doc = this.doc;
    if (!doc) return null;
    // Client rects are relative to the iframe viewport: the visible page(s)
    // span x ∈ [0, frameWidth). Text before has right ≤ 0, after has left ≥ width.
    const w = this.geometry.frameWidth;
    const first = firstPositionWhere(doc, (r) => r.right > 0.5 && r.left < w);
    const after = firstPositionWhere(doc, (r) => r.left >= w - 0.5);
    const start = first ? locatorAt(this.spine, first.node, first.offset) : null;
    const end = after ? locatorAt(this.spine, after.node, after.offset) : null;
    return { start: start ?? { kind: "flow", spine: this.spine, block: 0, offset: 0 }, end };
  }

  chapterFraction(): number {
    return this.screens <= 1 ? 0 : this.screen / this.screens;
  }

  pageInfo() {
    return { page: this.screen + 1, pages: this.screens };
  }

  async relayout(): Promise<void> {
    if (!this.doc) return;
    // STEP 1: remember where we are (content position, not page number).
    const loc = this.ctx.anchor() ?? this.visibleRange()?.start;
    // STEP 2: new geometry + styles, applied in place without reloading.
    this.geometry = this.computeGeometry();
    this.placeFrame();
    applyCss(this.doc, this.css());
    await nextLayout();
    this.measure();
    // STEP 3: back to the same text.
    if (loc) await this.display(loc);
  }

  documents() {
    return this.doc ? [{ spine: this.spine, doc: this.doc, frame: this.frame }] : [];
  }

  destroy(): void {
    clearTimeout(this.resizeTimer);
    this.resizeObs.disconnect();
    this.unwire?.();
    this.frame.remove();
  }
}
