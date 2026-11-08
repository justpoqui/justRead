// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Pure layout math for fixed-layout books (PDF, comics), unit-tested.

export interface Size {
  width: number;
  height: number;
}

export type FitMode = "page" | "width";

/** Portrait fallback when a page's size is unknown (0×0 in the manifest). */
export const DEFAULT_PAGE: Size = { width: 1000, height: 1500 };

export function safeSize(s: Size | undefined): Size {
  return s && s.width > 0 && s.height > 0 ? s : DEFAULT_PAGE;
}

/**
 * Group pages into screens for spread display.
 * The first page (the cover) stands alone, as in a printed book; landscape
 * pages (already a double-page scan) also stand alone; everything else pairs up.
 */
export function buildSpreads(sizes: Size[], spread: boolean): number[][] {
  // STEP 1: no spreads → one page per screen.
  if (!spread) return sizes.map((_, i) => [i]);

  // STEP 2: pair pages, keeping the cover and wide pages single.
  const out: number[][] = [];
  let i = 0;
  while (i < sizes.length) {
    const wide = (n: number) => {
      const s = safeSize(sizes[n]);
      return s.width > s.height;
    };
    if (i === 0 || wide(i) || i + 1 >= sizes.length || wide(i + 1)) {
      out.push([i]);
      i += 1;
    } else {
      out.push([i, i + 1]);
      i += 2;
    }
  }
  return out;
}

/** Index of the screen containing `page`. */
export function screenOfPage(screens: number[][], page: number): number {
  const i = screens.findIndex((s) => s.includes(page));
  return i < 0 ? 0 : i;
}

/**
 * CSS sizes of the pages on one screen. Pages of a spread are scaled to a
 * common height so they line up; the whole screen then fits the viewport
 * ("page") or its width ("width"), multiplied by the user's zoom.
 */
export function fitScreen(pages: Size[], viewport: Size, fit: FitMode, zoom: number, gap = 0): Size[] {
  // STEP 1: normalize every page to height 1.
  const norm = pages.map((p) => {
    const s = safeSize(p);
    return { width: s.width / s.height, height: 1 };
  });
  const totalW = norm.reduce((a, p) => a + p.width, 0);
  const free = Math.max(1, viewport.width - gap * (pages.length - 1));

  // STEP 2: choose the height that satisfies the fit mode.
  const byWidth = free / totalW;
  const h = fit === "width" ? byWidth : Math.min(byWidth, viewport.height);

  // STEP 3: apply zoom; round to whole CSS pixels.
  return norm.map((p) => ({ width: Math.round(p.width * h * zoom), height: Math.round(h * zoom) }));
}

/** Size of one page in continuous-scroll mode. */
export function scrollPageSize(page: Size, viewport: Size, fit: FitMode, zoom: number, maxWidth = 1400): Size {
  const s = safeSize(page);
  const aspect = s.width / s.height;
  // Fit-width is capped so huge monitors don't produce absurdly wide pages.
  const width =
    fit === "width" ? Math.min(viewport.width, maxWidth) : Math.min(viewport.width, viewport.height * aspect);
  const w = Math.round(width * zoom);
  return { width: w, height: Math.round(w / aspect) };
}

/** Canvas backing size for crisp rendering, capped to protect memory. */
export function backingScale(css: Size, devicePixelRatio: number, maxPixels = 16_000_000): number {
  const want = Math.max(1, devicePixelRatio);
  const pixels = css.width * css.height * want * want;
  return pixels <= maxPixels ? want : Math.sqrt(maxPixels / (css.width * css.height));
}
