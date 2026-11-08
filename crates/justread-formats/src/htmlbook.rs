// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Building a justBook from *generated* HTML.
//!
//! MOBI/AZW3, FB2, DOCX and plain HTML files are each first translated into
//! a handful of "virtual" HTML documents plus resources (images, CSS) and a
//! TOC. This module turns that into a package, using the same hardened
//! normalizer as EPUB:
//!
//! 1. resources are written under `resources/` (CSS scrubbed);
//! 2. each document is normalized and, if long, split into several chapters;
//! 3. internal links were emitted as placeholders and are resolved now that
//!    every chapter and anchor id is known (a target may sit in a split part);
//! 4. the TOC is mapped to chapter paths, or synthesized from headings.

use std::collections::HashMap;

use justbook::paths::{normalize_relative, relative_path};
use justbook::{JustBookWriter, Layout, Metadata, Resource, SpineItem, TocEntry};

use crate::css::scrub_stylesheet;
use crate::epub::toc::RawEntry;
use crate::html::{normalize_parts, LinkContext, NormalizeOptions, LINK_END, LINK_HREF, LINK_PKG};
use crate::{BookContent, FormatError, Result};

/// Default chapter size target for generated books (UTF-16 units).
pub const SPLIT_CHARS: u64 = 60_000;

/// One generated HTML document. `path` is virtual: it only has to be
/// unique and consistent with the hrefs used inside the documents.
pub struct VirtualDoc {
    pub path: String,
    pub html: String,
}

