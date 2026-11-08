# ADR 0004 — Block-id locators and the reading anchor

**Status:** accepted (Phase 1–2)

## Decision
Reading positions are `f:<spine>/<block>:<offset>` (docs/LOCATOR_SPEC.md), where `block` is a
`data-jrb` id stamped at import and `offset` counts UTF-16 units of text in that block.

The engine keeps an **anchor** locator: it changes only when the reader navigates or the anchor
leaves the screen. All re-layouts display the anchor.

## Why
* Pixel or page positions change with fonts, sizes, window size and layout mode.
* Using "first visible character" after every re-layout drifts: paginated pages start at line starts,
  which differ between layouts, so pages → scroll → pages would creep backwards. The anchor makes the
  round trip exact (asserted in both end-to-end suites).
* Text-only offsets survive highlight `<mark>` wrapping.
