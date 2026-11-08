// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Mobipocket / Kindle formats: MOBI, PRC, AZW (MOBI 6), AZW3 / KF8, and
//! plain PalmDOC (`.pdb` TEXtREAd).
//!
//! A file is a Palm database. Record 0 holds headers; then compressed text
//! records, images and index records follow. "Combo" files contain a MOBI 6
//! section *and* a KF8 section (EXTH 121 points at the KF8 header); we
//! prefer KF8. DRM-encrypted books are refused, never decrypted.
//! Format references: MobileRead wiki "MOBI", "KF8", "PalmDOC".

pub mod compression;
pub mod header;
pub mod indx;
mod kf8;
mod mobi6;
pub mod palmdb;

use std::path::Path;

use justbook::{JustBookWriter, Metadata};

use self::compression::{palmdoc, trailing_size, Huff};
use self::header::{Header, NONE};
use self::palmdb::PalmDb;
use crate::epub::file_stem;
use crate::htmlbook::write_html_book;
use crate::{
    BookContent, Confidence, DrmStatus, FormatAdapter, FormatError, FormatFamily, Inspection,
    Result,
};

const MAX_BYTES: u64 = 512 * 1024 * 1024;

pub struct MobiAdapter;

impl FormatAdapter for MobiAdapter {
    fn id(&self) -> &'static str {
        "mobi"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Reflowable
    }
    fn sniff(&self, head: &[u8], _ext: Option<&str>) -> Confidence {
        match head.get(60..68) {
            Some(b"BOOKMOBI") | Some(b"TEXtREAd") => Confidence::High,
            _ => Confidence::No,
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let book = Book::open(src, false)?;
        let drm = if book.encrypted() {
            DrmStatus::Protected(book.drm_name())
        } else {
            DrmStatus::None
        };
        Ok(Inspection {
            metadata: book.metadata(src),
            drm,
        })
    }
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        let book = Book::open(src, true)?;
        let html = if !book.h0.has_mobi {
            palmdoc_text(&book, src)?
        } else if book.kf8 {
            kf8::convert(&book, src)?
        } else {
            mobi6::convert(&book, src)?
        };
        write_html_book(html, out)
    }
}

/// Plain PalmDOC (TEXtREAd) books hold plain text: reuse the TXT heuristics.
fn palmdoc_text(book: &Book, src: &Path) -> Result<crate::htmlbook::HtmlBook> {
    use justbook::xhtml::escape;
    let text = encoding_rs::WINDOWS_1252
        .decode_without_bom_handling(&book.text()?)
        .0
        .replace("\r\n", "\n");
    let (mut metadata, chapters) = crate::txt::parse_document(&text, &file_stem(src));
    let name = String::from_utf8_lossy(&book.db.data[..32])
        .trim_end_matches('\0')
        .trim()
        .to_string();
    if metadata.title == file_stem(src) && !name.is_empty() {
        metadata.title = name;
    }
    let docs = chapters
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut body = String::new();
            if let Some(h) = &c.heading {
                body.push_str(&format!("<h2>{}</h2>", escape(h)));
            }
            for p in &c.paragraphs {
                body.push_str(&format!("<p>{}</p>", escape(p)));
            }
            crate::htmlbook::VirtualDoc {
                path: format!("pdb/c{:04}.html", i + 1),
                html: format!("<html><body>{body}</body></html>"),
            }
        })
        .collect();
    Ok(crate::htmlbook::HtmlBook {
        format: "PDB",
        metadata,
        docs,
        resources: vec![],
        toc: vec![],
        cover: None,
        warnings: vec![],
    })
}

/// An opened Mobipocket book: the database plus the section we read.
pub struct Book {
    pub db: PalmDb,
    /// Record index of the header we use (0, or the KF8 boundary).
    pub base: usize,
    pub h: Header,
    /// Header of record 0 (MOBI 6 part / metadata of combo files).
    pub h0: Header,
    pub kf8: bool,
}

