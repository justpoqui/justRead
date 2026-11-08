// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! XHTML/HTML content-document normalization.
//!
//! Takes one source document (an EPUB chapter, later HTML/DOCX output…) and
//! produces a justBook chapter:
//!
//! * scripts, forms, embedded objects and event handlers are removed;
//! * only an allow-list of elements and attributes survives (unknown
//!   elements are *unwrapped*: their text is kept, the tag is dropped);
//! * internal links and resource references are rewritten to package paths;
//! * stylesheets are collected so the chapter can link the scrubbed copies;
//! * output is well-formed and block-stamped by [`justbook::ChapterWriter`].
//!
//! The parser is quick-xml in a lenient configuration (mismatched and
//! unmatched end tags allowed, bare `&` allowed), so typical "almost XHTML"
//! files convert fine. If the input is too broken to continue, whatever was
//! read so far is kept and a warning is returned.

use justbook::paths::{fragment, is_external, relative_path, resolve_reference};
use justbook::{ChapterStats, ChapterWriter};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::css::{scrub_inline, scrub_stylesheet};
use crate::entities;

/// Elements whose whole subtree is discarded (active content, forms, media).
const SKIP: &[&str] = &[
    "script",
    "noscript",
    "iframe",
    "object",
    "embed",
    "frame",
    "frameset",
    "applet",
    "input",
    "button",
    "select",
    "textarea",
    "option",
    "template",
    "audio",
    "video",
    "canvas",
    "form-data",
];

/// HTML elements copied as-is (after attribute filtering).
const ALLOWED: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "main",
    "mark",
    "nav",
    "ol",
    "p",
    "pre",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "section",
    "small",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "u",
    "ul",
    "var",
    "wbr",
];

/// Generic attributes kept on any allowed HTML element.
const ALLOWED_ATTRS: &[&str] = &[
    "id", "class", "title", "dir", "alt", "width", "height", "colspan", "rowspan", "span", "start",
    "reversed", "value", "datetime", "abbr", "scope", "headers", "role",
];

/// Where a document lives and how references inside it are mapped.
pub struct LinkContext<'a> {
    /// Path of the source document inside its container (e.g. `OEBPS/ch1.xhtml`).
    pub source_path: &'a str,
    /// Package path of the chapter being written (e.g. `content/c0001.xhtml`).
    pub out_path: &'a str,
    /// Map a resolved source path to its package path (chapter or resource).
    pub map: &'a dyn Fn(&str) -> Option<String>,
    /// Language used when the document declares none (hyphenation needs one).
    pub default_lang: Option<&'a str>,
}

/// Result of normalizing one document.
pub struct Normalized {
    pub xhtml: String,
    pub stats: ChapterStats,
    /// `<title>` of the source document, if any.
    pub head_title: Option<String>,
    /// Text of the first h1–h3, if any (fallback chapter title).
    pub first_heading: Option<String>,
    pub warnings: Vec<String>,
}

/// Extra behavior used by converters that build books from generated HTML.
#[derive(Debug, Clone, Copy, Default)]
pub struct NormalizeOptions {
    /// Start a new chapter between top-level blocks once this many UTF-16
    /// units have been written (keeps pagination fast for huge documents).
    pub split_chars: Option<u64>,
    /// Emit internal links as placeholders (see [`LINK_HREF`]) to be resolved
    /// once every chapter and anchor id is known.
    pub defer_links: bool,
}

/// Placeholder prefixes written by `defer_links`; the value between the
/// prefix and [`LINK_END`] is `<resolved source path>#<fragment>`.
pub const LINK_HREF: &str = "@@JRH:";
pub const LINK_PKG: &str = "@@JRP:";
pub const LINK_END: &str = "@@";

/// One output chapter of [`normalize_parts`].
pub struct Part {
    pub normalized: Normalized,
    /// `id` attributes that ended up in this part (link targets).
    pub ids: Vec<String>,
}

/// What text events are currently feeding, besides the body.
#[derive(PartialEq)]
enum Capture {
    None,
    Title,
    Style,
}

/// Normalize `source` (already decoded to UTF-8). `title_hint` (usually
/// from the TOC) becomes the chapter's `<title>` when given.
pub fn normalize(source: &str, ctx: &LinkContext<'_>, title_hint: Option<&str>) -> Normalized {
    let mut parts = normalize_parts(source, ctx, title_hint, NormalizeOptions::default());
    parts.remove(0).normalized
}

