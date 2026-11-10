// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// The reader engine: owns one layout view (paginated or scrolled), loads
// chapters, paints annotations, and reports position changes.
//
// Position model: the engine always knows the *locator* at the top of the
// visible area. Switching layout, spread, font or size re-renders and then
// displays that same locator, so the reader never loses their place.

import { followFinger, slideBack, slideTurn } from "./slide";
import type { Annotation, Manifest } from "../ipc/types";
import type { ReaderSettings } from "../settings/reader-settings";
import { buildSrcdoc } from "./chapter-doc";
import { locatorAt, locatorsOfRange, rangeBetween, snapToWords } from "./dom-locator";
import type { FrameHooks } from "./frame";
import { clearAll, paintHighlight } from "./highlights";
import { compareLocators, locateProgression, parseLocator, type FlowLocator, type Locator } from "./locator";
import { PaginatedView } from "./paginated";
import { ScrolledView } from "./scrolled";
import type { View, ViewContext, VisibleRange } from "./view";

export interface RelocateEvent {
  /** Visible text range (reflowable books only). */
  range: VisibleRange | null;
  /** Visible page indices (fixed-layout books only). */
  visiblePages: number[] | null;
  /**
   * The reading position to persist and restore. It stays fixed while it is
   * still on screen, so switching layouts back and forth never drifts.
   */
  anchor: Locator;
  /** 0..1 through the whole book. */
  progression: number;
  /** 0..1 through the current chapter (reflowable only; null for fixed layout). */
  chapterFraction: number | null;
  spine: number;
  chapterTitle: string | null;
  page: number;
  pages: number;
}

export interface SelectionEvent {
  start: FlowLocator;
  end: FlowLocator;
  text: string;
  /** Bounding box in host-page (window) coordinates. */
  rect: DOMRect;
}

export interface EngineEvents {
  relocate: RelocateEvent;
  selection: SelectionEvent | null;
  highlight: { id: string; rect: DOMRect };
  externalLink: string;
  key: KeyboardEvent;
  center: void;
  /** Ctrl/⌘ + wheel or pinch on a fixed-layout page: multiply zoom by this. */
  zoom: number;
  error: Error;
}

type Listener<T> = (v: T) => void;

/** What the reader screen needs from either engine (reflowable or fixed). */
export interface Engine {
  on<K extends keyof EngineEvents>(name: K, cb: (v: EngineEvents[K]) => void): () => void;
  open(start?: string | null): Promise<void>;
  destroy(): void;
  setSettings(next: ReaderSettings): Promise<void>;
  next(): Promise<unknown>;
  prev(): Promise<unknown>;
  goToLocator(text: string): Promise<void>;
  goToHref(href: string): Promise<void>;
  goToFraction(fraction: number): Promise<void>;
  /** Jump to 0..1 within the current chapter (fixed layout: within the book). */
  goToChapterFraction(fraction: number): Promise<void>;
  setAnnotations(list: Annotation[]): void;
  clearSelection(): void;
  /** Table of contents discovered at runtime (PDF outline); null = use the manifest's. */
  toc(): Promise<import("../ipc/types").TocEntry[] | null>;
  /** Pages run right-to-left (manga): arrow keys and taps are mirrored. */
  readonly rtl: boolean;
}

export interface EngineOptions {
  host: HTMLElement;
  manifest: Manifest;
  settings: ReaderSettings;
  loadChapter(href: string): Promise<string>;
  /** URL of the package's `content/` directory (for the `<base>` element). */
  contentBase: string;
}

export class ReaderEngine implements Engine {
  readonly rtl = false;

  private view: View | null = null;
  private settings: ReaderSettings;
  private cache = new Map<number, string>();
  private annotations: Annotation[] = [];
  private listeners: { [K in keyof EngineEvents]?: Listener<EngineEvents[K]>[] } = {};
  private queue: Promise<unknown> = Promise.resolve();
  private relocateRaf = 0;
  /** See RelocateEvent.anchor. */
  private anchor: FlowLocator | null = null;

  constructor(private opts: EngineOptions) {
    this.settings = opts.settings;
  }

  // ----- events ------------------------------------------------------------

  on<K extends keyof EngineEvents>(name: K, cb: Listener<EngineEvents[K]>): () => void {
    (this.listeners[name] ??= []).push(cb as never);
    return () => {
      this.listeners[name] = (this.listeners[name] as Listener<EngineEvents[K]>[]).filter((x) => x !== cb) as never;
    };
  }

  private emit<K extends keyof EngineEvents>(name: K, value: EngineEvents[K]) {
    for (const cb of (this.listeners[name] ?? []) as Listener<EngineEvents[K]>[]) cb(value);
  }

  /** Serialize navigation so rapid key presses can't interleave chapter loads. */
  private run<T>(task: () => Promise<T>): Promise<T> {
    const next = this.queue.then(task, task);
    this.queue = next.catch((e) => this.emit("error", e instanceof Error ? e : new Error(String(e))));
    return next;
  }

  // ----- lifecycle ---------------------------------------------------------

