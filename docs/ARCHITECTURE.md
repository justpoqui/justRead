# justRead architecture

_Last updated: Phase 6 in progress (mobile UI, Open with, mobile CI builds)._

## Overview

```
┌──────────────────────────── Tauri app (one window, "main") ────────────────────────────┐
│  UI (Svelte 5 + TypeScript)                                                             │
│   views/Library ── views/Reader ── components/* (panels, menus)                         │
│                         │                                                               │
│                         ▼                                                               │
│   lib/reader  (framework-free engine)                                                   │
│     engine.ts ── PaginatedView | ScrolledView ── chapter iframes (srcdoc + CSP)          │
│   lib/fixed   (PDF & comics)                                                            │
│     fixed-engine.ts ── PageSource: PdfSource (PDF.js canvas) | ImageSource (<img>)      │
│     locator.ts / dom-locator.ts  ⇄  "f:spine/block:offset"                              │
│                         │ invoke()                    ▲ justbook://<id>/<path>          │
├─────────────────────────┼─────────────────────────────┼─────────────────────────────────┤
│  src-tauri (thin shell) │ commands.rs                 │ protocol.rs                      │
│                         ▼                             │                                 │
│  justread-library ── SQLite (books, progress, annotations, settings)                    │
│        │ import pipeline                                                                │
│        ▼                                                                                │
│  justread-formats ── Registry ── Epub Txt Pdf Cbz Cb7 Cbr Mobi Fb2 Docx Html (adapters)  │
│        └─ htmlbook: generated HTML → normalize/split → justBook (MOBI/AZW3/FB2/DOCX/HTML)│
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

**Generated-HTML pipelines** (Phase 4, `htmlbook.rs`): MOBI/AZW3, FB2, DOCX and HTML adapters
translate their input into *virtual* HTML documents + resources + a TOC; `write_html_book` then
normalizes them with the same sanitizer as EPUB, **splits long documents** between top-level blocks
(`NormalizeOptions::split_chars`, ~60k UTF-16 units per chapter) and resolves **deferred internal
links** once every chapter and anchor id is known (a target may land in a split part).

* **MOBI 6 / AZW / PRC** (`mobi/`): Palm database → MOBI/EXTH headers → text records (trailing entries
  stripped; PalmDOC LZ77 or HUFF/CDIC) → `filepos` byte offsets become anchors, `<mbp:pagebreak>`
  splits chapters, `recindex` images become resources; NCX index → TOC. Encrypted books are refused.
* **AZW3 / KF8** (also the KF8 half of combo `.mobi` files): FDST flows, SKEL/FRAG indexes rebuild
  each HTML file, `kindle:pos|embed|flow` URLs become anchors/images/CSS; NCX → TOC.
* **PDB** (PalmDOC `TEXtREAd`): plain text through the TXT heuristics.
* **FB2 / FB2.ZIP** (`fb2.rs`): one chapter per top-level section; titles, epigraphs, poems, notes
  (separate chapter, links resolved across chapters), base64 images, series metadata.
* **DOCX** (`docx.rs`): headings from styles/outline levels (chapters split at the top level used),
  lists, tables, inline formatting, hyperlinks, bookmarks, images, footnotes.
* **HTML** (`htmlfile.rs`): local images/CSS imported only from the page's own folder.
* **Recognized, not opened**: DjVu (adr/0007), KFX (DRM'd or undocumented), CBR (adr/0006).

**Fixed-layout pipelines** (Phase 3) produce `layout: "fixed"` packages with a `fixed` section
(docs/JUSTBOOK_SPEC.md):

* **PDF** (`pdf/`): DRM handlers (`/EBX_HANDLER`, LiveCycle, FileOpen, …) are refused; empty-user-password
  PDFs open; real passwords get a clear message. `lopdf` reads metadata and the page count; the file is
  stored unchanged as `content/book.pdf`. **Rendering is PDF.js in the UI**: no PDF engine of our own.
* **CBZ / CB7** (`comic/`): page images copied unchanged to `pages/`, sizes read from image headers
  (`images.rs`), natural page order (`p2` < `p10`), `ComicInfo.xml` metadata and manga RTL flag,
  first page as cover. ZIP via `SafeArchive`, 7z via `sevenz-rust2`, both with size limits.
* **CBR**: recognized but not converted (see adr/0006); `.cbr` files that are really ZIP/7z are imported.

For PDFs and comics the package already holds the source losslessly, so `original/` is not kept
(`BookContent.keep_original = false`).

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

## justBrowse (`src-tauri/src/browse`, `ui/src/browse`)

A separate window with two webviews: the **toolbar** (`browse.html`, our page; capability
`capabilities/browse.json` grants only `browse_*` commands) and the **content** (remote pages; in no
capability, so zero IPC; incognito; autofill off; third-party cookies refused). All content traffic
goes through a local **proxy** (`proxy.rs`) that applies the tracker/ad blocklist (`blocklist.txt`,
original list) to every request, tunnels HTTPS without decryption, refuses plain HTTP unless the
user accepted the warning for that host, and refuses local destinations after DNS resolution.
Navigation policy and address-bar parsing are pure functions in `policy.rs` (DuckDuckGo by default;
any `https://…{q}` template such as SearXNG). Book downloads are imported and the reader's library
refreshes via a `library-changed` event. Details and platform caveats: adr/0008-justbrowse.md.

