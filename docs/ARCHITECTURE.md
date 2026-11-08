# justRead architecture

_Last updated: Phase 2 complete._

## Overview

```
┌──────────────────────────── Tauri app (one window, "main") ────────────────────────────┐
│  UI (Svelte 5 + TypeScript)                                                             │
│   views/Library ── views/Reader ── components/* (panels, menus)                         │
│                         │                                                               │
│                         ▼                                                               │
│   lib/reader  (framework-free engine)                                                   │
│     engine.ts ── PaginatedView | ScrolledView ── chapter iframes (srcdoc + CSP)          │
│     locator.ts / dom-locator.ts  ⇄  "f:spine/block:offset"                              │
│                         │ invoke()                    ▲ justbook://<id>/<path>          │
├─────────────────────────┼─────────────────────────────┼─────────────────────────────────┤
│  src-tauri (thin shell) │ commands.rs                 │ protocol.rs                      │
│                         ▼                             │                                 │
│  justread-library ── SQLite (books, progress, annotations, settings)                    │
│        │ import pipeline                                                                │
│        ▼                                                                                │
│  justread-formats ── Registry ── EpubAdapter | TxtAdapter   (FormatAdapter trait)       │
│        │ normalize on import                                                            │
│        ▼                                                                                │
│  justbook ── package writer / manifest / ChapterWriter  →  <data>/books/<uuid>/         │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

## Data on disk

```
<app data dir>/                    e.g. ~/.local/share/app.justread.reader on Linux
  library.sqlite3                  WAL mode
  books/<uuid>/                    one justBook package per book (docs/JUSTBOOK_SPEC.md)
```

The app data dir comes from Tauri's path resolver
(<https://v2.tauri.app/reference/javascript/api/namespacepath/#appdatadir>).

## Rust crates

| Crate | Responsibility | Depends on |
|---|---|---|
| `justbook` | Package model (`Manifest`), atomic `JustBookWriter`, safe path handling, `ChapterWriter` (well-formed, block-stamped XHTML) | – |
| `justread-locator` | `Locator` grammar, ordering, progression estimates | – |
| `justread-formats` | `FormatAdapter` trait, `Registry`, EPUB + TXT adapters, CSS scrubbing, entity decoding | `justbook` |
| `justread-library` | SQLite schema/migrations, import pipeline, progress, settings, annotations | all of the above |
| `justread-app` (`src-tauri`) | IPC commands, `justbook://` protocol, app state | all of the above |

None of the core crates depends on Tauri, so they test with plain `cargo test` and can be reused
(e.g. a CLI importer: `cargo run -p justread-library --example import`).

### Format adapters (normalize on import)

```rust
pub trait FormatAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn family(&self) -> FormatFamily;                                   // Reflowable | Fixed
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence;
    fn inspect(&self, src: &Path) -> Result<Inspection>;                // metadata + DRM status
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent>;
}
```

Adding a format = one new module + one line in `Registry::with_defaults`. The reader never changes.

**EPUB pipeline**: `SafeArchive` (entry/size limits, no extraction by name) → DRM check
(`rights.xml`, `sinf.xml`, `license.lcpl`, non-font algorithms in `encryption.xml`) → `container.xml`
→ OPF (metadata, manifest, spine; EPUB 2 + 3 conventions) → path plan (`content/cNNNN.xhtml`,
`resources/<original path>`) → resources (CSS scrubbed, obfuscated fonts restored) → TOC (nav, else
NCX) → each spine document through `html::normalize` → cover.

**Normalizer** (`justread-formats/src/html.rs`): lenient quick-xml event loop; allow-listed elements and
attributes; scripts/forms/embeds removed; unknown elements unwrapped; links and resources rewritten to
package paths; `<style>`/`<link>` collected; output through `ChapterWriter`.

**TXT pipeline**: encoding detection (BOM → declared → UTF-8 → chardetng), Gutenberg header/boilerplate
handling, paragraph detection, heading heuristics, splitting of very long chapters.

### Import pipeline (`justread-library/src/import.rs`)

`sha256` → duplicate? → `Registry::detect` → `adapter.convert` into `<uuid>.partial/` → copy original
→ `writer.finish` (validate + atomic rename) → insert rows in one transaction (package removed if this
fails). A failed import never leaves files or rows behind (tested).

### Database (`justread-library/src/db.rs`)

Migrations are append-only SQL tracked with `PRAGMA user_version`.

