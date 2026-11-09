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

/// Phase 8: two imported books in a fresh library.
fn two_books() -> (tempfile::TempDir, Library, BookSummary, BookSummary) {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let reg = Registry::with_defaults();
    let a = txt(tmp.path(), "a.txt", "Title: Alpha\n\nOne.");
    let b = txt(tmp.path(), "b.txt", "Title: Beta\n\nTwo.");
    let ImportOutcome::Imported { book: ba, .. } = lib.import(&a, &reg).unwrap() else {
        panic!()
    };
    let ImportOutcome::Imported { book: bb, .. } = lib.import(&b, &reg).unwrap() else {
        panic!()
    };
    (tmp, lib, ba, bb)
}

fn query(status: Option<BookStatus>, collection: Option<&str>) -> BookQuery {
    BookQuery {
        status,
        collection: collection.map(Into::into),
        ..Default::default()
    }
}

#[test]
fn shelves_follow_opening_and_marking() {
    let (_tmp, lib, a, b) = two_books();
    // STEP 1: new books are unread; opening moves a book to Reading.
    assert_eq!(lib.get_book(&a.id).unwrap().status, BookStatus::Unread);
    lib.mark_opened(&a.id).unwrap();
    assert_eq!(lib.get_book(&a.id).unwrap().status, BookStatus::Reading);
    // STEP 2: finished records a date; opening again keeps it finished.
    lib.set_status(&a.id, BookStatus::Finished).unwrap();
    lib.mark_opened(&a.id).unwrap();
    let fa = lib.get_book(&a.id).unwrap();
    assert_eq!(fa.status, BookStatus::Finished);
    assert!(fa.finished_at.is_some());
    // STEP 3: shelf filter.
    let unread: Vec<_> = lib
        .list_books(&query(Some(BookStatus::Unread), None))
        .unwrap();
    assert_eq!(
        unread.iter().map(|x| &x.id).collect::<Vec<_>>(),
        vec![&b.id]
    );
    // Back to Reading clears the finish date.
    lib.set_status(&a.id, BookStatus::Reading).unwrap();
    assert_eq!(lib.get_book(&a.id).unwrap().finished_at, None);
    assert!(lib.set_status("nope", BookStatus::Reading).is_err());
}

#[test]
fn collections_lifecycle() {
    let (_tmp, lib, a, b) = two_books();
    let scifi = lib.create_collection("  Sci-fi  ").unwrap();
    assert_eq!(scifi.name, "Sci-fi");
    assert!(
        lib.create_collection("sci-FI").is_err(),
        "names are unique ignoring case"
    );
    assert!(lib.create_collection("   ").is_err());
    let club = lib.create_collection("Book club").unwrap();

    lib.set_in_collection(&a.id, &scifi.id, true).unwrap();
    lib.set_in_collection(&a.id, &scifi.id, true).unwrap(); // idempotent
    lib.set_in_collection(&a.id, &club.id, true).unwrap();
    lib.set_in_collection(&b.id, &club.id, true).unwrap();
    assert!(lib.set_in_collection(&a.id, "missing", true).is_err());

    let mut ca = lib.get_book(&a.id).unwrap().collections;
    ca.sort();
    let mut want = vec![scifi.id.clone(), club.id.clone()];
    want.sort();
    assert_eq!(ca, want);
    let in_scifi = lib.list_books(&query(None, Some(&scifi.id))).unwrap();
    assert_eq!(in_scifi.len(), 1);
    let names: Vec<_> = lib
        .list_collections()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.count))
        .collect();
    assert_eq!(
        names,
        vec![("Book club".to_string(), 2), ("Sci-fi".to_string(), 1)]
    );

    lib.rename_collection(&club.id, "Club").unwrap();
    assert!(lib.rename_collection(&club.id, "sci-fi").is_err());
    lib.set_in_collection(&b.id, &club.id, false).unwrap();
    lib.delete_collection(&scifi.id).unwrap();
    assert_eq!(
        lib.get_book(&a.id).unwrap().collections,
        vec![club.id.clone()]
    );
    assert_eq!(
        lib.list_books(&query(None, None)).unwrap().len(),
        2,
        "deleting a collection keeps its books"
    );
}

#[test]
fn reading_time_streaks_and_pace() {
    let (_tmp, lib, a, b) = two_books();
    // STEP 1: three days in a row, then a gap; jumps don't count as progress.
    lib.record_reading(&a.id, "2026-11-07", 300, 0.01).unwrap();
    lib.record_reading(&a.id, "2026-11-08", 600, 0.02).unwrap();
    lib.record_reading(&b.id, "2026-11-08", 120, 0.0).unwrap();
    lib.record_reading(&a.id, "2026-11-09", 60, 0.40).unwrap(); // a jump: time counts, progress doesn't
    lib.record_reading(&a.id, "2026-11-01", 30, 0.0).unwrap();
    // Invalid input is rejected.
    assert!(lib.record_reading(&a.id, "2026-02-30", 60, 0.0).is_err());
    assert!(lib.record_reading(&a.id, "2026-11-09", 0, 0.0).is_err());
    assert!(lib.record_reading(&a.id, "2026-11-09", 5000, 0.0).is_err());
    assert!(lib
        .record_reading("missing", "2026-11-09", 60, 0.0)
        .is_err());

    // STEP 2: library stats on the 9th.
    let s = lib.reading_stats("2026-11-09", 14).unwrap();
    assert_eq!(s.today_seconds, 60);
    assert_eq!(s.week_seconds, 300 + 600 + 120 + 60);
    assert_eq!(s.total_seconds, 300 + 600 + 120 + 60 + 30);
    assert_eq!(s.streak_days, 3);
    assert_eq!(s.daily.len(), 14);
    assert_eq!(s.daily.last().unwrap().day, "2026-11-09");
    assert_eq!(s.daily[13 - 1].seconds, 720);
    // A day without reading yet keeps yesterday's streak alive.
    assert_eq!(lib.reading_stats("2026-11-10", 7).unwrap().streak_days, 3);
    assert_eq!(lib.reading_stats("2026-11-11", 7).unwrap().streak_days, 0);

    // STEP 3: per-book pace: 990 s for 3 % → the remaining 70 % ≈ 23 100 s.
    lib.save_progress(&a.id, &loc("f:0/0:0"), 0.30).unwrap();
    let bs = lib.book_stats(&a.id).unwrap();
    assert_eq!(bs.seconds, 990);
    assert_eq!(bs.days_read, 4);
    assert_eq!(bs.first_day.as_deref(), Some("2026-11-01"));
    assert!((bs.progress_read - 0.03).abs() < 1e-9);
    assert_eq!(bs.seconds_left, Some(23_100));
    // Not enough data → no estimate.
    assert_eq!(lib.book_stats(&b.id).unwrap().seconds_left, None);
    // Finished books count toward the year.
    lib.set_status(&b.id, BookStatus::Finished).unwrap();
    let year = justread_library::export::iso_date(justread_library::db::now_ms());
    let s = lib.reading_stats(&year, 7).unwrap();
    assert_eq!((s.books_finished, s.finished_this_year), (1, 1));
}
