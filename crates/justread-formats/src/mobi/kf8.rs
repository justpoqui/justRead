// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! KF8 (AZW3) → HTML documents.
//!
//! KF8 stores HTML files split into a *skeleton* (the outer markup) and
//! *fragments* (content) that must be inserted back into the skeleton at
//! recorded positions; the SKEL and FRAG indexes describe this. The text
//! stream is divided into "flows" by the FDST record: flow 0 is the HTML,
//! further flows are CSS/SVG. References use special URLs:
//!   kindle:pos:fid:XXXX:off:YYYYYYYYYY   link target (base-32)
//!   kindle:embed:XXXX                   image/font resource (base-32, 1-based)
//!   kindle:flow:XXXX                    a flow (CSS)
//! We rebuild each file, insert anchors for every link target, and rewrite
//! those URLs to virtual paths.

use std::collections::HashMap;
use std::path::Path;

use super::indx::{read_index, IndexEntry};
use super::palmdb::be32;
use super::{insert_all, Book};
use crate::epub::toc::RawEntry;
use crate::htmlbook::{HtmlBook, VirtualDoc, VirtualResource};
use crate::Result;

/// Base-32 digits 0-9A-V as used by KF8 URLs.
pub fn base32(s: &str) -> Option<u32> {
    let mut v: u32 = 0;
    for c in s.chars() {
        let d = c.to_digit(32)?;
        v = v.checked_mul(32)?.checked_add(d)?;
    }
    Some(v)
}