* v1: `books`, `authors`, `book_authors`, `progress`, `settings`
* v2: `annotations` (highlight/note/bookmark, locators + denormalized sort keys)

## Tauri shell

* **Commands** (`src-tauri/src/commands.rs`) return `Result<T, String>` with user-facing messages;
  imports run on a blocking thread.
* **Permissions**: commands are declared in `build.rs` (`AppManifest::commands`), which makes each
  one *deny-by-default*; `capabilities/main.json` grants the main window exactly what it needs.
  No fs/shell/http/opener plugins are installed. This is the base for justBrowse's zero-permission
  webview in Phase 5. (<https://v2.tauri.app/security/capabilities/>)
* **CSP** in `tauri.conf.json`: scripts only from the app; images/styles/fonts additionally from
  `justbook:`; no objects, no form actions.
* **`justbook://` protocol** (`protocol.rs`): `/<uuid>/<package path>` → file bytes. UUID-validated,
  traversal- and symlink-safe, never serves `manifest.json` or `original/`, `nosniff`, scripts denied.

## Reader engine (`ui/src/lib/reader`)

| File | Role |
|---|---|
| `engine.ts` | Owns the active view, chapter cache, annotations, the **anchor**, events |
| `paginated.ts` | One chapter in CSS columns; 1 or 2 columns per screen; horizontal scroll per screen |
| `scrolled.ts` | Chapters stacked vertically, auto-height iframes, neighbours loaded near edges |
| `geometry.ts` | Pure page math (column width, gap, stride, auto-spread rule) |
| `styles.ts` | Reader CSS from settings (typography, themes, layout, highlight colors) |
| `chapter-doc.ts` | Builds the iframe `srcdoc` (CSP meta, `<base>`, reader styles) |
| `dom-locator.ts` | DOM ⇄ locator mapping, first-visible-character search, word snapping |
| `highlights.ts` | Paint/clear `<mark>` highlights without changing text offsets |
| `frame.ts` | iframe creation, loading, event wiring |

### Position model

Positions are **locators**, never page numbers (docs/LOCATOR_SPEC.md). The engine keeps an
**anchor**: the locator the reader is at. The anchor changes only when the reader navigates or the
anchor scrolls off screen. Re-layouts (font, size, margins, theme, window size, spread) and layout
switches (pages ⇄ scroll) re-render and then display the anchor, so repeated switching never drifts.
Progress saved to SQLite is the anchor.

### Chapter isolation

Chapters render in same-origin `srcdoc` iframes (so the engine can measure and annotate them) with
two script barriers: import-time sanitizing, and a `script-src 'none'` CSP `<meta>` at the top of
every chapter. See [adr/0003-chapter-isolation.md](adr/0003-chapter-isolation.md) for why the `sandbox`
attribute is not used.

## Testing

See [TESTING.md](TESTING.md). Summary: Rust unit + integration tests (generated EPUB fixtures),
TypeScript unit tests (jsdom), shared locator vectors (`tests/fixtures/locators.json`) run by both
languages, and end-to-end runs in Chromium (mocked IPC) and in the real app on WebKitGTK (WebDriver).

## Roadmap

| Phase | Scope | Status |
|---|---|---|
| 1 | Scaffold, SQLite library, EPUB + TXT → justBook, single page + scroll, themes & fonts, desktop | ✅ done |
| 2 | Two-page spread, highlights/notes/bookmarks, position persistence across layouts | ✅ done |
| 3 | PDF (PDF.js) and comics (CBZ/CBR/CB7) | next |
| 4 | MOBI, AZW3, FB2, HTML, DOCX adapters; DjVu | |
| 5 | justBrowse (privacy-hardened browser) | |
| 6 | Android and iOS | |
| 7 | Markdown highlight export / JustNotes | |

### Known limitations / follow-ups

* **Hyphenation** depends on the platform: WebKitGTK builds often ship without hyphenation
  dictionaries, so `hyphens: auto` has no effect there (Chromium/WebView2/WKWebView hyphenate).
  A built-in hyphenator (Liang patterns; pattern licenses to be vetted) is a candidate for later.
* Right-to-left and vertical writing modes are not yet paginated correctly.
* MathML is unwrapped to text; audio/video are dropped.
* Folder watch import is deferred (see open questions in the Phase 1 plan).
* Imports hold the library lock while converting; very large batches block other commands until done.
