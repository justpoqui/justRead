// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! FictionBook 2 (`.fb2`, `.fb2.zip`).
//!
//! FB2 is a single XML file: `<description>` (metadata), one main `<body>` of
//! nested `<section>`s, optional notes bodies, and `<binary>` base64 images.
//! Each top-level section becomes one HTML document; FB2 elements map to
//! HTML (`title`→`hN`, `epigraph`→`blockquote`, `poem/stanza/v`→`div/p` …).
//! Schema reference: <http://www.fictionbook.org/index.php/Eng:XML_Schema_Fictionbook_2.1>

use std::collections::HashMap;
use std::path::Path;

use justbook::xhtml::escape;
use justbook::{JustBookWriter, Metadata};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::archive::SafeArchive;
use crate::epub::file_stem;
use crate::epub::toc::RawEntry;
use crate::html::collapse_ws;
use crate::htmlbook::{write_html_book, HtmlBook, VirtualDoc, VirtualResource};
use crate::{
    decode_text, entities, BookContent, Confidence, DrmStatus, FormatAdapter, FormatError,
    FormatFamily, Inspection, Result,
};

const MAX_BYTES: u64 = 128 * 1024 * 1024;

/// Styles for FB2 constructs that have no direct HTML equivalent.
const FB2_CSS: &str = "\
.epigraph { margin: 1em 0 1em 30%; font-style: italic; }
.cite { margin: 1em 2em; }
.poem { margin: 1em 0 1em 2em; }
.stanza { margin: 0 0 1em 0; }
.v { margin: 0; text-indent: 0; }
.text-author { text-align: right; font-style: italic; }
.subtitle { text-align: center; font-weight: bold; margin: 1em 0; }
.empty-line { height: 1em; }
.image { text-align: center; text-indent: 0; }
.annotation { font-style: italic; margin: 1em 0; }
";

pub struct Fb2Adapter;

impl FormatAdapter for Fb2Adapter {
    fn id(&self) -> &'static str {
        "fb2"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Reflowable
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        let text = String::from_utf8_lossy(&head[..head.len().min(1024)]);
        // The root element, a .fb2 extension, or a ZIP whose first entry is a .fb2.
        let zipped = ext == Some("zip") && head.starts_with(b"PK\x03\x04") && text.contains(".fb2");
        if text.contains("<FictionBook") || ext == Some("fb2") || zipped {
            Confidence::High
        } else {
            Confidence::No
        }
    }

    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let xml = read(src)?;
        Ok(Inspection {
            metadata: convert_xml(&xml, src).metadata,
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        let xml = read(src)?;
        let book = convert_xml(&xml, src);
        if book.docs.is_empty() {
            return Err(FormatError::Invalid {
                format: "FB2",
                reason: "no <body> found".into(),
            });
        }
        write_html_book(book, out)
    }
}

/// Read a plain `.fb2` or the first `.fb2` inside a ZIP (`.fb2.zip`).
fn read(src: &Path) -> Result<String> {
    if std::fs::metadata(src)?.len() > MAX_BYTES {
        return Err(FormatError::TooLarge("FB2 file".into()));
    }
    let bytes = std::fs::read(src)?;
    if bytes.starts_with(b"PK\x03\x04") {
        let mut ar = SafeArchive::open(src, "FB2")?;
        let name = ar
            .names()
            .find(|n| n.to_ascii_lowercase().ends_with(".fb2"))
            .cloned()
            .ok_or(FormatError::Invalid {
                format: "FB2",
                reason: "no .fb2 file inside the ZIP".into(),
            })?;
        return Ok(decode_text(&ar.read(&name)?));
    }
    Ok(decode_text(&bytes))
}

/// Local name of an attribute key (`l:href` → `href`).
fn attr(e: &BytesStart<'_>, local: &str) -> Option<String> {
    e.html_attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == local)
        .map(|a| entities::unescape(&a.value))
}

/// Translate the XML into an [`HtmlBook`].
pub fn convert_xml(xml: &str, src: &Path) -> HtmlBook {
    let mut c = Converter::default();
    c.run(xml);
    c.finish(src)
}