## Mobile and "Open with" (Phase 6)

* **File associations** (tauri.conf.json `bundle.fileAssociations`): EPUB, MOBI/AZW/PRC, AZW3, FB2,
  CBZ, CB7 as default viewer; PDF as an alternate. These generate desktop installer associations,
  iOS document types and Android intent filters.
* **Receiving files** (`src-tauri/src/open_files.rs`): Windows/Linux get paths as launch arguments;
  macOS/iOS/Android get URLs via `RunEvent::Opened`. Files are queued; the library takes the queue on
  start (and on `open-files` events), imports, and opens the book.
* **URIs**: `import_books` accepts paths or `file://`/`content://` URIs; URIs are copied through the
  fs plugin's Rust API (Android content resolver, iOS security-scoped access) into a private folder.
  No webview is granted fs permissions.
* **Touch** (`ui/src/lib/reader/gestures.ts`): swipe to turn pages in both engines (mirrored for RTL
  comics), pinch to zoom PDFs/comics (one finger pans when zoomed), long-press selections reported via
  `selectionchange`.
* **Layout**: safe-area insets, compact header and bottom-sheet panels under 640px, larger touch
  targets on coarse pointers, edge arrows hidden on touch screens.
* **justBrowse** is desktop-only for now (`#[cfg(desktop)]`); phones get a clear message.
* **Builds**: `.github/workflows/mobile.yml` builds an Android debug APK and attempts an iOS simulator
  build. The Rust code is type-checked for `aarch64-linux-android`.

## Reader engine (`ui/src/lib/reader`)

| File | Role |
|---|---|
| `engine.ts` | Owns the active view, chapter cache, annotations, the **anchor**, events |
| `paginated.ts` | One chapter in CSS columns; 1 or 2 columns per screen; horizontal scroll per screen |
| `scrolled.ts` | Chapters stacked vertically, auto-height iframes, neighbours loaded near edges |
| `geometry.ts` | Pure page math (column width, gap, stride) |
| `styles.ts` | Reader CSS from settings (typography, themes, layout, highlight colors) |
| `chapter-doc.ts` | Builds the iframe `srcdoc` (CSP meta, `<base>`, reader styles) |
| `dom-locator.ts` | DOM ⇄ locator mapping, first-visible-character search, word snapping |
| `highlights.ts` | Paint/clear `<mark>` highlights without changing text offsets |
| `frame.ts` | iframe creation, loading, event wiring |

### Fixed-layout engine (`ui/src/lib/fixed`)

