// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Contract between the engine and its layout views.

import type { Manifest } from "../ipc/types";
import type { ReaderSettings } from "../settings/reader-settings";
import type { FrameHooks } from "./frame";
import type { FlowLocator } from "./locator";

export interface ViewContext {
  host: HTMLElement;
  manifest: Manifest;
  settings(): ReaderSettings;
  /** Complete srcdoc for a spine item, given the reader CSS to embed. */
  srcdoc(spine: number, css: string): Promise<string>;
  hooks: FrameHooks;
  /** Called by the view whenever the visible position may have changed. */
  relocated(): void;
  /** The position to keep when re-laying out (see RelocateEvent.anchor). */
  anchor(): FlowLocator | null;
  /** Paint stored highlights into a freshly loaded chapter. */
  paint(doc: Document, spine: number): void;
}

export interface VisibleRange {
  start: FlowLocator;
  /** Exclusive end; `null` when the end of the chapter is visible. */
  end: FlowLocator | null;
}

export interface View {
  display(loc: FlowLocator): Promise<void>;
  next(): Promise<void>;
  prev(): Promise<void>;
  visibleRange(): VisibleRange | null;
  /** 0..1 position within the current chapter (for accurate progress). */
  chapterFraction(): number;
  /** Move to 0..1 within the current chapter. */
  goToChapterFraction(fraction: number): Promise<void>;
  /** Pages of the current chapter (paginated only; `pages` is 0 when scrolling). */
  pageInfo(): { page: number; pages: number };
  /** Re-apply settings or react to a resize, keeping the position. */
  relayout(): Promise<void>;
  documents(): { spine: number; doc: Document; frame: HTMLIFrameElement }[];
  destroy(): void;
}