/// Like [`normalize`], optionally splitting into several chapters and
/// deferring link resolution. Always returns at least one part.
pub fn normalize_parts(
    source: &str,
    ctx: &LinkContext<'_>,
    title_hint: Option<&str>,
    opts: NormalizeOptions,
) -> Vec<Part> {
    // STEP 1: configure a forgiving parser.
    let mut reader = Reader::from_str(source);
    {
        let cfg = reader.config_mut();
        cfg.check_end_names = false;
        cfg.allow_unmatched_ends = true;
        cfg.allow_dangling_amp = true;
        cfg.check_comments = false;
    }

    // STEP 2: parsing state.
    let mut w = ChapterWriter::new();
    let mut warnings = Vec::new();
    let mut lang: Option<String> = None;
    let mut stylesheets: Vec<String> = Vec::new();
    let mut inline_css = String::new();
    let mut head_title = String::new();
    let mut heading: Option<(String, String)> = None; // (element, text so far)
    let mut first_heading: Option<String> = None;
    let mut capture = Capture::None;
    let mut skip_depth = 0usize;
    let mut svg_depth = 0usize;
    let mut in_head = false;
    let mut seen_body = false;
    // Finished parts (writer, first heading, ids) and the ids of the current one.
    let mut done: Vec<(ChapterWriter, Option<String>, Vec<String>)> = Vec::new();
    let mut ids: Vec<String> = Vec::new();

    // STEP 3: walk events.
    loop {
        let event = match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(ev) => ev,
            Err(e) => {
                warnings.push(format!(
                    "{}: parse error, kept partial content: {e}",
                    ctx.source_path
                ));
                break;
            }
        };

        match event {
            // ----- element starts (Empty = self-closed `<x/>`) -----
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(event, Event::Empty(_));
                let name = local_name(e);

                // Inside a discarded subtree: only track depth.
                if skip_depth > 0 {
                    if !is_empty {
                        skip_depth += 1;
                    }
                    continue;
                }

                // Split between top-level blocks once the chapter is long enough.
                if let Some(limit) = opts.split_chars {
                    if seen_body
                        && !in_head
                        && w.depth() == 0
                        && svg_depth == 0
                        && w.stats().chars >= limit
                    {
                        let next = w.sibling();
                        done.push((
                            std::mem::replace(&mut w, next),
                            first_heading.take(),
                            std::mem::take(&mut ids),
                        ));
                        heading = None;
                    }
                }
                if let Some(id) = attr(e, "id") {
                    ids.push(id);
                }

                match name.as_str() {
                    "html" => lang = attr(e, "lang").or_else(|| attr(e, "xml:lang")).or(lang),
                    "head" => in_head = !is_empty,
                    "body" => {
                        in_head = false;
                        seen_body = true;
                        let mut attrs = html_attrs(e, "body");
                        if let Some(l) = attr(e, "lang").or_else(|| attr(e, "xml:lang")) {
                            lang.get_or_insert(l);
                        }
                        attrs.retain(|(k, _)| k == "class" || k == "dir" || k == "lang");
                        w.set_body_attrs(attrs);
                    }
                    "title" if in_head && !is_empty => capture = Capture::Title,
                    "style" if !is_empty => capture = Capture::Style,
                    "link" => {
                        let rel = attr(e, "rel").unwrap_or_default().to_ascii_lowercase();
                        if rel.split_whitespace().any(|r| r == "stylesheet") {
                            if let Some(href) = attr(e, "href").filter(|h| !is_external(h)) {
                                if let Ok(p) = resolve_reference(ctx.source_path, &href) {
                                    stylesheets.push(p);
                                }
                            }
                        }
                    }
                    "meta" | "base" => {}
                    n if SKIP.contains(&n) || (in_head && !is_empty) => {
                        if !is_empty {
                            skip_depth = 1;
                        }
                    }
                    // SVG subtree: pass through with attribute filtering.
                    n if n == "svg" || svg_depth > 0 => {
                        let attrs = svg_attrs(e, ctx);
                        let refs: Vec<(&str, &str)> = attrs
                            .iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect();
                        w.start(n, &refs);
                        if is_empty {
                            w.end(n);
                        } else if n == "svg" || svg_depth > 0 {
                            svg_depth += 1;
                        }
                    }
                    n => {
                        // Map presentational legacy tags onto modern ones.
                        let mapped = map_legacy(n);
                        if !ALLOWED.contains(&mapped) {
                            continue; // unwrap: drop the tag, keep its content
                        }

                        // Build the attribute list, handling links and images.
                        let mut attrs = html_attrs(e, mapped);
                        if mapped == "a" {
                            if let Some(href) = attr(e, "href") {
                                attrs.extend(rewrite_link(&href, ctx, opts.defer_links));
                            }
                        }
                        if mapped == "img" {
                            match attr(e, "src").and_then(|s| rewrite_resource(&s, ctx)) {
                                Some(src) => attrs.push(("src".into(), src)),
                                None => {
                                    // An image we can't resolve is dropped (alt kept as text).
                                    if let Some(alt) = attr(e, "alt").filter(|a| !a.is_empty()) {
                                        w.text(&alt);
                                    }
                                    continue;
                                }
                            }
                        }

                        // Start capturing the first heading as a title fallback.
                        if first_heading.is_none()
                            && heading.is_none()
                            && matches!(mapped, "h1" | "h2" | "h3")
                            && !is_empty
                        {
                            heading = Some((mapped.to_string(), String::new()));
                        }

                        let refs: Vec<(&str, &str)> = attrs
                            .iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect();
                        w.start(mapped, &refs);
                        if is_empty {
                            w.end(mapped);
                        }
                    }
                }
            }

            // ----- element ends -----
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref().as_bytes())
                    .to_ascii_lowercase();
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                match name.as_str() {
                    "head" => in_head = false,
                    "title" | "style" => capture = Capture::None,
                    "html" | "body" => {}
                    n if svg_depth > 0 => {
                        w.end(n);
                        svg_depth -= 1;
                    }
                    n => {
                        let mapped = map_legacy(n);
                        if let Some((h, text)) = &heading {
                            if h == mapped {
                                let t = collapse_ws(text);
                                if !t.is_empty() {
                                    first_heading = Some(t);
                                }
                                heading = None;
                            }
                        }
                        w.end(mapped);
                    }
                }
            }

            // ----- character data -----
            Event::Text(ref t) => {
                let text = t.xml10_content();
                feed_text(
                    &text,
                    skip_depth,
                    &capture,
                    in_head,
                    seen_body,
                    &mut w,
                    &mut head_title,
                    &mut inline_css,
                    &mut heading,
                );
            }
            Event::CData(ref t) => {
                let text = String::from_utf8_lossy(t.as_ref().as_bytes()).into_owned();
                feed_text(
                    &text,
                    skip_depth,
                    &capture,
                    in_head,
                    seen_body,
                    &mut w,
                    &mut head_title,
                    &mut inline_css,
                    &mut heading,
                );
            }
            Event::GeneralRef(ref r) => {
                let name: &str = r;
                let text = entities::resolve(name).unwrap_or_else(|| format!("&{name};"));
                feed_text(
                    &text,
                    skip_depth,
                    &capture,
                    in_head,
                    seen_body,
                    &mut w,
                    &mut head_title,
                    &mut inline_css,
                    &mut heading,
                );
            }

            // Comments, processing instructions, doctype: dropped.
            _ => {}
        }
    }

    // STEP 4: link the (scrubbed, copied) stylesheets relative to the chapter.
    let links: Vec<String> = stylesheets
        .iter()
        .filter_map(|p| (ctx.map)(p))
        .map(|pkg| relative_path(ctx.out_path, &pkg))
        .collect();

    // STEP 5: assemble every part with the same head (title, language, styles).
    let head_title = Some(collapse_ws(&head_title)).filter(|t| !t.is_empty());
    let css = scrub_stylesheet(&inline_css);
    done.push((w, first_heading, ids));
    let count = done.len();
    done.into_iter()
        .enumerate()
        .map(|(i, (writer, heading, ids))| {
            // Later parts are titled by their own first heading when they have one.
            let title = if i == 0 {
                title_hint
                    .map(str::to_string)
                    .or_else(|| head_title.clone())
            } else {
                heading.clone()
            }
            .or_else(|| title_hint.map(str::to_string))
            .unwrap_or_default();
            let (xhtml, stats) = writer.finish(
                &title,
                lang.as_deref().or(ctx.default_lang),
                &links,
                Some(&css),
            );
            Part {
                normalized: Normalized {
                    xhtml,
                    stats,
                    head_title: head_title.clone(),
                    first_heading: heading,
                    warnings: if i + 1 == count {
                        warnings.clone()
                    } else {
                        vec![]
                    },
                },
                ids,
            }
        })
        .collect()
}

