// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! PDF adapter.
//!
//! PDFs are fixed-layout and are **rendered by PDF.js in the UI** — we never
//! write a PDF engine. Import only validates the file, refuses DRM, reads
//! metadata and the page count (with `lopdf`), and stores the PDF unchanged
//! as `content/book.pdf`. The cover is rendered later by the UI from page 1.
//!
//! Encryption (ISO 32000-1 §7.6):
//! * a *DRM* security handler (`/Filter` other than `/Standard`, e.g.
//!   Adobe ADEPT's `/EBX_HANDLER`) → refused as DRM-protected;
//! * the `/Standard` handler with an *empty user password* (common:
//!   "permissions-only" PDFs) → opens normally;
//! * the `/Standard` handler with a real user password → refused with a
//!   "password-protected" message.

use std::path::Path;

use justbook::{FixedContent, FixedKind, JustBookWriter, Layout, Metadata};
use lopdf::{Document, Object};

use crate::epub::file_stem;
use crate::{
    BookContent, Confidence, DrmStatus, FormatAdapter, FormatError, FormatFamily, Inspection,
    Result,
};

/// Largest PDF we import.
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Known DRM security handlers (`/Filter` values) and their display names.
const DRM_FILTERS: &[(&str, &str)] = &[
    ("EBX_HANDLER", "Adobe ADEPT"),
    ("Adobe.APS", "Adobe LiveCycle Rights Management"),
    ("FOPN_fLock", "FileOpen"),
    ("FOPN_foweb", "FileOpen"),
    ("MicrosoftIRMServices", "Microsoft IRM"),
    ("Microsoft.IRM", "Microsoft IRM"),
    ("LockLizard", "Locklizard"),
];

pub struct PdfAdapter;

impl FormatAdapter for PdfAdapter {
    fn id(&self) -> &'static str {
        "pdf"
    }

    fn family(&self) -> FormatFamily {
        FormatFamily::Fixed
    }

    fn sniff(&self, head: &[u8], _ext: Option<&str>) -> Confidence {
        // The header may be preceded by junk; readers search the first 1 KiB.
        let window = &head[..head.len().min(1024)];
        if window.windows(5).any(|w| w == b"%PDF-") {
            Confidence::High
        } else {
            Confidence::No
        }
    }

    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let bytes = read_limited(src)?;
        if let Some(scheme) = drm_scheme(&bytes) {
            return Ok(Inspection {
                metadata: Metadata {
                    title: file_stem(src),
                    ..Default::default()
                },
                drm: DrmStatus::Protected(scheme),
            });
        }
        let doc = load(&bytes)?;
        Ok(Inspection {
            metadata: metadata(&doc, src),
            drm: DrmStatus::None,
        })
    }

    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        // STEP 1: read and refuse DRM before parsing anything else.
        let bytes = read_limited(src)?;
        if let Some(scheme) = drm_scheme(&bytes) {
            return Err(FormatError::DrmProtected(scheme));
        }

        // STEP 2: parse (opens empty-user-password PDFs; fails on real passwords).
        let doc = load(&bytes)?;
        let page_count = doc.get_pages().len() as u32;
        if page_count == 0 {
            return Err(FormatError::Invalid {
                format: "PDF",
                reason: "the document has no pages".into(),
            });
        }

        // STEP 3: store the file unchanged; PDF.js renders it.
        let document = out.write_file("content/book.pdf", &bytes)?;
        Ok(BookContent {
            layout: Layout::Fixed,
            fixed: Some(FixedContent {
                kind: FixedKind::Pdf,
                page_count,
                document: Some(document),
                pages: vec![],
                right_to_left: false,
            }),
            // content/book.pdf *is* the original; don't store it twice.
            keep_original: false,
            metadata: metadata(&doc, src),
            spine: vec![],
            toc: vec![],
            resources: vec![],
            cover: None,
            warnings: vec![],
        })
    }
}