pub fn convert(book: &Book, src: &Path) -> Result<HtmlBook> {
    let raw = book.text()?;
    let mut warnings = Vec::new();

    // STEP 1: split the text into flows (FDST); without FDST everything is flow 0.
    let mut flows: Vec<Vec<u8>> = Vec::new();
    if let Some(f) = book
        .rec(book.h.fdst)
        .map(|r| book.db.record(r))
        .filter(|r| r.starts_with(b"FDST"))
    {
        let count = be32(f, 8) as usize;
        for i in 0..count.min(1000) {
            let (s, e) = (be32(f, 12 + i * 8) as usize, be32(f, 16 + i * 8) as usize);
            flows.push(
                raw.get(s.min(raw.len())..e.min(raw.len()))
                    .unwrap_or(&[])
                    .to_vec(),
            );
        }
    }
    if flows.is_empty() {
        flows.push(raw.clone());
    }
    let text = flows[0].clone();

    // STEP 2: rebuild files from skeletons + fragments.
    let skels = book
        .rec(book.h.skel)
        .map(|r| read_index(&book.db, r).0)
        .unwrap_or_default();
    let frags = book
        .rec(book.h.frag)
        .map(|r| read_index(&book.db, r).0)
        .unwrap_or_default();
    let (parts, frag_pos) = assemble(&text, &skels, &frags);

    // STEP 3: link targets (from the text and the NCX) → anchors.
    let (ncx, cncx) = book
        .rec(book.h.ncx)
        .map(|r| read_index(&book.db, r))
        .unwrap_or_default();
    let mut targets: Vec<(u32, u32)> = Vec::new();
    for p in &parts {
        let s = String::from_utf8_lossy(p);
        let mut from = 0;
        while let Some(i) = s[from..].find("kindle:pos:fid:").map(|i| i + from) {
            if let Some((fid, off)) = parse_pos(&s[i..]) {
                targets.push((fid, off));
            }
            from = i + 15;
        }
    }
    for e in &ncx {
        if let (Some(fid), Some(off)) = (e.tag(6, 0), e.tag(6, 1)) {
            targets.push((fid, off));
        }
    }
    targets.sort_unstable();
    targets.dedup();
    let mut per_part: HashMap<usize, Vec<(usize, Vec<u8>)>> = HashMap::new();
    let mut target_part: HashMap<(u32, u32), usize> = HashMap::new();
    for &(fid, off) in &targets {
        if let Some(&(part, pos)) = frag_pos.get(fid as usize) {
            per_part.entry(part).or_default().push((
                pos + off as usize,
                format!("<a id=\"k{fid}-{off}\"></a>").into_bytes(),
            ));
            target_part.insert((fid, off), part);
        }
    }

    // STEP 4: resources referenced from HTML and CSS.
    let mut resources: Vec<VirtualResource> = Vec::new();
    let mut seen: HashMap<u32, Option<String>> = HashMap::new();
    let mut embed = |n: u32, resources: &mut Vec<VirtualResource>| -> Option<String> {
        if let Some(p) = seen.get(&n) {
            return p.clone();
        }
        let p = book.image(n).map(|(bytes, ext, mime)| {
            let rel = format!("img/{n:05}.{ext}");
            resources.push(VirtualResource {
                path: format!("kf8/{rel}"),
                media_type: mime.into(),
                bytes: bytes.to_vec(),
            });
            rel
        });
        seen.insert(n, p.clone());
        p
    };
    for (i, flow) in flows.iter().enumerate().skip(1) {
        let css = rewrite_urls(
            &String::from_utf8_lossy(flow),
            &mut |kind, arg| match kind {
                "embed" => base32(arg).and_then(|n| embed(n, &mut resources)),
                _ => None,
            },
        );
        resources.push(VirtualResource {
            path: format!("kf8/flow{i:04}.css"),
            media_type: "text/css".into(),
            bytes: css.into_bytes(),
        });
    }

    // STEP 5: documents with anchors inserted and URLs rewritten.
    let part_path = |p: usize| format!("part{p:04}.html");
    let mut docs = Vec::new();
    for (p, mut bytes) in parts.into_iter().enumerate() {
        insert_all(&mut bytes, per_part.remove(&p).unwrap_or_default());
        let html = book.h.decode(&bytes);
        let html = rewrite_urls(&html, &mut |kind, arg| match kind {
            "pos" => {
                let (fid, off) = parse_pos(&format!("kindle:pos:{arg}"))?;
                target_part
                    .get(&(fid, off))
                    .map(|&tp| format!("{}#k{fid}-{off}", part_path(tp)))
            }
            "embed" => base32(arg).and_then(|n| embed(n, &mut resources)),
            "flow" => base32(arg).map(|n| format!("flow{n:04}.css")),
            _ => None,
        });
        docs.push(VirtualDoc {
            path: format!("kf8/{}", part_path(p)),
            html,
        });
    }
    if docs.is_empty() {
        warnings.push("KF8 index missing; using the raw text".into());
        docs.push(VirtualDoc {
            path: "kf8/part0000.html".into(),
            html: book.h.decode(&text),
        });
    }

    // STEP 6: TOC from the NCX (tag 3 = label, 4 = depth, 6 = fid/off).
    let mut toc: Vec<RawEntry> = Vec::new();
    let mut stack: Vec<(u32, RawEntry)> = Vec::new();
    for e in &ncx {
        let Some(label) = e.tag(3, 0).and_then(|o| cncx.get(&o)) else {
            continue;
        };
        let href = match (e.tag(6, 0), e.tag(6, 1)) {
            (Some(f), Some(o)) => target_part
                .get(&(f, o))
                .map(|&tp| format!("kf8/{}#k{f}-{o}", part_path(tp))),
            _ => None,
        };
        let depth = e.tag(4, 0).unwrap_or(0);
        while stack.last().is_some_and(|(d, _)| *d >= depth) {
            let (_, done) = stack.pop().unwrap();
            match stack.last_mut() {
                Some((_, parent)) => parent.children.push(done),
                None => toc.push(done),
            }
        }
        stack.push((
            depth,
            RawEntry {
                title: label.trim().to_string(),
                href,
                children: vec![],
            },
        ));
    }
    while let Some((_, done)) = stack.pop() {
        match stack.last_mut() {
            Some((_, parent)) => parent.children.push(done),
            None => toc.push(done),
        }
    }

    let cover = book
        .cover_image()
        .and_then(|n| embed(n, &mut resources))
        .map(|p| format!("kf8/{p}"));
    Ok(HtmlBook {
        format: "AZW3",
        metadata: book.metadata(src),
        docs,
        resources,
        toc,
        cover,
        warnings,
    })
}

