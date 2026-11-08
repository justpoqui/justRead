// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Phase 4 adapters: MOBI 6, KF8/AZW3, DOCX, HTML, FB2, plus DRM and
//! "not yet supported" formats. Every fixture is built in this file.

use std::io::Write;
use std::path::{Path, PathBuf};

use justbook::JustBookWriter;
use justread_formats::{BookContent, FormatError, Registry};

const NONE: u32 = 0xFFFF_FFFF;

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

fn palmdb(kind: &[u8; 8], records: &[Vec<u8>]) -> Vec<u8> {
    // STEP 1: 78-byte header with the type/creator and record count.
    let mut out = vec![0u8; 78];
    out[..8].copy_from_slice(b"testbook");
    out[60..68].copy_from_slice(kind);
    out[76..78].copy_from_slice(&(records.len() as u16).to_be_bytes());
    // STEP 2: record table, 2 padding bytes, then the records.
    let mut offset = 78 + records.len() * 8 + 2;
    for (i, r) in records.iter().enumerate() {
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&((i as u32) * 2).to_be_bytes());
        offset += r.len();
    }
    out.extend_from_slice(&[0, 0]);
    for r in records {
        out.extend_from_slice(r);
    }
    out
}

#[derive(Default)]
struct Hdr {
    compression: u16,
    text_len: u32,
    text_records: u16,
    encryption: u16,
    version: u32,
    extra_flags: u16,
    first_image: u32,
    ncx: u32,
    fdst: u32,
    frag: u32,
    skel: u32,
    exth: Vec<(u32, Vec<u8>)>,
    title: String,
}

fn record0(h: &Hdr) -> Vec<u8> {
    let mobi_len = 0xF8usize;
    let mut r = vec![0u8; 16 + mobi_len];
    let put32 =
        |r: &mut Vec<u8>, at: usize, v: u32| r[at..at + 4].copy_from_slice(&v.to_be_bytes());
    r[0..2].copy_from_slice(&h.compression.to_be_bytes());
    put32(&mut r, 4, h.text_len);
    r[8..10].copy_from_slice(&h.text_records.to_be_bytes());
    r[10..12].copy_from_slice(&4096u16.to_be_bytes());
    r[12..14].copy_from_slice(&h.encryption.to_be_bytes());
    r[16..20].copy_from_slice(b"MOBI");
    put32(&mut r, 20, mobi_len as u32);
    put32(&mut r, 24, 2);
    put32(&mut r, 28, 65001);
    put32(&mut r, 36, h.version);
    put32(&mut r, 108, h.first_image);
    put32(&mut r, 112, NONE);
    put32(&mut r, 128, 0x40);
    put32(&mut r, 192, h.fdst);
    r[0xF2..0xF4].copy_from_slice(&h.extra_flags.to_be_bytes());
    put32(&mut r, 0xF4, h.ncx);
    put32(&mut r, 0xF8, h.frag);
    put32(&mut r, 0xFC, h.skel);
    // EXTH
    let mut exth = Vec::new();
    for (k, v) in &h.exth {
        exth.extend_from_slice(&k.to_be_bytes());
        exth.extend_from_slice(&((v.len() + 8) as u32).to_be_bytes());
        exth.extend_from_slice(v);
    }
    r.extend_from_slice(b"EXTH");
    r.extend_from_slice(&((exth.len() + 12) as u32).to_be_bytes());
    r.extend_from_slice(&(h.exth.len() as u32).to_be_bytes());
    r.extend_from_slice(&exth);
    // Full name
    let name_off = r.len();
    r.extend_from_slice(h.title.as_bytes());
    put32(&mut r, 84, name_off as u32);
    put32(&mut r, 88, h.title.len() as u32);
    r
}

/// "Compress" with PalmDOC using only literals (valid, just not smaller).
fn palmdoc_literals(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in data {
        if b == 0 || (9..=0x7F).contains(&b) {
            out.push(b);
        } else {
            out.push(1);
            out.push(b);
        }
    }
    out
}

