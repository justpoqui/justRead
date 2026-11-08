# Contributing to justRead

## Clean-room rule

justRead is 100% original code. **Do not copy, port or closely paraphrase code** from other
readers: in particular Readest, KOReader (AGPL), Foliate (GPL) or Calibre (GPL). Reading their
documentation or using them to understand a problem is fine; their source must not shape ours.
Cite *specifications* (W3C EPUB, CSS, WHATWG) instead.

## Licensing

* Every source file starts with:
  ```
  // Copyright 2026 The justRead Authors
  // SPDX-License-Identifier: Apache-2.0
  ```
  (use `#` for TOML/shell/Python and `<!-- -->` for HTML/Svelte).
* New dependencies must be MIT, BSD, Apache-2.0, ISC or zlib licensed. `cargo deny check licenses`
  enforces this in CI (see `deny.toml` for the few reviewed transitive exceptions). **Ask before adding
  anything GPL/AGPL/LGPL/MPL.** Add every new direct dependency to `docs/THIRD_PARTY.md`.

## Code style

* Break functions into explicit, commented steps (`// STEP 1: …`) that explain *what* and *why*.
* Keep chunks small. Prefer a pure function with a unit test over logic hidden in UI code.
* Rust: `cargo fmt`, `cargo clippy --workspace --all-targets`. UI: `pnpm --dir ui check`.

## Tests

Parsers and the locator system must have tests from the start. Fixtures are **generated in code or by
`scripts/make-sample-epub.py`**; never commit third-party books.

## Documentation

Update `docs/ARCHITECTURE.md` when you add a module or change a data flow, and add an ADR in
`docs/adr/` for decisions that future contributors would otherwise question.