| File | Role |
|---|---|
| `fixed-engine.ts` | Same `Engine` contract as the text engine; paginated (single/spread) and continuous scroll; anchor `p:<page>@<offset>` |
| `sources.ts` | `PdfSource` (PDF.js → `<canvas>`, outline → TOC, cover rendering) and `ImageSource` (comic pages) |
| `layout.ts` | Pure math: spreads (cover alone, landscape pages alone), fit-page/fit-width, zoom, canvas memory cap |

PDF.js runs with its worker, local CMaps/standard fonts/wasm decoders (copied to `ui/public/pdfjs` by
`ui/scripts/copy-pdfjs.mjs`), no XFA and no scripting engine (QuickJS is not shipped). The app CSP
adds only `'wasm-unsafe-eval'` (image decoders), `worker-src 'self' blob:` and `connect-src justbook:`.
In scroll mode pages render only near the viewport and canvases far away are released.
PDF covers are rendered from page 1 by the library view and stored with `set_cover`.

### Layouts

The reader offers exactly three layouts, chosen by the user (toolbar layout menu or settings) and
**never changed automatically** by window size or orientation: **Single page**, **Two pages**,
**Scroll**. Internally they are `flow` (`paginated` | `scrolled`) + `spread` (`single` | `double`);
settings saved by older versions ("auto") migrate to Single page. For PDFs and comics, Two pages still
shows the cover and landscape (double-page) scans on their own, as printed books do.

### Position model

Positions are **locators**, never page numbers (docs/LOCATOR_SPEC.md). The engine keeps an
**anchor**: the locator the reader is at. The anchor changes only when the reader navigates or the
anchor scrolls off screen. Re-layouts (font, size, margins, theme, window size, spread) and layout
switches (pages ⇄ scroll) re-render and then display the anchor, so repeated switching never drifts.
Progress saved to SQLite is the anchor.

### Reader screen (`ui/src/views/Reader.svelte`)

- **Top bar:** back, title, ⚙ **Settings**: a full screen (`SettingsScreen.svelte`) with one category
  per group: Layout, Text (Page & zoom for PDFs/comics), Theme, Toolbar, Page indicator. The quick
  **Aa** panel still offers everything in one sheet.
- **Toolbar:** the user picks which buttons show (Contents, Highlights, Bookmark, Layout, Aa,
  Position), their order, and whether they sit in the bottom bar (default) or the top bar.
  Stored in `ReaderSettings.toolbar` / `toolbarPosition`.
- **Page indicator:** tapping it cycles pages left in chapter → pages left in book → % of chapter →
  % of book (`lib/reader/progress.ts`, unit-tested). Text books have no fixed page count, so "pages
  left in book" is estimated from this chapter's pages per character. Scroll layout only offers the
  percentages; PDFs/comics only whole-book views. A hidden `.pos-full` label always holds
  "Page x of y" for screen readers and tests.
- **Position popup** (replaces the always-on slider, which was easy to bump): previous/next chapter,
  a fine slider inside the chapter (`Engine.goToChapterFraction`), and a whole-book slider that only
  previews the target chapter until **Go**. Any jump from the popup offers "Back to where you were".
- **Hiding controls:** a tap in the middle or the ⌄ button hides both bars; hidden controls ignore
  taps and clicks, the indicator stays faintly visible, and a small handle brings them back.
- **Back (Esc / Android back button):** closes the topmost layer (note editor, Settings, layout
  menu, selection, panel), then leaves the book. `lib/back.ts` keeps a stack of handlers and takes
  over Android's back key with Tauri's `onBackButtonPress`; only in the library with nothing to clear
  does the app exit (`core:app:allow-exit`).

### Highlight export (Phase 7)

A book's highlights, notes and bookmarks become one plain Markdown document
(`justread-library/src/export.rs`, unit-tested): YAML front matter (title, author, date), a
heading per chapter, each highlight as a block quote (book text escaped so it shows literally;
colors other than yellow named in words), the reader's notes as written, then a bookmarks list.
No app-specific syntax: any Markdown or text app can read it. justRead does not write into other
apps' folders.

