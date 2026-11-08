# justBook package format — schema version 1

justBook is justRead's internal format. Every reflowable book is converted to it **once, at import**;
the reader renders nothing else. It is a directory (not an archive) so files can be served directly.

```
<uuid>/
  manifest.json           this spec
  content/c0001.xhtml     chapters, in spine order
  resources/<path>        images, stylesheets, fonts (original relative layout preserved)
  cover.<ext>             optional
  original/<file name>    untouched source file, kept for re-conversion
```

## manifest.json

All keys are camelCase.

| Key | Type | Notes |
|---|---|---|
| `schemaVersion` | int | `1`. Readers reject newer versions. |
| `id` | string | UUID v4 = library primary key = directory name |
| `layout` | `"reflowable"` \| `"fixed"` | Phase 1–2 produce `reflowable` only |
| `metadata` | object | `title` (required), `authors[]`, `language`, `publisher`, `description`, `published`, `series`, `seriesIndex`, `identifiers[]` |
| `spine[]` | array | `{ href, title, chars, blocks, linear }` |
| `toc[]` | tree | `{ title, href?, children? }`; `href` = `content/cNNNN.xhtml[#fragment]` |
| `resources[]` | array | `{ href, mediaType }` |
| `cover` | string? | package path |
| `source` | object | `{ format, fileName, sha256, converterVersion }` |

* `chars` = text length of the chapter in **UTF-16 code units** (same unit as DOM string offsets).
* `blocks` = number of `data-jrb` ids in the chapter, including `<body>` (block 0).
* All paths are relative, `/`-separated, normalized, and never contain `..`.

## Chapter documents

Produced only by `ChapterWriter` (`crates/justbook/src/xhtml.rs`):

* Well-formed XHTML that **parses to the same tree as HTML** (no self-closed non-void elements;
  `<p>` closed before block content, as the HTML parser would).
* `<head>` contains exactly: `<meta charset>`, `<title>`, `<link rel="stylesheet">` to package CSS,
  and optionally one scrubbed `<style>`. No scripts. The literal `<head>` start tag is a stable marker.
* `<body data-jrb="0">`; every block element (`p`, `h1`–`h6`, `li`, `blockquote`, `div`, `td`, …) gets
  `data-jrb="N"`, numbered in document order. These ids are the backbone of locators.
* Internal links: `href` relative to the chapter **and** `data-jr-href="content/cNNNN.xhtml#frag"`.
  External links (http/https/mailto only): `data-jr-external="1"`.
* Images/resources: paths relative to the chapter, pointing into `resources/`. Remote resources are
  removed (offline-only).

## Versioning

Bump `schemaVersion` for incompatible changes. `source.converterVersion` (from
`justread_formats::CONVERTER_VERSION`) records which converter produced the package; because the
original is kept, packages can be regenerated when the converter improves.
