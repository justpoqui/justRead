// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Table of contents: EPUB 3 navigation document and EPUB 2 NCX.
//!
//! Both are parsed into the same raw tree; hrefs are resolved against the
//! document that contains them and mapped to package paths by the caller.

use justbook::paths::{fragment, resolve_reference};
use justbook::TocEntry;
use quick_xml::events::Event;
use quick_xml::Reader;

use super::opf::attr;
use crate::entities;
use crate::html::collapse_ws;

/// TOC node before href mapping. `href` is a resolved container path plus fragment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawEntry {
    pub title: String,
    pub href: Option<String>,
    pub children: Vec<RawEntry>,
}

/// Parse an EPUB 3 nav document. Picks the `<nav epub:type="toc">`, or the
/// first `<nav>` if none is typed.
pub fn parse_nav(xhtml: &str, doc_path: &str) -> Vec<RawEntry> {
    // STEP 1: collect every <nav> with its epub:type and entry tree.
    let mut r = Reader::from_str(xhtml);
    {
        let c = r.config_mut();
        c.check_end_names = false;
        c.allow_unmatched_ends = true;
        c.allow_dangling_amp = true;
    }
    let mut navs: Vec<(String, Vec<RawEntry>)> = Vec::new();
    let mut nav_type: Option<String> = None;
    let mut roots: Vec<RawEntry> = Vec::new();
    let mut items: Vec<RawEntry> = Vec::new();
    let mut capturing = 0usize;

    loop {
        let ev = match r.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(ev) => ev,
        };
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = String::from_utf8_lossy(e.local_name().as_ref().as_bytes())
                    .to_ascii_lowercase();
                match name.as_str() {
                    "nav" if !empty => {
                        nav_type = Some(attr(e, "epub:type").unwrap_or_default());
                        roots.clear();
                    }
                    "li" if nav_type.is_some() && !empty => items.push(RawEntry::default()),
                    "a" | "span" if nav_type.is_some() && !items.is_empty() => {
                        if name == "a" {
                            if let Some(h) = attr(e, "href") {
                                items.last_mut().unwrap().href =
                                    resolve_with_fragment(doc_path, &h);
                            }
                        }
                        if !empty {
                            capturing += 1;
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(ref t) if capturing > 0 => {
                if let Some(it) = items.last_mut() {
                    it.title.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(ref g) if capturing > 0 => {
                let n: &str = g;
                if let Some(it) = items.last_mut() {
                    it.title.push_str(&entities::resolve(n).unwrap_or_default());
                }
            }
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref().as_bytes())
                    .to_ascii_lowercase();
                match name.as_str() {
                    "a" | "span" if capturing > 0 => capturing -= 1,
                    "li" if nav_type.is_some() => {
                        // STEP 2: a finished <li> attaches to its parent item (or the root).
                        if let Some(mut done) = items.pop() {
                            done.title = collapse_ws(&done.title);
                            match items.last_mut() {
                                Some(parent) => parent.children.push(done),
                                None => roots.push(done),
                            }
                        }
                    }
                    "nav" => {
                        if let Some(t) = nav_type.take() {
                            navs.push((t, std::mem::take(&mut roots)));
                        }
                        items.clear();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // STEP 3: prefer the "toc" nav.
    let pick = navs
        .iter()
        .position(|(t, _)| t.split_whitespace().any(|x| x == "toc"))
        .unwrap_or(0);
    navs.into_iter()
        .nth(pick)
        .map(|(_, e)| e)
        .unwrap_or_default()
}

/// Parse an EPUB 2 NCX (`navMap/navPoint`).
pub fn parse_ncx(xml: &str, doc_path: &str) -> Vec<RawEntry> {
    let mut r = Reader::from_str(xml);
    r.config_mut().check_end_names = false;
    let mut roots = Vec::new();
    let mut items: Vec<RawEntry> = Vec::new();
    let mut in_label = false;
    let mut in_text = false;

    loop {
        let ev = match r.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(ev) => ev,
        };
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                match e.local_name().as_ref() {
                    "navPoint" if !empty => items.push(RawEntry::default()),
                    "navLabel" if !empty => in_label = true,
                    "text" if in_label && !empty => in_text = true,
                    "content" => {
                        if let (Some(it), Some(src)) = (items.last_mut(), attr(e, "src")) {
                            if it.href.is_none() {
                                it.href = resolve_with_fragment(doc_path, &src);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(ref t) if in_text => {
                if let Some(it) = items.last_mut() {
                    it.title.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(ref g) if in_text => {
                let n: &str = g;
                if let Some(it) = items.last_mut() {
                    it.title.push_str(&entities::resolve(n).unwrap_or_default());
                }
            }
            Event::End(ref e) => match e.local_name().as_ref() {
                "text" => in_text = false,
                "navLabel" => in_label = false,
                "navPoint" => {
                    if let Some(mut done) = items.pop() {
                        done.title = collapse_ws(&done.title);
                        match items.last_mut() {
                            Some(parent) => parent.children.push(done),
                            None => roots.push(done),
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    roots
}

/// Resolve `href` against `doc_path`, keeping the fragment.
fn resolve_with_fragment(doc_path: &str, href: &str) -> Option<String> {
    let path = resolve_reference(doc_path, href).ok()?;
    Some(match fragment(href) {
        Some(f) => format!("{path}#{f}"),
        None => path,
    })
}

/// Convert raw entries to manifest entries using `map` (container path →
/// package chapter path). Unmappable targets keep their title but lose the
/// link; empty, link-less leaves are dropped.
pub fn map_entries(raw: &[RawEntry], map: &dyn Fn(&str) -> Option<String>) -> Vec<TocEntry> {
    raw.iter()
        .filter_map(|e| {
            let href = e.href.as_ref().and_then(|h| {
                let (path, frag) = match h.split_once('#') {
                    Some((p, f)) => (p, Some(f)),
                    None => (h.as_str(), None),
                };
                let pkg = map(path)?;
                Some(match frag {
                    Some(f) => format!("{pkg}#{f}"),
                    None => pkg,
                })
            });
            let children = map_entries(&e.children, map);
            if href.is_none() && children.is_empty() {
                return None;
            }
            let title = if e.title.is_empty() {
                "Untitled".to_string()
            } else {
                e.title.clone()
            };
            Some(TocEntry {
                title,
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
    fn nav_document() {
        let x = r##"<html xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="landmarks"><ol><li><a href="x.xhtml">Landmark</a></li></ol></nav>
<nav epub:type="toc"><h1>Contents</h1><ol>
 <li><a href="text/c1.xhtml">One &amp; Only</a></li>
 <li><span>Part II</span><ol><li><a href="text/c2.xhtml#s1">Two <em>b</em></a></li></ol></li>
</ol></nav></body></html>"##;
        let t = parse_nav(x, "OEBPS/nav.xhtml");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].title, "One & Only");
        assert_eq!(t[0].href.as_deref(), Some("OEBPS/text/c1.xhtml"));
        assert_eq!(t[1].title, "Part II");
        assert_eq!(t[1].href, None);
        assert_eq!(t[1].children[0].title, "Two b");
        assert_eq!(
            t[1].children[0].href.as_deref(),
            Some("OEBPS/text/c2.xhtml#s1")
        );
    }

    #[test]
    fn ncx_document() {
        let x = r#"<ncx><navMap>
<navPoint id="a" playOrder="1"><navLabel><text>First</text></navLabel><content src="c1.html"/>
  <navPoint id="b" playOrder="2"><navLabel><text>Sub</text></navLabel><content src="c1.html#x"/></navPoint>
</navPoint>
<navPoint id="c"><navLabel><text>Second</text></navLabel><content src="c2.html"/></navPoint>
</navMap></ncx>"#;
        let t = parse_ncx(x, "toc.ncx");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].children[0].href.as_deref(), Some("c1.html#x"));
        assert_eq!(t[1].title, "Second");
    }

    #[test]
    fn mapping_drops_dead_leaves() {
        let raw = vec![
            RawEntry {
                title: "A".into(),
                href: Some("a.html#f".into()),
                children: vec![],
            },
            RawEntry {
                title: "Dead".into(),
                href: Some("gone.html".into()),
                children: vec![],
            },
        ];
        let map = |p: &str| (p == "a.html").then(|| "content/c0001.xhtml".to_string());
        let out = map_entries(&raw, &map);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].href.as_deref(), Some("content/c0001.xhtml#f"));
    }
}