/// A resource referenced by the documents under its virtual `path`.
pub struct VirtualResource {
    pub path: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

/// Everything a converter produces before packaging.
pub struct HtmlBook {
    pub format: &'static str,
    pub metadata: Metadata,
    pub docs: Vec<VirtualDoc>,
    pub resources: Vec<VirtualResource>,
    /// Entries point at `<virtual doc path>#<fragment>`.
    pub toc: Vec<RawEntry>,
    /// Virtual path of the cover image, if any.
    pub cover: Option<String>,
    pub warnings: Vec<String>,
}

/// Package an [`HtmlBook`]. See module docs.
pub fn write_html_book(book: HtmlBook, out: &mut JustBookWriter) -> Result<BookContent> {
    let mut warnings = book.warnings;

    // STEP 1: resources → resources/<virtual path>.
    let mut res_map: HashMap<String, String> = HashMap::new();
    let mut resources = Vec::new();
    for r in book.resources {
        let Ok(path) = normalize_relative(&r.path) else {
            warnings.push(format!("skipped resource with unsafe path {}", r.path));
            continue;
        };
        // Resources need an extension: the protocol serves MIME types by extension
        // (with nosniff), and FB2/MOBI image ids often have none.
        let target = match (crate::images::extension(&path), ext_for(&r.media_type)) {
            (Some(_), _) | (None, None) => format!("resources/{path}"),
            (None, Some(ext)) => format!("resources/{path}.{ext}"),
        };
        let bytes = if r.media_type == "text/css" {
            scrub_stylesheet(&crate::decode_text(&r.bytes)).into_bytes()
        } else {
            r.bytes
        };
        let href = out.write_file(&target, &bytes)?;
        res_map.insert(path, href.clone());
        resources.push(Resource {
            href,
            media_type: r.media_type,
        });
    }

    // STEP 2: normalize every document; long ones become several chapters.
    struct Chapter {
        path: String,
        xhtml: String,
        title: Option<String>,
        chars: u64,
        blocks: u32,
    }
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut first_part: HashMap<String, String> = HashMap::new(); // doc → first chapter
    let mut id_part: HashMap<(String, String), String> = HashMap::new(); // (doc, id) → chapter
    let map = |p: &str| res_map.get(p).cloned();
    for doc in &book.docs {
        let Ok(doc_path) = normalize_relative(&doc.path) else {
            continue;
        };
        let out_path = format!("content/c{:04}.xhtml", chapters.len() + 1);
        let ctx = LinkContext {
            source_path: &doc_path,
            out_path: &out_path,
            map: &map,
            default_lang: book.metadata.language.as_deref(),
        };
        let parts = normalize_parts(
            &doc.html,
            &ctx,
            None,
            NormalizeOptions {
                split_chars: Some(SPLIT_CHARS),
                defer_links: true,
            },
        );
        for part in parts {
            let path = format!("content/c{:04}.xhtml", chapters.len() + 1);
            first_part
                .entry(doc_path.clone())
                .or_insert_with(|| path.clone());
            for id in part.ids {
                id_part
                    .entry((doc_path.clone(), id))
                    .or_insert_with(|| path.clone());
            }
            let n = part.normalized;
            warnings.extend(n.warnings);
            chapters.push(Chapter {
                path,
                xhtml: n.xhtml,
                title: n.first_heading,
                chars: n.stats.chars,
                blocks: n.stats.blocks,
            });
        }
    }
    if chapters.is_empty() {
        return Err(FormatError::Invalid {
            format: book.format,
            reason: "no readable text".into(),
        });
    }

    // STEP 3: resolve link placeholders and write chapters.
    let resolve = |target: &str| -> Option<String> {
        let (doc, frag) = target.split_once('#').unwrap_or((target, ""));
        let chapter = if frag.is_empty() {
            first_part.get(doc)
        } else {
            id_part
                .get(&(doc.to_string(), frag.to_string()))
                .or_else(|| first_part.get(doc))
        }?;
        Some(if frag.is_empty() {
            chapter.clone()
        } else {
            format!("{chapter}#{frag}")
        })
    };
    let mut spine = Vec::new();
    for ch in &chapters {
        let xhtml = resolve_placeholders(&ch.xhtml, &ch.path, &resolve);
        out.write_file(&ch.path, xhtml.as_bytes())?;
        spine.push(SpineItem {
            href: ch.path.clone(),
            title: ch.title.clone(),
            chars: ch.chars,
            blocks: ch.blocks,
            linear: true,
        });
    }

    // STEP 4: TOC — mapped from the converter's entries, else from headings.
    let mut toc = map_toc(&book.toc, &resolve);
    if toc.is_empty() {
        toc = spine
            .iter()
            .enumerate()
            .filter(|(i, s)| s.title.is_some() || *i == 0)
            .map(|(i, s)| TocEntry {
                title: s.title.clone().unwrap_or_else(|| format!("Part {}", i + 1)),
                href: Some(s.href.clone()),
                children: vec![],
            })
            .collect();
    }

    // STEP 5: cover.
    let cover = match book
        .cover
        .as_deref()
        .and_then(|c| normalize_relative(c).ok())
        .and_then(|c| res_map.get(&c).cloned())
    {
        Some(href) => {
            let bytes = std::fs::read(out.working_dir().join(&href))?;
            let ext = href
                .rsplit('.')
                .next()
                .unwrap_or("jpg")
                .to_ascii_lowercase();
            Some(out.write_file(&format!("cover.{ext}"), &bytes)?)
        }
        None => None,
    };

    Ok(BookContent {
        layout: Layout::Reflowable,
        fixed: None,
        keep_original: true,
        metadata: book.metadata,
        spine,
        toc,
        resources,
        cover,
        warnings,
    })
}

/// File extension for a resource media type.
fn ext_for(media_type: &str) -> Option<&'static str> {
    Some(match media_type {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "text/css" => "css",
        _ => return None,
    })
}

