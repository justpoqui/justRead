// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! MOBI 6 text → HTML documents.
//!
//! MOBI 6 text is one HTML stream. Links are byte offsets
//! (`<a filepos=12345>`), images are record numbers (`<img recindex="3">`),
//! and `<mbp:pagebreak/>` separates chapters. We insert `<a id="fpN">`
//! anchors at every link target (byte positions are exact before decoding),
//! split at page breaks, and rewrite links/images to virtual paths.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::indx::read_index;
use super::{insert_all, replace_attr, Book};
use crate::epub::toc::RawEntry;
use crate::htmlbook::{HtmlBook, VirtualDoc, VirtualResource};
use crate::Result;

pub fn convert(book: &Book, src: &Path) -> Result<HtmlBook> {
    let text = book.text()?;
    let lower = text.to_ascii_lowercase();

    // STEP 1: every byte offset something links to (inline links + NCX).
    let mut targets: BTreeSet<usize> = BTreeSet::new();
    let mut i = 0;
    while let Some(p) = find(&lower, b"filepos=", i) {
        let mut j = p + 8;
        while j < text.len() && (text[j] == b'"' || text[j] == b'\'') {
            j += 1;
        }
        let digits: String = text[j..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .map(|&b| b as char)
            .collect();
        if let Ok(v) = digits.parse::<usize>() {
            if v < text.len() {
                targets.insert(v);
            }
        }
        i = p + 8;
    }
    let (ncx, cncx) = book
        .rec(book.h.ncx)
        .map(|r| read_index(&book.db, r))
        .unwrap_or_default();
    for e in &ncx {
        if let Some(pos) = e.tag(1, 0) {
            targets.insert(pos as usize);
        }
    }

    // STEP 2: chapter boundaries at page breaks.
    let mut starts = vec![0usize];
    let mut k = 0;
    while let Some(p) = find(&lower, b"<mbp:pagebreak", k) {
        if p > *starts.last().unwrap() + 1 {
            starts.push(p);
        }
        k = p + 1;
    }
    let chunk_of = |pos: usize| starts.iter().rposition(|&s| s <= pos).unwrap_or(0);
    let path_of = |c: usize| format!("c{:04}.html", c + 1);

    // STEP 3: build each chunk with anchors inserted at target offsets.
    let mut resources: Vec<VirtualResource> = Vec::new();
    let mut images: HashMap<u32, String> = HashMap::new();
    let mut docs = Vec::new();
    for (c, &start) in starts.iter().enumerate() {
        let end = starts.get(c + 1).copied().unwrap_or(text.len());
        let mut bytes = text[start..end].to_vec();
        let inserts = targets
            .range(start..end)
            .map(|&t| (t - start, format!("<a id=\"fp{t}\"></a>").into_bytes()))
            .collect();
        insert_all(&mut bytes, inserts);
        let html = book.h.decode(&bytes);

        // STEP 4: links → chunk#anchor, images → resource paths.
        let html = replace_attr(&html, "filepos", |v| {
            let n: usize = v
                .trim_matches(|c| c == '"' || c == '\'')
                .parse()
                .unwrap_or(0);
            format!("href=\"{}#fp{n}\"", path_of(chunk_of(n)))
        });
        let html = replace_attr(&html, "recindex", |v| {
            let n: u32 = v.parse().unwrap_or(0);
            match image_path(book, n, &mut images, &mut resources) {
                Some(p) => format!("src=\"{p}\""),
                None => String::new(),
            }
        });
        let html = replace_attr(&html, "hirecindex", |_| String::new());
        let html = replace_attr(&html, "lorecindex", |_| String::new());
        docs.push(VirtualDoc {
            path: format!("mobi/{}", path_of(c)),
            html,
        });
    }

    // STEP 5: TOC from the NCX index, nested by depth (tag 4).
    let mut toc: Vec<RawEntry> = Vec::new();
    let mut stack: Vec<(u32, RawEntry)> = Vec::new();
    for e in &ncx {
        let (Some(pos), Some(label)) = (e.tag(1, 0), e.tag(3, 0).and_then(|o| cncx.get(&o))) else {
            continue;
        };
        let depth = e.tag(4, 0).unwrap_or(0);
        let entry = RawEntry {
            title: label.trim().to_string(),
            href: Some(format!("mobi/{}#fp{pos}", path_of(chunk_of(pos as usize)))),
            children: vec![],
        };
        while stack.last().is_some_and(|(d, _)| *d >= depth) {
            let (_, done) = stack.pop().unwrap();
            match stack.last_mut() {
                Some((_, parent)) => parent.children.push(done),
                None => toc.push(done),
            }
        }
        stack.push((depth, entry));
    }
    while let Some((_, done)) = stack.pop() {
        match stack.last_mut() {
            Some((_, parent)) => parent.children.push(done),
            None => toc.push(done),
        }
    }

    // STEP 6: cover.
    let cover = book
        .cover_image()
        .and_then(|n| image_path(book, n, &mut images, &mut resources))
        .map(|p| format!("mobi/{p}"));
    Ok(HtmlBook {
        format: "MOBI",
        metadata: book.metadata(src),
        docs,
        resources,
        toc,
        cover,
        warnings: vec![],
    })
}

/// Register image `n` as a resource (once); returns its path relative to `mobi/`.
pub(super) fn image_path(
    book: &Book,
    n: u32,
    seen: &mut HashMap<u32, String>,
    resources: &mut Vec<VirtualResource>,
) -> Option<String> {
    if let Some(p) = seen.get(&n) {
        return Some(p.clone());
    }
    let (bytes, ext, mime) = book.image(n)?;
    let rel = format!("img/{n:05}.{ext}");
    resources.push(VirtualResource {
        path: format!("mobi/{rel}"),
        media_type: mime.into(),
        bytes: bytes.to_vec(),
    });
    seen.insert(n, rel.clone());
    Some(rel)
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}
