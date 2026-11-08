// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Integration tests: write a package, read it back, and check safety rules.

use justbook::*;

fn manifest(id: &str) -> Manifest {
    Manifest {
        schema_version: SCHEMA_VERSION,
        id: id.into(),
        layout: Layout::Reflowable,
        metadata: Metadata {
            title: "Test".into(),
            ..Default::default()
        },
        spine: vec![SpineItem {
            href: "content/c0001.xhtml".into(),
            title: Some("One".into()),
            chars: 5,
            blocks: 2,
            linear: true,
        }],
        toc: vec![TocEntry {
            title: "One".into(),
            href: Some("content/c0001.xhtml#a".into()),
            children: vec![],
        }],
        resources: vec![],
        cover: None,
        source: SourceInfo {
            format: "txt".into(),
            file_name: "t.txt".into(),
            sha256: "ab".into(),
            converter_version: "1".into(),
        },
        fixed: None,
    }
}

#[test]
fn write_then_read_round_trip() {
    // STEP 1: write a one-chapter package.
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("book");
    let mut w = JustBookWriter::create(&dest).unwrap();
    w.write_file("content/c0001.xhtml", b"<html/>").unwrap();
    let m = manifest("id-1");
    let out = w.finish(&m).unwrap();

    // STEP 2: the temp dir is gone and the manifest reads back unchanged.
    assert_eq!(out, dest);
    assert!(!root.path().join("book.partial").exists());
    assert_eq!(load_manifest(&dest).unwrap(), m);

    // STEP 3: the protocol resolver allows package files and rejects traversal.
    assert!(resolve_in_package(&dest, "content/c0001.xhtml").is_ok());
    assert!(resolve_in_package(&dest, "../book/manifest.json").is_err());
    assert!(resolve_in_package(&dest, "/etc/passwd").is_err());
}

#[test]
fn invalid_package_is_rolled_back() {
    // STEP 1: forget to write the chapter the manifest points to.
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("book");
    let w = JustBookWriter::create(&dest).unwrap();
    let err = w.finish(&manifest("id-2")).unwrap_err();

    // STEP 2: nothing is left behind.
    assert!(err.to_string().contains("chapter missing"));
    assert!(!dest.exists());
    assert!(!root.path().join("book.partial").exists());
}

#[test]
fn writer_rejects_unsafe_paths() {
    let root = tempfile::tempdir().unwrap();
    let mut w = JustBookWriter::create(&root.path().join("b")).unwrap();
    assert!(w.write_file("../escape.txt", b"x").is_err());
    assert!(w.write_file("manifest.json", b"x").is_err());
    assert!(w.write_file("resources/./img/a.png", b"x").unwrap() == "resources/img/a.png");
    assert!(w.has("resources/img/a.png"));
}

#[test]
fn newer_schema_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut m = manifest("id-3");
    m.schema_version = SCHEMA_VERSION + 1;
    std::fs::write(
        root.path().join("manifest.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        load_manifest(root.path()),
        Err(Error::UnsupportedSchema { .. })
    ));
}

#[test]
fn fixed_layout_packages_validate() {
    // STEP 1: an image-based (comic) package with two pages.
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("comic");
    let mut w = JustBookWriter::create(&dest).unwrap();
    w.write_file("pages/p0001.png", b"x").unwrap();
    w.write_file("pages/p0002.png", b"x").unwrap();
    let mut m = manifest("id-4");
    m.layout = Layout::Fixed;
    m.spine.clear();
    m.toc.clear();
    let page = |h: &str| FixedPage {
        href: h.into(),
        width: 100,
        height: 150,
    };
    m.fixed = Some(FixedContent {
        kind: FixedKind::Images,
        page_count: 2,
        document: None,
        pages: vec![page("pages/p0001.png"), page("pages/p0002.png")],
        right_to_left: false,
    });
    w.finish(&m).unwrap();
    assert_eq!(load_manifest(&dest).unwrap().fixed.unwrap().pages.len(), 2);

    // STEP 2: a fixed manifest pointing at a missing page is rejected.
    let dest2 = root.path().join("broken");
    let w2 = JustBookWriter::create(&dest2).unwrap();
    assert!(w2
        .finish(&m)
        .unwrap_err()
        .to_string()
        .contains("page missing"));
}
