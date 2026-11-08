# Third-party components

Policy: MIT, BSD, Apache-2.0, ISC or zlib (see CONTRIBUTING.md). Enforced by `cargo deny check licenses`
(`deny.toml`) and reviewed for npm with `pnpm --dir ui licenses list --prod`.

## Rust — direct dependencies

| Crate | Used for | License |
|---|---|---|
| tauri, tauri-build | App shell | Apache-2.0 OR MIT |
| tauri-plugin-dialog | Import file picker | Apache-2.0 OR MIT |
| rusqlite (`bundled`) | SQLite access | MIT (SQLite itself: public domain) |
| zip | EPUB container | MIT |
| quick-xml | OPF/NCX/nav/XHTML parsing | MIT |
| encoding_rs | Text decoding | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| chardetng | Encoding detection for TXT | Apache-2.0 OR MIT |
| sha1 | IDPF font de-obfuscation key | MIT OR Apache-2.0 |
| sha2 | Duplicate detection | MIT OR Apache-2.0 |
| serde, serde_json | Manifest / IPC serialization | MIT OR Apache-2.0 |
| thiserror | Error types | MIT OR Apache-2.0 |
| uuid | Book ids | Apache-2.0 OR MIT |
| lopdf | PDF metadata, page count, encryption check | MIT |
| sevenz-rust2 | CB7 (7-Zip) comics | Apache-2.0 |
| tempfile (dev) | Tests | MIT OR Apache-2.0 |

## Rust — reviewed transitive exceptions (from Tauri)

| Crates | License | Why acceptable |
|---|---|---|
| cssparser, cssparser-macros, selectors, dtoa-short, option-ext | MPL-2.0 | File-level copyleft; used unmodified as libraries, which permits distribution under Apache-2.0. Source is available from crates.io. |
| icu_* , zerovec*, yoke*, zerofrom*, tinystr, litemap, writeable, potential_utf, zerotrie, unicode-ident, unicode-normalization, unicode-properties | Unicode-3.0 | Permissive (MIT-style) Unicode data license. |

`r-efi` is offered as MIT OR Apache-2.0 OR LGPL-2.1+; we use it under MIT/Apache-2.0.

## npm — shipped (production) dependencies

| Package | License |
|---|---|
| @tauri-apps/api | Apache-2.0 OR MIT |
| @tauri-apps/plugin-dialog | MIT OR Apache-2.0 |
| pdfjs-dist (PDF.js) | Apache-2.0 |

PDF.js data shipped with the app (`ui/public/pdfjs/`): CMaps and standard-font metrics (Apache-2.0,
part of PDF.js) and wasm decoders: OpenJPEG (BSD-2-Clause), JBIG2 from PDFium (BSD-3-Clause), QCMS
(MIT), with their license files. The QuickJS engine (MIT) is *not* shipped. `@napi-rs/canvas` (MIT)
is an optional Node-only dependency of pdfjs-dist and is never bundled into the app.

Development-only tools (not shipped): svelte (MIT; compiled into the bundle, MIT), vite (MIT),
@sveltejs/vite-plugin-svelte (MIT), typescript (Apache-2.0), svelte-check (MIT), vitest (MIT),
jsdom (MIT), @tauri-apps/cli (Apache-2.0 OR MIT), @tsconfig/svelte (MIT). Playwright (Apache-2.0) is
used from the environment for end-to-end tests only.

## Not used (license)

| Component | Why |
|---|---|
| unRAR / `unrar` crate | Bundles RARLAB's proprietary unRAR source license; see adr/0006 |

## Platform components (not bundled)

| Platform | Web engine | Notes |
|---|---|---|
| Linux | WebKitGTK | LGPL-2.1; a system library linked dynamically, not distributed by us |
| Windows | Microsoft WebView2 | System runtime |
| macOS / iOS | WKWebView | System framework |
| Android | System WebView | System component |

## Fonts and artwork

No fonts are bundled (ADR 0005). The app icon (`src-tauri/icons/icon.svg`) is original artwork,
Apache-2.0. UI icons are original stroke drawings in `ui/src/components/Icon.svelte`.
