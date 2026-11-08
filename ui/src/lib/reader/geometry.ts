// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Page geometry for paginated mode (pure math, unit-tested).
//
// The chapter is laid out with CSS multi-column layout inside an iframe
// (https://www.w3.org/TR/css-multicol-1/). Each column is one page; we show
// `columns` pages per screen (1 = single page, 2 = spread) and turn screens
// by scrolling the iframe document horizontally by `stride` pixels.
//
//   |<-margin->|<---- frame width ---->|<-margin->|
//              | col | gap | col |
//   stride = columns * (columnWidth + gap) = frameWidth + gap

export interface Geometry {
  /** Iframe position and size inside the host element. */
  left: number;
  top: number;
  frameWidth: number;
  frameHeight: number;
  columns: 1 | 2;
  columnWidth: number;
  gap: number;
  /** Horizontal scroll distance of one screen turn. */
  stride: number;
}

export interface GeometryInput {
  hostWidth: number;
  hostHeight: number;
  margin: number;
  /** The reader's choice; never overridden by window size or orientation. */
  spread: "single" | "double";
}

/** Two pages per screen exactly when the reader asked for it. */
export function useSpread(i: GeometryInput): boolean {
  return i.spread === "double";
}

export function computeGeometry(i: GeometryInput): Geometry {
  // STEP 1: margins shrink on small screens so text keeps most of the width.
  const margin = Math.round(Math.min(i.margin, i.hostWidth * 0.12));
  const vMargin = Math.round(Math.max(24, Math.min(64, i.hostHeight * 0.06)));
  const columns: 1 | 2 = useSpread(i) ? 2 : 1;

  // STEP 2: the gap between two spread pages equals both inner margins.
  const gap = margin * 2;

  // STEP 3: integer column width so scrolling never accumulates sub-pixel drift.
  const available = Math.max(100, i.hostWidth - margin * 2);
  const columnWidth = Math.floor((available - (columns - 1) * gap) / columns);
  const frameWidth = columnWidth * columns + (columns - 1) * gap;
  const left = Math.floor((i.hostWidth - frameWidth) / 2);
  const frameHeight = Math.max(100, i.hostHeight - vMargin * 2);

  return { left, top: vMargin, frameWidth, frameHeight, columns, columnWidth, gap, stride: frameWidth + gap };
}

/** Number of screens needed for a document `scrollWidth` wide. */
export function screenCount(scrollWidth: number, g: Geometry): number {
  return Math.max(1, Math.ceil((scrollWidth + g.gap - 1) / g.stride));
}

/** Screen index that contains document x-coordinate `x`. */
export function screenOf(x: number, g: Geometry): number {
  return Math.max(0, Math.floor((x + 1) / g.stride));
}
