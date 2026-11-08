// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! The `manifest.json` data model.
//!
//! The manifest is the single source of truth about a package: what the
//! book is called, in which order its chapters are read, how its table of
//! contents is structured and which files it contains.

use serde::{Deserialize, Serialize};

/// Current manifest schema version. Bump on any incompatible change and
/// teach `reader::load_manifest` how to read (or reject) older versions.
pub const SCHEMA_VERSION: u32 = 1;

/// Root of `manifest.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Schema version, see [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable package id (UUID v4 string) — also the library primary key.
    pub id: String,
    /// Reflowable or fixed layout. Phase 1 only produces `Reflowable`.
    pub layout: Layout,
    /// Descriptive metadata.
    pub metadata: Metadata,
    /// Reading order. Index in this list == `spine` field of a locator.
    pub spine: Vec<SpineItem>,
    /// Hierarchical table of contents.
    pub toc: Vec<TocEntry>,
    /// Every non-chapter file shipped in the package (images, CSS, fonts).
    pub resources: Vec<Resource>,
    /// Package-relative path of the cover image, if any.
    pub cover: Option<String>,
    /// Where the book came from.
    pub source: SourceInfo,
    /// Page data for fixed-layout books (PDF, comics). `None` for reflowable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<FixedContent>,
}

/// Fixed-layout content: either one PDF document or a sequence of page images.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedContent {
    pub kind: FixedKind,
    pub page_count: u32,
    /// Package path of the PDF (`kind == Pdf`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    /// Page images in reading order (`kind == Images`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<FixedPage>,
    /// Pages read right-to-left (manga). Affects spreads and page turns.
    #[serde(default)]
    pub right_to_left: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FixedKind {
    Pdf,
    Images,
}

/// One page image with its intrinsic size (so layout needs no image decoding).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedPage {
    pub href: String,
    pub width: u32,
    pub height: u32,
}

/// How the book is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    /// Text that reflows to the screen (justBook XHTML).
    Reflowable,
    /// Page images / PDF pages (Phase 3+).
    Fixed,
}

/// Descriptive metadata. Everything except `title` is optional because
/// real-world files are frequently incomplete.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub published: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    #[serde(default)]
    pub identifiers: Vec<String>,
}

/// One chapter in reading order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpineItem {
    /// Package-relative path, e.g. `content/c0001.xhtml`.
    pub href: String,
    /// Human title (taken from the TOC when available).
    pub title: Option<String>,
    /// Text length in UTF-16 code units — matches DOM `textContent.length`,
    /// so progress estimates computed in Rust agree with the UI.
    pub chars: u64,
    /// Number of `data-jrb` blocks stamped in this chapter.
    pub blocks: u32,
    /// `false` for auxiliary content (EPUB `linear="no"`).
    pub linear: bool,
}

/// A table-of-contents node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TocEntry {
    pub title: String,
    /// Package-relative target with optional fragment,
    /// e.g. `content/c0003.xhtml#sec2`. `None` for pure grouping nodes.
    pub href: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TocEntry>,
}

/// A non-chapter file inside the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub href: String,
    pub media_type: String,
}

/// Provenance of the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    /// Adapter id that produced the package (`epub`, `txt`, …).
    pub format: String,
    /// Original file name (no directories).
    pub file_name: String,
    /// Hex SHA-256 of the original file — used for duplicate detection.
    pub sha256: String,
    /// Version of the converter, so stale packages can be re-converted.
    pub converter_version: String,
}

impl Manifest {
    /// Total text length of the book in UTF-16 code units.
    pub fn total_chars(&self) -> u64 {
        self.spine.iter().map(|s| s.chars).sum()
    }

    /// Index of the spine item whose href equals `href` (fragment ignored).
    pub fn spine_index_of(&self, href: &str) -> Option<usize> {
        let path = href.split('#').next().unwrap_or(href);
        self.spine.iter().position(|s| s.href == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manifest {
        Manifest {
            schema_version: SCHEMA_VERSION,
            id: "00000000-0000-4000-8000-000000000000".into(),
            layout: Layout::Reflowable,
            metadata: Metadata {
                title: "T".into(),
                ..Default::default()
            },
            spine: vec![
                SpineItem {
                    href: "content/c0001.xhtml".into(),
                    title: None,
                    chars: 10,
                    blocks: 2,
                    linear: true,
                },
                SpineItem {
                    href: "content/c0002.xhtml".into(),
                    title: None,
                    chars: 30,
                    blocks: 3,
                    linear: true,
                },
            ],
            toc: vec![],
            resources: vec![],
            cover: None,
            source: SourceInfo {
                format: "txt".into(),
                file_name: "t.txt".into(),
                sha256: "00".into(),
                converter_version: "1".into(),
            },
            fixed: None,
        }
    }

    #[test]
    fn json_round_trip_uses_camel_case() {
        // STEP 1: serialize and check the wire format is camelCase (the UI relies on it).
        let m = sample();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"schemaVersion\":1"));
        // STEP 2: and that it deserializes back to an identical value.
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn helpers() {
        let m = sample();
        assert_eq!(m.total_chars(), 40);
        assert_eq!(m.spine_index_of("content/c0002.xhtml#x"), Some(1));
        assert_eq!(m.spine_index_of("content/nope.xhtml"), None);
    }
}
