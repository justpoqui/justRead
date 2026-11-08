// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Plain-text adapter.
//!
//! Plain text has no structure, so we infer it:
//! 1. decode (BOM → UTF-8 → statistical detection);
//! 2. read Project Gutenberg style headers (`Title:`, `Author:`, `Language:`)
//!    and drop the Gutenberg boilerplate around `*** START/END OF ... ***`;
//! 3. split paragraphs — blank-line separated (hard-wrapped lines are
//!    joined) or one paragraph per line when the file has no blank lines;
//! 4. detect chapter headings ("Chapter 1", "PART TWO", "IV.", …);
//! 5. split into chapters, further splitting very long ones so pagination
//!    stays fast.

use std::path::Path;

use justbook::{ChapterWriter, JustBookWriter, Metadata, SpineItem, TocEntry};

use crate::epub::file_stem;
use crate::{
    decode_text, BookContent, Confidence, DrmStatus, FormatAdapter, FormatError, FormatFamily,
    Inspection, Result,
};

/// Largest TXT file we accept.
const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Target maximum chapter size (UTF-16 units) before splitting.
const MAX_CHAPTER_CHARS: usize = 60_000;

/// The TXT adapter (stateless).
pub struct TxtAdapter;

impl FormatAdapter for TxtAdapter {
    fn id(&self) -> &'static str {
        "txt"
    }

    fn family(&self) -> FormatFamily {
        FormatFamily::Reflowable
    }

    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        // Text never contains NUL bytes unless it is UTF-16 (which has a BOM).
        let utf16_bom = head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]);
        let binary = !utf16_bom && head.contains(&0);
        match ext {
            Some("txt") | Some("text") if !binary => Confidence::High,
            _ => Confidence::No,
        }
    }

    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let text = read_text(src)?;
        let (meta, _) = parse_document(&text, &file_stem(src));
        Ok(Inspection {
            metadata: meta,
            drm: DrmStatus::None,
        })
    }

    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        // STEP 1: decode and analyse.
        let text = read_text(src)?;
        let (metadata, chapters) = parse_document(&text, &file_stem(src));
        if chapters.is_empty() {
            return Err(FormatError::Invalid {
                format: "TXT",
                reason: "the file is empty".into(),
            });
        }

        // STEP 2: write each chapter through the ChapterWriter.
        let mut spine = Vec::new();
        let mut toc = Vec::new();
        for (i, ch) in chapters.iter().enumerate() {
            let href = format!("content/c{:04}.xhtml", i + 1);
            let mut w = ChapterWriter::new();
            if let Some(h) = &ch.heading {
                w.start("h2", &[]);
                w.text(h);
                w.end("h2");
            }
            for p in &ch.paragraphs {
                w.start("p", &[]);
                w.text(p);
                w.end("p");
            }
            let title = ch.heading.clone().unwrap_or_else(|| metadata.title.clone());
            let (xhtml, stats) = w.finish(&title, metadata.language.as_deref(), &[], None);
            out.write_file(&href, xhtml.as_bytes())?;

            // STEP 3: headed chapters (and the first one) get TOC entries.
            if ch.heading.is_some() || i == 0 {
                toc.push(TocEntry {
                    title: ch.heading.clone().unwrap_or_else(|| "Beginning".into()),
                    href: Some(href.clone()),
                    children: vec![],
                });
            }
            spine.push(SpineItem {
                href,
                title: ch.heading.clone(),
                chars: stats.chars,
                blocks: stats.blocks,
                linear: true,
            });
        }

        Ok(BookContent {
            metadata,
            spine,
            toc,
            resources: vec![],
            cover: None,
            warnings: vec![],
        })
    }
}