impl Book {
    /// Open and pick the section; `refuse_drm` makes encrypted books an error.
    pub fn open(src: &Path, refuse_drm: bool) -> Result<Self> {
        // STEP 1: read with a size limit and parse the container.
        if std::fs::metadata(src)?.len() > MAX_BYTES {
            return Err(FormatError::TooLarge("MOBI file".into()));
        }
        let db = PalmDb::parse(std::fs::read(src)?)?;
        if &db.kind != b"BOOKMOBI" && &db.kind != b"TEXtREAd" {
            return Err(FormatError::Invalid {
                format: "MOBI",
                reason: "not a Mobipocket or PalmDOC book".into(),
            });
        }
        let h0 = Header::parse(db.record(0));

        // STEP 2: DRM — the encryption field is authoritative.
        let book = |base: usize, h: Header, kf8: bool, h0: Header, db: PalmDb| Book {
            db,
            base,
            h,
            h0,
            kf8,
        };
        if refuse_drm && h0.encryption != 0 {
            let b = book(0, h0.clone(), false, h0, db);
            return Err(FormatError::DrmProtected(b.drm_name()));
        }

        // STEP 3: KF8 — either the whole file, or the section after EXTH 121.
        if h0.version >= 8 {
            return Ok(book(0, h0.clone(), true, h0, db));
        }
        if let Some(boundary) = h0
            .exth_u32(121)
            .map(|b| b as usize)
            .filter(|&b| b > 0 && b < db.len())
        {
            let hk = Header::parse(db.record(boundary));
            if hk.has_mobi && hk.version >= 8 {
                return Ok(book(boundary, hk, true, h0, db));
            }
        }
        Ok(book(0, h0.clone(), false, h0, db))
    }

    pub fn encrypted(&self) -> bool {
        self.h0.encryption != 0
    }

    pub fn drm_name(&self) -> String {
        match self.h0.encryption {
            1 => "Mobipocket (old) DRM".into(),
            _ => "Kindle/Mobipocket DRM".into(),
        }
    }

    /// Absolute record number for an index relative to this section.
    pub fn rec(&self, rel: u32) -> Option<usize> {
        (rel != NONE)
            .then(|| self.base + rel as usize)
            .filter(|&r| r < self.db.len())
    }

    /// The decompressed text of this section (raw bytes, book encoding).
    pub fn text(&self) -> Result<Vec<u8>> {
        // STEP 1: a Huffman decoder if the book uses HUFF/CDIC.
        let mut huff = if self.h.compression == 17480 {
            let first = self.rec(self.h.huff_offset).ok_or(FormatError::Invalid {
                format: "MOBI",
                reason: "missing HUFF record".into(),
            })?;
            let cdics: Vec<&[u8]> = (1..self.h.huff_count as usize)
                .map(|i| self.db.record(first + i))
                .collect();
            Some(Huff::new(self.db.record(first), &cdics)?)
        } else {
            None
        };

        // STEP 2: each text record, minus trailing entries, decompressed.
        let mut out = Vec::new();
        for i in 1..=self.h.text_records as usize {
            let rec = self.db.record(self.base + i);
            let body = &rec[..rec.len() - trailing_size(rec, self.h.extra_flags)];
            match self.h.compression {
                1 => out.extend_from_slice(body),
                2 => out.extend_from_slice(&palmdoc(body)),
                17480 => out.extend_from_slice(&huff.as_mut().unwrap().unpack(body)),
                c => {
                    return Err(FormatError::Invalid {
                        format: "MOBI",
                        reason: format!("unknown compression {c}"),
                    })
                }
            }
        }
        if self.h.text_length > 0 {
            out.truncate(self.h.text_length as usize);
        }
        Ok(out)
    }

    /// Find the record of image `n` (1-based, as used by `recindex` and
    /// `kindle:embed`). Combo files number images from the MOBI 6 part, so
    /// several bases are tried and the record must look like an image.
    pub fn image(&self, n: u32) -> Option<(&[u8], &'static str, &'static str)> {
        let n = n.checked_sub(1)? as usize;
        let mut bases = vec![];
        for fi in [self.h.first_image, self.h0.first_image] {
            if fi != NONE {
                bases.push(self.base + fi as usize);
                bases.push(fi as usize);
            }
        }
        bases
            .into_iter()
            .map(|b| self.db.record(b + n))
            .find_map(|r| image_type(r).map(|(ext, mime)| (r, ext, mime)))
    }

    /// Title, authors and more from the EXTH block (falling back to the name).
    pub fn metadata(&self, src: &Path) -> Metadata {
        let h = &self.h0;
        let title = h
            .exth_strings(503)
            .into_iter()
            .next()
            .or_else(|| Some(h.decode(&h.full_name)).filter(|t| !t.trim().is_empty()))
            .unwrap_or_else(|| file_stem(src));
        let lang = h.exth_strings(524).into_iter().next();
        Metadata {
            title: title.trim().to_string(),
            authors: h.exth_strings(100),
            language: lang,
            publisher: h.exth_strings(101).into_iter().next(),
            description: h
                .exth_strings(103)
                .into_iter()
                .next()
                .map(|d| crate::html::collapse_ws(&strip_tags(&d))),
            published: h.exth_strings(106).into_iter().next(),
            series: None,
            series_index: None,
            identifiers: h.exth_strings(104),
        }
    }