/// Route one run of text to wherever the parser state says it belongs.
#[allow(clippy::too_many_arguments)]
fn feed_text(
    text: &str,
    skip_depth: usize,
    capture: &Capture,
    in_head: bool,
    seen_body: bool,
    w: &mut ChapterWriter,
    head_title: &mut String,
    inline_css: &mut String,
    heading: &mut Option<(String, String)>,
) {
    if skip_depth > 0 {
        return;
    }
    match capture {
        Capture::Title => head_title.push_str(text),
        Capture::Style => {
            inline_css.push_str(text);
            inline_css.push('\n');
        }
        Capture::None => {
            // Head text and pre-body whitespace are not content.
            if in_head || (!seen_body && text.trim().is_empty()) {
                return;
            }
            if let Some((_, h)) = heading.as_mut() {
                h.push_str(text);
            }
            w.text(text);
        }
    }
}

/// Lower-case local name (namespace prefix stripped).
fn local_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.local_name().as_ref().as_bytes()).to_ascii_lowercase()
}

/// Value of attribute `key` (exact qualified name, ASCII case-insensitive).
fn attr(e: &BytesStart<'_>, key: &str) -> Option<String> {
    e.html_attributes()
        .flatten()
        .find(|a| a.key.as_ref().eq_ignore_ascii_case(key))
        .map(|a| entities::unescape(&a.value))
}

