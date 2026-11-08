// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Fixed-layout adapters (PDF, CBZ, CB7, CBR). All fixtures are generated here.

use std::io::Write;
use std::path::{Path, PathBuf};

use justbook::{FixedKind, JustBookWriter};
use justread_formats::comic::{Cb7Adapter, CbrAdapter, CbzAdapter};
use justread_formats::pdf::PdfAdapter;
use justread_formats::{BookContent, DrmStatus, FormatAdapter, FormatError, Registry};

/// Build a minimal valid PDF with `pages` empty pages and an /Info dictionary.
/// Offsets in the xref table are computed, so the file is well-formed.
pub fn make_pdf(pages: usize, title: &str, extra_trailer: &str) -> Vec<u8> {
    // STEP 1: objects — 1 catalog, 2 page tree, 3 info, 4.. pages.
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 4 + i)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {pages} >>",
            kids.join(" ")
        ),
        format!("<< /Title ({title}) /Author (Pat Writer) >>"),
    ];
    for _ in 0..pages {
        objects.push("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >>".to_string());
    }
    // STEP 2: body with recorded offsets, then xref + trailer.
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R {extra_trailer}>>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    out
}

/// PNG header bytes with a given size (enough for the size sniffer).
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 2, 0, 0, 0]);
    v
}

fn convert(adapter: &dyn FormatAdapter, src: &Path, root: &Path) -> (BookContent, PathBuf) {
    let dir = root.join("pkg");
    let mut w = JustBookWriter::create(&dir).unwrap();
    let c = adapter.convert(src, &mut w).unwrap();
    // Keep the temp output for inspection by returning its working dir.
    let wd = w.working_dir().to_path_buf();
    std::mem::forget(w); // tests read files directly; skip the finish/rollback step
    (c, wd)
}

#[test]
fn pdf_import() {
    // STEP 1: a three-page PDF.
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("doc.pdf");
    std::fs::write(&src, make_pdf(3, "A Study of Lanterns", "")).unwrap();
    assert_eq!(Registry::with_defaults().detect(&src).unwrap().id(), "pdf");

    // STEP 2: converted metadata, page count and stored document.
    let (c, dir) = convert(&PdfAdapter, &src, tmp.path());
    let f = c.fixed.unwrap();
    assert_eq!(f.kind, FixedKind::Pdf);
    assert_eq!(f.page_count, 3);
    assert_eq!(c.metadata.title, "A Study of Lanterns");
    assert_eq!(c.metadata.authors, vec!["Pat Writer"]);
    assert!(!c.keep_original);
    assert_eq!(
        std::fs::read(dir.join("content/book.pdf")).unwrap(),
        std::fs::read(&src).unwrap()
    );
}

#[test]
fn pdf_drm_and_damage() {
    let tmp = tempfile::tempdir().unwrap();
    // An Adobe ADEPT security handler is refused as DRM.
    let drm = tmp.path().join("drm.pdf");
    let mut bytes = make_pdf(1, "X", "/Encrypt 9 0 R ");
    bytes.extend_from_slice(b"9 0 obj\n<< /Filter /EBX_HANDLER >>\nendobj\n");
    std::fs::write(&drm, &bytes).unwrap();
    assert_eq!(
        PdfAdapter.inspect(&drm).unwrap().drm,
        DrmStatus::Protected("Adobe ADEPT".into())
    );
    let mut w = JustBookWriter::create(&tmp.path().join("o1")).unwrap();
    assert!(matches!(
        PdfAdapter.convert(&drm, &mut w),
        Err(FormatError::DrmProtected(_))
    ));

    // A truncated file is reported as damaged, not a crash.
    let bad = tmp.path().join("bad.pdf");
    std::fs::write(&bad, b"%PDF-1.4\n1 0 obj << broken").unwrap();
    let mut w = JustBookWriter::create(&tmp.path().join("o2")).unwrap();
    assert!(PdfAdapter.convert(&bad, &mut w).is_err());
}

#[test]
fn cbz_import_orders_pages_naturally() {
    // STEP 1: pages stored out of order, plus junk and metadata.
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("issue.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&src).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (name, bytes) in [
        ("p10.png", png(800, 1200)),
        ("p2.png", png(801, 1200)),
        ("p1.png", png(802, 1200)),
        ("__MACOSX/._p1.png", png(1, 1)),
        ("notes.txt", b"hi".to_vec()),
        ("ComicInfo.xml", b"<ComicInfo><Series>Harbor</Series><Number>2</Number><Writer>Ann A</Writer><Manga>YesAndRightToLeft</Manga></ComicInfo>".to_vec()),
    ] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    assert_eq!(Registry::with_defaults().detect(&src).unwrap().id(), "cbz");

    // STEP 2: three pages in natural order with sizes, RTL from ComicInfo, cover = page 1.
    let (c, _) = convert(&CbzAdapter, &src, tmp.path());
    let f = c.fixed.unwrap();
    assert_eq!(f.page_count, 3);
    assert_eq!(
        f.pages.iter().map(|p| p.width).collect::<Vec<_>>(),
        vec![802, 801, 800]
    );
    assert!(f.right_to_left);
    assert_eq!(c.metadata.title, "Harbor #2");
    assert_eq!(c.cover.as_deref(), Some("cover.png"));
}

#[test]
fn cb7_import() {
    // STEP 1: build a 7z from a folder of pages.
    let tmp = tempfile::tempdir().unwrap();
    let pages = tmp.path().join("pages");
    std::fs::create_dir_all(&pages).unwrap();
    std::fs::write(pages.join("b.png"), png(500, 700)).unwrap();
    std::fs::write(pages.join("a.png"), png(400, 600)).unwrap();
    let src = tmp.path().join("book.cb7");
    sevenz_rust2::compress_to_path(&pages, &src).unwrap();
    assert_eq!(Registry::with_defaults().detect(&src).unwrap().id(), "cb7");

    // STEP 2: both pages, in order.
    let (c, _) = convert(&Cb7Adapter, &src, tmp.path());
    let f = c.fixed.unwrap();
    assert_eq!(
        f.pages
            .iter()
            .map(|p| (p.width, p.height))
            .collect::<Vec<_>>(),
        vec![(400, 600), (500, 700)]
    );
    assert_eq!(c.metadata.title, "book");
}

#[test]
fn cbr_is_recognized_with_a_clear_message() {
    let tmp = tempfile::tempdir().unwrap();
    // A real RAR signature → "not yet supported".
    let rar = tmp.path().join("x.cbr");
    std::fs::write(&rar, b"Rar!\x1A\x07\x01\x00rest").unwrap();
    let mut w = JustBookWriter::create(&tmp.path().join("o")).unwrap();
    let err = CbrAdapter.convert(&rar, &mut w).unwrap_err();
    assert!(err.to_string().contains("CBZ"), "{err}");

    // A ".cbr" that is actually a ZIP is imported as CBZ.
    let fake = tmp.path().join("y.cbr");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&fake).unwrap());
    zip.start_file("1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&png(10, 20)).unwrap();
    zip.finish().unwrap();
    let (c, _) = convert(&CbrAdapter, &fake, tmp.path());
    assert_eq!(c.fixed.unwrap().page_count, 1);
}