#[derive(Default)]
struct Converter {
    // ----- metadata -----
    title: String,
    authors: Vec<String>,
    author_parts: Vec<String>,
    lang: Option<String>,
    series: Option<(String, Option<f64>)>,
    annotation: String,
    publisher: Option<String>,
    year: Option<String>,
    cover_id: Option<String>,
    // ----- output -----
    docs: Vec<(String, String, Option<String>)>, // (path, html body, title)
    cur: String,
    cur_title: Option<String>,
    binaries: Vec<VirtualResource>,
    /// FB2 id → document path, to resolve `#id` links across chapters.
    ids: HashMap<String, String>,
    toc: Vec<RawEntry>,
    toc_stack: Vec<RawEntry>,
    // ----- state -----
    path: Vec<String>,
    closers: Vec<&'static str>,
    section_depth: usize,
    body_kind: Option<String>,
    title_lines: usize,
    title_text: String,
    in_binary: Option<(String, String, String)>, // (id, content type, data)
    field: Option<String>,
    section_seq: usize,
    /// Text of the current paragraph (used to name untitled sections).
    para_text: String,
}

impl Converter {
    fn run(&mut self, xml: &str) {
        let mut r = Reader::from_str(xml);
        {
            let c = r.config_mut();
            c.check_end_names = false;
            c.allow_unmatched_ends = true;
            c.allow_dangling_amp = true;
        }
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) => self.start(&e, false),
                Ok(Event::Empty(e)) => {
                    self.start(&e, true);
                    self.end(&String::from_utf8_lossy(e.local_name().as_ref().as_bytes()));
                }
                Ok(Event::End(e)) => {
                    self.end(&String::from_utf8_lossy(e.local_name().as_ref().as_bytes()))
                }
                Ok(Event::Text(t)) => self.text(&t.xml10_content()),
                Ok(Event::CData(t)) => self.text(&String::from_utf8_lossy(t.as_ref().as_bytes())),
                Ok(Event::GeneralRef(g)) => {
                    let n: &str = &g;
                    self.text(&entities::resolve(n).unwrap_or_else(|| format!("&{n};")));
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        self.flush_doc();
    }

    fn in_(&self, name: &str) -> bool {
        self.path.iter().any(|p| p == name)
    }

    fn doc_path(&self) -> String {
        format!("fb2/{:04}.html", self.docs.len() + 1)
    }

    /// Close the current document (if it has content) and start a new one.
    fn flush_doc(&mut self) {
        if self.cur.trim().is_empty() {
            self.cur.clear();
            return;
        }
        let path = self.doc_path();
        self.docs
            .push((path, std::mem::take(&mut self.cur), self.cur_title.take()));
    }