/// Legacy/presentational tags → modern equivalents.
fn map_legacy(name: &str) -> &str {
    match name {
        "center" => "div",
        "font" | "big" | "blink" => "span",
        "tt" => "code",
        "strike" => "s",
        "acronym" => "abbr",
        "listing" | "xmp" | "plaintext" => "pre",
        n => n,
    }
}

/// Filter the attributes of an HTML element (href/src are handled by the caller).
fn html_attrs(e: &BytesStart<'_>, element: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for a in e.html_attributes().flatten() {
        let key = a.key.as_ref().to_ascii_lowercase();
        let value = entities::unescape(&a.value);
        let kept = match key.as_str() {
            k if ALLOWED_ATTRS.contains(&k) => Some((key.clone(), value)),
            "lang" | "xml:lang" => Some(("lang".to_string(), value)),
            "style" => Some(("style".to_string(), scrub_inline(&value))),
            "epub:type" => Some(("data-epub-type".to_string(), value)),
            "type" if matches!(element, "ol" | "ul" | "li") => Some((key.clone(), value)),
            k if k.starts_with("aria-") => Some((key.clone(), value)),
            _ => None,
        };
        // Avoid duplicates such as lang + xml:lang.
        if let Some((k, v)) = kept {
            if !out.iter().any(|(ek, _)| *ek == k) {
                out.push((k, v));
            }
        }
    }
    out
}

/// Filter SVG attributes: drop event handlers and script URLs, rewrite image refs.
fn svg_attrs(e: &BytesStart<'_>, ctx: &LinkContext<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in e.html_attributes().flatten() {
        let key = String::from_utf8_lossy(a.key.as_ref().as_bytes()).into_owned();
        let lower = key.to_ascii_lowercase();
        let value = entities::unescape(&a.value);
        if lower.starts_with("on")
            || lower.starts_with("xmlns")
            || value.to_ascii_lowercase().contains("javascript:")
        {
            continue;
        }
        if lower == "href" || lower == "xlink:href" {
            if value.starts_with('#') {
                out.push((key, value));
            } else if let Some(r) = rewrite_resource(&value, ctx) {
                out.push((key, r));
            }
            continue;
        }
        if lower == "style" {
            out.push((key, scrub_inline(&value)));
            continue;
        }
        out.push((key, value));
    }
    out
}

