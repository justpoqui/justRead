// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Continuous-scroll view: chapters are stacked vertically, each in its own
// auto-height iframe, inside one scrolling container. Neighbouring chapters
// are loaded on demand as the reader approaches them, so opening a book is
// as fast as opening one chapter.

import { applyCss } from "./chapter-doc";
import { firstPositionWhere, locatorAt, rectAt } from "./dom-locator";
import { createFrame, loadFrame, nextLayout, wireFrame } from "./frame";
import type { FlowLocator } from "./locator";
import { readerCss } from "./styles";
import type { View, ViewContext, VisibleRange } from "./view";

interface Section {
  spine: number;
  el: HTMLElement;
  frame: HTMLIFrameElement;
  doc: Document;
  unwire: () => void;
  resize: ResizeObserver;
}

export class ScrolledView implements View {
  private scroller = document.createElement("div");
  private column = document.createElement("div");
  private sections: Section[] = [];
  private loading = false;
  private scrollRaf = 0;
  private resizeObs: ResizeObserver;
  private resizeTimer = 0;

  constructor(private ctx: ViewContext) {
    // STEP 1: a full-size scroll container holding a column of chapters.
    Object.assign(this.scroller.style, { position: "absolute", inset: "0", overflowY: "auto", overflowX: "hidden" });
    this.scroller.className = "jr-scroller";
    this.scroller.tabIndex = -1;
    this.column.style.padding = `0 ${ctx.settings().margin}px`;
    this.scroller.appendChild(this.column);
    ctx.host.appendChild(this.scroller);

    // STEP 2: load neighbours and report position while scrolling.
    this.scroller.addEventListener("scroll", () => {
      cancelAnimationFrame(this.scrollRaf);
      this.scrollRaf = requestAnimationFrame(() => void this.onScroll());
    }, { passive: true });

    this.resizeObs = new ResizeObserver(() => {
      clearTimeout(this.resizeTimer);
      this.resizeTimer = window.setTimeout(() => void this.relayout(), 120);
    });
    this.resizeObs.observe(ctx.host);
  }

  /** Text column width: comfortable line length, never wider than the screen. */
  private textWidth(): number {
    const s = this.ctx.settings();
    const byChars = Math.round(s.maxLineChars * s.fontSize * 0.55);
    return Math.max(200, Math.min(byChars, this.ctx.host.clientWidth - s.margin * 2));
  }

  private css(): string {
    return readerCss(this.ctx.settings(), { kind: "scrolled", width: this.textWidth() });
  }

  /** Create and load the section for `spine`, inserting it in order. */
  private async loadSection(spine: number): Promise<Section> {
    // STEP 1: the element is inserted first (hidden) so loading happens in the real width.
    const el = document.createElement("section");
    el.dataset.spine = String(spine);
    const frame = createFrame();
    frame.style.width = "100%";
    frame.style.height = "100px";
    el.appendChild(frame);
    const after = this.sections.find((s) => s.spine > spine);
    this.column.insertBefore(el, after?.el ?? null);

    // STEP 2: load, wire and paint.
    const doc = await loadFrame(frame, await this.ctx.srcdoc(spine, this.css()));
    const unwire = wireFrame(doc, spine, frame, this.ctx.hooks);
    this.ctx.paint(doc, spine);

    // STEP 3: keep the iframe exactly as tall as its content.
    const fit = () => (frame.style.height = `${doc.documentElement.scrollHeight}px`);
    const resize = new ResizeObserver(fit);
    resize.observe(doc.documentElement);
    fit();

    const section: Section = { spine, el, frame, doc, unwire, resize };
    this.sections.push(section);
    this.sections.sort((a, b) => a.spine - b.spine);
    return section;
  }

  private clear() {
    for (const s of this.sections) {
      s.unwire();
      s.resize.disconnect();
      s.el.remove();
    }
    this.sections = [];
  }

  /** Scroll so that `loc` is at the top of the viewport. */
  private scrollToLocator(loc: FlowLocator) {
    const sec = this.sections.find((s) => s.spine === loc.spine);
    if (!sec) return;
    const rect = rectAt(sec.doc, loc);
    this.scroller.scrollTop = sec.el.offsetTop + Math.max(0, (rect?.top ?? 0) - 8);
  }

