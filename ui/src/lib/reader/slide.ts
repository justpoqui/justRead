// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// The page-slide used by swipes in both engines: the page follows the
// finger, then slides away and the new page slides in from the other side
// (or slides back if the swipe wasn't far enough or there is no page).

const OUT_MS = 160;
const IN_MS = 180;

function reducedMotion(): boolean {
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function waitFrame(): Promise<void> {
  return new Promise((r) => requestAnimationFrame(() => r()));
}

/** Follow the finger (no transition), damped slightly so it feels attached but heavy. */
export function followFinger(el: HTMLElement, dx: number) {
  el.style.transition = "none";
  el.style.transform = `translateX(${dx * 0.9}px)`;
}

/** Put the page back where it was. */
export function slideBack(el: HTMLElement) {
  if (!el.style.transform) return;
  el.style.transition = reducedMotion() ? "none" : `transform ${IN_MS}ms ease-out`;
  el.style.transform = "";
}

/**
 * Slide the page out (left when going forward), run `turn`, then slide the
 * new page in from the other side. If `turn` reports no page change (start
 * or end of the book), the page slides back instead.
 */
export async function slideTurn(el: HTMLElement, forward: boolean, width: number, turn: () => Promise<boolean>) {
  if (reducedMotion()) {
    el.style.transform = "";
    await turn();
    return;
  }
  const sign = forward ? -1 : 1;
  // STEP 1: out.
  el.style.transition = `transform ${OUT_MS}ms ease-in`;
  el.style.transform = `translateX(${sign * width}px)`;
  await new Promise((r) => setTimeout(r, OUT_MS));
  // STEP 2: turn; a new page comes in from the opposite side, otherwise the old one returns.
  const moved = await turn();
  el.style.transition = "none";
  el.style.transform = `translateX(${(moved ? -sign : sign) * width * 0.35}px)`;
  await waitFrame();
  await waitFrame();
  el.style.transition = `transform ${IN_MS}ms ease-out`;
  el.style.transform = "";
}