/// Rewrite an `<a href>`. Returns the attributes to add (possibly none).
///
/// * internal → `href` relative to the chapter + `data-jr-href` package path
/// * external web/mail → kept, marked `data-jr-external` (the reader never
///   navigates the reading view to the web)
/// * anything else (javascript:, unknown targets) → dropped
fn rewrite_link(href: &str, ctx: &LinkContext<'_>, defer: bool) -> Vec<(String, String)> {
    let href = href.trim();
    if href.is_empty() {
        return vec![];
    }

    // STEP 0: deferred mode — record the resolved target, decide later.
    if defer && !is_external(href) {
        let Ok(resolved) = resolve_reference(ctx.source_path, href) else {
            return vec![];
        };
        let target = format!("{resolved}#{}", fragment(href).unwrap_or(""));
        return vec![
            ("href".into(), format!("{LINK_HREF}{target}{LINK_END}")),
            (
                "data-jr-href".into(),
                format!("{LINK_PKG}{target}{LINK_END}"),
            ),
        ];
    }

    // STEP 1: same-document fragment.
    if let Some(frag) = href.strip_prefix('#') {
        return vec![
            ("href".into(), href.to_string()),
            ("data-jr-href".into(), format!("{}#{frag}", ctx.out_path)),
        ];
    }

    // STEP 2: external links.
    if is_external(href) {
        let l = href.to_ascii_lowercase();
        if l.starts_with("http:") || l.starts_with("https:") || l.starts_with("mailto:") {
            return vec![
                ("href".into(), href.to_string()),
                ("data-jr-external".into(), "1".into()),
            ];
        }
        return vec![];
    }

    // STEP 3: internal links to other documents in the book.
    let Ok(resolved) = resolve_reference(ctx.source_path, href) else {
        return vec![];
    };
    let Some(pkg) = (ctx.map)(&resolved) else {
        return vec![];
    };
    let frag = fragment(href).map(|f| format!("#{f}")).unwrap_or_default();
    vec![
        (
            "href".into(),
            format!("{}{frag}", relative_path(ctx.out_path, &pkg)),
        ),
        ("data-jr-href".into(), format!("{pkg}{frag}")),
    ]
}

/// Rewrite a resource reference (img src, svg image href) to a path relative
/// to the chapter. `None` if it is remote or not part of the book.
fn rewrite_resource(src: &str, ctx: &LinkContext<'_>) -> Option<String> {
    let src = src.trim();
    if src.is_empty() || is_external(src) {
        return None; // offline-only: remote images are never fetched
    }
    let resolved = resolve_reference(ctx.source_path, src).ok()?;
    let pkg = (ctx.map)(&resolved)?;
    Some(relative_path(ctx.out_path, &pkg))
}

