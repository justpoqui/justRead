// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Reader engine for fixed-layout books (PDF, comics). It implements the same
// `Engine` contract as the reflowable engine, so the reader screen, progress
// saving and bookmarks work the same way.
//
// Modes:
//   paginated — one page or a two-page spread per screen, fit-page/fit-width
//               plus zoom; the screen scrolls (pans) when zoomed in.
//   scrolled  — all pages stacked vertically; pages are rendered only near
//               the viewport and released when far away (canvas memory).
//
// Position: a fixed locator `p:<page>@<offset>` (offset = fraction of the
// page above the viewport top). Like the text engine, an *anchor* is kept
// while it is visible so mode/zoom/fit changes return to the same spot.

import type { Annotation, Manifest, TocEntry } from "../ipc/types";
import type { ReaderSettings } from "../settings/reader-settings";
import type { Engine, EngineEvents } from "../reader/engine";
import { formatLocator, parseLocator, type FixedLocator } from "../reader/locator";
import { buildSpreads, fitScreen, screenOfPage, scrollPageSize, type Size } from "./layout";
import type { PageSource } from "./sources";
import { GestureTracker, points } from "../reader/gestures";

type Listener<T> = (v: T) => void;

/** Gap between pages, CSS px. */
const GAP = 12;
/** In scroll mode, keep rendered pages within this distance of the current page. */
const KEEP_RENDERED = 6;

export class FixedEngine implements Engine {
  readonly rtl: boolean;
  private root = document.createElement("div");
  private inner = document.createElement("div");
  private settings: ReaderSettings;
  private anchor: FixedLocator = { kind: "fixed", page: 0, offset: 0 };
  private listeners: { [K in keyof EngineEvents]?: Listener<EngineEvents[K]>[] } = {};
  private screens: number[][] = [];
  private screen = 0;
  private slots = new Map<number, HTMLElement>();
  private rendered = new Set<number>();
  private observer: IntersectionObserver | null = null;
  private resizeObs: ResizeObserver;
  private resizeTimer = 0;
  private scrollRaf = 0;
  private suppressScroll = false;

  constructor(
    private host: HTMLElement,
    private manifest: Manifest,
    private source: PageSource,
    settings: ReaderSettings,
  ) {
    this.settings = settings;
    this.rtl = source.rtl;

    // STEP 1: a scrolling root (pans zoomed pages / scrolls the page column).
    Object.assign(this.root.style, { position: "absolute", inset: "0", overflow: "auto" });
    this.root.className = "jr-fixed";
    this.root.tabIndex = -1;
    this.root.appendChild(this.inner);
    host.appendChild(this.root);

    // STEP 2: interaction.
    this.root.addEventListener("click", (e) => this.onClick(e));
    this.root.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    this.wireTouch();
    this.root.addEventListener("scroll", () => {
      if (this.suppressScroll) return;
      cancelAnimationFrame(this.scrollRaf);
      this.scrollRaf = requestAnimationFrame(() => this.onScroll());
    }, { passive: true });

    // STEP 3: re-layout on resize, keeping the anchor.
    this.resizeObs = new ResizeObserver(() => {
      clearTimeout(this.resizeTimer);
      this.resizeTimer = window.setTimeout(() => void this.layout(), 120);
    });
    this.resizeObs.observe(host);
  }

  // ----- events --------------------------------------------------------------

  on<K extends keyof EngineEvents>(name: K, cb: Listener<EngineEvents[K]>): () => void {
    (this.listeners[name] ??= []).push(cb as never);
    return () => {
      this.listeners[name] = (this.listeners[name] as Listener<EngineEvents[K]>[]).filter((x) => x !== cb) as never;
    };
  }

  private emit<K extends keyof EngineEvents>(name: K, value: EngineEvents[K]) {
    for (const cb of (this.listeners[name] ?? []) as Listener<EngineEvents[K]>[]) cb(value);
  }

  // ----- lifecycle -------------------------------------------------------------

  async open(start?: string | null): Promise<void> {
    const loc = start ? parseLocator(start) : null;
    if (loc?.kind === "fixed") this.anchor = { ...loc, page: Math.min(loc.page, this.source.count - 1) };
    await this.layout();
  }