    fn start(&mut self, e: &BytesStart<'_>, empty: bool) {
        let name = String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into_owned();
        self.path.push(name.clone());

        // STEP 1: binaries (images) — collect base64 text.
        if name == "binary" {
            self.in_binary = Some((
                attr(e, "id").unwrap_or_default(),
                attr(e, "content-type").unwrap_or_default(),
                String::new(),
            ));
            self.closers.push("");
            return;
        }

        // STEP 2: metadata inside <description>.
        if self.in_("description") {
            match name.as_str() {
                "sequence" if self.in_("title-info") => {
                    if let Some(n) = attr(e, "name") {
                        self.series = Some((n, attr(e, "number").and_then(|x| x.parse().ok())));
                    }
                }
                "image" if self.in_("coverpage") => {
                    self.cover_id = attr(e, "href").map(|h| h.trim_start_matches('#').to_string())
                }
                "book-title" | "first-name" | "middle-name" | "last-name" | "nickname" | "lang"
                | "publisher" | "year" => {
                    self.field = Some(name.clone());
                }
                "author" if self.in_("title-info") => self.author_parts.clear(),
                "p" if self.in_("annotation") && !self.annotation.is_empty() => {
                    self.annotation.push(' ')
                }
                _ => {}
            }
            self.closers.push("");
            return;
        }

        // STEP 3: body content → HTML.
        if !self.in_("body") {
            self.closers.push("");
            return;
        }
        let id = attr(e, "id");
        if let Some(id) = &id {
            let path = self.doc_path();
            self.ids.insert(id.clone(), path);
        }
        let id_attr = id
            .as_ref()
            .map(|i| format!(" id=\"{}\"", escape(i)))
            .unwrap_or_default();
        let closer: &'static str = match name.as_str() {
            "body" => {
                self.flush_doc();
                self.body_kind = attr(e, "name");
                ""
            }
            "section" => {
                // A top-level section of the main body starts a new chapter.
                if self.section_depth == 0 && self.body_kind.is_none() {
                    self.flush_doc();
                }
                self.section_depth += 1;
                self.section_seq += 1;
                // Every section gets an id so the TOC can point into it.
                let sid = id
                    .clone()
                    .unwrap_or_else(|| format!("fb2s-{}", self.section_seq));
                self.ids.insert(sid.clone(), self.doc_path());
                if self.body_kind.is_none() {
                    self.toc_stack.push(RawEntry {
                        title: String::new(),
                        href: Some(format!("{}#{sid}", self.doc_path())),
                        children: vec![],
                    });
                }
                self.cur
                    .push_str(&format!("<section id=\"{}\">", escape(&sid)));
                "</section>"
            }
            "title" => {
                let level = (self.section_depth + 1).min(6);
                self.title_lines = 0;
                self.title_text.clear();
                self.cur.push_str(&format!("<h{level}{id_attr}>"));
                [
                    "</h1>", "</h1>", "</h2>", "</h3>", "</h4>", "</h5>", "</h6>",
                ][level]
            }
            "p" if self.in_("title") => {
                // Title lines are joined with line breaks inside one heading.
                if self.title_lines > 0 {
                    self.cur.push_str("<br/>");
                    self.title_text.push(' ');
                }
                self.title_lines += 1;
                ""
            }
            "p" => {
                self.para_text.clear();
                self.cur.push_str(&format!("<p{id_attr}>"));
                "</p>"
            }
            "subtitle" => {
                self.cur
                    .push_str(&format!("<p class=\"subtitle\"{id_attr}>"));
                "</p>"
            }
            "empty-line" => {
                self.cur.push_str("<div class=\"empty-line\"></div>");
                ""
            }
            "poem" | "stanza" | "annotation" => {
                self.cur
                    .push_str(&format!("<div class=\"{name}\"{id_attr}>"));
                "</div>"
            }
            "v" | "text-author" => {
                self.cur.push_str(&format!("<p class=\"{name}\"{id_attr}>"));
                "</p>"
            }
            "epigraph" | "cite" => {
                self.cur
                    .push_str(&format!("<blockquote class=\"{name}\"{id_attr}>"));
                "</blockquote>"
            }
            "emphasis" => self.open("<em>", "</em>"),
            "strong" => self.open("<strong>", "</strong>"),
            "strikethrough" => self.open("<s>", "</s>"),
            "sub" => self.open("<sub>", "</sub>"),
            "sup" => self.open("<sup>", "</sup>"),
            "code" => self.open("<code>", "</code>"),
            "table" => self.open("<table>", "</table>"),
            "tr" => self.open("<tr>", "</tr>"),
            "td" => self.open("<td>", "</td>"),
            "th" => self.open("<th>", "</th>"),
            "a" => {
                // Links point at FB2 ids; the target document is resolved in `finish`.
                let href = attr(e, "href").unwrap_or_default();
                let note = attr(e, "type").as_deref() == Some("note");
                let target = match href.strip_prefix('#') {
                    Some(id) => format!("fb2id:{}", escape(id)),
                    None => escape(&href),
                };
                self.cur.push_str(&format!(
                    "{}<a href=\"{target}\">",
                    if note { "<sup>" } else { "" }
                ));
                if note {
                    "</a></sup>"
                } else {
                    "</a>"
                }
            }
            "image" => {
                let src = attr(e, "href")
                    .map(|h| h.trim_start_matches('#').to_string())
                    .unwrap_or_default();
                let block = matches!(
                    self.path.iter().rev().nth(1).map(String::as_str),
                    Some("section" | "body")
                );
                let img = format!("<img src=\"img/{}\" alt=\"\"/>", escape(&src));
                if block {
                    self.cur.push_str(&format!("<p class=\"image\">{img}</p>"));
                } else {
                    self.cur.push_str(&img);
                }
                ""
            }
            _ => "",
        };
        let _ = empty;
        self.closers.push(closer);
    }

    fn open(&mut self, tag: &str, close: &'static str) -> &'static str {
        self.cur.push_str(tag);
        close
    }

    fn end(&mut self, name: &str) {
        // STEP 1: finish binaries and metadata fields.
        if name == "binary" {
            if let Some((id, ct, data)) = self.in_binary.take() {
                let bytes = crate::base64::decode(&data);
                if !bytes.is_empty() && !id.is_empty() {
                    let media_type = if ct.is_empty() {
                        "image/jpeg".to_string()
                    } else {
                        ct
                    };
                    self.binaries.push(VirtualResource {
                        path: format!("fb2/img/{id}"),
                        media_type,
                        bytes,
                    });
                }
            }
        }
        if name == "author"
            && self.in_("title-info")
            && self.path.last().map(String::as_str) == Some("author")
        {
            let full = collapse_ws(&self.author_parts.join(" "));
            if !full.is_empty() {
                self.authors.push(full);
            }
        }
        if self.field.as_deref() == Some(name) {
            self.field = None;
        }

        // STEP 2: body bookkeeping.
        match name {
            "title" if self.in_("body") && !self.in_("description") => {
                let t = collapse_ws(&self.title_text);
                if self.cur_title.is_none() && !t.is_empty() {
                    self.cur_title = Some(t.clone());
                }
                if let Some(entry) = self.toc_stack.last_mut() {
                    if entry.title.is_empty() {
                        entry.title = t;
                    }
                }
            }
            // Untitled sections (common in converted files) are named by a short first paragraph.
            "p" if self.in_("body") && !self.in_("title") => {
                if let Some(entry) = self.toc_stack.last_mut() {
                    let t = collapse_ws(&self.para_text);
                    if entry.title.is_empty() && !t.is_empty() {
                        entry.title = if t.chars().count() <= 80 {
                            t
                        } else {
                            "\u{0}".into()
                        };
                    }
                }
            }
            "section" if self.in_("body") => {
                self.section_depth = self.section_depth.saturating_sub(1);
                if self.body_kind.is_none() {
                    if let Some(mut done) = self.toc_stack.pop() {
                        if done.title.is_empty() || done.title == "\u{0}" {
                            done.title = format!("Section {}", self.toc.len() + 1);
                        }
                        match self.toc_stack.last_mut() {
                            Some(parent) => parent.children.push(done),
                            None => self.toc.push(done),
                        }
                    }
                }
            }
            "body" => {
                // A notes body becomes its own chapter titled "Notes".
                if self.body_kind.is_some() && !self.cur.trim().is_empty() {
                    let path = self.doc_path();
                    self.toc.push(RawEntry {
                        title: "Notes".into(),
                        href: Some(path),
                        children: vec![],
                    });
                    self.cur_title.get_or_insert_with(|| "Notes".into());
                }
                self.flush_doc();
                self.body_kind = None;
            }
            _ => {}
        }

        // STEP 3: emit the closing markup and pop.
        if let Some(c) = self.closers.pop() {
            self.cur.push_str(c);
        }
        self.path.pop();
    }

    fn text(&mut self, t: &str) {
        if let Some((_, _, data)) = self.in_binary.as_mut() {
            data.push_str(t);
            return;
        }
        if self.in_("description") {
            match self.field.as_deref() {
                Some("book-title") => self.title.push_str(t),
                Some("first-name" | "middle-name" | "last-name" | "nickname")
                    if self.in_("author") =>
                {
                    self.author_parts.push(t.trim().to_string())
                }
                Some("lang") => self.lang = Some(t.trim().to_string()),
                Some("publisher") => self.publisher = Some(t.trim().to_string()),
                Some("year") => self.year = Some(t.trim().to_string()),
                _ if self.in_("annotation") => self.annotation.push_str(t),
                _ => {}
            }
            return;
        }
        if self.in_("body") {
            if self.in_("title") {
                self.title_text.push_str(t);
            }
            self.para_text.push_str(t);
            self.cur.push_str(&escape(t));
        }
    }

    fn finish(self, src: &Path) -> HtmlBook {
        // STEP 1: wrap each body in a full document and resolve fb2id: links.
        let ids = &self.ids;
        let lang = self.lang.clone().filter(|l| !l.is_empty());
        let docs = self
            .docs
            .iter()
            .map(|(path, body, title)| {
                let resolved = resolve_ids(body, ids);
                let html = format!(
                    "<html><head><title>{}</title><link rel=\"stylesheet\" href=\"fb2.css\"/></head><body>{resolved}</body></html>",
                    escape(title.as_deref().unwrap_or(""))
                );
                VirtualDoc { path: path.clone(), html }
            })
            .collect();

        // STEP 2: resources — the stylesheet plus every binary.
        let mut resources = vec![VirtualResource {
            path: "fb2/fb2.css".into(),
            media_type: "text/css".into(),
            bytes: FB2_CSS.as_bytes().to_vec(),
        }];
        resources.extend(self.binaries);
        let cover = self.cover_id.map(|id| format!("fb2/img/{id}"));

        let title = collapse_ws(&self.title);
        let metadata = Metadata {
            title: if title.is_empty() {
                file_stem(src)
            } else {
                title
            },
            authors: self.authors,
            language: lang,
            publisher: self.publisher,
            description: Some(collapse_ws(&self.annotation)).filter(|a| !a.is_empty()),
            published: self.year,
            series: self.series.as_ref().map(|s| s.0.clone()),
            series_index: self.series.and_then(|s| s.1),
            identifiers: vec![],
        };
        HtmlBook {
            format: "FB2",
            metadata,
            docs,
            resources,
            toc: self.toc,
            cover,
            warnings: vec![],
        }
    }
}

