// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// CSS injected into every chapter document (after the book's own styles,
// so it wins where it needs to). Generated from settings + layout.

import { FONT_STACKS, themeColors, type ReaderSettings } from "../settings/reader-settings";
import type { Geometry } from "./geometry";

export type LayoutCss =
  | { kind: "paginated"; geometry: Geometry }
  | { kind: "scrolled"; width: number };

/** Highlight colors (translucent so text stays readable on every theme). */
export const HIGHLIGHT_COLORS: Record<string, string> = {
  yellow: "rgba(255, 213, 79, 0.45)",
  green: "rgba(129, 199, 132, 0.45)",
  blue: "rgba(100, 181, 246, 0.45)",
  pink: "rgba(240, 98, 146, 0.40)",
  purple: "rgba(186, 104, 200, 0.40)",
};

export function fontStack(s: ReaderSettings): string | null {
  if (s.font === "publisher") return null;
  if (s.font === "custom") return s.customFont ? `"${s.customFont}", ${FONT_STACKS.serif}` : null;
  return FONT_STACKS[s.font];
}

export function readerCss(s: ReaderSettings, layout: LayoutCss): string {
  const c = themeColors(s);
  const font = fontStack(s);
  const parts: string[] = [];

  // STEP 1: base reset and theme. `color-scheme` makes form controls and
  // scrollbars match dark themes.
  parts.push(`
:root { color-scheme: ${c.forceColors ? "dark" : "light"}; }
html, body { margin: 0 !important; padding: 0 !important; background: ${c.background} !important; color: ${c.text} !important; }
a:link, a:visited { color: ${c.link} !important; }
::selection { background: rgba(120, 160, 255, 0.35); }`);

  // STEP 2: typography. Size goes on <html> so the book's em/rem units scale.
  parts.push(`
html { font-size: ${s.fontSize}px !important; -webkit-text-size-adjust: 100%; }
body { line-height: ${s.lineHeight} !important; ${font ? `font-family: ${font} !important;` : ""}
  overflow-wrap: break-word; -webkit-font-smoothing: antialiased; }`);
  if (!s.publisherStyles) {
    // Neutralize the book's fonts and spacing (headings keep their sizes).
    parts.push(`
body *:not(code):not(pre):not(kbd):not(samp) { font-family: inherit !important; }
p, li, dd, blockquote, div { line-height: inherit !important; }`);
  }
  parts.push(`
p, li, dd, blockquote { text-align: ${s.justify ? "justify" : "start"}; }
body { hyphens: ${s.hyphenation ? "auto" : "manual"}; -webkit-hyphens: ${s.hyphenation ? "auto" : "manual"}; }`);

  // STEP 3: dark/custom themes must override hard-coded book colors.
  if (c.forceColors) {
    parts.push(`
body *:not(mark) { color: inherit !important; background-color: transparent !important; border-color: currentColor; }
a:link *, a:visited * { color: ${c.link} !important; }
img { filter: brightness(0.9); }`);
  }

  // STEP 4: layout.
  if (layout.kind === "paginated") {
    const g = layout.geometry;
    parts.push(`
html { height: ${g.frameHeight}px !important; overflow: hidden !important; touch-action: pan-y; }
body { height: ${g.frameHeight}px !important; box-sizing: border-box;
  column-width: ${g.columnWidth}px !important; column-gap: ${g.gap}px !important; column-fill: auto !important;
  max-width: none !important; width: auto !important; }
img, svg, video { max-width: 100% !important; max-height: ${Math.floor(g.frameHeight * 0.95)}px !important;
  object-fit: contain; break-inside: avoid; }
h1, h2, h3, h4, h5, h6 { break-after: avoid; break-inside: avoid; }
figure, table { break-inside: avoid; }`);
  } else {
    parts.push(`
html { overflow: hidden !important; height: auto !important; }
body { max-width: ${layout.width}px; margin: 0 auto !important; padding: 1.5em 0 !important; box-sizing: border-box; }
img, svg, video { max-width: 100% !important; height: auto; }`);
  }

  // STEP 5: highlights (painted as <mark class="jr-hl jr-c-<color>">).
  parts.push(`
mark.jr-hl { color: inherit !important; border-radius: 2px; cursor: pointer; }
${Object.entries(HIGHLIGHT_COLORS)
  .map(([k, v]) => `mark.jr-hl.jr-c-${k} { background-color: ${v} !important; }`)
  .join("\n")}
mark.jr-hl.jr-c-underline { background: transparent !important; text-decoration: underline 2px ${c.link}; text-underline-offset: 3px; }
mark.jr-hl.jr-note { border-bottom: 1px dashed currentColor; }`);

  return parts.join("\n");
}