  async display(loc: FlowLocator): Promise<void> {
    // STEP 1: start fresh with only the target chapter.
    this.loading = true;
    this.clear();
    const spine = Math.max(0, Math.min(loc.spine, this.ctx.manifest.spine.length - 1));
    await this.loadSection(spine);
    await nextLayout();
    this.scrollToLocator({ ...loc, spine });
    this.loading = false;

    // STEP 2: neighbours make scrolling seamless in both directions.
    await this.onScroll();
    this.ctx.relocated();
  }

  /** Load the previous/next chapter when the viewport approaches an edge. */
  private async onScroll(): Promise<void> {
    if (this.loading || this.sections.length === 0) {
      this.ctx.relocated();
      return;
    }
    this.loading = true;
    try {
      const view = this.scroller.clientHeight;
      const first = this.sections[0]!;
      const last = this.sections[this.sections.length - 1]!;

      // STEP 1: approaching the end → append the next chapter.
      if (this.scroller.scrollTop + view * 2 > this.scroller.scrollHeight && last.spine < this.ctx.manifest.spine.length - 1) {
        await this.loadSection(last.spine + 1);
      }
      // STEP 2: approaching the start → prepend, compensating the scroll offset
      // so the text under the reader's eyes doesn't jump.
      if (this.scroller.scrollTop < view && first.spine > 0) {
        const before = this.scroller.scrollHeight;
        await this.loadSection(first.spine - 1);
        await nextLayout();
        this.scroller.scrollTop += this.scroller.scrollHeight - before;
      }
    } finally {
      this.loading = false;
    }
    this.ctx.relocated();
  }

  /** Section under the given viewport y (relative to the scroller top). */
  private sectionAt(y: number): Section | null {
    const abs = this.scroller.scrollTop + y;
    return this.sections.find((s) => abs >= s.el.offsetTop && abs < s.el.offsetTop + s.el.offsetHeight) ?? this.sections[0] ?? null;
  }

  visibleRange(): VisibleRange | null {
    // STEP 1: chapter at the top of the viewport and the y offset into it.
    const top = this.sectionAt(1);
    if (!top) return null;
    const y0 = this.scroller.scrollTop - top.el.offsetTop;
    const first = firstPositionWhere(top.doc, (r) => r.bottom > y0 + 1);
    const start = first ? locatorAt(top.spine, first.node, first.offset) : null;

    // STEP 2: chapter at the bottom of the viewport.
    const h = this.scroller.clientHeight;
    const bottom = this.sectionAt(h - 1) ?? top;
    const y1 = this.scroller.scrollTop + h - bottom.el.offsetTop;
    const after = firstPositionWhere(bottom.doc, (r) => r.top >= y1);
    const end = after ? locatorAt(bottom.spine, after.node, after.offset) : null;
    return { start: start ?? { kind: "flow", spine: top.spine, block: 0, offset: 0 }, end };
  }

  chapterFraction(): number {
    const top = this.sectionAt(1);
    if (!top || top.el.offsetHeight === 0) return 0;
    return Math.min(1, Math.max(0, (this.scroller.scrollTop - top.el.offsetTop) / top.el.offsetHeight));
  }

  pageInfo() {
    return { page: 0, pages: 0 };
  }

  async next(): Promise<void> {
    this.scroller.scrollBy({ top: this.scroller.clientHeight * 0.9, behavior: "smooth" });
  }

  async prev(): Promise<void> {
    this.scroller.scrollBy({ top: -this.scroller.clientHeight * 0.9, behavior: "smooth" });
  }

  async relayout(): Promise<void> {
    // STEP 1: remember the position, STEP 2: restyle every loaded chapter in place.
    const loc = this.ctx.anchor() ?? this.visibleRange()?.start;
    this.column.style.padding = `0 ${this.ctx.settings().margin}px`;
    const css = this.css();
    for (const s of this.sections) applyCss(s.doc, css);
    await nextLayout();
    for (const s of this.sections) s.frame.style.height = `${s.doc.documentElement.scrollHeight}px`;
    // STEP 3: restore.
    if (loc) this.scrollToLocator(loc);
    this.ctx.relocated();
  }

  documents() {
    return this.sections.map((s) => ({ spine: s.spine, doc: s.doc, frame: s.frame }));
  }

  destroy(): void {
    clearTimeout(this.resizeTimer);
    this.resizeObs.disconnect();
    this.clear();
    this.scroller.remove();
  }
}