/// Rebuild files. Returns the files and, per fragment id, (file, position).
fn assemble(
    text: &[u8],
    skels: &[IndexEntry],
    frags: &[IndexEntry],
) -> (Vec<Vec<u8>>, Vec<(usize, usize)>) {
    let mut parts = Vec::new();
    let mut frag_pos = Vec::new();
    let mut fi = 0usize;
    for (p, skel) in skels.iter().enumerate() {
        // STEP 1: the skeleton text.
        let (start, len) = (
            skel.tag(6, 0).unwrap_or(0) as usize,
            skel.tag(6, 1).unwrap_or(0) as usize,
        );
        let count = skel.tag(1, 0).unwrap_or(0) as usize;
        let mut file = text
            .get(start..(start + len).min(text.len()))
            .unwrap_or(&[])
            .to_vec();
        // STEP 2: fragments follow the skeleton in the text; each is inserted
        // at its recorded position (stored as the entry name, in decimal).
        let mut cursor = start + len;
        for _ in 0..count {
            let Some(frag) = frags.get(fi) else { break };
            fi += 1;
            let insert = frag
                .name_str()
                .trim()
                .parse::<usize>()
                .unwrap_or(0)
                .saturating_sub(start)
                .min(file.len());
            let flen = frag.tag(6, 1).unwrap_or(0) as usize;
            let piece = text
                .get(cursor..(cursor + flen).min(text.len()))
                .unwrap_or(&[])
                .to_vec();
            frag_pos.push((p, insert));
            file.splice(insert..insert, piece);
            cursor += flen;
        }
        parts.push(file);
    }
    (parts, frag_pos)
}

/// Parse `kindle:pos:fid:XXXX:off:YYYYYYYYYY` at the start of `s`.
fn parse_pos(s: &str) -> Option<(u32, u32)> {
    let rest = s.strip_prefix("kindle:pos:fid:")?;
    let fid = base32(rest.get(0..4)?)?;
    let rest = rest.get(4..)?.strip_prefix(":off:")?;
    let off = base32(rest.get(0..10)?)?;
    Some((fid, off))
}

/// Replace every `kindle:<kind>:<arg>[?query]` URL using `f(kind, arg)`;
/// unresolvable URLs become empty.
fn rewrite_urls(s: &str, f: &mut dyn FnMut(&str, &str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("kindle:") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 7..];
        // The URL ends at a quote, parenthesis, whitespace or '>'.
        let end = after
            .find(|c: char| c == '"' || c == '\'' || c == ')' || c == '>' || c.is_whitespace())
            .unwrap_or(after.len());
        let url = &after[..end];
        let url_no_query = url.split('?').next().unwrap_or(url);
        let (kind, arg) = url_no_query.split_once(':').unwrap_or((url_no_query, ""));
        out.push_str(&f(kind, arg).unwrap_or_default());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_and_urls() {
        assert_eq!(base32("0001"), Some(1));
        assert_eq!(base32("000V"), Some(31));
        assert_eq!(base32("0010"), Some(32));
        assert_eq!(
            parse_pos("kindle:pos:fid:0002:off:000000000A\""),
            Some((2, 10))
        );
        let r = rewrite_urls(
            r#"<img src="kindle:embed:0003?mime=image/jpeg"/> url(kindle:flow:0001?mime=text/css)"#,
            &mut |k, a| Some(format!("[{k}/{a}]")),
        );
        assert_eq!(r, r#"<img src="[embed/0003]"/> url([flow/0001])"#);
    }
}