/// Replace `@@JRH:target@@` / `@@JRP:target@@` with real hrefs for the
/// chapter at `from`. Unresolvable links become inert (`href=""`).
fn resolve_placeholders(
    xhtml: &str,
    from: &str,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(xhtml.len());
    let mut rest = xhtml;
    loop {
        // STEP 1: next placeholder of either kind.
        let next = [LINK_HREF, LINK_PKG]
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, *m)))
            .min_by_key(|(i, _)| *i);
        let Some((i, marker)) = next else { break };
        out.push_str(&rest[..i]);
        let after = &rest[i + marker.len()..];
        let Some(end) = after.find(LINK_END) else {
            out.push_str(&rest[i..]);
            return out;
        };
        // STEP 2: the target text was XML-escaped when written; '&' is the only
        // character a path could contain that changes, so unescape just that.
        let target = after[..end].replace("&amp;", "&");
        let value = match resolve(&target) {
            Some(pkg) if marker == LINK_PKG => pkg,
            Some(pkg) => {
                let (path, frag) = pkg
                    .split_once('#')
                    .map(|(p, f)| (p, Some(f)))
                    .unwrap_or((pkg.as_str(), None));
                let rel = if path == from {
                    String::new()
                } else {
                    relative_path(from, path)
                };
                match frag {
                    Some(f) => format!("{rel}#{f}"),
                    None => rel,
                }
            }
            None => String::new(),
        };
        justbook::xhtml::escape_into(&mut out, &value, true);
        rest = &after[end + LINK_END.len()..];
    }
    out.push_str(rest);
    out
}

/// Map raw TOC entries (virtual hrefs) to chapter hrefs.
fn map_toc(raw: &[RawEntry], resolve: &dyn Fn(&str) -> Option<String>) -> Vec<TocEntry> {
    raw.iter()
        .filter_map(|e| {
            let href = e.href.as_deref().and_then(|h| {
                let (doc, frag) = h.split_once('#').unwrap_or((h, ""));
                let doc = normalize_relative(doc).ok()?;
                resolve(&format!("{doc}#{frag}"))
            });
            let children = map_toc(&e.children, resolve);
            if href.is_none() && children.is_empty() {
                return None;
            }
            Some(TocEntry {
                title: if e.title.is_empty() {
                    "Untitled".into()
                } else {
                    e.title.clone()
                },
                href,
                children,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_book_with_cross_document_links() {
        // STEP 1: two virtual docs, one image, a TOC entry pointing at an anchor.
        let tmp = tempfile::tempdir().unwrap();
        let mut w = JustBookWriter::create(&tmp.path().join("b")).unwrap();
        let book = HtmlBook {
            format: "test",
            metadata: Metadata { title: "T".into(), ..Default::default() },
            docs: vec![
                VirtualDoc { path: "a.html".into(), html: r#"<html><body><h1>One</h1><p><a href="b.html#n1">note</a> <img src="img/x.png"/></p></body></html>"#.into() },
                VirtualDoc { path: "b.html".into(), html: r#"<html><body><h1>Two</h1><p id="n1">the note</p><p><a href="nowhere.html">dead</a></p></body></html>"#.into() },
            ],
            resources: vec![VirtualResource { path: "img/x.png".into(), media_type: "image/png".into(), bytes: b"\x89PNG".to_vec() }],
            toc: vec![RawEntry { title: "Notes".into(), href: Some("b.html#n1".into()), children: vec![] }],
            cover: Some("img/x.png".into()),
            warnings: vec![],
        };
        let c = write_html_book(book, &mut w).unwrap();

        // STEP 2: chapters, resolved links, TOC and cover.
        assert_eq!(c.spine.len(), 2);
        assert_eq!(c.spine[0].title.as_deref(), Some("One"));
        let a = std::fs::read_to_string(w.working_dir().join("content/c0001.xhtml")).unwrap();
        assert!(
            a.contains(r##"href="c0002.xhtml#n1" data-jr-href="content/c0002.xhtml#n1""##),
            "{a}"
        );
        assert!(a.contains(r#"src="../resources/img/x.png""#));
        let b = std::fs::read_to_string(w.working_dir().join("content/c0002.xhtml")).unwrap();
        assert!(b.contains(r#"<a href="" data-jr-href="">dead</a>"#), "{b}");
        assert!(!b.contains("@@"));
        assert_eq!(c.toc[0].href.as_deref(), Some("content/c0002.xhtml#n1"));
        assert_eq!(c.cover.as_deref(), Some("cover.png"));
    }
}