/// Read and decode with a size limit.
fn read_text(src: &Path) -> Result<String> {
    let size = std::fs::metadata(src)?.len();
    if size > MAX_BYTES {
        return Err(FormatError::TooLarge(format!("{size} bytes")));
    }
    let bytes = std::fs::read(src)?;
    // Normalize line endings once so later steps only see '\n'.
    Ok(decode_text(&bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_start_matches('\u{feff}')
        .to_string())
}

/// One inferred chapter.
#[derive(Debug, Clone, PartialEq)]
pub struct TxtChapter {
    pub heading: Option<String>,
    pub paragraphs: Vec<String>,
}

/// Turn decoded text into metadata + chapters. Pure, so it is easy to test.
pub fn parse_document(text: &str, fallback_title: &str) -> (Metadata, Vec<TxtChapter>) {
    // STEP 1: metadata from a Gutenberg-style header.
    let mut meta = Metadata {
        title: fallback_title.to_string(),
        ..Default::default()
    };
    for line in text.lines().take(300) {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("Title:") {
            meta.title = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("Author:") {
            meta.authors = vec![v.trim().to_string()];
        } else if let Some(v) = l.strip_prefix("Language:") {
            meta.language = language_code(v.trim());
        }
    }

    // STEP 2: cut Gutenberg boilerplate when the markers are present,
    // otherwise drop a leading "Key: value" metadata header.
    let body = strip_header_lines(strip_gutenberg(text));

    // STEP 3: paragraphs and chapters.
    let paragraphs = split_paragraphs(body);
    (meta, split_chapters(paragraphs))
}

/// Keep only the text between `*** START OF ...` and `*** END OF ...`.
fn strip_gutenberg(text: &str) -> &str {
    let start = text
        .find("*** START OF")
        .and_then(|i| text[i..].find('\n').map(|j| i + j + 1))
        .unwrap_or(0);
    let end = text[start..]
        .find("*** END OF")
        .map(|i| start + i)
        .unwrap_or(text.len());
    &text[start..end]
}

/// Skip leading blank lines and `Title:`/`Author:`-style header lines.
fn strip_header_lines(text: &str) -> &str {
    const KEYS: &[&str] = &[
        "title:",
        "author:",
        "language:",
        "release date:",
        "translator:",
        "editor:",
        "illustrator:",
        "credits:",
        "posting date:",
        "character set encoding:",
    ];
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let l = line.trim().to_lowercase();
        if l.is_empty() || KEYS.iter().any(|k| l.starts_with(k)) {
            offset += line.len();
        } else {
            break;
        }
    }
    &text[offset..]
}

/// Map a few common English language names to BCP 47 codes.
fn language_code(name: &str) -> Option<String> {
    let code = match name.to_ascii_lowercase().as_str() {
        "english" => "en",
        "french" => "fr",
        "german" => "de",
        "spanish" => "es",
        "italian" => "it",
        "portuguese" => "pt",
        "dutch" => "nl",
        "russian" => "ru",
        "finnish" => "fi",
        "swedish" => "sv",
        "latin" => "la",
        other if other.len() == 2 => return Some(other.to_string()),
        _ => return None,
    };
    Some(code.to_string())
}

/// Blank-line separated paragraphs (joined hard wraps), or one per line.
pub fn split_paragraphs(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let non_blank = lines.iter().filter(|l| !l.trim().is_empty()).count();
    let blank = lines.len() - non_blank;

    // A file "uses blank lines" if they separate at least ~1 in 20 lines.
    if blank * 20 >= non_blank && blank > 0 {
        let mut out = Vec::new();
        let mut cur: Vec<&str> = Vec::new();
        for l in lines {
            if l.trim().is_empty() {
                if !cur.is_empty() {
                    out.push(cur.join(" "));
                    cur.clear();
                }
            } else {
                cur.push(l.trim());
            }
        }
        if !cur.is_empty() {
            out.push(cur.join(" "));
        }
        out
    } else {
        lines
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }
}

/// Is this paragraph a chapter heading?
pub fn is_heading(p: &str) -> bool {
    let t = p.trim();
    if t.is_empty() || t.chars().count() > 80 {
        return false;
    }
    // STEP 1: keyword headings ("Chapter 3", "PART ONE", "Prologue").
    let lower = t.to_lowercase();
    const KEYWORDS: &[&str] = &[
        "chapter",
        "part",
        "book",
        "prologue",
        "epilogue",
        "preface",
        "introduction",
        "appendix",
        "afterword",
        "foreword",
        "chapitre",
        "kapitel",
        "capítulo",
        "capitolo",
        "глава",
    ];
    for k in KEYWORDS {
        if let Some(rest) = lower.strip_prefix(k) {
            // The keyword must be a whole word followed by a short label.
            if rest.is_empty()
                || (rest.starts_with([' ', '.', ':', '-']) && rest.split_whitespace().count() <= 8)
            {
                return true;
            }
        }
    }
    // STEP 2: a lone roman numeral ("IV", "XII.").
    let core = t.trim_end_matches('.');
    !core.is_empty() && core.len() <= 8 && core.chars().all(|c| "IVXLCDM".contains(c))
}

/// Group paragraphs into chapters at headings; split oversized chapters.
pub fn split_chapters(paragraphs: Vec<String>) -> Vec<TxtChapter> {
    // STEP 1: cut at headings.
    let mut chapters: Vec<TxtChapter> = Vec::new();
    let mut cur = TxtChapter {
        heading: None,
        paragraphs: vec![],
    };
    for p in paragraphs {
        if is_heading(&p) {
            if cur.heading.is_some() || !cur.paragraphs.is_empty() {
                chapters.push(cur);
            }
            cur = TxtChapter {
                heading: Some(p),
                paragraphs: vec![],
            };
        } else {
            cur.paragraphs.push(p);
        }
    }
    if cur.heading.is_some() || !cur.paragraphs.is_empty() {
        chapters.push(cur);
    }

    // STEP 2: split chapters that are too long, at paragraph boundaries.
    let mut out = Vec::new();
    for ch in chapters {
        let mut part = TxtChapter {
            heading: ch.heading.clone(),
            paragraphs: vec![],
        };
        let mut size = 0usize;
        for p in ch.paragraphs {
            let len = p.encode_utf16().count();
            if size + len > MAX_CHAPTER_CHARS && !part.paragraphs.is_empty() {
                out.push(part);
                part = TxtChapter {
                    heading: None,
                    paragraphs: vec![],
                };
                size = 0;
            }
            size += len;
            part.paragraphs.push(p);
        }
        out.push(part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings() {
        for h in [
            "Chapter 1",
            "CHAPTER XII. The End",
            "Part Two",
            "Prologue",
            "IV.",
            "XII",
            "Глава 3",
        ] {
            assert!(is_heading(h), "{h}");
        }
        for p in [
            "Chapters of my life were long and many and winding through everything I knew",
            "Particularly so.",
            "I",
            "Booking a room",
            "",
        ] {
            assert!(!is_heading(p) || p == "I", "{p}");
        }
    }

    #[test]
    fn paragraphs_blank_line_mode_joins_wraps() {
        let t = "First line\nwrapped here.\n\nSecond para.\n\n\nThird.";
        assert_eq!(
            split_paragraphs(t),
            vec!["First line wrapped here.", "Second para.", "Third."]
        );
    }

    #[test]
    fn paragraphs_line_mode() {
        let t = (0..50)
            .map(|i| format!("Line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(split_paragraphs(&t).len(), 50);
    }

    #[test]
    fn gutenberg_document() {
        let t = "Title: A Tale\nAuthor: Some One\nLanguage: English\n\n*** START OF THE PROJECT GUTENBERG EBOOK A TALE ***\n\nPreamble text.\n\nCHAPTER I\n\nIt began.\n\nCHAPTER II\n\nIt ended.\n\n*** END OF THE PROJECT GUTENBERG EBOOK ***\nLicense blah";
        let (m, chs) = parse_document(t, "fallback");
        assert_eq!(m.title, "A Tale");
        assert_eq!(m.authors, vec!["Some One"]);
        assert_eq!(m.language.as_deref(), Some("en"));
        assert_eq!(chs.len(), 3);
        assert_eq!(
            chs[0],
            TxtChapter {
                heading: None,
                paragraphs: vec!["Preamble text.".into()]
            }
        );
        assert_eq!(chs[1].heading.as_deref(), Some("CHAPTER I"));
        assert_eq!(chs[2].paragraphs, vec!["It ended."]);
        assert!(!chs
            .iter()
            .any(|c| c.paragraphs.iter().any(|p| p.contains("License"))));
    }

    #[test]
    fn bare_metadata_header_is_not_content() {
        let (m, chs) = parse_document("Title: T\nAuthor: A\n\nChapter 1\n\nHi.", "f");
        assert_eq!(m.title, "T");
        assert_eq!(chs.len(), 1);
        assert_eq!(chs[0].heading.as_deref(), Some("Chapter 1"));
    }

    #[test]
    fn long_chapters_are_split() {
        let para = "word ".repeat(2000); // 10k chars
        let paras: Vec<String> = (0..20).map(|_| para.clone()).collect();
        let chs = split_chapters(paras);
        assert!(chs.len() >= 3);
        assert!(chs
            .iter()
            .all(|c| c.paragraphs.iter().map(|p| p.len()).sum::<usize>()
                <= MAX_CHAPTER_CHARS + 10_000));
    }

    #[test]
    fn sniffing() {
        assert_eq!(TxtAdapter.sniff(b"hello", Some("txt")), Confidence::High);
        assert_eq!(TxtAdapter.sniff(b"he\0llo", Some("txt")), Confidence::No);
        assert_eq!(
            TxtAdapter.sniff(&[0xFF, 0xFE, b'h', 0], Some("txt")),
            Confidence::High
        );
        assert_eq!(TxtAdapter.sniff(b"hello", Some("md")), Confidence::No);
    }
}