fn read_limited(src: &Path) -> Result<Vec<u8>> {
    let size = std::fs::metadata(src)?.len();
    if size > MAX_BYTES {
        return Err(FormatError::TooLarge(format!("{size} bytes")));
    }
    Ok(std::fs::read(src)?)
}

/// Parse with lopdf, mapping failures to user-facing errors.
fn load(bytes: &[u8]) -> Result<Document> {
    match Document::load_mem(bytes) {
        Ok(doc) => Ok(doc),
        // An /Encrypt dictionary we could not open with the empty password.
        Err(_) if contains(bytes, b"/Encrypt") => Err(FormatError::PasswordProtected),
        Err(e) => Err(FormatError::Invalid {
            format: "PDF",
            reason: e.to_string(),
        }),
    }
}

/// Name of a DRM scheme if the encryption dictionary uses a non-standard handler.
///
/// This scans for `/Filter /<Handler>` byte patterns; the encryption
/// dictionary is never inside a compressed object stream (ISO 32000-1
/// §7.5.7), so a byte scan is reliable here and works even when the file
/// can't be parsed without the key.
pub fn drm_scheme(bytes: &[u8]) -> Option<String> {
    if !contains(bytes, b"/Encrypt") {
        return None;
    }
    DRM_FILTERS
        .iter()
        .find(|(filter, _)| contains(bytes, format!("/{filter}").as_bytes()))
        .map(|(_, name)| name.to_string())
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Title/author/subject from the document information dictionary.
fn metadata(doc: &Document, src: &Path) -> Metadata {
    // STEP 1: find /Info in the trailer (it is usually a reference).
    let info = doc.trailer.get(b"Info").ok().and_then(|o| match o {
        Object::Reference(id) => doc.get_dictionary(*id).ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    });
    let field = |key: &[u8]| -> Option<String> {
        let v = info?.get(key).ok()?;
        let s = match v {
            Object::String(bytes, _) => decode_text_string(bytes),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::String(bytes, _) => decode_text_string(bytes),
                _ => return None,
            },
            _ => return None,
        };
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    };

    // STEP 2: many PDFs carry junk titles such as "Microsoft Word - x.docx"; keep them
    // only when they look like a real title, otherwise use the file name.
    let title = field(b"Title").filter(|t| {
        !t.starts_with("Microsoft Word") && !t.ends_with(".doc") && !t.ends_with(".docx")
    });
    Metadata {
        title: title.unwrap_or_else(|| file_stem(src)),
        authors: field(b"Author")
            .map(|a| {
                a.split([';', '&'])
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        description: field(b"Subject"),
        ..Default::default()
    }
}

/// Decode a PDF "text string": UTF-16BE or UTF-8 with BOM, else PDFDocEncoding
/// (which matches Latin-1 for all printable characters we care about).
pub fn decode_text_string(b: &[u8]) -> String {
    if let Some(rest) = b.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    b.iter().map(|&c| c as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_strings() {
        assert_eq!(
            decode_text_string(&[0xFE, 0xFF, 0x00, 0x48, 0x00, 0xE9]),
            "Hé"
        );
        assert_eq!(decode_text_string(b"Caf\xe9"), "Café");
        assert_eq!(decode_text_string(&[0xEF, 0xBB, 0xBF, b'o', b'k']), "ok");
    }

    #[test]
    fn drm_detection_by_handler() {
        assert_eq!(
            drm_scheme(b"trailer << /Encrypt 5 0 R >> 5 0 obj << /Filter /EBX_HANDLER >>")
                .as_deref(),
            Some("Adobe ADEPT")
        );
        assert_eq!(
            drm_scheme(b"trailer << /Encrypt 5 0 R >> 5 0 obj << /Filter /Standard >>"),
            None
        );
        assert_eq!(
            drm_scheme(b"no encryption /EBX_HANDLER mentioned in text"),
            None
        );
    }
}
