// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Phase 7: a book's highlights, notes and bookmarks as one standard
//! Markdown document, for sharing to any app, saving or copying.
//!
//! Only plain CommonMark is produced (headings, block quotes, lists,
//! emphasis), plus a small YAML front-matter block that most Markdown apps
//! either use or hide. No app-specific syntax, so the file reads well
//! everywhere, including as plain text.

use crate::{Annotation, AnnotationKind, BookSummary};

/// Highlight colors as words (the file must not depend on color rendering).
fn color_label(color: &str) -> Option<&'static str> {
    match color {
        "green" => Some("green"),
        "blue" => Some("blue"),
        "pink" => Some("pink"),
        "purple" => Some("purple"),
        "underline" => Some("underlined"),
        _ => None, // yellow is the default: not worth a label on every quote
    }
}

/// Build the Markdown document. `date` is the export date (`YYYY-MM-DD`).
pub fn annotations_markdown(book: &BookSummary, list: &[Annotation], date: &str) -> String {
    let mut out = String::new();
    let authors = book.authors.join(", ");
    let marks: Vec<&Annotation> = list
        .iter()
        .filter(|a| a.kind != AnnotationKind::Bookmark)
        .collect();
    let bookmarks: Vec<&Annotation> = list
        .iter()
        .filter(|a| a.kind == AnnotationKind::Bookmark)
        .collect();
    let notes = marks.iter().filter(|a| !a.note.trim().is_empty()).count();

    // STEP 1: front matter (quoted YAML strings, so titles with ':' stay valid).
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", yaml_str(&book.title)));
    if !authors.is_empty() {
        out.push_str(&format!("author: {}\n", yaml_str(&authors)));
    }
    out.push_str(&format!("exported: {date}\n"));
    out.push_str("source: justRead\n---\n\n");

    // STEP 2: title and a one-line summary.
    out.push_str(&format!("# {}\n\n", escape_inline(&book.title)));
    let mut summary = Vec::new();
    if !authors.is_empty() {
        summary.push(format!("by {}", escape_inline(&authors)));
    }
    summary.push(count(marks.len(), "highlight"));
    if notes > 0 {
        summary.push(count(notes, "note"));
    }
    if !bookmarks.is_empty() {
        summary.push(count(bookmarks.len(), "bookmark"));
    }
    out.push_str(&format!("*{}*\n", summary.join(" · ")));

    // STEP 3: highlights in reading order (the list already is), grouped by chapter.
    let mut chapter: Option<&str> = None;
    for (i, a) in marks.iter().enumerate() {
        let ch = a
            .chapter
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty());
        if i == 0 || ch != chapter {
            if let Some(c) = ch {
                out.push_str(&format!("\n## {}\n", escape_inline(c)));
            }
            chapter = ch;
        }
        out.push('\n');
        out.push_str(&quote_block(&a.quote));
        if let Some(label) = color_label(&a.color) {
            out.push_str(&format!(">\n> *({label})*\n"));
        }
        // Notes are the reader's own words: kept as written (they may be Markdown).
        let note = a.note.trim();
        if !note.is_empty() {
            out.push('\n');
            out.push_str(&format!("**Note:** {note}\n"));
        }
    }

    // STEP 4: bookmarks as a list.
    if !bookmarks.is_empty() {
        out.push_str("\n## Bookmarks\n\n");
        for b in bookmarks {
            let snippet = one_line(&b.quote);
            let mut line = match b
                .chapter
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
            {
                Some(c) => format!("- **{}**", escape_inline(c)),
                None => "-".to_string(),
            };
            if !snippet.is_empty() {
                if line.len() > 1 {
                    line.push_str(" —");
                }
                line.push_str(&format!(" “{}”", escape_inline(&snippet)));
            }
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// A file name for the export: the title, made safe on every OS.
pub fn export_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let squashed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = squashed.trim_matches(|c: char| c == '.' || c == ' ');
    // Keep names short enough for every file system (counting characters, not bytes).
    let short: String = trimmed.chars().take(100).collect();
    let base = if short.trim().is_empty() {
        "Book".to_string()
    } else {
        short.trim().to_string()
    };
    format!("{base} — highlights.md")
}

/// Civil date (UTC) for a Unix time in milliseconds, as `YYYY-MM-DD`.
/// (Days-to-date conversion after Howard Hinnant's well-known public algorithm.)
pub fn iso_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

// ----- helpers ---------------------------------------------------------------

fn count(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn yaml_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", one_line(&escaped))
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Book text is *text*, not Markdown: escape characters that would change
/// its meaning (emphasis, links, code, HTML) so it shows exactly as in the book.
fn escape_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '&' => out.push_str("&amp;"),
            _ => out.push(c),
        }
    }
    out
}