fn varint(v: u32) -> Vec<u8> {
    let mut groups = vec![(v & 0x7F) as u8];
    let mut x = v >> 7;
    while x > 0 {
        groups.push((x & 0x7F) as u8);
        x >>= 7;
    }
    groups.reverse();
    let last = groups.len() - 1;
    groups[last] |= 0x80;
    groups
}

/// An index: header record (with TAGX), one data record, optional CNCX record.
fn indx(
    tagx: &[(u8, u8, u8, u8)],
    entries: &[(Vec<u8>, Vec<u8>)],
    cncx: Option<Vec<u8>>,
) -> Vec<Vec<u8>> {
    // STEP 1: header record.
    let mut head = vec![0u8; 0xC0];
    head[0..4].copy_from_slice(b"INDX");
    head[4..8].copy_from_slice(&0xC0u32.to_be_bytes());
    head[0x18..0x1C].copy_from_slice(&1u32.to_be_bytes());
    head[0x34..0x38].copy_from_slice(&(cncx.is_some() as u32).to_be_bytes());
    head.extend_from_slice(b"TAGX");
    head.extend_from_slice(&((12 + tagx.len() * 4) as u32).to_be_bytes());
    head.extend_from_slice(&1u32.to_be_bytes());
    for t in tagx {
        head.extend_from_slice(&[t.0, t.1, t.2, t.3]);
    }
    // STEP 2: data record with IDXT.
    let mut data = vec![0u8; 0xC0];
    data[0..4].copy_from_slice(b"INDX");
    let mut offsets = Vec::new();
    for (name, body) in entries {
        offsets.push(data.len() as u16);
        data.push(name.len() as u8);
        data.extend_from_slice(name);
        data.extend_from_slice(body);
    }
    let idxt = data.len();
    data[0x14..0x18].copy_from_slice(&(idxt as u32).to_be_bytes());
    data[0x18..0x1C].copy_from_slice(&(entries.len() as u32).to_be_bytes());
    data.extend_from_slice(b"IDXT");
    for o in offsets {
        data.extend_from_slice(&o.to_be_bytes());
    }
    let mut out = vec![head, data];
    if let Some(c) = cncx {
        out.push(c);
    }
    out
}

fn png() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    v.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0, 3, 8, 2, 0, 0, 0]);
    v
}

fn convert(src: &Path, root: &Path) -> (BookContent, PathBuf) {
    let reg = Registry::with_defaults();
    let adapter = reg.detect(src).unwrap();
    let dir = root.join(format!(
        "pkg-{}",
        src.file_name().unwrap().to_string_lossy()
    ));
    let mut w = JustBookWriter::create(&dir).unwrap();
    let c = adapter.convert(src, &mut w).unwrap();
    let wd = w.working_dir().to_path_buf();
    std::mem::forget(w);
    (c, wd)
}

fn read(dir: &Path, p: &str) -> String {
    std::fs::read_to_string(dir.join(p)).unwrap()
}

// ---------------------------------------------------------------------------
// MOBI 6
// ---------------------------------------------------------------------------

