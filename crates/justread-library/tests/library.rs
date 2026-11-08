// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Library integration tests: import, duplicates, sorting, progress,
//! settings and annotations — all against a temporary directory.

use justread_formats::Registry;
use justread_library::*;
use justread_locator::Locator;

fn txt(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

fn loc(s: &str) -> Locator {
    s.parse().unwrap()
}

#[test]
fn import_list_progress_delete() {
    // STEP 1: open a library on disk (exercises migrations on a real file).
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let reg = Registry::with_defaults();

    // STEP 2: import two books.
    let a = txt(
        tmp.path(),
        "a.txt",
        "Title: The Zebra Book\nAuthor: Ann Alpha\n\nChapter 1\n\nHello.\n\nChapter 2\n\nBye.",
    );
    let b = txt(
        tmp.path(),
        "b.txt",
        "Title: Apple Tales\nAuthor: Bob Beta\n\nOnce.",
    );
    let ImportOutcome::Imported { book: ba, .. } = lib.import(&a, &reg).unwrap() else {
        panic!()
    };
    let ImportOutcome::Imported { book: bb, .. } = lib.import(&b, &reg).unwrap() else {
        panic!()
    };
    assert_eq!(ba.title, "The Zebra Book");
    assert_eq!(ba.authors, vec!["Ann Alpha"]);

    // STEP 3: package exists with the original kept.
    let m = lib.manifest(&ba.id).unwrap();
    assert_eq!(m.spine.len(), 2);
    assert!(lib
        .package_dir(&ba.id)
        .unwrap()
        .join("original/a.txt")
        .exists());

    // STEP 4: duplicates are detected by content hash.
    assert!(matches!(
        lib.import(&a, &reg).unwrap(),
        ImportOutcome::Duplicate { .. }
    ));

    // STEP 5: sorting ("The" is ignored for title sort) and search.
    let by_title = lib.list_books(&BookQuery::default()).unwrap();
    assert_eq!(
        by_title
            .iter()
            .map(|b| b.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Apple Tales", "The Zebra Book"]
    );
    let found = lib
        .list_books(&BookQuery {
            search: Some("bob".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(found.len(), 1);
    let none = lib
        .list_books(&BookQuery {
            search: Some("100%_".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(none.is_empty());

    // STEP 6: progress and "recently read".
    lib.save_progress(&bb.id, &loc("f:0/1:3"), 0.4).unwrap();
    lib.mark_opened(&bb.id).unwrap();
    let p = lib.progress(&bb.id).unwrap().unwrap();
    assert_eq!(p.locator, loc("f:0/1:3"));
    let by_progress = lib
        .list_books(&BookQuery {
            sort: SortKey::Progress,
            descending: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_progress[0].id, bb.id);
    let recent = lib
        .list_books(&BookQuery {
            sort: SortKey::Recent,
            descending: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(recent[0].id, bb.id);

    // STEP 7: delete removes the row, package and progress.
    lib.delete_book(&bb.id).unwrap();
    assert!(lib.get_book(&bb.id).is_err());
    assert!(!tmp.path().join("books").join(&bb.id).exists());
    assert!(lib.progress(&bb.id).unwrap().is_none());
}

#[test]
fn failed_imports_leave_nothing_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let books = tmp.path().join("books");
    let mut lib = Library::open_in_memory(&books).unwrap();
    let reg = Registry::with_defaults();

    // Unsupported extension and an empty text file.
    let bad = txt(tmp.path(), "x.pdfx", "nope");
    assert!(lib.import(&bad, &reg).is_err());
    let empty = txt(tmp.path(), "empty.txt", "   \n\n");
    assert!(lib.import(&empty, &reg).is_err());
    assert_eq!(std::fs::read_dir(&books).unwrap().count(), 0);
    assert!(lib.list_books(&BookQuery::default()).unwrap().is_empty());
}

#[test]
fn package_dir_rejects_non_uuid_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::open_in_memory(tmp.path()).unwrap();
    assert!(lib.package_dir("../etc").is_err());
    assert!(lib
        .package_dir("00000000-0000-4000-8000-000000000000")
        .is_ok());
}

#[test]
fn settings_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::open_in_memory(tmp.path()).unwrap();
    let v = serde_json::json!({ "fontSize": 18, "theme": "sepia" });
    lib.set_setting("reader.typography", &v).unwrap();
    assert_eq!(lib.get_setting("reader.typography").unwrap(), Some(v));
    assert_eq!(lib.get_setting("missing").unwrap(), None);
    assert!(lib.set_setting("bad key!", &serde_json::json!(1)).is_err());
}

#[test]
fn annotations_lifecycle() {
    // STEP 1: a book to annotate.
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open_in_memory(&tmp.path().join("books")).unwrap();
    let src = txt(tmp.path(), "a.txt", "Some text here.\n\nMore text.");
    let ImportOutcome::Imported { book, .. } =
        lib.import(&src, &Registry::with_defaults()).unwrap()
    else {
        panic!()
    };

    let new = |kind, start: &str, end: Option<&str>| NewAnnotation {
        book_id: book.id.clone(),
        kind,
        start: loc(start),
        end: end.map(loc),
        quote: "q".into(),
        note: String::new(),
        color: None,
        chapter: None,
    };

    // STEP 2: create a highlight and a bookmark; they come back in reading order.
    let hl = lib
        .add_annotation(new(AnnotationKind::Highlight, "f:0/2:0", Some("f:0/2:4")))
        .unwrap();
    let bm = lib
        .add_annotation(new(AnnotationKind::Bookmark, "f:0/1:0", None))
        .unwrap();
    let list = lib.list_annotations(&book.id).unwrap();
    assert_eq!(
        list.iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
        vec![bm.id.clone(), hl.id.clone()]
    );

    // STEP 3: invalid shapes are rejected.
    assert!(lib
        .add_annotation(new(AnnotationKind::Highlight, "f:0/2:4", Some("f:0/2:4")))
        .is_err());
    assert!(lib
        .add_annotation(new(AnnotationKind::Highlight, "f:0/2:0", None))
        .is_err());
    assert!(lib
        .add_annotation(new(AnnotationKind::Bookmark, "f:0/2:0", Some("f:0/2:4")))
        .is_err());
    let mut bad_color = new(AnnotationKind::Highlight, "f:0/1:0", Some("f:0/1:2"));
    bad_color.color = Some("red; drop table".into());
    assert!(lib.add_annotation(bad_color).is_err());

    // STEP 4: adding a note turns a highlight into a note; clearing it turns it back.
    let n = lib
        .update_annotation(
            &hl.id,
            AnnotationPatch {
                note: Some("why".into()),
                color: Some("blue".into()),
            },
        )
        .unwrap();
    assert_eq!(
        (n.kind, n.note.as_str(), n.color.as_str()),
        (AnnotationKind::Note, "why", "blue")
    );
    let h = lib
        .update_annotation(
            &hl.id,
            AnnotationPatch {
                note: Some(" ".into()),
                color: None,
            },
        )
        .unwrap();
    assert_eq!(h.kind, AnnotationKind::Highlight);

    // STEP 5: delete one; deleting the book cascades the rest.
    lib.delete_annotation(&bm.id).unwrap();
    assert_eq!(lib.list_annotations(&book.id).unwrap().len(), 1);
    lib.delete_book(&book.id).unwrap();
    assert!(lib.get_annotation(&hl.id).is_err());
}

#[test]
fn migrations_are_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    drop(Library::open(tmp.path()).unwrap());
    drop(Library::open(tmp.path()).unwrap()); // second open must not re-run migrations
    let conn = rusqlite::Connection::open(tmp.path().join("library.sqlite3")).unwrap();
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, db::latest_version());
}

#[test]
fn fixed_layout_import_and_cover() {
    // STEP 1: a tiny PDF (one page) built by hand.
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open_in_memory(&tmp.path().join("books")).unwrap();
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>",
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offs = vec![];
    for (i, o) in objs.iter().enumerate() {
        offs.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for o in offs {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n").as_bytes(),
    );
    let src = tmp.path().join("Field Notes.pdf");
    std::fs::write(&src, &pdf).unwrap();

    // STEP 2: import → fixed layout, no duplicate original, title from file name.
    let ImportOutcome::Imported { book, .. } = lib
        .import(&src, &justread_formats::Registry::with_defaults())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(book.title, "Field Notes");
    assert_eq!(book.format, "pdf");
    let m = lib.manifest(&book.id).unwrap();
    assert_eq!(m.layout, justbook::Layout::Fixed);
    assert_eq!(m.fixed.as_ref().unwrap().page_count, 1);
    let dir = lib.package_dir(&book.id).unwrap();
    assert!(!dir.join("original").exists());

    // STEP 3: the UI-rendered cover is stored; junk is refused.
    assert!(lib.set_cover(&book.id, b"<svg/>").is_err());
    assert_eq!(
        lib.set_cover(&book.id, b"\x89PNG\r\n\x1a\nrest").unwrap(),
        "cover.png"
    );
    assert_eq!(
        lib.get_book(&book.id).unwrap().cover.as_deref(),
        Some("cover.png")
    );
    assert_eq!(
        lib.manifest(&book.id).unwrap().cover.as_deref(),
        Some("cover.png")
    );
}