/// Collapse runs of whitespace and trim.
pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Normalized {
        let map = |p: &str| -> Option<String> {
            match p {
                "OEBPS/ch2.xhtml" => Some("content/c0002.xhtml".into()),
                "OEBPS/img/a.png" => Some("resources/OEBPS/img/a.png".into()),
                "OEBPS/style.css" => Some("resources/OEBPS/style.css".into()),
                _ => None,
            }
        };
        let ctx = LinkContext {
            source_path: "OEBPS/ch1.xhtml",
            out_path: "content/c0001.xhtml",
            map: &map,
            default_lang: None,
        };
        normalize(src, &ctx, None)
    }

    #[test]
    fn splits_long_documents_between_top_level_blocks() {
        // STEP 1: 30 paragraphs of 100 chars, split every ~1000 chars, one link and one id.
        let para = "x".repeat(100);
        let mut src = String::from(r##"<html><body><p><a href="#far">jump</a></p>"##);
        for i in 0..30 {
            src.push_str(&format!("<div><p id=\"p{i}\">{para}</p></div>"));
        }
        src.push_str(r#"<p id="far">end</p></body></html>"#);
        let map = |_: &str| None;
        let ctx = LinkContext {
            source_path: "doc.html",
            out_path: "content/c0001.xhtml",
            map: &map,
            default_lang: None,
        };
        let parts = normalize_parts(
            &src,
            &ctx,
            Some("T"),
            NormalizeOptions {
                split_chars: Some(1000),
                defer_links: true,
            },
        );

        // STEP 2: several parts, each restarting block numbering and never cutting a <div>.
        assert!(parts.len() >= 3, "{}", parts.len());
        for p in &parts {
            assert!(p.normalized.xhtml.contains(r#"<body data-jrb="0">"#));
            assert_eq!(
                p.normalized.xhtml.matches("<div").count(),
                p.normalized.xhtml.matches("</div>").count()
            );
        }
        // STEP 3: the link is a placeholder; the target id lives in the last part.
        assert!(parts[0].normalized.xhtml.contains("@@JRH:doc.html#far@@"));
        assert!(parts.last().unwrap().ids.contains(&"far".to_string()));
        assert_eq!(parts.iter().map(|p| p.ids.len()).sum::<usize>(), 31);
    }

    #[test]
    fn sanitizes_and_rewrites() {
        let n = run(r##"<?xml version="1.0"?><!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en"><head><title>One</title>
<link rel="stylesheet" href="style.css"/><style>p{color:red}</style><script>alert(1)</script></head>
<body class="calibre"><h1 onclick="x()">Chapter&nbsp;1</h1>
<p style="color:blue" data-x="1">Go <a href="ch2.xhtml#s2">next</a>, <a href="#top">top</a>,
<a href="https://example.org">web</a>, <a href="javascript:alert(1)">bad</a>.</p>
<img src="img/a.png" alt="A"/><img src="http://x/y.png" alt="remote"/>
<script>evil()</script><form><input value="x"/>kept</form><custom>unwrapped</custom></body></html>"##);
        let x = &n.xhtml;
        assert_eq!(n.head_title.as_deref(), Some("One"));
        // nbsp is not whitespace for split_whitespace? It is (Unicode White_Space), so it collapses.
        assert_eq!(n.first_heading.as_deref(), Some("Chapter 1"));
        assert!(x.contains(r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="en""#));
        assert!(x.contains(
            r#"<link rel="stylesheet" type="text/css" href="../resources/OEBPS/style.css" />"#
        ));
        assert!(x.contains("<style>p{color:red}"));
        assert!(
            !x.contains("alert")
                && !x.contains("evil")
                && !x.contains("onclick")
                && !x.contains("data-x")
        );
        assert!(x.contains(r#"<body class="calibre" data-jrb="0">"#));
        assert!(x.contains(r#"<h1 data-jrb="1">Chapter"#));
        assert!(x.contains(r#"<p style="color:blue" data-jrb="2">"#));
        assert!(x.contains(
            r##"<a href="c0002.xhtml#s2" data-jr-href="content/c0002.xhtml#s2">next</a>"##
        ));
        assert!(x.contains(r##"<a href="#top" data-jr-href="content/c0001.xhtml#top">top</a>"##));
        assert!(x.contains(r#"<a href="https://example.org" data-jr-external="1">web</a>"#));
        assert!(x.contains("<a>bad</a>"));
        assert!(x.contains(r#"<img alt="A" src="../resources/OEBPS/img/a.png" />"#));
        assert!(x.contains("remote") && !x.contains("http://x/y.png"));
        assert!(x.contains("kept") && !x.contains("<input"));
        assert!(x.contains("unwrapped") && !x.contains("<custom"));
    }

    #[test]
    fn tolerates_broken_markup() {
        // Mismatched/unclosed tags and a bare ampersand.
        let n = run("<html><body><p>A & B <b>bold<i>both</b> tail<p>next</body></html>");
        assert!(n.xhtml.contains("A &amp; B"), "{}", n.xhtml);
        assert!(n.xhtml.contains("next"));
        assert!(n.xhtml.ends_with("</body>\n</html>\n"));
        assert!(n.stats.blocks >= 3);
    }

    #[test]
    fn keeps_svg_images() {
        let n = run(
            r#"<html><body><svg viewBox="0 0 10 10" onload="x()"><image xlink:href="img/a.png" width="10"/></svg></body></html>"#,
        );
        assert!(
            n.xhtml.contains(
                r#"<image xlink:href="../resources/OEBPS/img/a.png" width="10"></image>"#
            ),
            "{}",
            n.xhtml
        );
        assert!(!n.xhtml.contains("onload"));
    }

    #[test]
    fn legacy_tags_are_mapped() {
        let n = run("<html><body><center><font>x</font></center><tt>y</tt></body></html>");
        assert!(
            n.xhtml
                .contains(r#"<div data-jrb="1"><span>x</span></div><code>y</code>"#),
            "{}",
            n.xhtml
        );
    }
}