  /** Open the book at `start` (a locator string) or at the beginning. */
  open(start?: string | null): Promise<void> {
    const loc = start ? parseLocator(start) : null;
    return this.run(async () => {
      this.view = this.createView();
      const target: FlowLocator = loc?.kind === "flow" ? loc : { kind: "flow", spine: 0, block: 0, offset: 0 };
      this.anchor = target;
      await this.view.display(target);
    });
  }

  destroy() {
    cancelAnimationFrame(this.relocateRaf);
    this.view?.destroy();
    this.view = null;
    this.listeners = {};
  }

  private createView(): View {
    const ctx: ViewContext = {
      host: this.opts.host,
      manifest: this.opts.manifest,
      settings: () => this.settings,
      srcdoc: (spine, css) => this.srcdoc(spine, css),
      hooks: this.hooks(),
      relocated: () => this.scheduleRelocate(),
      anchor: () => this.anchor,
      paint: (doc, spine) => this.paint(doc, spine),
    };
    return this.settings.flow === "scrolled" ? new ScrolledView(ctx) : new PaginatedView(ctx);
  }

  private async srcdoc(spine: number, css: string): Promise<string> {
    // Keep a handful of recent chapters in memory; chapter files are small.
    let xhtml = this.cache.get(spine);
    if (xhtml === undefined) {
      xhtml = await this.opts.loadChapter(this.opts.manifest.spine[spine]!.href);
      this.cache.set(spine, xhtml);
      if (this.cache.size > 8) this.cache.delete(this.cache.keys().next().value!);
    }
    return buildSrcdoc(xhtml, this.opts.contentBase, css);
  }

  // ----- settings ----------------------------------------------------------

  /** Apply new settings without losing the reading position. */
  setSettings(next: ReaderSettings): Promise<void> {
    return this.run(async () => {
      const prev = this.settings;
      this.settings = next;
      if (!this.view) return;
      if (prev.flow !== next.flow) {
        // STEP 1: layout family changed — rebuild the view at the anchor.
        const loc = this.anchor ?? this.view.visibleRange()?.start ?? { kind: "flow", spine: 0, block: 0, offset: 0 };
        this.view.destroy();
        this.view = this.createView();
        await this.view.display(loc);
      } else {
        // STEP 2: same family — restyle in place (the view returns to the anchor).
        await this.view.relayout();
      }
    });
  }

  // ----- navigation --------------------------------------------------------

  next = () => this.run(async () => this.view?.next());
  prev = () => this.run(async () => this.view?.prev());

  goTo(loc: FlowLocator): Promise<void> {
    return this.run(async () => {
      this.anchor = loc;
      await this.view?.display(loc);
    });
  }

  goToLocator(text: string): Promise<void> {
    const loc = parseLocator(text);
    return loc?.kind === "flow" ? this.goTo(loc) : Promise.resolve();
  }

  /** Follow a package href such as `content/c0003.xhtml#note-4`. */
  goToHref(href: string): Promise<void> {
    return this.run(async () => {
      // STEP 1: which chapter?
      const [path, frag] = href.split("#");
      const spine = this.opts.manifest.spine.findIndex((s) => s.href === path);
      if (spine < 0 || !this.view) return;
      // STEP 2: show the chapter start, then jump to the fragment element if any.
      this.anchor = { kind: "flow", spine, block: 0, offset: 0 };
      await this.view.display(this.anchor);
      if (!frag) return;
      const doc = this.view.documents().find((d) => d.spine === spine)?.doc;
      const target = doc?.getElementById(decodeURIComponent(frag));
      const loc = target ? locatorAt(spine, target, 0) : null;
      if (loc) {
        this.anchor = loc;
        await this.view.display(loc);
      }
    });
  }

  goToFraction(fraction: number): Promise<void> {
    return this.goTo(locateProgression(this.opts.manifest.spine, fraction));
  }

  goToChapterFraction(fraction: number): Promise<void> {
    return this.run(async () => {
      // A fresh anchor is taken from what ends up on screen.
      this.anchor = null;
      await this.view?.goToChapterFraction(Math.min(1, Math.max(0, fraction)));
    });
  }

  async toc() {
    return null;
  }

  /** The current visible range (top-of-page locator and exclusive end). */
  visibleRange(): VisibleRange | null {
    return this.view?.visibleRange() ?? null;
  }

  // ----- annotations -------------------------------------------------------

  setAnnotations(list: Annotation[]) {
    this.annotations = list;
    for (const { doc, spine } of this.view?.documents() ?? []) {
      clearAll(doc);
      this.paint(doc, spine);
    }
  }

  private paint(doc: Document, spine: number) {
    for (const a of this.annotations) {
      if (a.kind === "bookmark" || !a.end) continue;
      const s = parseLocator(a.start);
      const e = parseLocator(a.end);
      if (s?.kind !== "flow" || e?.kind !== "flow" || s.spine !== spine) continue;
      const range = rangeBetween(doc, s, e);
      if (range && !range.collapsed) paintHighlight(doc, range, a.id, a.color, a.note.trim() !== "");
    }
  }