/// A quote as a block quote; paragraph breaks are kept, and a line that would
/// start a heading, list or rule is escaped.
fn quote_block(quote: &str) -> String {
    let paragraphs: Vec<String> = quote
        .split("\n\n")
        .map(one_line)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut p = escape_inline(&p);
            let starts_block = p.starts_with(['#', '-', '+', '='])
                || p.split_once(". ")
                    .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            if starts_block {
                p.insert(0, '\\');
            }
            p
        })
        .collect();
    let mut out = String::new();
    for (i, p) in paragraphs.iter().enumerate() {
        if i > 0 {
            out.push_str(">\n");
        }
        out.push_str(&format!("> {p}\n"));
    }
    if out.is_empty() {
        out.push_str(">\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use justread_locator::Locator;

    fn book() -> BookSummary {
        BookSummary {
            id: "b1".into(),
            title: "The Lantern: Harbor".into(),
            authors: vec!["Ann Lee".into(), "Bo Ma".into()],
            series: None,
            series_index: None,
            language: Some("en".into()),
            format: "epub".into(),
            cover: None,
            added_at: 0,
            last_opened_at: None,
            progression: 0.0,
        }
    }

    fn ann(
        kind: AnnotationKind,
        quote: &str,
        note: &str,
        color: &str,
        chapter: Option<&str>,
    ) -> Annotation {
        Annotation {
            id: "x".into(),
            book_id: "b1".into(),
            kind,
            start: "f:0/1:0".parse::<Locator>().unwrap(),
            end: None,
            quote: quote.into(),
            note: note.into(),
            color: color.into(),
            chapter: chapter.map(Into::into),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn builds_a_readable_document() {
        let list = vec![
            ann(
                AnnotationKind::Highlight,
                "First *light*.",
                "",
                "yellow",
                Some("Chapter 1"),
            ),
            ann(
                AnnotationKind::Note,
                "Second\n\nparagraph",
                "My **own** note",
                "green",
                Some("Chapter 1"),
            ),
            ann(
                AnnotationKind::Highlight,
                "# not a heading",
                "",
                "underline",
                Some("Chapter 2"),
            ),
            ann(
                AnnotationKind::Bookmark,
                "The road  went\non.",
                "",
                "yellow",
                Some("Chapter 3"),
            ),
        ];
        let md = annotations_markdown(&book(), &list, "2026-11-09");
        let expected = "---\n\
title: \"The Lantern: Harbor\"\n\
author: \"Ann Lee, Bo Ma\"\n\
exported: 2026-11-09\n\
source: justRead\n\
---\n\n\
# The Lantern: Harbor\n\n\
*by Ann Lee, Bo Ma · 3 highlights · 1 note · 1 bookmark*\n\n\
## Chapter 1\n\n\
> First \\*light\\*.\n\n\
> Second\n>\n> paragraph\n>\n> *(green)*\n\n\
**Note:** My **own** note\n\n\
## Chapter 2\n\n\
> \\# not a heading\n>\n> *(underlined)*\n\n\
## Bookmarks\n\n\
- **Chapter 3** — “The road went on.”\n";
        assert_eq!(md, expected);
    }

    #[test]
    fn empty_and_untitled() {
        let mut b = book();
        b.authors.clear();
        let md = annotations_markdown(&b, &[], "2026-01-01");
        assert!(md.contains("*0 highlights*"));
        assert!(!md.contains("author:"));
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(export_file_name("A/B: C?"), "A B C — highlights.md");
        assert_eq!(export_file_name("..."), "Book — highlights.md");
        assert_eq!(
            export_file_name(&"x".repeat(300)).chars().count(),
            100 + " — highlights.md".chars().count()
        );
    }

    #[test]
    fn dates() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_762_646_400_000), "2025-11-09");
        assert_eq!(iso_date(951_782_400_000), "2000-02-29");
    }
}