fn mobi6_bytes(encryption: u16) -> Vec<u8> {
    // STEP 1: text with a filepos link (10 digits, patched below), an image and a page break.
    let mut text = "<html><head><guide></guide></head><body><h2>One</h2><p>Caf\u{e9}: go to <a filepos=0000000000>two</a>.</p><p><img recindex=\"00001\"/></p><mbp:pagebreak/><h2>Two</h2><p>Target here.</p></body></html>".to_string();
    let target = text.find("<h2>Two").unwrap();
    text = text.replace("0000000000", &format!("{target:010}"));
    let bytes = text.as_bytes();

    // STEP 2: two compressed text records with trailing entries (flags = 3).
    let mid = bytes.len() / 2;
    let rec = |d: &[u8]| {
        let mut r = palmdoc_literals(d);
        r.extend_from_slice(&[0x00, 0x81]);
        r
    };
    let h = Hdr {
        compression: 2,
        text_len: bytes.len() as u32,
        text_records: 2,
        encryption,
        version: 6,
        extra_flags: 3,
        first_image: 3,
        ncx: NONE,
        fdst: NONE,
        frag: NONE,
        skel: NONE,
        exth: vec![
            (100, b"Mo Writer".to_vec()),
            (503, b"Mobi Test".to_vec()),
            (524, b"en".to_vec()),
            (201, 0u32.to_be_bytes().to_vec()),
        ],
        title: "fallback".into(),
    };
    palmdb(
        b"BOOKMOBI",
        &[record0(&h), rec(&bytes[..mid]), rec(&bytes[mid..]), png()],
    )
}

#[test]
fn mobi6_book() {
    // STEP 1: build and detect.
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("book.mobi");
    std::fs::write(&src, mobi6_bytes(0)).unwrap();
    assert_eq!(Registry::with_defaults().detect(&src).unwrap().id(), "mobi");

    // STEP 2: metadata, chapters split at the page break, cover.
    let (c, dir) = convert(&src, tmp.path());
    assert_eq!(c.metadata.title, "Mobi Test");
    assert_eq!(c.metadata.authors, vec!["Mo Writer"]);
    assert_eq!(c.spine.len(), 2);
    assert_eq!(c.spine[1].title.as_deref(), Some("Two"));
    assert_eq!(c.cover.as_deref(), Some("cover.png"));

    // STEP 3: the filepos link points at the anchor in chapter 2; image rewritten; UTF-8 decoded.
    let c1 = read(&dir, "content/c0001.xhtml");
    assert!(c1.contains("Caf\u{e9}"), "{c1}");
    assert!(c1.contains("href=\"c0002.xhtml#fp"), "{c1}");
    assert!(
        c1.contains("src=\"../resources/mobi/img/00001.png\""),
        "{c1}"
    );
    let c2 = read(&dir, "content/c0002.xhtml");
    assert!(c2.contains("<a id=\"fp"), "{c2}");
}

#[test]
fn mobi_drm_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("drm.azw");
    std::fs::write(&src, mobi6_bytes(2)).unwrap();
    let reg = Registry::with_defaults();
    let a = reg.detect(&src).unwrap();
    let mut w = JustBookWriter::create(&tmp.path().join("o")).unwrap();
    assert!(matches!(
        a.convert(&src, &mut w),
        Err(FormatError::DrmProtected(_))
    ));
    assert!(matches!(
        a.inspect(&src).unwrap().drm,
        justread_formats::DrmStatus::Protected(_)
    ));
}

// ---------------------------------------------------------------------------
// KF8 / AZW3
// ---------------------------------------------------------------------------