/// Replace `href="fb2id:ID"` with `href="<doc file>#ID"` (relative to `fb2/`).
fn resolve_ids(body: &str, ids: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(i) = rest.find("href=\"fb2id:") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 12..];
        let end = after.find('"').unwrap_or(after.len());
        let id = &after[..end];
        let raw_id = id
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&lt;", "<")
            .replace("&gt;", ">");
        let doc = ids
            .get(&raw_id)
            .map(|p| p.trim_start_matches("fb2/").to_string())
            .unwrap_or_default();
        out.push_str(&format!("href=\"{doc}#{id}\""));
        // Skip the original closing quote (we wrote our own).
        rest = after.get(end + 1..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><author><first-name>Anna</first-name><last-name>Writer</last-name></author>
<book-title>River Songs</book-title><lang>en</lang><sequence name="Waters" number="2"/>
<annotation><p>A short book.</p></annotation><coverpage><image l:href="#cover.png"/></coverpage></title-info></description>
<body><title><p>River Songs</p></title>
<section><title><p>Chapter One</p><p>The Start</p></title><epigraph><p>Quiet.</p><text-author>Someone</text-author></epigraph>
<p>Hello <emphasis>world</emphasis><a l:href="#n1" type="note">1</a>.</p>
<section><title><p>Part A</p></title><p>Nested.</p></section></section>
<section><title><p>Chapter Two</p></title><poem><stanza><v>Line one</v><v>Line two</v></stanza></poem><image l:href="#cover.png"/></section>
</body>
<body name="notes"><section id="n1"><title><p>1</p></title><p>A footnote.</p></section></body>
<binary id="cover.png" content-type="image/png">iVBORw0KGgo=</binary>
</FictionBook>"##;

    #[test]
    fn converts_structure_and_metadata() {
        // STEP 1: metadata.
        let b = convert_xml(SAMPLE, Path::new("x.fb2"));
        assert_eq!(b.metadata.title, "River Songs");
        assert_eq!(b.metadata.authors, vec!["Anna Writer"]);
        assert_eq!(b.metadata.series.as_deref(), Some("Waters"));
        assert_eq!(b.metadata.series_index, Some(2.0));
        assert_eq!(b.metadata.description.as_deref(), Some("A short book."));
        assert_eq!(b.cover.as_deref(), Some("fb2/img/cover.png"));
        assert_eq!(b.resources[1].bytes, b"\x89PNG\r\n\x1a\n");

        // STEP 2: chapters — body title, two sections, notes.
        assert_eq!(
            b.docs.len(),
            4,
            "{:?}",
            b.docs.iter().map(|d| &d.path).collect::<Vec<_>>()
        );
        let ch1 = &b.docs[1].html;
        assert!(ch1.contains("<h2>Chapter One<br/>The Start</h2>"), "{ch1}");
        assert!(ch1.contains(r#"<blockquote class="epigraph">"#));
        assert!(ch1.contains("<em>world</em>"));
        assert!(
            ch1.contains(r##"<sup><a href="0004.html#n1">1</a></sup>"##),
            "{ch1}"
        );
        assert!(ch1.contains("<h3>Part A</h3>"));
        assert!(b.docs[2].html.contains(r#"<p class="v">Line one</p>"#));
        assert!(b.docs[2]
            .html
            .contains(r#"<p class="image"><img src="img/cover.png" alt=""/></p>"#));

        // STEP 3: TOC with nesting and a Notes entry.
        assert_eq!(b.toc.len(), 3);
        assert_eq!(b.toc[0].title, "Chapter One The Start");
        assert_eq!(b.toc[0].children[0].title, "Part A");
        assert_eq!(b.toc[2].title, "Notes");
    }
}
