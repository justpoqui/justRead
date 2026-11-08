// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! End-to-end EPUB conversion tests. Fixtures are generated in code so the
//! repository contains no third-party books.

use std::io::Write;
use std::path::{Path, PathBuf};

use justbook::{load_manifest, JustBookWriter, Layout, Manifest, SourceInfo, SCHEMA_VERSION};
use justread_formats::{
    epub::EpubAdapter, Confidence, DrmStatus, FormatAdapter, FormatError, Registry,
};
use zip::write::SimpleFileOptions;

const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

/// Build an EPUB from (path, bytes) pairs; `mimetype` is added first, stored.
fn build_epub(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join(name);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (p, bytes) in files {
        zip.start_file(*p, deflated).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

/// A small but realistic EPUB 3 book.
fn epub3(dir: &Path) -> PathBuf {
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
 <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:identifier id="uid">urn:uuid:11111111-2222-3333-4444-555555555555</dc:identifier>
  <dc:title>Test Book</dc:title><dc:creator>Jane Doe</dc:creator><dc:language>en</dc:language>
 </metadata>
 <manifest>
  <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  <item id="c1" href="text/one.xhtml" media-type="application/xhtml+xml"/>
  <item id="c2" href="text/two.xhtml" media-type="application/xhtml+xml"/>
  <item id="css" href="styles/book.css" media-type="text/css"/>
  <item id="img" href="images/pic.png" media-type="image/png"/>
  <item id="cover" href="images/cover.jpg" media-type="image/jpeg" properties="cover-image"/>
  <item id="font" href="fonts/f.otf" media-type="font/otf"/>
  <item id="js" href="app.js" media-type="application/javascript"/>
 </manifest>
 <spine><itemref idref="c1"/><itemref idref="c2"/></spine>
</package>"#;
    let nav = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="toc"><ol><li><a href="text/one.xhtml">Opening</a></li>
<li><a href="text/two.xhtml#mid">Middle</a></li></ol></nav></body></html>"#;
    let one = r#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml">
<head><title>One</title><link rel="stylesheet" href="../styles/book.css"/><script>bad()</script></head>
<body><h1>Opening</h1><p>Hello&nbsp;world. <a href="two.xhtml#mid">Jump</a></p>
<p><img src="../images/pic.png" alt="pic"/></p></body></html>"#;
    let two = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Two</title></head>
<body><p id="mid">Middle part<p>unclosed paragraph &amp; more</body></html>"#;
    let css = "p { margin: 0; background: url(https://tracker.example/p.gif) }\n.x { background: url(../images/pic.png) }";
    build_epub(
        dir,
        "book3.epub",
        &[
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/nav.xhtml", nav.as_bytes()),
            ("OEBPS/text/one.xhtml", one.as_bytes()),
            ("OEBPS/text/two.xhtml", two.as_bytes()),
            ("OEBPS/styles/book.css", css.as_bytes()),
            ("OEBPS/images/pic.png", b"\x89PNG fake"),
            ("OEBPS/images/cover.jpg", b"\xff\xd8 fake jpeg"),
            ("OEBPS/fonts/f.otf", b"OTTO fake font"),
            ("OEBPS/app.js", b"alert(1)"),
            // Zip-slip attempt: must never be written anywhere.
            ("../../evil.txt", b"pwned"),
        ],
    )
}

/// Convert `src` into a fresh package and load the result.
fn convert(src: &Path, out_root: &Path) -> (Manifest, PathBuf) {
    let dest = out_root.join("pkg");
    let mut w = JustBookWriter::create(&dest).unwrap();
    let content = EpubAdapter.convert(src, &mut w).unwrap();
    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        id: "test".into(),
        layout: Layout::Reflowable,
        metadata: content.metadata,
        spine: content.spine,
        toc: content.toc,
        resources: content.resources,
        cover: content.cover,
        source: SourceInfo {
            format: "epub".into(),
            file_name: "x".into(),
            sha256: "0".into(),
            converter_version: "1".into(),
        },
        fixed: content.fixed,
    };
    w.finish(&manifest).unwrap();
    (load_manifest(&dest).unwrap(), dest)
}

#[test]
fn converts_epub3() {
    // STEP 1: build and convert.
    let tmp = tempfile::tempdir().unwrap();
    let src = epub3(tmp.path());
    assert_eq!(
        EpubAdapter.sniff(&std::fs::read(&src).unwrap()[..64], Some("epub")),
        Confidence::High
    );
    let (m, dir) = convert(&src, tmp.path());

    // STEP 2: metadata and structure.
    assert_eq!(m.metadata.title, "Test Book");
    assert_eq!(m.metadata.authors, vec!["Jane Doe"]);
    assert_eq!(m.spine.len(), 2);
    assert_eq!(m.spine[0].href, "content/c0001.xhtml");
    assert_eq!(m.spine[0].title.as_deref(), Some("Opening"));
    assert_eq!(m.toc[1].href.as_deref(), Some("content/c0002.xhtml#mid"));
    assert_eq!(m.cover.as_deref(), Some("cover.jpg"));
    assert!(m
        .resources
        .iter()
        .any(|r| r.href == "resources/OEBPS/images/pic.png"));
    assert!(
        !m.resources.iter().any(|r| r.href.ends_with(".js")),
        "scripts are never copied"
    );

    // STEP 3: chapter content was normalized and links rewritten.
    let c1 = std::fs::read_to_string(dir.join("content/c0001.xhtml")).unwrap();
    assert!(c1.contains(r#"href="../resources/OEBPS/styles/book.css""#));
    assert!(c1.contains(
        r##"<a href="c0002.xhtml#mid" data-jr-href="content/c0002.xhtml#mid">Jump</a>"##
    ));
    assert!(c1.contains(r#"src="../resources/OEBPS/images/pic.png""#));
    assert!(c1.contains("Hello\u{a0}world"));
    assert!(!c1.contains("bad()"));
    assert!(c1.contains(r#"lang="en""#), "book language is the default");
    let c2 = std::fs::read_to_string(dir.join("content/c0002.xhtml")).unwrap();
    assert!(c2.contains("unclosed paragraph &amp; more</p>"));

    // STEP 4: CSS was scrubbed but local urls kept.
    let css = std::fs::read_to_string(dir.join("resources/OEBPS/styles/book.css")).unwrap();
    assert!(!css.contains("tracker.example"));
    assert!(css.contains("url(../images/pic.png)"));

    // STEP 5: the zip-slip entry went nowhere.
    assert!(!tmp.path().parent().unwrap().join("evil.txt").exists());
    assert!(!dir.join("evil.txt").exists());
}

#[test]
fn converts_epub2_with_ncx() {
    let tmp = tempfile::tempdir().unwrap();
    let opf = r#"<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
<dc:title>Old Book</dc:title><dc:creator opf:role="aut">Old Author</dc:creator><dc:identifier id="id">isbn-1</dc:identifier>
</metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
<item id="a" href="a.html" media-type="application/xhtml+xml"/></manifest>
<spine toc="ncx"><itemref idref="a"/></spine></package>"#;
    let ncx = r#"<ncx><navMap><navPoint id="p"><navLabel><text>Only Chapter</text></navLabel><content src="a.html"/></navPoint></navMap></ncx>"#;
    let a = "<html><head><title>A</title></head><body><p>Text</p></body></html>";
    let src = build_epub(
        tmp.path(),
        "b2.epub",
        &[
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/toc.ncx", ncx.as_bytes()),
            ("OEBPS/a.html", a.as_bytes()),
        ],
    );
    let (m, _) = convert(&src, tmp.path());
    assert_eq!(m.metadata.authors, vec!["Old Author"]);
    assert_eq!(m.toc.len(), 1);
    assert_eq!(m.toc[0].title, "Only Chapter");
    assert_eq!(m.spine[0].title.as_deref(), Some("Only Chapter"));
}

#[test]
fn detects_drm() {
    let tmp = tempfile::tempdir().unwrap();
    let opf = r#"<package version="3.0"><metadata><dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">X</dc:title></metadata>
<manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#;
    let base: Vec<(&str, &[u8])> = vec![
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/a.xhtml", b"<html><body><p>x</p></body></html>"),
    ];

    // Adobe ADEPT marker file.
    let mut files = base.clone();
    files.push(("META-INF/rights.xml", b"<rights/>"));
    let src = build_epub(tmp.path(), "adept.epub", &files);
    assert_eq!(
        EpubAdapter.inspect(&src).unwrap().drm,
        DrmStatus::Protected("Adobe ADEPT".into())
    );
    let mut w = JustBookWriter::create(&tmp.path().join("out")).unwrap();
    assert!(matches!(
        EpubAdapter.convert(&src, &mut w),
        Err(FormatError::DrmProtected(_))
    ));

    // Real encryption listed in encryption.xml.
    let enc = r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
<enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/>
<enc:CipherData><enc:CipherReference URI="OEBPS/a.xhtml"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let mut files = base.clone();
    files.push(("META-INF/encryption.xml", enc.as_bytes()));
    let src = build_epub(tmp.path(), "enc.epub", &files);
    assert!(matches!(
        EpubAdapter.inspect(&src).unwrap().drm,
        DrmStatus::Protected(_)
    ));

    // Font obfuscation alone is NOT DRM.
    let obf = enc.replace(
        "http://www.w3.org/2001/04/xmlenc#aes128-cbc",
        "http://www.idpf.org/2008/embedding",
    );
    let mut files = base.clone();
    files.push(("META-INF/encryption.xml", obf.as_bytes()));
    let src = build_epub(tmp.path(), "obf.epub", &files);
    assert_eq!(EpubAdapter.inspect(&src).unwrap().drm, DrmStatus::None);
}

#[test]
fn deobfuscates_fonts_on_import() {
    use sha1::{Digest, Sha1};
    let tmp = tempfile::tempdir().unwrap();
    let uid = "urn:uuid:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    // STEP 1: obfuscate a fake font with the IDPF algorithm.
    let font: Vec<u8> = (0..1500u32).map(|i| (i % 256) as u8).collect();
    let key = Sha1::digest(uid.as_bytes());
    let mut obf = font.clone();
    for i in 0..1040 {
        obf[i] ^= key[i % 20];
    }
    let opf = format!(
        r#"<package version="3.0" unique-identifier="u"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:identifier id="u">{uid}</dc:identifier><dc:title>F</dc:title></metadata>
<manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="f" href="f.ttf" media-type="font/ttf"/></manifest>
<spine><itemref idref="a"/></spine></package>"#
    );
    let enc = r#"<encryption xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData>
<enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/><enc:CipherData><enc:CipherReference URI="OEBPS/f.ttf"/></enc:CipherData>
</enc:EncryptedData></encryption>"#;
    let src = build_epub(
        tmp.path(),
        "font.epub",
        &[
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("META-INF/encryption.xml", enc.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/a.xhtml", b"<html><body><p>x</p></body></html>"),
            ("OEBPS/f.ttf", &obf),
        ],
    );

    // STEP 2: after import the font must be byte-identical to the original.
    let (_, dir) = convert(&src, tmp.path());
    assert_eq!(
        std::fs::read(dir.join("resources/OEBPS/f.ttf")).unwrap(),
        font
    );
}

#[test]
fn rejects_non_epub_and_registry_detects() {
    let tmp = tempfile::tempdir().unwrap();
    let txt = tmp.path().join("a.txt");
    std::fs::write(&txt, "hello").unwrap();
    let reg = Registry::with_defaults();
    assert_eq!(reg.detect(&txt).unwrap().id(), "txt");
    let src = epub3(tmp.path());
    assert_eq!(reg.detect(&src).unwrap().id(), "epub");
    let junk = tmp.path().join("junk.epub");
    std::fs::write(&junk, "not a zip").unwrap();
    assert!(reg.detect(&junk).is_err());
    let mut w = JustBookWriter::create(&tmp.path().join("o")).unwrap();
    assert!(EpubAdapter.convert(&junk, &mut w).is_err());
}
