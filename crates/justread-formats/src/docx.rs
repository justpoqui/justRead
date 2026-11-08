// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Word documents (`.docx`, Office Open XML WordprocessingML, ECMA-376).
//!
//! The document is read as a sequence of *blocks* (paragraphs, lists,
//! tables). Headings are recognized from paragraph styles (`Heading N`,
//! `Title`) or outline levels; the book is split into chapters at the
//! highest heading level the document actually uses. Formatting kept:
//! bold/italic/underline/strike, super/subscript, alignment, lists, tables,
//! hyperlinks, bookmarks, images (PNG/JPEG/GIF) and footnotes.
//! Spec: ECMA-376 Part 1, §17 (<https://ecma-international.org/publications-and-standards/standards/ecma-376/>).

use std::collections::HashMap;
use std::path::Path;

use justbook::paths::{normalize_relative, resolve_reference};
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
    entities, BookContent, Confidence, DrmStatus, FormatAdapter, FormatError, FormatFamily,
    Inspection, Result,
};

pub struct DocxAdapter;

impl FormatAdapter for DocxAdapter {
    fn id(&self) -> &'static str {
        "docx"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Reflowable
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        if head.starts_with(b"PK\x03\x04") && ext == Some("docx") {
            Confidence::High
        } else {
            Confidence::No
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let mut ar = SafeArchive::open(src, "DOCX")?;
        Ok(Inspection {
            metadata: core_metadata(&mut ar, src),
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        let mut ar = SafeArchive::open(src, "DOCX")?;
        // Password-protected Word files are OLE containers, not ZIPs; an
        // "EncryptedPackage" entry means IRM/encryption.
        if ar.has("EncryptedPackage") {
            return Err(FormatError::DrmProtected("Office encryption".into()));
        }
        let book = convert_archive(&mut ar, src)?;
        write_html_book(book, out)
    }
}

// ---------------------------------------------------------------------------
// Package parts
// ---------------------------------------------------------------------------

/// `Id → (relationship type suffix, target, external?)` from a .rels part.
fn read_rels(
    ar: &mut SafeArchive,
    rels_path: &str,
    base_part: &str,
) -> HashMap<String, (String, String, bool)> {
    let mut out = HashMap::new();
    let Ok(xml) = ar.read_text(rels_path) else {
        return out;
    };
    let mut r = Reader::from_str(&xml);
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e))
                if e.local_name().as_ref() == "Relationship" =>
            {
                let (Some(id), Some(target)) = (attr(&e, "Id"), attr(&e, "Target")) else {
                    continue;
                };
                let kind = attr(&e, "Type")
                    .unwrap_or_default()
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let external = attr(&e, "TargetMode").as_deref() == Some("External");
                let target = if external {
                    target
                } else if let Some(abs) = target.strip_prefix('/') {
                    abs.to_string()
                } else {
                    resolve_reference(base_part, &target).unwrap_or_default()
                };
                out.insert(id, (kind, target, external));
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Title/author/language/description from `docProps/core.xml`.
fn core_metadata(ar: &mut SafeArchive, src: &Path) -> Metadata {
    let mut md = Metadata::default();
    if let Ok(xml) = ar.read_text("docProps/core.xml") {
        let mut r = Reader::from_str(&xml);
        let mut field = String::new();
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) => {
                    field = String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).into_owned()
                }
                Ok(Event::Text(t)) => {
                    let v = collapse_ws(&t.xml10_content());
                    if v.is_empty() {
                        continue;
                    }
                    match field.as_str() {
                        "title" => md.title = v,
                        "creator" => {
                            md.authors = v
                                .split([';', ','])
                                .map(|a| a.trim().to_string())
                                .filter(|a| !a.is_empty())
                                .collect()
                        }
                        "language" => md.language = Some(v),
                        "description" => md.description = Some(v),
                        _ => {}
                    }
                }
                Ok(Event::End(_)) => field.clear(),
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    if md.title.is_empty() {
        md.title = file_stem(src);
    }
    md
}

/// Paragraph style id → heading level, from `styles.xml`.
fn heading_styles(ar: &mut SafeArchive, styles_path: &str) -> HashMap<String, u8> {
    let mut out = HashMap::new();
    let Ok(xml) = ar.read_text(styles_path) else {
        return out;
    };
    let mut r = Reader::from_str(&xml);
    let mut cur: Option<String> = None;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                "style" => cur = attr(&e, "styleId"),
                "name" => {
                    if let (Some(id), Some(name)) = (&cur, attr(&e, "val")) {
                        if let Some(level) = level_from_name(&name) {
                            out.insert(id.clone(), level);
                        }
                    }
                }
                "outlineLvl" => {
                    if let (Some(id), Some(v)) =
                        (&cur, attr(&e, "val").and_then(|v| v.parse::<u8>().ok()))
                    {
                        if v < 6 {
                            out.entry(id.clone()).or_insert(v + 1);
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if e.local_name().as_ref() == "style" => cur = None,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// "heading 2" → 2, "Title" → 1 (case-insensitive; also matches style ids).
fn level_from_name(name: &str) -> Option<u8> {
    let l = name.to_ascii_lowercase().replace(' ', "");
    if l == "title" {
        return Some(1);
    }
    l.strip_prefix("heading")
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| (1..=6).contains(n))
}

/// numId → ordered? (anything but bullets), from `numbering.xml`, level 0.
fn list_kinds(ar: &mut SafeArchive, path: &str) -> HashMap<String, bool> {
    let mut abstract_fmt: HashMap<String, bool> = HashMap::new();
    let mut num_to_abs: HashMap<String, String> = HashMap::new();
    let Ok(xml) = ar.read_text(path) else {
        return HashMap::new();
    };
    let mut r = Reader::from_str(&xml);
    let (mut cur_abs, mut cur_num, mut lvl) = (None::<String>, None::<String>, None::<String>);
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                "abstractNum" => cur_abs = attr(&e, "abstractNumId"),
                "num" => cur_num = attr(&e, "numId"),
                "lvl" => lvl = attr(&e, "ilvl"),
                "numFmt" if lvl.as_deref() == Some("0") => {
                    if let Some(a) = &cur_abs {
                        let fmt = attr(&e, "val").unwrap_or_default();
                        abstract_fmt.insert(a.clone(), fmt != "bullet" && fmt != "none");
                    }
                }
                "abstractNumId" => {
                    if let (Some(n), Some(v)) = (&cur_num, attr(&e, "val")) {
                        num_to_abs.insert(n.clone(), v);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                "abstractNum" => cur_abs = None,
                "num" => cur_num = None,
                "lvl" => lvl = None,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    num_to_abs
        .into_iter()
        .map(|(n, a)| (n, abstract_fmt.get(&a).copied().unwrap_or(false)))
        .collect()
}

fn attr(e: &BytesStart<'_>, local: &str) -> Option<String> {
    e.html_attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == local)
        .map(|a| entities::unescape(&a.value))
}

/// `w:b`, `w:i` … are on unless `w:val` is "0"/"false"/"none".
fn toggle_on(e: &BytesStart<'_>) -> bool {
    !matches!(attr(e, "val").as_deref(), Some("0" | "false" | "none"))
}

// ---------------------------------------------------------------------------
// Body conversion
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct RunProps {
    b: bool,
    i: bool,
    u: bool,
    s: bool,
    sup: bool,
    sub: bool,
}

#[derive(Default)]
struct Para {
    style: Option<String>,
    outline: Option<u8>,
    num_id: Option<String>,
    ilvl: usize,
    align: Option<String>,
    html: String,
    text: String,
}

/// Context shared by the main document and the footnotes part.
struct Ctx<'a> {
    rels: &'a HashMap<String, (String, String, bool)>,
    headings: &'a HashMap<String, u8>,
    lists: &'a HashMap<String, bool>,
    images: &'a mut Vec<VirtualResource>,
    warnings: &'a mut Vec<String>,
    ar: &'a mut SafeArchive,
    notes_doc: &'a str,
}

/// Top-level blocks: (heading level, heading text, html).
type Blocks = Vec<(Option<u8>, String, String)>;

/// Convert a WordprocessingML part into blocks.
fn convert_part(xml: &str, cx: &mut Ctx<'_>) -> Blocks {
    let mut r = Reader::from_str(xml);
    r.config_mut().check_end_names = false;

    let mut blocks: Blocks = Vec::new();
    let mut para: Option<Para> = None;
    let mut run: Option<RunProps> = None;
    let mut in_rpr = false;
    let mut in_ppr = false;
    let mut in_text = false;
    let mut tables: Vec<String> = Vec::new(); // nested table buffers
    let mut lists: Vec<(bool, usize)> = Vec::new(); // open lists (ordered, level)
    let mut list_html = String::new();
    let mut footnote: Option<String> = None; // id while inside <w:footnote>

    // Close all open lists into one block.
    fn flush_list(lists: &mut Vec<(bool, usize)>, list_html: &mut String, blocks: &mut Blocks) {
        while let Some((ordered, _)) = lists.pop() {
            list_html.push_str(if ordered { "</li></ol>" } else { "</li></ul>" });
        }
        if !list_html.is_empty() {
            blocks.push((None, String::new(), std::mem::take(list_html)));
        }
    }

    loop {
        let ev = match r.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(ev) => ev,
        };
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                match e.local_name().as_ref() {
                    // ----- structure -----
                    "footnote" => {
                        let separator = attr(e, "type").is_some_and(|t| t.contains("eparator"));
                        footnote = if separator { None } else { attr(e, "id") };
                    }
                    "p" => {
                        para = Some(Para::default());
                    }
                    "pPr" if !empty => in_ppr = true,
                    "pStyle" if in_ppr => {
                        if let Some(p) = para.as_mut() {
                            p.style = attr(e, "val");
                        }
                    }
                    "outlineLvl" if in_ppr => {
                        if let Some(p) = para.as_mut() {
                            p.outline = attr(e, "val")
                                .and_then(|v| v.parse::<u8>().ok())
                                .filter(|v| *v < 6)
                                .map(|v| v + 1);
                        }
                    }
                    "numId" if in_ppr => {
                        if let Some(p) = para.as_mut() {
                            p.num_id = attr(e, "val").filter(|v| v != "0");
                        }
                    }
                    "ilvl" if in_ppr => {
                        if let Some(p) = para.as_mut() {
                            p.ilvl = attr(e, "val")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0)
                                .min(8);
                        }
                    }
                    "jc" if in_ppr => {
                        if let Some(p) = para.as_mut() {
                            p.align = attr(e, "val")
                                .filter(|v| matches!(v.as_str(), "center" | "right" | "end"));
                        }
                    }
                    "tbl" => {
                        if tables.is_empty() {
                            flush_list(&mut lists, &mut list_html, &mut blocks);
                        }
                        tables.push("<table>".into());
                    }
                    "tr" if !tables.is_empty() => tables.last_mut().unwrap().push_str("<tr>"),
                    "tc" if !tables.is_empty() => tables.last_mut().unwrap().push_str("<td>"),
                    // ----- runs -----
                    "r" if para.is_some() => run = Some(RunProps::default()),
                    "rPr" if run.is_some() && !empty => in_rpr = true,
                    "b" if in_rpr => run.as_mut().unwrap().b = toggle_on(e),
                    "i" if in_rpr => run.as_mut().unwrap().i = toggle_on(e),
                    "u" if in_rpr => run.as_mut().unwrap().u = toggle_on(e),
                    "strike" | "dstrike" if in_rpr => run.as_mut().unwrap().s = toggle_on(e),
                    "vertAlign" if in_rpr => {
                        let v = attr(e, "val").unwrap_or_default();
                        let rp = run.as_mut().unwrap();
                        rp.sup = v == "superscript";
                        rp.sub = v == "subscript";
                    }
                    "t" if run.is_some() && !empty => in_text = true,
                    "tab" if run.is_some() => push_text(&mut para, &run, " "),
                    "br" | "cr" if para.is_some() => {
                        if attr(e, "type").is_none_or(|t| t == "textWrapping") {
                            para.as_mut().unwrap().html.push_str("<br/>");
                        }
                    }
                    "hyperlink" if para.is_some() => {
                        let href = match (attr(e, "id"), attr(e, "anchor")) {
                            (Some(id), _) => cx.rels.get(&id).filter(|r| r.2).map(|r| r.1.clone()),
                            (None, Some(a)) => Some(format!("#{a}")),
                            _ => None,
                        };
                        if let Some(h) = href {
                            para.as_mut()
                                .unwrap()
                                .html
                                .push_str(&format!("<a href=\"{}\">", escape(&h)));
                        } else {
                            para.as_mut().unwrap().html.push_str("<a>");
                        }
                    }
                    "bookmarkStart" if para.is_some() => {
                        if let Some(name) = attr(e, "name").filter(|n| !n.starts_with('_')) {
                            para.as_mut()
                                .unwrap()
                                .html
                                .push_str(&format!("<a id=\"{}\"></a>", escape(&name)));
                        }
                    }
                    "footnoteReference" if para.is_some() => {
                        if let Some(id) = attr(e, "id") {
                            para.as_mut().unwrap().html.push_str(&format!(
                                "<sup><a href=\"{}#fn-{id}\">{id}</a></sup>",
                                cx.notes_doc
                            ));
                        }
                    }
                    // ----- images (DrawingML and legacy VML) -----
                    "blip" | "imagedata" if para.is_some() => {
                        let rid = attr(e, "embed").or_else(|| attr(e, "id"));
                        if let Some(src) = rid.and_then(|id| image(cx, &id)) {
                            para.as_mut()
                                .unwrap()
                                .html
                                .push_str(&format!("<img src=\"{}\" alt=\"\"/>", escape(&src)));
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(ref t) if in_text => push_text(&mut para, &run, &t.xml10_content()),
            Event::GeneralRef(ref g) if in_text => {
                let n: &str = g;
                push_text(&mut para, &run, &entities::resolve(n).unwrap_or_default());
            }
            Event::End(ref e) => match e.local_name().as_ref() {
                "t" => in_text = false,
                "rPr" => in_rpr = false,
                "pPr" => in_ppr = false,
                "r" => run = None,
                "hyperlink" => {
                    if let Some(p) = para.as_mut() {
                        p.html.push_str("</a>");
                    }
                }
                "p" => {
                    // STEP 1: decide what the finished paragraph is.
                    let Some(mut p) = para.take() else { continue };
                    if let Some(fid) = &footnote {
                        // First paragraph of a footnote carries its anchor.
                        if !blocks
                            .iter()
                            .any(|b| b.2.contains(&format!("id=\"fn-{fid}\"")))
                        {
                            p.html = format!(
                                "<a id=\"fn-{}\"></a><strong>{fid}.</strong> {}",
                                escape(fid),
                                p.html
                            );
                        }
                    }
                    if p.html.trim().is_empty() {
                        continue; // Word uses empty paragraphs for spacing
                    }
                    let level = p
                        .style
                        .as_ref()
                        .and_then(|s| cx.headings.get(s).copied().or_else(|| level_from_name(s)))
                        .or(p.outline);
                    let style = p
                        .align
                        .as_ref()
                        .map(|a| {
                            format!(
                                " style=\"text-align:{}\"",
                                if a == "end" { "right" } else { a }
                            )
                        })
                        .unwrap_or_default();

                    // STEP 2: inside a table cell everything is a plain paragraph.
                    if let Some(t) = tables.last_mut() {
                        t.push_str(&format!("<p{style}>{}</p>", p.html));
                        continue;
                    }
                    // STEP 3: list items, opening/closing nested lists as needed.
                    if let (Some(num), None) = (&p.num_id, level) {
                        let ordered = cx.lists.get(num).copied().unwrap_or(false);
                        while lists.last().is_some_and(|(_, l)| *l > p.ilvl) {
                            list_html.push_str(if lists.pop().unwrap().0 {
                                "</li></ol>"
                            } else {
                                "</li></ul>"
                            });
                        }
                        if lists.last().is_some_and(|(_, l)| *l == p.ilvl) {
                            list_html.push_str("</li>");
                        } else {
                            lists.push((ordered, p.ilvl));
                            list_html.push_str(if ordered { "<ol>" } else { "<ul>" });
                        }
                        list_html.push_str(&format!("<li>{}", p.html));
                        continue;
                    }
                    flush_list(&mut lists, &mut list_html, &mut blocks);
                    // STEP 4: headings and paragraphs.
                    match level {
                        Some(l) => blocks.push((
                            Some(l),
                            collapse_ws(&p.text),
                            format!("<h{l}{style}>{}</h{l}>", p.html),
                        )),
                        None => {
                            blocks.push((None, String::new(), format!("<p{style}>{}</p>", p.html)))
                        }
                    }
                }
                "tc" => {
                    if let Some(t) = tables.last_mut() {
                        t.push_str("</td>");
                    }
                }
                "tr" => {
                    if let Some(t) = tables.last_mut() {
                        t.push_str("</tr>");
                    }
                }
                "tbl" => {
                    if let Some(mut t) = tables.pop() {
                        t.push_str("</table>");
                        match tables.last_mut() {
                            Some(outer) => outer.push_str(&t),
                            None => blocks.push((None, String::new(), t)),
                        }
                    }
                }
                "footnote" => footnote = None,
                _ => {}
            },
            _ => {}
        }
    }
    flush_list(&mut lists, &mut list_html, &mut blocks);
    blocks
}

/// Append run text with its formatting.
fn push_text(para: &mut Option<Para>, run: &Option<RunProps>, text: &str) {
    let Some(p) = para.as_mut() else { return };
    p.text.push_str(text);
    let rp = run.clone().unwrap_or_default();
    let mut s = escape(text);
    for (on, open, close) in [
        (rp.sup, "<sup>", "</sup>"),
        (rp.sub, "<sub>", "</sub>"),
        (rp.s, "<s>", "</s>"),
        (rp.u, "<u>", "</u>"),
        (rp.i, "<em>", "</em>"),
        (rp.b, "<strong>", "</strong>"),
    ] {
        if on {
            s = format!("{open}{s}{close}");
        }
    }
    p.html.push_str(&s);
}

/// Import an image relationship as a resource; returns its virtual path.
fn image(cx: &mut Ctx<'_>, rid: &str) -> Option<String> {
    let (_, target, external) = cx.rels.get(rid)?.clone();
    if external {
        return None; // offline-only: linked (remote) images are not fetched
    }
    let ext = crate::images::extension(&target)?;
    let media_type = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        other => {
            cx.warnings
                .push(format!("skipped {other} image {target} (not displayable)"));
            return None;
        }
    };
    let path = normalize_relative(&format!("docx/{target}")).ok()?;
    if !cx.images.iter().any(|i| i.path == path) {
        let bytes = cx.ar.read(&target).ok()?;
        cx.images.push(VirtualResource {
            path: path.clone(),
            media_type: media_type.into(),
            bytes,
        });
    }
    // Documents live in docx/, so the reference is relative to that folder.
    Some(path.trim_start_matches("docx/").to_string())
}

/// Convert the whole package into an [`HtmlBook`].
pub fn convert_archive(ar: &mut SafeArchive, src: &Path) -> Result<HtmlBook> {
    // STEP 1: locate the main document through the package relationships.
    let root = read_rels(ar, "_rels/.rels", "");
    let main = root
        .values()
        .find(|(k, _, _)| k == "officeDocument")
        .map(|r| r.1.clone())
        .unwrap_or_else(|| "word/document.xml".into());
    let dir = main
        .rsplit_once('/')
        .map(|(d, _)| format!("{d}/"))
        .unwrap_or_default();
    let file = main.rsplit('/').next().unwrap_or("document.xml");
    let rels = read_rels(ar, &format!("{dir}_rels/{file}.rels"), &main);
    let part_of = |kind: &str| {
        rels.values()
            .find(|(k, _, _)| k == kind)
            .map(|r| r.1.clone())
    };

    // STEP 2: supporting parts.
    let headings = part_of("styles")
        .map(|p| heading_styles(ar, &p))
        .unwrap_or_default();
    let lists = part_of("numbering")
        .map(|p| list_kinds(ar, &p))
        .unwrap_or_default();
    let mut metadata = core_metadata(ar, src);
    let xml = ar.read_text(&main).map_err(|_| FormatError::Invalid {
        format: "DOCX",
        reason: "no word/document.xml".into(),
    })?;

    // STEP 3: body and footnotes.
    let mut images = Vec::new();
    let mut warnings = Vec::new();
    let notes_doc = "notes.html";
    let blocks = {
        let mut cx = Ctx {
            rels: &rels,
            headings: &headings,
            lists: &lists,
            images: &mut images,
            warnings: &mut warnings,
            ar,
            notes_doc,
        };
        convert_part(&xml, &mut cx)
    };
    let footnotes = match part_of("footnotes") {
        Some(p) => {
            let fxml = ar.read_text(&p).unwrap_or_default();
            let note_rels = read_rels(
                ar,
                &format!("{dir}_rels/{}.rels", p.rsplit('/').next().unwrap_or("")),
                &p,
            );
            let mut cx = Ctx {
                rels: &note_rels,
                headings: &headings,
                lists: &lists,
                images: &mut images,
                warnings: &mut warnings,
                ar,
                notes_doc,
            };
            convert_part(&fxml, &mut cx)
        }
        None => vec![],
    };

    // STEP 4: chapters split at the highest heading level used.
    let top = blocks.iter().filter_map(|b| b.0).min();
    let mut docs: Vec<(String, String)> = Vec::new(); // (title, body html)
    for (level, text, html) in blocks {
        let starts = top.is_some() && level == top;
        if docs.is_empty() || (starts && !docs.last().unwrap().1.is_empty()) {
            docs.push((
                if starts { text.clone() } else { String::new() },
                String::new(),
            ));
        } else if starts && docs.last().unwrap().0.is_empty() {
            docs.last_mut().unwrap().0 = text.clone();
        }
        docs.last_mut().unwrap().1.push_str(&html);
    }
    if metadata.title == file_stem(src) {
        if let Some(t) = docs.first().map(|d| d.0.clone()).filter(|t| !t.is_empty()) {
            metadata.title = t;
        }
    }

    // STEP 5: virtual documents (+ notes) and a TOC from chapter headings.
    let mut toc = Vec::new();
    let mut vdocs = Vec::new();
    for (i, (title, body)) in docs.into_iter().enumerate() {
        let path = format!("docx/c{:04}.html", i + 1);
        if !title.is_empty() {
            toc.push(RawEntry {
                title: title.clone(),
                href: Some(path.clone()),
                children: vec![],
            });
        }
        vdocs.push(VirtualDoc {
            path,
            html: format!(
                "<html><head><title>{}</title></head><body>{body}</body></html>",
                escape(&title)
            ),
        });
    }
    if !footnotes.is_empty() {
        let body: String = footnotes.into_iter().map(|b| b.2).collect();
        toc.push(RawEntry {
            title: "Notes".into(),
            href: Some(format!("docx/{notes_doc}")),
            children: vec![],
        });
        vdocs.push(VirtualDoc {
            path: format!("docx/{notes_doc}"),
            html: format!("<html><body><h2>Notes</h2>{body}</body></html>"),
        });
    }
    Ok(HtmlBook {
        format: "DOCX",
        metadata,
        docs: vdocs,
        resources: images,
        toc,
        cover: None,
        warnings,
    })
}