The **Export** row in the Highlights panel sends it anywhere:
- **Share…**: Android uses our own plugin `crates/tauri-plugin-justshare` (Kotlin, ~60 lines):
  the file goes to a private cache folder, is exposed read-only through the plugin's own
  `FileProvider`, and `ACTION_SEND` carries both the `.md` file and the text, so the system share
  sheet lists every app that takes either. iOS, Windows and macOS use the web Share API
  (`navigator.share` with the file). Linux has no share sheet, so the button is hidden there.
- **Save…**: the OS Save dialog is opened from Rust (`save_annotations`), so the webview never
  supplies a path; Android/iOS document URLs are written through the fs plugin.
- **Copy**: the text on the clipboard.

### Reading stats and library organisation (Phase 8)

- **Shelves:** `books.status` is `unread` / `reading` / `finished` (schema v3). Opening a book
  moves it from Unread to Reading; reaching the last page offers "Mark as finished" (once per
  visit); the book options sheet (⋯ on a book) changes it any time. `finished_at` records when.
- **Collections:** user-named groups (`collections`, `book_collections`; names unique ignoring
  case, 1–60 chars). A book can be in several; deleting a collection keeps its books.
- **Reading time:** `ui/src/lib/stats/tracker.ts` counts only *active* reading — the app visible
  and a page turn/scroll/selection in the last 2 minutes — and reports about once a minute, on
  hide and on close, per local calendar day. Forward movement is summed for the reader's pace;
  jumps over 2 % (contents, position popup) and going back are ignored. The backend
  (`justread-library/src/stats.rs`) validates every report (real date, 1–600 s, progress ≤ 5 %).
  Stored in `reading_days`, on this device only.
- **Stats screen:** today, last 7 days, streak (today or yesterday back), finished this year,
  and a 14-day bar chart with hover values and a table view. Per book (in the options sheet):
  time read, days read, and time left at the reader's own pace once there are 10 minutes and
  2 % of the book of data.
- **Library:** a Continue reading row (books on the Reading shelf, most recent first) and filter
  chips for shelves and collections, persisted with the other library preferences.

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
| 3 | PDF (PDF.js) and comics (CBZ/CB7; CBR pending license decision) | ✅ done |
| 4 | MOBI, AZW3, FB2, HTML, DOCX adapters; DjVu | ✅ done, except DjVu (decision pending, adr/0007) |
| 5 | justBrowse (privacy-hardened browser) | ✅ done on desktop (mobile design in adr/0008) |
| 6 | Android and iOS | ✅ Android done; iOS builds in CI (device install pending) |
| 7 | Highlight export (Markdown, to any app) | ✅ done |
| 8 | Reading stats, shelves, collections, Continue reading | ✅ done |

### Known limitations / follow-ups

* **Hyphenation** depends on the platform: WebKitGTK builds often ship without hyphenation
  dictionaries, so `hyphens: auto` has no effect there (Chromium/WebView2/WKWebView hyphenate).
  A built-in hyphenator (Liang patterns; pattern licenses to be vetted) is a candidate for later.
* Right-to-left and vertical writing modes are not yet paginated correctly.
* MathML is unwrapped to text; audio/video are dropped.
* PDFs: no text selection, highlights or search yet (bookmarks work); highlights need the PDF.js text
  layer (later). Annotations in fixed books are bookmarks only.
* justBrowse: macOS < 14 has no proxy (no blocklist for sub-resources); Windows third-party cookie
  behaviour unverified; blocklist is domain-level and modest; Tauri `unstable` API dependency.
* HUFF/CDIC-compressed MOBI is covered by a synthetic test only (no real sample available offline).
* DOCX: EMF/WMF images, text boxes, comments and tracked changes are not converted.
* CBR import awaits a decision on the unRAR license (adr/0006).
* Folder watch import is deferred (see open questions in the Phase 1 plan).
* Imports hold the library lock while converting; very large batches block other commands until done.
