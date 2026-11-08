// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! # justBook
//!
//! justBook is justRead's internal, normalized package format. Every
//! reflowable source format (EPUB, TXT, later MOBI/FB2/DOCX…) is converted
//! into it exactly once, at import time, so the reader only has to render
//! one well-understood thing.
//!
//! A package is a directory:
//!
//! ```text
//! <package>/
//!   manifest.json          metadata, spine, TOC, resources (see `Manifest`)
//!   content/c0001.xhtml    normalized, well-formed XHTML chapters
//!   resources/...          images, fonts and stylesheets
//!   cover.<ext>            optional cover image
//!   original/<file>        the untouched source file (for re-conversion)
//! ```
//!
//! The full specification lives in `docs/JUSTBOOK_SPEC.md`.

pub mod error;
pub mod manifest;
pub mod paths;
pub mod reader;
pub mod writer;
pub mod xhtml;

pub use error::{Error, Result};
pub use manifest::{
    Layout, Manifest, Metadata, Resource, SourceInfo, SpineItem, TocEntry, SCHEMA_VERSION,
};
pub use reader::{load_manifest, resolve_in_package};
pub use writer::JustBookWriter;
pub use xhtml::{ChapterStats, ChapterWriter};