    /// 1-based image number of the cover (EXTH 201 is a 0-based offset).
    pub fn cover_image(&self) -> Option<u32> {
        self.h0.exth_u32(201).filter(|&o| o != NONE).map(|o| o + 1)
    }
}

/// Image type by magic number.
pub fn image_type(r: &[u8]) -> Option<(&'static str, &'static str)> {
    if r.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("jpg", "image/jpeg"))
    } else if r.starts_with(b"\x89PNG") {
        Some(("png", "image/png"))
    } else if r.starts_with(b"GIF8") {
        Some(("gif", "image/gif"))
    } else if r.starts_with(b"BM") && r.len() > 26 {
        Some(("bmp", "image/bmp"))
    } else {
        None
    }
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers shared by the MOBI 6 and KF8 converters
// ---------------------------------------------------------------------------

/// Move an insertion point out of a tag: if `pos` falls between `<` and `>`,
/// return the position of that `<`.
pub fn safe_insert_pos(text: &[u8], pos: usize) -> usize {
    let pos = pos.min(text.len());
    let lt = text[..pos].iter().rposition(|&b| b == b'<');
    let gt = text[..pos].iter().rposition(|&b| b == b'>');
    match (lt, gt) {
        (Some(l), Some(g)) if l > g => l,
        (Some(l), None) => l,
        _ => pos,
    }
}

/// Insert `(position, bytes)` pairs into `text` (positions refer to the
/// original text; processed back to front so they stay valid).
pub fn insert_all(text: &mut Vec<u8>, mut inserts: Vec<(usize, Vec<u8>)>) {
    inserts.sort_by_key(|i| std::cmp::Reverse(i.0));
    for (pos, bytes) in inserts {
        let at = safe_insert_pos(text, pos);
        text.splice(at..at, bytes);
    }
}

/// Find `name=value` attributes (case-insensitive name, quoted or bare
/// value) and replace each whole attribute with `f(value)` (empty = remove).
pub fn replace_attr(html: &str, name: &str, mut f: impl FnMut(&str) -> String) -> String {
    let lower = html.to_ascii_lowercase();
    let needle = format!("{}=", name.to_ascii_lowercase());
    let mut out = String::with_capacity(html.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(i) = lower[from..].find(&needle).map(|i| i + from) {
        // Must be a whole attribute name (preceded by whitespace).
        let preceded = i > 0 && lower.as_bytes()[i - 1].is_ascii_whitespace();
        let vstart = i + needle.len();
        if !preceded || vstart >= html.len() {
            from = vstart.min(html.len());
            continue;
        }
        let bytes = html.as_bytes();
        let (value, end) = match bytes[vstart] {
            q @ (b'"' | b'\'') => {
                let close = html[vstart + 1..]
                    .find(q as char)
                    .map(|j| vstart + 1 + j)
                    .unwrap_or(html.len());
                (&html[vstart + 1..close], (close + 1).min(html.len()))
            }
            _ => {
                let stop = html[vstart..]
                    .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
                    .map(|j| vstart + j)
                    .unwrap_or(html.len());
                (&html[vstart..stop], stop)
            }
        };
        out.push_str(&html[last..i]);
        out.push_str(&f(value));
        last = end;
        from = end;
    }
    out.push_str(&html[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_replacement() {
        let s = r#"<a filepos=0000123>x</a><img recindex="00002" /><p xfilepos=1>"#;
        let r = replace_attr(s, "filepos", |v| {
            format!(r##"href="#fp{}""##, v.trim_start_matches('0'))
        });
        assert_eq!(
            r,
            r##"<a href="#fp123">x</a><img recindex="00002" /><p xfilepos=1>"##
        );
        let r = replace_attr(&r, "recindex", |v| format!(r#"src="img/{v}.jpg""#));
        assert!(r.contains(r#"<img src="img/00002.jpg" />"#));
    }

    #[test]
    fn insertion_avoids_tags() {
        let mut t = b"<p class=x>hello</p>".to_vec();
        insert_all(
            &mut t,
            vec![
                (5, b"<a id=a></a>".to_vec()),
                (13, b"<a id=b></a>".to_vec()),
            ],
        );
        assert_eq!(
            String::from_utf8(t).unwrap(),
            "<a id=a></a><p class=x>he<a id=b></a>llo</p>"
        );
    }
}
