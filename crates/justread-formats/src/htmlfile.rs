// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Single HTML files (`.html`, `.htm`, `.xhtml`).
//!
//! The page is normalized like an EPUB chapter (and split into chapters if
//! long). Images and stylesheets it references are imported **only** if they
//! are local files inside the HTML file's own folder (or below it): nothing
//! is fetched from the network, and `../` references can't pull in
//! unrelated files from elsewhere on the disk.

use std::path::Path;

use justbook::paths::{is_external, resolve_reference};
use justbook::{JustBookWriter, Metadata};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::epub::file_stem;
use crate::html::collapse_ws;
use crate::htmlbook::{write_html_book, HtmlBook, VirtualDoc, VirtualResource};
use crate::{
    decode_text, entities, BookContent, Confidence, DrmStatus, FormatAdapter, FormatError,
    FormatFamily, Inspection, Result,
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RESOURCE_BYTES: u64 = 32 * 1024 * 1024;

pub struct HtmlAdapter;

impl FormatAdapter for HtmlAdapter {
    fn id(&self) -> &'static str {
        "html"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Reflowable
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        let binary = head.contains(&0)
            && !head.starts_with(&[0xFF, 0xFE])
            && !head.starts_with(&[0xFE, 0xFF]);
        match ext {
            Some("html" | "htm" | "xhtml") if !binary => Confidence::High,
            _ => Confidence::No,
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let html = read(src)?;
        Ok(Inspection {
            metadata: metadata(&html, src),
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        // STEP 1: read the page; its virtual path is just its file name.
        let html = read(src)?;
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("index.html")
            .to_string();
        let base = src.parent().unwrap_or(Path::new("."));

        // STEP 2: import local images/stylesheets that live inside the page's folder.
        let mut resources = Vec::new();
        let mut warnings = Vec::new();
        for reference in local_references(&html) {
            let Ok(rel) = resolve_reference(&name, &reference) else {
                continue;
            };
            let Some(media_type) = media_type(&rel) else {
                continue;
            };
            if resources.iter().any(|r: &VirtualResource| r.path == rel) {
                continue;
            }
            let file = base.join(&rel);
            match std::fs::metadata(&file) {
                Ok(m) if m.is_file() && m.len() <= MAX_RESOURCE_BYTES => {
                    resources.push(VirtualResource {
                        path: rel,
                        media_type: media_type.into(),
                        bytes: std::fs::read(&file)?,
                    })
                }
                _ => warnings.push(format!("missing or too large: {rel}")),
            }
        }

        // STEP 3: package (normalize, split, TOC from headings).
        write_html_book(
            HtmlBook {
                format: "HTML",
                metadata: metadata(&html, src),
                docs: vec![VirtualDoc { path: name, html }],
                resources,
                toc: vec![],
                cover: None,
                warnings,
            },
            out,
        )
    }
}

fn read(src: &Path) -> Result<String> {
    let size = std::fs::metadata(src)?.len();
    if size > MAX_BYTES {
        return Err(FormatError::TooLarge(format!("{size} bytes")));
    }
    Ok(decode_text(&std::fs::read(src)?))
}

/// `src` of images and `href` of stylesheet links that are not remote.
fn local_references(html: &str) -> Vec<String> {
    let mut r = Reader::from_str(html);
    {
        let c = r.config_mut();
        c.check_end_names = false;
        c.allow_unmatched_ends = true;
        c.allow_dangling_amp = true;
    }
    let mut out = Vec::new();
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref().as_bytes())
                    .to_ascii_lowercase();
                let get = |k: &str| {
                    e.html_attributes()
                        .flatten()
                        .find(|a| a.key.as_ref().eq_ignore_ascii_case(k))
                        .map(|a| entities::unescape(&a.value))
                };
                let reference = match name.as_str() {
                    "img" => get("src"),
                    "image" => get("xlink:href").or_else(|| get("href")),
                    "link"
                        if get("rel")
                            .is_some_and(|r| r.to_ascii_lowercase().contains("stylesheet")) =>
                    {
                        get("href")
                    }
                    _ => None,
                };
                if let Some(v) = reference
                    .filter(|v| !v.is_empty() && !is_external(v) && !v.starts_with("data:"))
                {
                    out.push(v);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Resource types we import, by extension.
fn media_type(path: &str) -> Option<&'static str> {
    Some(match crate::images::extension(path)?.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "css" => "text/css",
        _ => return None,
    })
}

/// `<title>`, `<meta name="author">` and `<html lang>`.
fn metadata(html: &str, src: &Path) -> Metadata {
    let mut r = Reader::from_str(html);
    {
        let c = r.config_mut();
        c.check_end_names = false;
        c.allow_unmatched_ends = true;
        c.allow_dangling_amp = true;
    }
    let mut md = Metadata {
        title: String::new(),
        ..Default::default()
    };
    let mut in_title = false;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref().as_bytes())
                    .to_ascii_lowercase();
                let get = |k: &str| {
                    e.html_attributes()
                        .flatten()
                        .find(|a| a.key.as_ref().eq_ignore_ascii_case(k))
                        .map(|a| entities::unescape(&a.value))
                };
                match name.as_str() {
                    "html" => md.language = get("lang").or_else(|| get("xml:lang")),
                    "title" => in_title = true,
                    "meta" if get("name").is_some_and(|n| n.eq_ignore_ascii_case("author")) => {
                        if let Some(a) = get("content").filter(|a| !a.trim().is_empty()) {
                            md.authors.push(a.trim().to_string());
                        }
                    }
                    "body" => break,
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if in_title => md.title.push_str(&t.xml10_content()),
            Ok(Event::End(e)) if e.local_name().as_ref().eq_ignore_ascii_case("title") => {
                in_title = false
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    md.title = collapse_ws(&md.title);
    if md.title.is_empty() {
        md.title = file_stem(src);
    }
    md
}
