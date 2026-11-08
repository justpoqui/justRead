// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! # Format adapters
//!
//! Every supported source format is one *adapter* implementing
//! [`FormatAdapter`]. Reflowable adapters convert their input into a
//! justBook package (normalize-on-import); the reader never sees the
//! original format. Adding a format = adding an adapter + registering it in
//! [`Registry::with_defaults`]; nothing else in the app changes.

pub mod archive;
pub mod base64;
pub mod comic;
pub mod css;
pub mod docx;
pub mod entities;
pub mod epub;
pub mod fb2;
pub mod html;
pub mod htmlbook;
pub mod htmlfile;
pub mod images;
pub mod mobi;
pub mod pdf;
pub mod txt;
pub mod unsupported;

use std::fs::File;
use std::io::Read;
use std::path::Path;

use justbook::{FixedContent, JustBookWriter, Layout, Metadata, Resource, SpineItem, TocEntry};
use thiserror::Error;

/// Version of the conversion pipeline, recorded in every manifest so that
/// packages produced by an older converter can be re-converted later.
pub const CONVERTER_VERSION: &str = "1";

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// Reflowable text vs. fixed pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatFamily {
    Reflowable,
    Fixed,
}

/// How sure an adapter is that it can read a file. Higher wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    No = 0,
    /// Only the extension matches.
    Low = 1,
    /// Magic bytes match.
    High = 2,
}

/// DRM state of a file. We only ever *detect* DRM — never remove it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrmStatus {
    None,
    /// Protected; the string names the scheme (e.g. "Adobe ADEPT").
    Protected(String),
}

/// Cheap facts about a file, gathered without converting it.
#[derive(Debug, Clone)]
pub struct Inspection {
    pub metadata: Metadata,
    pub drm: DrmStatus,
}

/// What an adapter produced. The import pipeline turns this into the final
/// `Manifest` (adding the id and provenance).
#[derive(Debug, Clone)]
pub struct BookContent {
    pub layout: Layout,
    /// Page data for fixed-layout formats.
    pub fixed: Option<FixedContent>,
    /// When false, the pipeline does not keep `original/` (the package already
    /// holds the source losslessly, e.g. the PDF itself or all comic pages).
    pub keep_original: bool,
    pub metadata: Metadata,
    pub spine: Vec<SpineItem>,
    pub toc: Vec<TocEntry>,
    pub resources: Vec<Resource>,
    pub cover: Option<String>,
    /// Non-fatal problems found while converting (shown in logs/diagnostics).
    pub warnings: Vec<String>,
}

/// Errors common to all adapters.
#[derive(Debug, Error)]
pub enum FormatError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("this file is DRM-protected ({0}) and can't be opened")]
    DrmProtected(String),
    #[error("unsupported file format")]
    Unsupported,
    #[error("{0}")]
    NotYetSupported(String),
    #[error("this PDF is password-protected; remove the password with the tool that created it, then import it again")]
    PasswordProtected,
    #[error("the file is damaged or not a valid {format}: {reason}")]
    Invalid {
        format: &'static str,
        reason: String,
    },
    #[error("the file exceeds a safety limit: {0}")]
    TooLarge(String),
    #[error(transparent)]
    Package(#[from] justbook::Error),
}

pub type Result<T> = std::result::Result<T, FormatError>;

// ---------------------------------------------------------------------------
// The adapter trait
// ---------------------------------------------------------------------------

/// One input format. Implementations must be stateless and thread-safe.
pub trait FormatAdapter: Send + Sync {
    /// Short stable id stored in the manifest (`"epub"`, `"txt"`).
    fn id(&self) -> &'static str;

    /// Which family this format belongs to.
    fn family(&self) -> FormatFamily;

    /// Decide from the first bytes of the file (up to 4 KiB) and the
    /// lower-case extension whether this adapter handles it.
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence;

    /// Read metadata and DRM status without converting.
    fn inspect(&self, src: &Path) -> Result<Inspection>;

    /// Convert `src` into the package being written by `out`.
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent>;
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// The set of known adapters.
pub struct Registry {
    adapters: Vec<Box<dyn FormatAdapter>>,
}

impl Registry {
    /// All adapters shipped with this build.
    pub fn with_defaults() -> Self {
        Self {
            adapters: vec![
                Box::new(epub::EpubAdapter),
                Box::new(txt::TxtAdapter),
                Box::new(pdf::PdfAdapter),
                Box::new(comic::CbzAdapter),
                Box::new(comic::Cb7Adapter),
                Box::new(comic::CbrAdapter),
                Box::new(mobi::MobiAdapter),
                Box::new(fb2::Fb2Adapter),
                Box::new(docx::DocxAdapter),
                Box::new(htmlfile::HtmlAdapter),
                Box::new(unsupported::DjvuAdapter),
                Box::new(unsupported::KfxAdapter),
            ],
        }
    }

    /// Lower-case extensions accepted by the file picker.
    pub fn extensions(&self) -> Vec<&'static str> {
        vec![
            "epub", "mobi", "azw", "azw3", "prc", "pdb", "fb2", "zip", "docx", "html", "htm",
            "xhtml", "txt", "pdf", "cbz", "cb7", "cbr", "djvu", "kfx",
        ]
    }

    /// Pick the adapter with the highest confidence for `path`.
    pub fn detect(&self, path: &Path) -> Result<&dyn FormatAdapter> {
        // STEP 1: read the first 4 KiB — enough for every magic number we use.
        let mut head = vec![0u8; 4096];
        let n = File::open(path)?.read(&mut head)?;
        head.truncate(n);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());

        // STEP 2: ask every adapter and keep the most confident one.
        self.adapters
            .iter()
            .map(|a| (a.sniff(&head, ext.as_deref()), a))
            .filter(|(c, _)| *c > Confidence::No)
            .max_by_key(|(c, _)| *c)
            .map(|(_, a)| a.as_ref())
            .ok_or(FormatError::Unsupported)
    }

    /// Look an adapter up by id.
    pub fn get(&self, id: &str) -> Option<&dyn FormatAdapter> {
        self.adapters
            .iter()
            .find(|a| a.id() == id)
            .map(|a| a.as_ref())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ---------------------------------------------------------------------------
// Helpers shared by adapters
// ---------------------------------------------------------------------------

/// Decode a text document to UTF-8: BOM first, then an XML/HTML encoding
/// declaration, then strict UTF-8, then statistical detection.
pub fn decode_text(bytes: &[u8]) -> String {
    // STEP 1: a BOM is authoritative.
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc
            .decode_without_bom_handling(&bytes[bom_len..])
            .0
            .into_owned();
    }

    // STEP 2: a declared charset in the first 1 KiB (e.g. `encoding="windows-1252"`).
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    for key in ["encoding=", "charset="] {
        if let Some(i) = head.find(key) {
            let rest = head[i + key.len()..].trim_start_matches(['"', '\'']);
            let label: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || "-_:".contains(*c))
                .collect();
            if let Some(enc) = encoding_rs::Encoding::for_label(label.as_bytes()) {
                if enc != encoding_rs::UTF_8 {
                    return enc.decode_without_bom_handling(bytes).0.into_owned();
                }
            }
        }
    }

    // STEP 3: valid UTF-8 is by far the most common case.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    // STEP 4: guess (chardetng implements the Firefox detector).
    // ISO-2022-JP is denied: it can smuggle markup and is vanishingly rare in books.
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(bytes, true);
    det.guess(None, chardetng::Utf8Detection::Allow)
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}
