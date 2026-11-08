// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Turning a justBook chapter into an iframe `srcdoc`.
//
// A srcdoc iframe is same-origin with the reader, so the reader can measure,
// paginate and annotate the chapter DOM. Book scripts must never run:
//
//   layer 1  import-time sanitizing removes <script>, on* handlers and
//            javascript: URLs (crates/justread-formats/src/html.rs);
//   layer 2  every chapter starts with its own CSP <meta>: script-src 'none'
//            blocks any script that might still be present, in every engine.
//
// Why not `<iframe sandbox>`? WebKit (WebKitGTK, Safari, iOS) does not run
// even the *reader's* event listeners inside a sandboxed frame without
// `allow-scripts`, which would break selection, links and page taps — and
// `allow-scripts` + `allow-same-origin` makes a sandbox escapable anyway.
// See docs/adr/0003-chapter-isolation.md for the probe that showed this.

export const READER_STYLE_ID = "jr-reader-style";

/** Policy for chapter documents: no scripts, plugins, frames or forms. */
export const CHAPTER_CSP = "script-src 'none'; object-src 'none'; frame-src 'none'; form-action 'none'";

/** Build the srcdoc string: CSP and `<base>` first in <head>, reader CSS last. */
export function buildSrcdoc(xhtml: string, baseHref: string, css: string): string {
  // STEP 1: the CSP must precede everything it protects; <base> must come
  // before any relative URL in the document.
  const head =
    `<meta http-equiv="Content-Security-Policy" content="${CHAPTER_CSP}" />\n` +
    `<base href="${escapeAttr(baseHref)}" />`;
  let doc = xhtml.includes("<head>") ? xhtml.replace("<head>", `<head>\n${head}`) : `<head>${head}</head>${xhtml}`;

  // STEP 2: reader styles go after the book's, so they win at equal specificity.
  const style = `<style id="${READER_STYLE_ID}">${css.replace(/<\//g, "<\\/")}</style>`;
  doc = doc.includes("</head>") ? doc.replace("</head>", `${style}\n</head>`) : doc + style;
  return doc;
}

/** Update reader styles in an already-loaded chapter (no reload needed). */
export function applyCss(doc: Document, css: string): void {
  let el = doc.getElementById(READER_STYLE_ID);
  if (!el) {
    el = doc.createElement("style");
    el.id = READER_STYLE_ID;
    doc.head.appendChild(el);
  }
  el.textContent = css;
}

function escapeAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}
