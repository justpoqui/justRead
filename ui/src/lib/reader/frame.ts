// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Creating chapter iframes and wiring their events to the engine.

/** Callbacks a view receives from a chapter document. */
export interface FrameHooks {
  key(e: KeyboardEvent): void;
  link(href: string, external: boolean): void;
  selection(doc: Document, spine: number, frame: HTMLIFrameElement): void;
  highlight(id: string, mark: HTMLElement, frame: HTMLIFrameElement): void;
  /** Click on plain text/background; x,y are relative to the iframe viewport. */
  tap(x: number, y: number, frame: HTMLIFrameElement): void;
  wheel(e: WheelEvent): void;
}

/**
 * A chapter iframe. Deliberately *not* `sandbox`ed — scripts are blocked by
 * the CSP inside each chapter instead (see chapter-doc.ts for why).
 */
export function createFrame(): HTMLIFrameElement {
  const f = document.createElement("iframe");
  f.setAttribute("title", "Book content");
  f.setAttribute("tabindex", "-1");
  f.style.border = "0";
  f.style.display = "block";
  f.style.background = "transparent";
  return f;
}

/** Load `srcdoc` into `frame` and resolve once it is laid out with fonts. */
export async function loadFrame(frame: HTMLIFrameElement, srcdoc: string): Promise<Document> {
  // STEP 1: attach the listener before assigning srcdoc to avoid a race.
  const loaded = new Promise<void>((resolve) => frame.addEventListener("load", () => resolve(), { once: true }));
  frame.srcdoc = srcdoc;
  await loaded;
  const doc = frame.contentDocument;
  if (!doc) throw new Error("chapter document unavailable");

  // STEP 2: wait for web fonts (bounded — a broken font must not hang the reader).
  const fonts = (doc as Document & { fonts?: FontFaceSet }).fonts;
  if (fonts?.ready) await Promise.race([fonts.ready, sleep(1500)]);
  return doc;
}

/** Attach listeners; returns a function that removes them. */
export function wireFrame(doc: Document, spine: number, frame: HTMLIFrameElement, hooks: FrameHooks): () => void {
  const onKey = (e: KeyboardEvent) => hooks.key(e);

  // STEP 1: clicks — links, highlights, or a tap on the page.
  const onClick = (e: MouseEvent) => {
    const target = e.target as Element | null;
    const link = target?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (link) {
      e.preventDefault(); // the reading view never navigates away
      const internal = link.getAttribute("data-jr-href");
      if (internal) hooks.link(internal, false);
      else if (link.hasAttribute("data-jr-external")) hooks.link(link.getAttribute("href") ?? "", true);
      return;
    }
    const mark = target?.closest?.("mark.jr-hl") as HTMLElement | null;
    const sel = doc.getSelection();
    if (sel && !sel.isCollapsed) return; // finishing a selection is not a tap
    if (mark?.dataset.hl) {
      hooks.highlight(mark.dataset.hl, mark, frame);
      return;
    }
    hooks.tap(e.clientX, e.clientY, frame);
  };

  // STEP 2: selection ends on pointer/key up.
  const onUp = () => setTimeout(() => hooks.selection(doc, spine, frame), 0);
  const onWheel = (e: WheelEvent) => hooks.wheel(e);

  doc.addEventListener("keydown", onKey);
  doc.addEventListener("click", onClick);
  doc.addEventListener("mouseup", onUp);
  doc.addEventListener("keyup", onUp);
  doc.addEventListener("wheel", onWheel, { passive: false });
  return () => {
    doc.removeEventListener("keydown", onKey);
    doc.removeEventListener("click", onClick);
    doc.removeEventListener("mouseup", onUp);
    doc.removeEventListener("keyup", onUp);
    doc.removeEventListener("wheel", onWheel);
  };
}

export function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/** Two animation frames: enough for the engine to apply new styles/layout. */
export function nextLayout(): Promise<void> {
  return new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
}