#[test]
fn kf8_book() {
    // STEP 1: two skeletons with one fragment each, a CSS flow, a link and an image.
    let skel0 = r#"<html><head><link href="kindle:flow:0001?mime=text/css" rel="stylesheet" type="text/css"/></head><body aid="0"></body></html>"#;
    let frag0 = r#"<h1>Alpha</h1><p>See <a href="kindle:pos:fid:0001:off:0000000000">beta</a>.</p><p><img src="kindle:embed:0001?mime=image/png"/></p>"#;
    let skel1 = r#"<html><head></head><body aid="1"></body></html>"#;
    let frag1 = r#"<h1>Beta</h1><p>Second file.</p>"#;
    let css = "p { margin: 0 } .x { background: url(kindle:embed:0001) }";
    let s0 = 0;
    let s1 = skel0.len() + frag0.len();
    let flow0 = format!("{skel0}{frag0}{skel1}{frag1}");
    let raw = format!("{flow0}{css}");
    let ins0 = s0 + skel0.find("></body>").unwrap() + 1;
    let ins1 = s1 + skel1.find("></body>").unwrap() + 1;

    // STEP 2: indexes.
    let mut skel_entries = Vec::new();
    for (i, (start, len)) in [(s0, skel0.len()), (s1, skel1.len())].iter().enumerate() {
        let mut body = vec![0x05];
        body.extend(varint(1));
        body.extend(varint(*start as u32));
        body.extend(varint(*len as u32));
        skel_entries.push((format!("SKEL{i:07}").into_bytes(), body));
    }
    let skel = indx(
        &[(1, 1, 0x03, 0), (6, 2, 0x0C, 0), (0, 0, 0, 1)],
        &skel_entries,
        None,
    );
    let mut frag_entries = Vec::new();
    for (i, (ins, len)) in [(ins0, frag0.len()), (ins1, frag1.len())]
        .iter()
        .enumerate()
    {
        let mut body = vec![0x0F];
        for v in [0, i as u32, i as u32, 0, *len as u32] {
            body.extend(varint(v));
        }
        frag_entries.push((format!("{ins:010}").into_bytes(), body));
    }
    let frag = indx(
        &[
            (2, 1, 0x01, 0),
            (3, 1, 0x02, 0),
            (4, 1, 0x04, 0),
            (6, 2, 0x08, 0),
            (0, 0, 0, 1),
        ],
        &frag_entries,
        None,
    );
    let mut cncx = Vec::new();
    let mut label_off = Vec::new();
    for l in ["Alpha", "Beta"] {
        label_off.push(cncx.len() as u32);
        cncx.extend(varint(l.len() as u32));
        cncx.extend_from_slice(l.as_bytes());
    }
    let mut ncx_entries = Vec::new();
    for (i, off) in label_off.iter().enumerate() {
        let mut body = vec![0x1F];
        for v in [0, 0, *off, 0, i as u32, 0] {
            body.extend(varint(v));
        }
        ncx_entries.push((format!("{i}").into_bytes(), body));
    }
    let ncx = indx(
        &[
            (1, 1, 0x01, 0),
            (2, 1, 0x02, 0),
            (3, 1, 0x04, 0),
            (4, 1, 0x08, 0),
            (6, 2, 0x10, 0),
            (0, 0, 0, 1),
        ],
        &ncx_entries,
        Some(cncx),
    );

    // STEP 3: FDST and the record layout.
    let mut fdst = b"FDST".to_vec();
    fdst.extend_from_slice(&12u32.to_be_bytes());
    fdst.extend_from_slice(&2u32.to_be_bytes());
    for (a, b) in [(0, flow0.len()), (flow0.len(), raw.len())] {
        fdst.extend_from_slice(&(a as u32).to_be_bytes());
        fdst.extend_from_slice(&(b as u32).to_be_bytes());
    }
    // 0 hdr, 1 text, 2 image, 3-4 skel, 5-6 frag, 7-9 ncx, 10 fdst
    let h = Hdr {
        compression: 1,
        text_len: raw.len() as u32,
        text_records: 1,
        version: 8,
        first_image: 2,
        skel: 3,
        frag: 5,
        ncx: 7,
        fdst: 10,
        exth: vec![(100, b"Kay Author".to_vec()), (503, b"KF8 Test".to_vec())],
        title: "kf8".into(),
        ..Default::default()
    };
    let mut records = vec![record0(&h), raw.as_bytes().to_vec(), png()];
    records.extend(skel);
    records.extend(frag);
    records.extend(ncx);
    records.push(fdst);
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("book.azw3");
    std::fs::write(&src, palmdb(b"BOOKMOBI", &records)).unwrap();

    // STEP 4: convert and check reassembly, links, image, CSS flow and TOC.
    let (c, dir) = convert(&src, tmp.path());
    assert_eq!(c.metadata.title, "KF8 Test");
    assert_eq!(c.spine.len(), 2);
    let p1 = read(&dir, "content/c0001.xhtml");
    assert!(p1.contains("<h1 data-jrb=\"1\">Alpha</h1>"), "{p1}");
    assert!(p1.contains("href=\"c0002.xhtml#k1-0\""), "{p1}");
    assert!(
        p1.contains("src=\"../resources/kf8/img/00001.png\""),
        "{p1}"
    );
    assert!(
        p1.contains("href=\"../resources/kf8/flow0001.css\""),
        "{p1}"
    );
    let p2 = read(&dir, "content/c0002.xhtml");
    assert!(p2.contains("<a id=\"k1-0\"></a>"), "{p2}");
    let css_out = read(&dir, "resources/kf8/flow0001.css");
    assert!(css_out.contains("url(img/00001.png)"), "{css_out}");
    assert_eq!(
        c.toc.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        vec!["Alpha", "Beta"]
    );
    assert_eq!(c.toc[1].href.as_deref(), Some("content/c0002.xhtml#k1-0"));
}

