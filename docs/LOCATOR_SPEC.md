# Locators

A locator identifies a reading position independently of layout, font or screen. Implementations:
`crates/justread-locator` (Rust) and `ui/src/lib/reader/locator.ts` (TypeScript), both tested
against `tests/fixtures/locators.json`.

## Grammar

```
locator  = flow / fixed
flow     = "f:" uint "/" uint ":" uint          ; spine / block : offset
fixed    = "p:" uint "@" fraction               ; page @ vertical position
uint     = "0" / %x31-39 *DIGIT                 ; canonical, ≤ 4294967295
fraction = decimal in [0, 1], printed with ≤ 4 decimals, trailing zeros trimmed
```

Examples: `f:3/127:42`, `p:12@0.25`.

### Flow locators (justBook)

* `spine`: index into `manifest.spine`.
* `block`: value of the `data-jrb` attribute of the **innermost** block containing the position.
* `offset`: UTF-16 code units into that block's `textContent`.

Why not EPUB CFI (<https://idpf.org/epub/linking/cfi/>)? CFI encodes element paths so it can address
arbitrary publisher markup. Because justRead normalizes every book and stamps stable block ids at
import, a (block, text offset) pair is enough: shorter, trivial to resolve, and **unchanged when
highlights wrap text in `<mark>` elements** (offsets count text only).

### Fixed locators (PDF, comics)

`page` is 0-based; `offset` is the fraction of the page above the viewport top (0 in paginated mode).
The fixed engine keeps an anchor exactly like the text engine, so pages ⇄ scroll, zoom and fit changes
return to the same page and position.

## Ordering

Flow locators order by `(spine, block, offset)`. Blocks are numbered in pre-order, so text in an outer
block *after* a nested block sorts before that nested block. This only affects ordering of such
edge positions and is acceptable for "furthest read" and annotation lists.

## Progress

`progression(spine, loc)` weights chapters by `chars` and interpolates within a chapter by block index.
The reader UI refines this with the exact on-screen position (page/scroll fraction) where available.
