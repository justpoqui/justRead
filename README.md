# justRead

An open-source, cross-platform ebook reader: part of the **just** suite.

* **Offline only.** No accounts, no cloud, no telemetry, no analytics. Everything stays on your device.
  The one exception is the Android update check (asks GitHub about once a day whether a newer
  version is published; sends nothing about you; can be turned off in Settings → Updates).
  See [docs/RELEASES.md](docs/RELEASES.md).
* **Your books, any layout.** Paginated, two-page spread or continuous scroll; switch at any time without losing your place.
* **Apache-2.0**, written from scratch.

> Status: Phases 1–5 are implemented on desktop: reader; layouts, highlights, notes, bookmarks; PDF and comics; MOBI/AZW3/FB2/DOCX/HTML; justBrowse. DjVu and CBR await licensing decisions. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#roadmap).

## Features (today)

| | |
|---|---|
| Formats | EPUB 2/3, MOBI/AZW/PRC, AZW3 (KF8), FB2, DOCX, HTML, TXT, PDB; PDF; CBZ/CB7 comics. DRM-protected files are detected and refused with a clear message. |
| Layouts | Single page, two pages, or continuous scroll: always exactly what you choose |
| Typography | Font family (system fonts or any installed font), size, line height, margins, justification, hyphenation, publisher-style toggle |
| Themes | Light, sepia, dark, black, custom colors |
| Library | Grid and list views, covers, search, sort by title/author/series/recently read/progress/date added, drag-and-drop import |
| Annotations | Highlights in five colors + underline, notes, bookmarks; all stored locally in SQLite |
| justBrowse | A privacy-hardened browser for free books: HTTPS-only, tracker blocklist, private sessions, no third-party cookies, DuckDuckGo or your own SearXNG; downloaded books go straight to your library |

## Building

Prerequisites: [Rust](https://rustup.rs) ≥ 1.80, Node.js ≥ 20 with [pnpm](https://pnpm.io), and the
[Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS
(on Debian/Ubuntu: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev`).

```sh
pnpm install          # installs UI and Tauri CLI dependencies
pnpm dev              # run the app with hot reload
pnpm build            # build installers for your platform
```

## Testing

```sh
cargo test --workspace     # parsers, locators, library, protocol (Rust)
pnpm --dir ui test         # locator/DOM/geometry/style tests (TypeScript)
pnpm --dir ui check        # type-check the UI
```

End-to-end tests (real layout engines) are described in [docs/TESTING.md](docs/TESTING.md).

## Project layout

```
crates/justbook           justBook package format (manifest, writer, XHTML chapter writer)
crates/justread-locator   format-independent reading positions
crates/justread-formats   format adapters (EPUB, TXT) + DRM detection
crates/justread-library   SQLite library, import pipeline, annotations
src-tauri                 thin Tauri shell: commands + justbook:// protocol
ui                        Svelte 5 + TypeScript UI and the reader engine
docs                      architecture, specs and decision records
```

## License

Apache License 2.0; see [LICENSE](LICENSE) and [NOTICE](NOTICE). Third-party components: [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md).
