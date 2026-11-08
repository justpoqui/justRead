# ADR 0006 — CBR (RAR) comics: detect, don't decode (pending decision)

**Status:** open, awaiting the maintainer's decision (Phase 3)

## Context
CBR files are RAR archives. The only mature RAR decoder is RARLAB's unRAR C++ source, which the `unrar`
Rust crate bundles. Although the crate's own metadata says MIT/Apache-2.0, the bundled code is under the
**unRAR license**: freeware, redistributable, but forbidding use of the code to re-create the RAR
compression algorithm. It is not an OSI license and not on the project's allow-list. No pure-Rust RAR5
decoder is mature. libarchive (BSD-2) reads RAR4/RAR5 but is a C library that would have to be built
and shipped for Windows and macOS.

## Current behavior
`CbrAdapter` recognizes RAR comics and explains that CBZ works today. Files named `.cbr` that are really
ZIP or 7z archives (common) are imported normally.

## Options
1. Accept the unRAR license as a reviewed exception (simplest; license not open source).
2. Use libarchive via `compress-tools` (BSD-2 + MIT/Apache) and ship libarchive on every platform
   (more build work, fully open source).
3. Keep CBR unsupported.
