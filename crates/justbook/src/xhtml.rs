// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Building normalized justBook chapters.
//!
//! [`ChapterWriter`] is the only way adapters produce chapter XHTML. It
//! guarantees three properties the reader depends on:
//!
//! 1. **Well-formed output** — every element is closed, stray end tags are
//!    dropped, text and attributes are escaped.
//! 2. **Parses identically as HTML** — the reader loads chapters through an
//!    HTML parser (`iframe.srcdoc`), so we never self-close non-void
//!    elements and we close an open `<p>` before block content, exactly as
//!    the HTML parser would.
//! 3. **Stable block ids** — every block element receives `data-jrb="N"`.
//!    Locators address text as *(block, character offset)*, which stays
//!    valid when the reader later wraps text in highlight `<mark>`s.
//!    `<body>` is always block 0 so that no text is ever un-addressable.

/// Elements that get a `data-jrb` block id.
const BLOCK_ELEMENTS: &[&str] = &[
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "dt",
    "dd",
    "blockquote",
    "pre",
    "figure",
    "figcaption",
    "td",
    "th",
    "caption",
    "div",
    "section",
    "article",
    "aside",
    "header",
    "footer",
    "address",
    "nav",
    "main",
    "details",
    "summary",
];

/// Elements whose start implicitly closes an open `<p>` in the HTML parser.
const CLOSES_P: &[&str] = &[
    "p",
    "div",
    "ul",
    "ol",
    "dl",
    "table",
    "blockquote",
    "pre",
    "figure",
    "section",
    "article",
    "aside",
    "header",
    "footer",
    "address",
    "nav",
    "main",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "details",
    "fieldset",
];

/// Void elements: written as `<x ... />` and never pushed on the stack.
const VOID_ELEMENTS: &[&str] = &["br", "hr", "img", "wbr", "col", "area", "source"];

/// Per-chapter statistics recorded in the manifest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChapterStats {
    /// Text length in UTF-16 code units (same unit as DOM string offsets).
    pub chars: u64,
    /// Number of stamped blocks, including `<body>` (block 0).
    pub blocks: u32,
}

/// Incrementally writes the `<body>` of one chapter. See module docs.
#[derive(Debug)]
pub struct ChapterWriter {
    body: String,
    stack: Vec<String>,
    stats: ChapterStats,
    /// Depth inside `<svg>`; SVG children are not HTML blocks or voids.
    in_svg: usize,
    /// Attributes copied onto `<body>` (e.g. `class`, which book CSS targets).
    body_attrs: Vec<(String, String)>,
}

impl Default for ChapterWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ChapterWriter {
    /// Start an empty chapter. Block 0 is reserved for `<body>`.
    pub fn new() -> Self {
        Self {
            body: String::new(),
            stack: Vec::new(),
            stats: ChapterStats {
                chars: 0,
                blocks: 1,
            },
            in_svg: 0,
            body_attrs: Vec::new(),
        }
    }

    /// Set sanitized attributes for the `<body>` element.
    pub fn set_body_attrs(&mut self, attrs: Vec<(String, String)>) {
        self.body_attrs = attrs.into_iter().filter(|(k, _)| k != "data-jrb").collect();
    }

    /// Open element `name` (lower-case local name) with already-sanitized
    /// attributes. Void elements are written self-closed automatically.
    pub fn start(&mut self, name: &str, attrs: &[(&str, &str)]) {
        // STEP 1: emulate the HTML parser closing a paragraph before block content.
        if self.in_svg == 0 && CLOSES_P.contains(&name) && self.is_open("p") {
            self.end("p");
        }

        // STEP 2: write the tag and its attributes (dropping any forged data-jrb).
        self.body.push('<');
        self.body.push_str(name);
        if name == "svg" && self.in_svg == 0 {
            self.body.push_str(
                r#" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#,
            );
        }
        for (k, v) in attrs {
            if *k == "data-jrb" || k.starts_with("xmlns") {
                continue;
            }
            self.body.push(' ');
            self.body.push_str(k);
            self.body.push_str("=\"");
            escape_into(&mut self.body, v, true);
            self.body.push('"');
        }

        // STEP 3: stamp a block id on HTML block elements (not inside SVG).
        if self.in_svg == 0 && BLOCK_ELEMENTS.contains(&name) {
            self.body
                .push_str(&format!(" data-jrb=\"{}\"", self.stats.blocks));
            self.stats.blocks += 1;
        }

        // STEP 4: void elements end here; others are remembered for closing.
        if self.in_svg == 0 && VOID_ELEMENTS.contains(&name) {
            self.body.push_str(" />");
        } else {
            self.body.push('>');
            if name == "svg" {
                self.in_svg += 1;
            }
            self.stack.push(name.to_string());
        }
    }