  destroy(): void {
    clearTimeout(this.resizeTimer);
    cancelAnimationFrame(this.scrollRaf);
    this.resizeObs.disconnect();
    this.observer?.disconnect();
    for (const slot of this.slots.values()) this.source.release(slot);
    this.source.destroy();
    this.root.remove();
    this.listeners = {};
  }

  async setSettings(next: ReaderSettings): Promise<void> {
    this.settings = next;
    await this.layout();
  }

  async toc(): Promise<TocEntry[] | null> {
    const t = await this.source.outline();
    return t.length ? t : null;
  }

  // ----- geometry --------------------------------------------------------------

  private viewport(): Size {
    // Leave a small margin so page edges and shadows stay visible.
    return { width: Math.max(100, this.root.clientWidth - 32), height: Math.max(100, this.root.clientHeight - 32) };
  }

  /** Two pages per screen exactly when the reader chose it (any window shape). */
  private spread(): boolean {
    return this.settings.spread === "double";
  }

  private sizes(): Size[] {
    return Array.from({ length: this.source.count }, (_, i) => this.source.size(i));
  }

  /** Rebuild the DOM for the current mode/settings and show the anchor. */
  private async layout(): Promise<void> {
    // STEP 1: clear the previous view.
    this.observer?.disconnect();
    for (const slot of this.slots.values()) this.source.release(slot);
    this.slots.clear();
    this.rendered.clear();
    this.inner.replaceChildren();

    // STEP 2: build the chosen mode.
    if (this.settings.flow === "scrolled") this.buildScroll();
    else await this.showScreen(screenOfPage((this.screens = buildSpreads(this.sizes(), this.spread())), this.anchor.page));
  }

  // ----- paginated -------------------------------------------------------------

  private async showScreen(index: number): Promise<void> {
    // STEP 1: which pages, in which visual order.
    this.screen = Math.max(0, Math.min(index, this.screens.length - 1));
    const pages = this.screens[this.screen] ?? [0];
    const visual = this.rtl ? [...pages].reverse() : pages;
    const sizes = fitScreen(visual.map((p) => this.source.size(p)), this.viewport(), this.settings.fit, this.settings.zoom, 0);

    // STEP 2: a centered row of page slots.
    for (const slot of this.slots.values()) this.source.release(slot);
    this.slots.clear();
    Object.assign(this.inner.style, {
      display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "center",
      minWidth: "100%", minHeight: "100%", width: "max-content", padding: "16px", boxSizing: "border-box", gap: "0",
    });
    const row = visual.map((p, i) => {
      const slot = this.makeSlot(p, sizes[i]!);
      this.slots.set(p, slot);
      return slot;
    });
    this.inner.replaceChildren(...row);

    // STEP 3: start at the top (and horizontally centered when zoomed).
    this.suppressScroll = true;
    this.root.scrollTop = 0;
    this.root.scrollLeft = Math.max(0, (this.root.scrollWidth - this.root.clientWidth) / 2);
    this.suppressScroll = false;

    // STEP 4: keep the anchor if it is on this screen, otherwise move it here.
    if (!pages.includes(this.anchor.page)) this.anchor = { kind: "fixed", page: pages[0]!, offset: 0 };
    this.relocate(pages);

    // STEP 5: draw (after reporting the position, so the UI updates instantly).
    await Promise.all(visual.map((p, i) => this.source.render(p, this.slots.get(p)!, sizes[i]!).catch((e) => this.emit("error", e))));
  }

  // ----- continuous scroll ----------------------------------------------------

  private buildScroll(): void {
    // STEP 1: a column of correctly sized placeholders (no rendering yet).
    Object.assign(this.inner.style, {
      display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "flex-start",
      minWidth: "100%", minHeight: "0", width: "max-content", padding: "16px", boxSizing: "border-box", gap: `${GAP}px`,
    });
    const vp = this.viewport();
    const nodes: HTMLElement[] = [];
    for (let p = 0; p < this.source.count; p++) {
      const slot = this.makeSlot(p, scrollPageSize(this.source.size(p), vp, this.settings.fit, this.settings.zoom));
      this.slots.set(p, slot);
      nodes.push(slot);
    }
    this.inner.replaceChildren(...nodes);

    // STEP 2: render pages as they come near the viewport.
    this.observer = new IntersectionObserver((entries) => {
      for (const e of entries) if (e.isIntersecting) this.renderSlot(Number((e.target as HTMLElement).dataset.page));
    }, { root: this.root, rootMargin: "150% 0px" });
    nodes.forEach((n) => this.observer!.observe(n));

    // STEP 3: jump to the anchor.
    this.scrollToAnchor();
    this.onScroll();
  }

