// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Page sources: where fixed-layout pages come from and how one is drawn.
//   ImageSource — comic pages (plain <img>, the webview decodes them)
//   PdfSource   — PDF pages rendered to <canvas> by PDF.js (Apache-2.0,
//                 https://mozilla.github.io/pdf.js/)

import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import type { Manifest, TocEntry } from "../ipc/types";
import { backingScale, safeSize, type Size } from "./layout";

export interface PageSource {
  readonly count: number;
  readonly rtl: boolean;
  /** Intrinsic size (any unit; only the aspect ratio matters). */
  size(index: number): Size;
  /** Draw page `index` into `slot` at the given CSS size. */
  render(index: number, slot: HTMLElement, css: Size): Promise<void>;
  /** Free whatever `slot` holds (canvas memory). */
  release(slot: HTMLElement): void;
  /** Table of contents, if the format has one. */
  outline(): Promise<TocEntry[]>;
  destroy(): void;
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

export class ImageSource implements PageSource {
  readonly count: number;
  readonly rtl: boolean;

  constructor(
    private manifest: Manifest,
    private url: (href: string) => string,
  ) {
    this.count = manifest.fixed!.pages.length;
    this.rtl = manifest.fixed!.rightToLeft;
  }

  size(i: number): Size {
    return safeSize(this.manifest.fixed!.pages[i]);
  }

  async render(i: number, slot: HTMLElement, css: Size): Promise<void> {
    // Reuse the element when re-rendering at a new size.
    let img = slot.querySelector("img");
    if (!img) {
      img = document.createElement("img");
      img.alt = `Page ${i + 1}`;
      img.decoding = "async";
      img.draggable = false;
      img.src = this.url(this.manifest.fixed!.pages[i]!.href);
      slot.appendChild(img);
    }
    img.style.width = `${css.width}px`;
    img.style.height = `${css.height}px`;
    if (!img.complete) {
      // Resolve on load *or* error: a broken page must not block the reader.
      await new Promise<void>((resolve) => {
        img!.addEventListener("load", () => resolve(), { once: true });
        img!.addEventListener("error", () => resolve(), { once: true });
      });
    }
  }

  release(slot: HTMLElement): void {
    slot.replaceChildren();
  }

  async outline(): Promise<TocEntry[]> {
    return [];
  }

  destroy(): void {}
}

// ---------------------------------------------------------------------------
// PDF
// ---------------------------------------------------------------------------

/** Base URL of the bundled PDF.js data (see scripts/copy-pdfjs.mjs). */
const ASSETS = new URL("pdfjs/", document.baseURI).href;
pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

export class PdfSource implements PageSource {
  readonly rtl = false;
  private sizes = new Map<number, Size>();
  private tasks = new WeakMap<HTMLElement, pdfjs.RenderTask>();

  private constructor(
    private task: pdfjs.PDFDocumentLoadingTask,
    private doc: pdfjs.PDFDocumentProxy,
    readonly count: number,
    private first: Size,
  ) {}

  /** Load the document. Scripting and eval are disabled; data is local. */
  static async open(url: string): Promise<PdfSource> {
    // STEP 1: fetch the bytes ourselves (one request, works with custom schemes).
    const res = await fetch(url);
    if (!res.ok) throw new Error(`could not load PDF (${res.status})`);
    const data = new Uint8Array(await res.arrayBuffer());

    // STEP 2: open with local assets only. (PDF.js 6 never uses eval; form
    // scripting needs the QuickJS engine, which we don't ship.)
    const task = pdfjs.getDocument({
      data,
      cMapUrl: `${ASSETS}cmaps/`,
      cMapPacked: true,
      standardFontDataUrl: `${ASSETS}standard_fonts/`,
      wasmUrl: `${ASSETS}wasm/`,
      enableXfa: false,
    });
    const doc = await task.promise;

    // STEP 3: page 1's size is the default until other pages are measured.
    const p1 = await doc.getPage(1);
    const v = p1.getViewport({ scale: 1 });
    return new PdfSource(task, doc, doc.numPages, { width: v.width, height: v.height });
  }

  size(i: number): Size {
    return this.sizes.get(i) ?? this.first;
  }

  async render(i: number, slot: HTMLElement, css: Size): Promise<void> {
    // STEP 1: cancel a render still running in this slot (fast page turns).
    this.tasks.get(slot)?.cancel();

    // STEP 2: learn the real size of this page.
    const page = await this.doc.getPage(i + 1);
    const base = page.getViewport({ scale: 1 });
    this.sizes.set(i, { width: base.width, height: base.height });

    // STEP 3: draw at device resolution (capped) into a fresh canvas.
    const ratio = backingScale(css, window.devicePixelRatio || 1);
    const viewport = page.getViewport({ scale: (css.width / base.width) * ratio });
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    canvas.style.width = `${css.width}px`;
    canvas.style.height = `${css.height}px`;
    const task = page.render({ canvas, viewport });
    this.tasks.set(slot, task);
    try {
      await task.promise;
      slot.replaceChildren(canvas); // swap only when finished: no blank flashes
    } catch (e) {
      if ((e as Error)?.name !== "RenderingCancelledException") throw e;
    }
  }

  release(slot: HTMLElement): void {
    this.tasks.get(slot)?.cancel();
    for (const c of slot.querySelectorAll("canvas")) {
      c.width = 0; // frees the backing store immediately in WebKit
      c.height = 0;
    }
    slot.replaceChildren();
  }

  /** PDF outline → TOC entries with "page:N" targets. */
  async outline(): Promise<TocEntry[]> {
    const items = (await this.doc.getOutline()) ?? [];
    const toEntry = async (it: (typeof items)[number]): Promise<TocEntry> => {
      let href: string | null = null;
      try {
        const dest = typeof it.dest === "string" ? await this.doc.getDestination(it.dest) : it.dest;
        if (dest?.[0]) {
          const ref = dest[0];
          const index = typeof ref === "number" ? ref : await this.doc.getPageIndex(ref);
          href = `page:${index}`;
        }
      } catch {
        href = null; // broken destinations are common; keep the title
      }
      return { title: it.title || "Untitled", href, children: await Promise.all((it.items ?? []).map(toEntry)) };
    };
    return Promise.all(items.map(toEntry));
  }

  destroy(): void {
    void this.task.destroy();
  }
}

/** Render page 1 of a PDF to a JPEG for the library cover. */
export async function renderPdfCover(url: string, width = 360): Promise<Uint8Array> {
  const src = await PdfSource.open(url);
  try {
    const slot = document.createElement("div");
    const s = src.size(0);
    await src.render(0, slot, { width, height: Math.round((width * s.height) / s.width) });
    const canvas = slot.querySelector("canvas")!;
    const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, "image/jpeg", 0.85));
    if (!blob) throw new Error("could not encode cover");
    return new Uint8Array(await blob.arrayBuffer());
  } finally {
    src.destroy();
  }
}