    /// Append character data (unescaped input; escaped here).
    pub fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.stats.chars += text.encode_utf16().count() as u64;
        escape_into(&mut self.body, text, false);
    }

    /// Close element `name`. If it is open, any elements opened after it are
    /// closed too (mis-nested input); if it isn't open, the call is ignored.
    pub fn end(&mut self, name: &str) {
        if !self.is_open(name) {
            return;
        }
        while let Some(top) = self.stack.pop() {
            self.body.push_str("</");
            self.body.push_str(&top);
            self.body.push('>');
            if top == "svg" {
                self.in_svg -= 1;
            }
            if top == name {
                break;
            }
        }
    }

    /// Is an element called `name` currently open?
    pub fn is_open(&self, name: &str) -> bool {
        self.stack.iter().any(|n| n == name)
    }

    /// Statistics so far.
    pub fn stats(&self) -> ChapterStats {
        self.stats
    }

    /// Close everything and wrap the body in a full XHTML document.
    ///
    /// * `stylesheets` — hrefs relative to the chapter file.
    /// * `inline_css` — already-scrubbed CSS placed in a `<style>` element.
    pub fn finish(
        mut self,
        title: &str,
        lang: Option<&str>,
        stylesheets: &[String],
        inline_css: Option<&str>,
    ) -> (String, ChapterStats) {
        // STEP 1: close whatever the source left open.
        while let Some(top) = self.stack.last().cloned() {
            self.end(&top);
        }

        // STEP 2: <html> with namespaces and language.
        let mut doc = String::with_capacity(self.body.len() + 512);
        doc.push_str("<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"");
        if let Some(l) = lang {
            doc.push_str(" lang=\"");
            escape_into(&mut doc, l, true);
            doc.push_str("\" xml:lang=\"");
            escape_into(&mut doc, l, true);
            doc.push('"');
        }
        doc.push_str(">\n");

        // STEP 3: <head>. The reader injects its own <base> and styles right
        // after the literal "<head>" marker, so keep it exactly like this.
        doc.push_str("<head>\n<meta charset=\"utf-8\" />\n<title>");
        escape_into(&mut doc, title, false);
        doc.push_str("</title>\n");
        for href in stylesheets {
            doc.push_str("<link rel=\"stylesheet\" type=\"text/css\" href=\"");
            escape_into(&mut doc, href, true);
            doc.push_str("\" />\n");
        }
        if let Some(css) = inline_css.filter(|c| !c.trim().is_empty()) {
            doc.push_str("<style>");
            // "</" inside <style> would end the element early when parsed as HTML.
            doc.push_str(&css.replace("</", "<\\/"));
            doc.push_str("</style>\n");
        }
        doc.push_str("</head>\n");

        // STEP 4: body is block 0.
        doc.push_str("<body");
        for (k, v) in &self.body_attrs {
            doc.push(' ');
            doc.push_str(k);
            doc.push_str("=\"");
            escape_into(&mut doc, v, true);
            doc.push('"');
        }
        doc.push_str(" data-jrb=\"0\">");
        doc.push_str(&self.body);
        doc.push_str("</body>\n</html>\n");
        (doc, self.stats)
    }
}

/// Escape `s` for XML text (or attribute values when `attr` is true).
pub fn escape_into(out: &mut String, s: &str, attr: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            // Control characters other than tab/newline are invalid in XML.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
}

/// Escape to a new string.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    escape_into(&mut out, s, true);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_blocks_and_counts_utf16() {
        // STEP 1: two paragraphs, one with a non-BMP character (2 UTF-16 units).
        let mut w = ChapterWriter::new();
        w.start("p", &[]);
        w.text("Hi 😀");
        w.end("p");
        w.start("p", &[("class", "x")]);
        w.text("a<b");
        w.end("p");
        let (doc, stats) = w.finish("T", Some("en"), &[], None);

        // STEP 2: body + 2 paragraphs = 3 blocks; "Hi 😀" is 5 units, "a<b" is 3.
        assert_eq!(
            stats,
            ChapterStats {
                chars: 8,
                blocks: 3
            }
        );
        assert!(doc.contains(r#"<body data-jrb="0"><p data-jrb="1">Hi 😀</p><p class="x" data-jrb="2">a&lt;b</p></body>"#));
        assert!(doc.contains("<head>\n"));
    }

    #[test]
    fn repairs_bad_nesting() {
        let mut w = ChapterWriter::new();
        w.start("p", &[]);
        w.start("em", &[]);
        w.text("x");
        w.end("p"); // closes <em> too
        w.end("span"); // stray: ignored
        w.start("p", &[]);
        w.start("div", &[]); // closes the open <p> first, like the HTML parser
        w.text("y");
        let (doc, _) = w.finish("T", None, &[], None);
        assert!(doc.contains(r#"<p data-jrb="1"><em>x</em></p><p data-jrb="2"></p><div data-jrb="3">y</div></body>"#), "{doc}");
    }

    #[test]
    fn voids_svg_and_forged_attributes() {
        let mut w = ChapterWriter::new();
        w.start("img", &[("src", "a\"b.png"), ("data-jrb", "99")]);
        w.start("svg", &[("viewBox", "0 0 1 1")]);
        w.start("image", &[("xlink:href", "c.png")]);
        w.end("image");
        w.start("div", &[]); // inside SVG: no block id, no void handling
        w.end("svg");
        let (doc, stats) = w.finish("T", None, &["s.css".into()], Some("p{}</style>"));
        assert!(doc.contains(r#"<img src="a&quot;b.png" />"#));
        assert!(doc.contains(r#"<svg xmlns="http://www.w3.org/2000/svg""#));
        assert!(doc.contains(r#"<image xlink:href="c.png"></image><div></div></svg>"#));
        assert!(doc.contains(r#"<link rel="stylesheet" type="text/css" href="s.css" />"#));
        assert!(doc.contains("p{}<\\/style>"));
        assert_eq!(stats.blocks, 1);
    }

    #[test]
    fn strips_control_characters() {
        assert_eq!(escape("a\u{1}b\tc"), "ab\tc");
    }
}