  private renderSlot(p: number) {
    if (this.rendered.has(p)) return;
    const slot = this.slots.get(p);
    if (!slot) return;
    this.rendered.add(p);
    const css = { width: parseFloat(slot.style.width), height: parseFloat(slot.style.height) };
    void this.source.render(p, slot, css).catch((e) => this.emit("error", e));
  }

  private scrollToAnchor() {
    const slot = this.slots.get(this.anchor.page);
    if (!slot) return;
    this.suppressScroll = true;
    this.root.scrollTop = slot.offsetTop + this.anchor.offset * slot.offsetHeight - 8;
    this.suppressScroll = false;
  }

  /** Page at the viewport top and how far into it (0..1). */
  private topPosition(): FixedLocator {
    const y = this.root.scrollTop + 8;
    let lo = 0;
    let hi = this.source.count - 1;
    // Binary search: slots are in increasing offsetTop order.
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (this.slots.get(mid)!.offsetTop <= y) lo = mid;
      else hi = mid - 1;
    }
    const slot = this.slots.get(lo)!;
    const offset = Math.min(0.9999, Math.max(0, (y - slot.offsetTop) / Math.max(1, slot.offsetHeight)));
    return { kind: "fixed", page: lo, offset: Math.round(offset * 10000) / 10000 };
  }

  private visibleInScroll(): number[] {
    const top = this.root.scrollTop;
    const bottom = top + this.root.clientHeight;
    const out: number[] = [];
    for (const [p, slot] of this.slots) if (slot.offsetTop < bottom && slot.offsetTop + slot.offsetHeight > top) out.push(p);
    return out.sort((a, b) => a - b);
  }

  private onScroll() {
    if (this.settings.flow !== "scrolled") return;
    const visible = this.visibleInScroll();
    // STEP 1: move the anchor only once it has left the screen.
    const a = this.slots.get(this.anchor.page);
    const anchorY = a ? a.offsetTop + this.anchor.offset * a.offsetHeight : -1;
    if (!a || anchorY < this.root.scrollTop || anchorY > this.root.scrollTop + this.root.clientHeight) {
      this.anchor = this.topPosition();
    }
    // STEP 2: free canvases far from the reader.
    for (const p of [...this.rendered]) {
      if (Math.abs(p - this.anchor.page) > KEEP_RENDERED) {
        this.source.release(this.slots.get(p)!);
        this.rendered.delete(p);
      }
    }
    this.relocate(visible);
  }

  // ----- shared -----------------------------------------------------------------

  private makeSlot(page: number, size: Size): HTMLElement {
    const slot = document.createElement("div");
    slot.className = "jr-page";
    slot.dataset.page = String(page);
    Object.assign(slot.style, {
      width: `${size.width}px`, height: `${size.height}px`, flex: "0 0 auto",
      background: "#fff", boxShadow: "0 1px 6px rgba(0,0,0,.18)", overflow: "hidden",
    });
    return slot;
  }

  private relocate(visible: number[]) {
    const count = this.source.count;
    const last = Math.max(...visible, this.anchor.page);
    const progression = this.settings.flow === "scrolled" ? (this.anchor.page + this.anchor.offset) / count : (last + 1) / count;
    this.emit("relocate", {
      range: null,
      visiblePages: visible,
      anchor: this.anchor,
      progression: Math.min(1, progression),
      chapterFraction: null,
      spine: -1,
      chapterTitle: null,
      page: this.anchor.page + 1,
      pages: count,
    });
  }

  // ----- navigation -------------------------------------------------------------

  async next(): Promise<void> {
    if (this.settings.flow === "scrolled") this.root.scrollBy({ top: this.root.clientHeight * 0.9, behavior: "smooth" });
    else if (this.screen < this.screens.length - 1) await this.showScreen(this.screen + 1);
  }

  async prev(): Promise<void> {
    if (this.settings.flow === "scrolled") this.root.scrollBy({ top: -this.root.clientHeight * 0.9, behavior: "smooth" });
    else if (this.screen > 0) await this.showScreen(this.screen - 1);
  }

  async goToPage(page: number, offset = 0): Promise<void> {
    this.anchor = { kind: "fixed", page: Math.max(0, Math.min(page, this.source.count - 1)), offset };
    if (this.settings.flow === "scrolled") {
      this.scrollToAnchor();
      this.onScroll();
    } else await this.showScreen(screenOfPage(this.screens, this.anchor.page));
  }

  async goToLocator(text: string): Promise<void> {
    const loc = parseLocator(text);
    if (loc?.kind === "fixed") await this.goToPage(loc.page, loc.offset);
  }

  /** TOC targets are "page:N" (0-based). */
  async goToHref(href: string): Promise<void> {
    const m = /^page:(\d+)$/.exec(href);
    if (m) await this.goToPage(Number(m[1]));
  }

  async goToFraction(fraction: number): Promise<void> {
    await this.goToPage(Math.floor(Math.min(0.9999, Math.max(0, fraction)) * this.source.count));
  }

  /** No chapters in fixed layout: "within the chapter" means within the book. */
  goToChapterFraction(fraction: number): Promise<void> {
    return this.goToFraction(fraction);
  }

  /** Bookmarks are handled by the reader screen using `visiblePages`. */
  setAnnotations(_list: Annotation[]): void {}
  clearSelection(): void {}

  // ----- input --------------------------------------------------------------------

  private onClick(e: MouseEvent) {
    // Taps: outer thirds turn pages in paginated mode (mirrored for RTL), middle toggles chrome.
    const r = this.host.getBoundingClientRect();
    const x = e.clientX - r.left;
    const third = r.width / 3;
    if (this.settings.flow === "paginated" && x < third) void (this.rtl ? this.next() : this.prev());
    else if (this.settings.flow === "paginated" && x > third * 2) void (this.rtl ? this.prev() : this.next());
    else this.emit("center", undefined);
  }

  /** Swipe to turn pages (unless zoomed in, when one finger pans), pinch to zoom. */
  private wireTouch() {
    const tracker = new GestureTracker();
    this.root.style.touchAction = "pan-x pan-y"; // we handle pinch ourselves
    this.root.addEventListener("touchstart", (e) => tracker.touch(points(e.touches), e.timeStamp, true), { passive: true });
    this.root.addEventListener("touchmove", (e) => {
      tracker.touch(points(e.touches), e.timeStamp, false);
      if (e.touches.length > 1) e.preventDefault(); // no browser page-zoom
    }, { passive: false });
    this.root.addEventListener("touchend", (e) => {
      if (e.touches.length > 0) return;
      const g = tracker.end(e.timeStamp);
      if (g?.kind === "pinch") this.emit("zoom", g.scale);
      else if (g?.kind === "swipe" && this.settings.flow === "paginated" && this.root.scrollWidth <= this.root.clientWidth + 1) {
        // Swipe left = forward, mirrored for right-to-left comics.
        const forward = (g.direction === "left") !== this.rtl;
        void (forward ? this.next() : this.prev());
      }
    }, { passive: true });
  }

  private wheelLock = 0;
  private onWheel(e: WheelEvent) {
    // STEP 1: Ctrl/⌘ + wheel (and trackpad pinch, which arrives as ctrl+wheel) zooms.
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      this.emit("zoom", e.deltaY < 0 ? 1.1 : 1 / 1.1);
      return;
    }
    // STEP 2: in paginated mode, the wheel turns pages unless the page is zoomed and can scroll.
    if (this.settings.flow !== "paginated") return;
    const scrollable = this.root.scrollHeight > this.root.clientHeight + 1;
    const atTop = this.root.scrollTop <= 0;
    const atBottom = this.root.scrollTop + this.root.clientHeight >= this.root.scrollHeight - 1;
    if (scrollable && !((e.deltaY > 0 && atBottom) || (e.deltaY < 0 && atTop))) return;
    e.preventDefault();
    const now = performance.now();
    if (Math.abs(e.deltaY) < 4 || now - this.wheelLock < 350) return;
    this.wheelLock = now;
    void (e.deltaY > 0 ? this.next() : this.prev());
  }

  /** For tests and the bookmark list. */
  locatorText(): string {
    return formatLocator(this.anchor);
  }

  get pageCount(): number {
    return this.manifest.fixed?.pageCount ?? this.source.count;
  }
}
