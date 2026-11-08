# Testing

| Layer | Command | What it covers |
|---|---|---|
| Rust | `cargo test --workspace` | package writer/paths, ChapterWriter, locators, CSS scrub, entities, XHTML normalizer, OPF/nav/NCX, DRM detection, font de-obfuscation, zip-slip, TXT heuristics, SQLite library (import/dedupe/rollback/sort/search/progress/settings/annotations/migrations), protocol path safety |
| TypeScript | `pnpm --dir ui test` | locator grammar (shared vectors), DOM⇄locator mapping, highlight painting keeps offsets, word snapping, page geometry, reader CSS, settings sanitizing, bookmark visibility |
| Types | `pnpm --dir ui check` | svelte-check over the whole UI |
| E2E (Chromium) | see below | the real UI and engine in a real layout engine, with a mocked backend |
| E2E (WebKitGTK) | see below | the real desktop app (Rust backend + WebKitGTK) via WebDriver with native input |

Fixtures are generated (`scripts/make-sample-epub.py`, `scripts/make-sample-fixed.py`, in-test
EPUB/PDF/CBZ/CB7/MOBI/KF8/DOCX/FB2/HTML builders), never third-party books.

**Optional real-world check (not part of CI).** To exercise the Phase 4 adapters on files produced by
another tool, convert the generated sample EPUB with Calibre's `ebook-convert` (installed from the OS
package manager and used only as an external generator; nothing from it is committed or linked):

```sh
ebook-convert /tmp/sample.epub /tmp/t-azw3.azw3 --title "Lantern (AZW3)"
ebook-convert /tmp/sample.epub /tmp/t-mobi.mobi --mobi-file-type old --title "Lantern (MOBI)"
ebook-convert /tmp/sample.epub /tmp/t-fb2.fb2 --title "Lantern (FB2)"
ebook-convert /tmp/sample.epub /tmp/t-docx.docx --title "Lantern (DOCX)"
cargo run -p justread-library --example import -- /tmp/jr-lib /tmp/t-*.*
```
The Chromium suite then also checks that each of these renders text and images and that its 6-chapter
TOC navigates.

## End-to-end

```sh
# 1. a library with real justBook packages
python3 scripts/make-sample-epub.py /tmp/sample.epub
python3 scripts/make-sample-fixed.py /tmp          # sample.pdf + sample.cbz
printf 'Title: A Plain Text Tale\n\nCHAPTER I\n\nIt was a quiet morning.\n' > /tmp/plain.txt
cargo run -p justread-library --example import -- /tmp/jr-lib /tmp/sample.epub /tmp/plain.txt /tmp/sample.pdf /tmp/sample.cbz

# 2a. Chromium (Playwright), mocked IPC, real packages
cd ui && node e2e/run.mjs /tmp/jr-lib /tmp/shots

# 2b. WebKitGTK (Linux), real app, over WebDriver
#     needs: webkit2gtk-driver, `cargo install tauri-driver`, a display (or Xvfb)
pnpm tauri build --debug --no-bundle
export XDG_DATA_HOME=/tmp/xdg
cargo run -p justread-library --example import -- $XDG_DATA_HOME/app.justread.reader /tmp/sample.epub /tmp/plain.txt /tmp/sample.pdf /tmp/sample.cbz
tauri-driver &
cd ui && node e2e/webkit.mjs ../target/debug/justread-app /tmp/wshots
```

The Chromium scenario checks: library listing; pagination; image loading; keyboard paging; exact
locator round-trip pages → scroll → pages; font-size re-layout keeps the locator; automatic spread;
selection → highlight (saved + painted, repainted after layout switch); neighbour loading in scroll
mode; bookmarks; TOC; internal links to fragments; dark theme; resume on reopen; TXT rendering; and
for Phase 3: PDF cover generation, PDF text actually drawn, spread/page locators, pages ⇄ scroll with
lazy rendering, outline → TOC, zoom and fit-width, PDF bookmarks, comic natural order, cover-alone and
landscape-alone spreads.

**justBrowse (WebKitGTK, real app):** a local Node server plays "the web"; its page tries to call Tauri
commands, emit events, open dialogs, read `justbook://` files and storage, and reports the results
from inside the sandbox. The run checks: toolbar-only IPC, HTTP warning before any request, all escape
attempts denied by the ACL, trackers blocked and counted, third-party cookies refused (with a
first-party control), EPUB download imported into the library, non-books discarded, DuckDuckGo
default, template validation, SearXNG template. Run (debug build only):

```sh
JUSTREAD_BROWSE_ALLOW_LOOPBACK=1 XDG_DATA_HOME=/tmp/xdg-b tauri-driver &
cd ui && node e2e/browse.mjs ../target/debug/justread-app /tmp/sample.epub /tmp/bshots
```
`JUSTREAD_BROWSE_ALLOW_LOOPBACK` is honoured only by debug builds; release builds always refuse local
destinations. Note: `tauri-driver` passes *its* environment to the app, so restart it to change
`XDG_DATA_HOME`.

The WebKitGTK scenario checks the same core paths against the real backend, including **native**
drag-selection and clicks inside the chapter iframe (see adr/0003).