  /** Clear the selection in every loaded chapter. */
  clearSelection() {
    for (const { doc } of this.view?.documents() ?? []) doc.getSelection()?.removeAllRanges();
  }

  // ----- events from chapter documents --------------------------------------

  private hooks(): FrameHooks {
    return {
      key: (e) => this.emit("key", e),
      link: (href, external) => {
        if (external) this.emit("externalLink", href);
        else void this.goToHref(href);
      },
      selection: (doc, spine, frame) => {
        // STEP 1: only real, non-empty selections.
        const sel = doc.getSelection();
        if (!sel || sel.isCollapsed || sel.rangeCount === 0 || !sel.toString().trim()) {
          this.emit("selection", null);
          return;
        }
        // STEP 2: convert to locators (start must precede end) and snap to whole words.
        const range = sel.getRangeAt(0);
        const raw = locatorsOfRange(spine, range);
        if (!raw || (compareLocators(raw.start, raw.end) ?? 0) >= 0) return;
        const locs = snapToWords(doc, raw.start, raw.end);
        const text = rangeBetween(doc, locs.start, locs.end)?.toString() ?? sel.toString();
        this.emit("selection", { ...locs, text, rect: toHostRect(range.getBoundingClientRect(), frame) });
      },
      highlight: (id, mark, frame) => this.emit("highlight", { id, rect: toHostRect(mark.getBoundingClientRect(), frame) }),
      tap: (x, _y, frame) => {
        // Left/right thirds turn pages (paginated); the middle toggles the UI chrome.
        const hostRect = this.opts.host.getBoundingClientRect();
        const hx = frame.getBoundingClientRect().left - hostRect.left + x;
        const third = hostRect.width / 3;
        if (this.settings.flow === "paginated" && hx < third) void this.prev();
        else if (this.settings.flow === "paginated" && hx > third * 2) void this.next();
        else this.emit("center", undefined);
      },
      swipe: (direction, frame) => {
        // Paginated only (scrolling uses the native vertical pan). Swiping
        // left reveals the next page; the page slides out and the new one in.
        if (this.settings.flow !== "paginated") return slideBack(frame);
        const forward = direction === "left";
        void slideTurn(frame, forward, this.opts.host.clientWidth, async () => {
          const before = this.pagePosition();
          await (forward ? this.next() : this.prev());
          return this.pagePosition() !== before;
        });
      },
      drag: (dx, frame) => {
        if (this.settings.flow === "paginated") followFinger(frame, dx);
      },
      dragEnd: (frame) => slideBack(frame),
      wheel: (e) => {
        if (this.settings.flow !== "paginated") return;
        e.preventDefault();
        this.wheelTurn(e.deltaY || e.deltaX);
      },
    };
  }

  /** Where the paginated view is (chapter and page), to tell whether a turn moved. */
  private pagePosition(): string {
    const p = this.view?.pageInfo();
    return `${this.view?.visibleRange()?.start.spine}:${p?.page}`;
  }

  private wheelLock = 0;
  /** One page per wheel gesture (trackpads send many small events). */
  private wheelTurn(delta: number) {
    const now = performance.now();
    if (Math.abs(delta) < 4 || now - this.wheelLock < 350) return;
    this.wheelLock = now;
    void (delta > 0 ? this.next() : this.prev());
  }

  private scheduleRelocate() {
    cancelAnimationFrame(this.relocateRaf);
    this.relocateRaf = requestAnimationFrame(() => {
      const range = this.view?.visibleRange();
      if (!range || !this.view) return;
      // Keep the anchor while it is visible; otherwise the reader moved on.
      if (!this.anchor || !isVisible(this.anchor, range)) this.anchor = range.start;
      this.emit("relocate", this.describe(range, this.view, this.anchor));
    });
  }

  private describe(range: VisibleRange, view: View, anchor: FlowLocator): RelocateEvent {
    // Progress: chapters weighted by text length + exact position inside the chapter.
    const spine = this.opts.manifest.spine;
    const total = spine.reduce((s, x) => s + x.chars, 0) || 1;
    const idx = range.start.spine;
    const before = spine.slice(0, idx).reduce((s, x) => s + x.chars, 0);
    const progression = Math.min(1, (before + (spine[idx]?.chars ?? 0) * view.chapterFraction()) / total);
    const { page, pages } = view.pageInfo();
    const chapterFraction = view.chapterFraction();
    return { range, visiblePages: null, anchor, progression, chapterFraction, spine: idx, chapterTitle: spine[idx]?.title ?? null, page, pages };
  }
}

/** Is `loc` inside the visible range [start, end)? */
export function isVisible(loc: FlowLocator, range: VisibleRange): boolean {
  if ((compareLocators(loc, range.start) ?? -1) < 0) return false;
  if (range.end) return (compareLocators(loc, range.end) ?? 1) < 0;
  return loc.spine === range.start.spine; // chapter end is visible
}

/** Convert a rect from iframe-viewport coordinates to window coordinates. */
function toHostRect(r: DOMRect, frame: HTMLIFrameElement): DOMRect {
  const f = frame.getBoundingClientRect();
  return new DOMRect(f.left + r.left, f.top + r.top, r.width, r.height);
}