// ---------------------------------------------------------------------------
// DOCX
// ---------------------------------------------------------------------------

#[test]
fn docx_book() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("report.docx");
    let w = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
    let p = |style: &str, runs: &str| {
        format!(r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>{runs}</w:p>"#)
    };
    let run = |t: &str| format!("<w:r><w:t xml:space=\"preserve\">{t}</w:t></w:r>");
    let body = [
        p("Heading1", &run("First Chapter")),
        "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r><w:r><w:rPr><w:i/><w:b w:val=\"0\"/></w:rPr><w:t> italic</w:t></w:r><w:r><w:footnoteReference w:id=\"1\"/></w:r></w:p>".to_string(),
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>item one</w:t></w:r></w:p>"#.to_string(),
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>nested</w:t></w:r></w:p>"#.to_string(),
        r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#.to_string(),
        r#"<w:p><w:hyperlink r:id="rIdLink"><w:r><w:t>site</w:t></w:r></w:hyperlink><w:r><w:drawing><a:blip r:embed="rIdImg"/></w:drawing></w:r></w:p>"#.to_string(),
        p("Heading1", &run("Second Chapter")),
        p("Normal", &run("Text two.")),
    ]
    .join("");
    let files: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", "<Types/>".into()),
        ("_rels/.rels", r#"<Relationships><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.into()),
        ("word/document.xml", format!("<w:document {w}><w:body>{body}</w:body></w:document>")),
        ("word/_rels/document.xml.rels", r#"<Relationships>
<Relationship Id="rS" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rN" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
<Relationship Id="rF" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
<Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
<Relationship Id="rIdLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.org/" TargetMode="External"/>
</Relationships>"#.into()),
        ("word/styles.xml", format!(r#"<w:styles {w}><w:style w:styleId="Heading1"><w:name w:val="heading 1"/></w:style><w:style w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#)),
        ("word/numbering.xml", format!(r#"<w:numbering {w}><w:abstractNum w:abstractNumId="7"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="7"/></w:num></w:numbering>"#)),
        ("word/footnotes.xml", format!(r#"<w:footnotes {w}><w:footnote w:type="separator" w:id="0"><w:p><w:r><w:t>---</w:t></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:t>A note.</w:t></w:r></w:p></w:footnote></w:footnotes>"#)),
        ("docProps/core.xml", r#"<cp:coreProperties xmlns:cp="x" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Annual Report</dc:title><dc:creator>Dee Writer</dc:creator></cp:coreProperties>"#.into()),
    ];
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&src).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (n, c) in &files {
        zip.start_file(*n, opts).unwrap();
        zip.write_all(c.as_bytes()).unwrap();
    }
    zip.start_file("word/media/image1.png", opts).unwrap();
    zip.write_all(&png()).unwrap();
    zip.finish().unwrap();

    let (c, dir) = convert(&src, tmp.path());
    assert_eq!(c.metadata.title, "Annual Report");
    assert_eq!(c.metadata.authors, vec!["Dee Writer"]);
    assert_eq!(c.spine.len(), 3, "two chapters + notes");
    let c1 = read(&dir, "content/c0001.xhtml");
    assert!(c1.contains("Bold</strong>"), "{c1}");
    assert!(c1.contains("<em> italic</em>"), "{c1}");
    // Decimal numbering → ordered list, with the level-1 item nested inside the first item.
    assert!(
        c1.contains(
            r#"<ol><li data-jrb="3">item one<ol><li data-jrb="4">nested</li></ol></li></ol>"#
        ),
        "{c1}"
    );
    assert!(c1.contains("<table") && c1.contains("cell"), "{c1}");
    assert!(c1.contains("data-jr-external=\"1\""), "{c1}");
    assert!(
        c1.contains("src=\"../resources/docx/word/media/image1.png\""),
        "{c1}"
    );
    assert!(c1.contains("href=\"c0003.xhtml#fn-1\""), "{c1}");
    assert!(read(&dir, "content/c0003.xhtml").contains("A note."));
    assert_eq!(
        c.toc.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        vec!["First Chapter", "Second Chapter", "Notes"]
    );
}

// ---------------------------------------------------------------------------
// HTML, FB2, unsupported
// ---------------------------------------------------------------------------

#[test]
fn html_file_imports_only_local_resources_inside_its_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let book = tmp.path().join("site");
    std::fs::create_dir_all(book.join("img")).unwrap();
    std::fs::write(book.join("img/a.png"), png()).unwrap();
    std::fs::write(tmp.path().join("secret.png"), png()).unwrap();
    let src = book.join("page.html");
    std::fs::write(&src, r#"<html lang="en"><head><title>A Page</title><meta name="author" content="Hal"></head>
<body><h1>Hello</h1><img src="img/a.png"><img src="../secret.png"><img src="https://x/y.png"><script>evil()</script></body></html>"#).unwrap();
    let (c, dir) = convert(&src, tmp.path());
    assert_eq!(c.metadata.title, "A Page");
    assert_eq!(c.metadata.authors, vec!["Hal"]);
    assert_eq!(
        c.resources
            .iter()
            .map(|r| r.href.as_str())
            .collect::<Vec<_>>(),
        vec!["resources/img/a.png"]
    );
    let x = read(&dir, "content/c0001.xhtml");
    assert!(!x.contains("evil") && !x.contains("secret") && !x.contains("https://x"));
}

#[test]
fn fb2_zip_book() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tale.fb2.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&src).unwrap());
    zip.start_file("tale.fb2", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(r#"<?xml version="1.0" encoding="utf-8"?><FictionBook><description><title-info><book-title>Zipped Tale</book-title></title-info></description>
<body><section><title><p>Start</p></title><p>Once.</p></section></body></FictionBook>"#.as_bytes()).unwrap();
    zip.finish().unwrap();
    let (c, _) = convert(&src, tmp.path());
    assert_eq!(c.metadata.title, "Zipped Tale");
    assert_eq!(c.toc[0].title, "Start");
}

#[test]
fn djvu_and_kfx_have_clear_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = Registry::with_defaults();
    let djvu = tmp.path().join("scan.djvu");
    std::fs::write(&djvu, b"AT&TFORM\0\0\0\x10DJVUINFO").unwrap();
    let a = reg.detect(&djvu).unwrap();
    let mut w = JustBookWriter::create(&tmp.path().join("o1")).unwrap();
    assert!(a
        .convert(&djvu, &mut w)
        .unwrap_err()
        .to_string()
        .contains("DjVu"));
    let kfx = tmp.path().join("b.kfx");
    std::fs::write(&kfx, b"\xeaDRMION\xee rest").unwrap();
    let mut w = JustBookWriter::create(&tmp.path().join("o2")).unwrap();
    assert!(matches!(
        reg.detect(&kfx).unwrap().convert(&kfx, &mut w),
        Err(FormatError::DrmProtected(_))
    ));
}
