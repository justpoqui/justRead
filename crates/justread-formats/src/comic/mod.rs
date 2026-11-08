// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Comic book archives: CBZ (ZIP), CB7 (7-Zip) and CBR (RAR, detection only).
//!
//! A comic is a sequence of page images. Import copies every image
//! (unchanged, lossless) into `pages/`, records each page's size from its
//! header, orders pages naturally by file name (`p2` before `p10`), and reads
//! optional `ComicInfo.xml` metadata (title, series, writers, manga direction).

mod info;

use std::io::Read;
use std::path::Path;

use justbook::{FixedContent, FixedKind, FixedPage, JustBookWriter, Layout, Metadata};

use crate::archive::SafeArchive;
use crate::epub::file_stem;
use crate::images::{dimensions, extension, natural_cmp, PAGE_EXTENSIONS};
use crate::{
    BookContent, Confidence, DrmStatus, FormatAdapter, FormatError, FormatFamily, Inspection,
    Result,
};

pub use info::ComicInfo;

/// Most pages a single comic may have.
const MAX_PAGES: usize = 10_000;
/// Largest single page image.
const MAX_PAGE_BYTES: u64 = 128 * 1024 * 1024;
/// Largest total of decompressed page data (7z; ZIP uses SafeArchive's own limit).
const MAX_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const SEVEN_Z_MAGIC: &[u8] = b"7z\xBC\xAF\x27\x1C";
const RAR_MAGIC: &[u8] = b"Rar!\x1A\x07";

/// Is this archive entry a page image we should use?
fn is_page(name: &str) -> bool {
    // Skip macOS resource forks and hidden files/folders anywhere in the path.
    if name.starts_with("__MACOSX/") || name.split('/').any(|seg| seg.starts_with('.')) {
        return false;
    }
    extension(name).is_some_and(|e| PAGE_EXTENSIONS.contains(&e.as_str()))
}

/// Collects pages as they are extracted, then orders them.
struct PageCollector {
    /// (original name, written page)
    pages: Vec<(String, FixedPage)>,
    info: Option<ComicInfo>,
    total: u64,
    warnings: Vec<String>,
}

impl PageCollector {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            info: None,
            total: 0,
            warnings: Vec::new(),
        }
    }

    /// Store one archive entry: a page image, ComicInfo.xml, or nothing.
    fn add(&mut self, name: &str, bytes: Vec<u8>, out: &mut JustBookWriter) -> Result<()> {
        // STEP 1: metadata file.
        if name
            .rsplit('/')
            .next()
            .is_some_and(|f| f.eq_ignore_ascii_case("ComicInfo.xml"))
        {
            self.info = Some(ComicInfo::parse(&crate::decode_text(&bytes)));
            return Ok(());
        }
        if !is_page(name) {
            return Ok(());
        }

        // STEP 2: limits.
        if self.pages.len() >= MAX_PAGES {
            return Err(FormatError::TooLarge(format!(
                "more than {MAX_PAGES} pages"
            )));
        }
        self.total += bytes.len() as u64;
        if self.total > MAX_TOTAL_BYTES {
            return Err(FormatError::TooLarge(
                "comic decompresses past the limit".into(),
            ));
        }

        // STEP 3: write under a neutral name; order is decided in `finish`.
        let ext = extension(name).unwrap_or_else(|| "jpg".into());
        let (width, height) = dimensions(&bytes).unwrap_or_else(|| {
            self.warnings.push(format!("unknown image size for {name}"));
            (0, 0)
        });
        let href = out.write_file(&format!("pages/{:05}.{ext}", self.pages.len() + 1), &bytes)?;
        self.pages.push((
            name.to_string(),
            FixedPage {
                href,
                width,
                height,
            },
        ));
        Ok(())
    }

    /// Order pages, copy the cover, and build the book description.
    fn finish(
        mut self,
        src: &Path,
        format: &'static str,
        out: &mut JustBookWriter,
    ) -> Result<BookContent> {
        // STEP 1: a comic needs at least one page.
        if self.pages.is_empty() {
            return Err(FormatError::Invalid {
                format,
                reason: "no page images found".into(),
            });
        }
        self.pages.sort_by(|a, b| natural_cmp(&a.0, &b.0));
        let pages: Vec<FixedPage> = self.pages.into_iter().map(|(_, p)| p).collect();

        // STEP 2: metadata — ComicInfo.xml if present, else the file name.
        let info = self.info.unwrap_or_default();
        let metadata = info.to_metadata(&file_stem(src));

        // STEP 3: the first page doubles as the cover.
        let first = &pages[0];
        let cover_ext = extension(&first.href).unwrap_or_else(|| "jpg".into());
        let cover_bytes = std::fs::read(out.working_dir().join(&first.href))?;
        let cover = Some(out.write_file(&format!("cover.{cover_ext}"), &cover_bytes)?);

        Ok(BookContent {
            layout: Layout::Fixed,
            fixed: Some(FixedContent {
                kind: FixedKind::Images,
                page_count: pages.len() as u32,
                document: None,
                pages,
                right_to_left: info.right_to_left,
            }),
            // Every page is stored unchanged, so the archive itself need not be kept.
            keep_original: false,
            metadata,
            spine: vec![],
            toc: vec![],
            resources: vec![],
            cover,
            warnings: self.warnings,
        })
    }
}

// ---------------------------------------------------------------------------
// CBZ
// ---------------------------------------------------------------------------

/// ZIP-based comics.
pub struct CbzAdapter;

impl FormatAdapter for CbzAdapter {
    fn id(&self) -> &'static str {
        "cbz"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Fixed
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        if head.starts_with(ZIP_MAGIC) && ext == Some("cbz") {
            Confidence::High
        } else {
            Confidence::No
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        let mut ar = SafeArchive::open(src, "CBZ")?;
        let info_name = ar
            .names()
            .find(|n| {
                n.rsplit('/')
                    .next()
                    .is_some_and(|f| f.eq_ignore_ascii_case("ComicInfo.xml"))
            })
            .cloned();
        let info = info_name
            .and_then(|n| ar.read_text(&n).ok())
            .map(|x| ComicInfo::parse(&x))
            .unwrap_or_default();
        Ok(Inspection {
            metadata: info.to_metadata(&file_stem(src)),
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        // STEP 1: SafeArchive enforces entry-count and size limits.
        let mut ar = SafeArchive::open(src, "CBZ")?;
        let mut names: Vec<String> = ar.names().cloned().collect();
        names.sort();
        // STEP 2: extract pages (and ComicInfo.xml).
        let mut pc = PageCollector::new();
        for name in names {
            if is_page(&name) || name.to_ascii_lowercase().ends_with("comicinfo.xml") {
                let bytes = ar.read(&name)?;
                pc.add(&name, bytes, out)?;
            }
        }
        pc.finish(src, "CBZ", out)
    }
}

// ---------------------------------------------------------------------------
// CB7
// ---------------------------------------------------------------------------

/// 7-Zip-based comics.
pub struct Cb7Adapter;

impl FormatAdapter for Cb7Adapter {
    fn id(&self) -> &'static str {
        "cb7"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Fixed
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        if head.starts_with(SEVEN_Z_MAGIC) && matches!(ext, Some("cb7") | Some("7z")) {
            Confidence::High
        } else {
            Confidence::No
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        Ok(Inspection {
            metadata: Metadata {
                title: file_stem(src),
                ..Default::default()
            },
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, src: &Path, out: &mut JustBookWriter) -> Result<BookContent> {
        let invalid = |e: sevenz_rust2::Error| FormatError::Invalid {
            format: "CB7",
            reason: e.to_string(),
        };

        // STEP 1: open (no password support: encrypted archives are refused).
        let mut reader = sevenz_rust2::ArchiveReader::open(src, sevenz_rust2::Password::empty())
            .map_err(invalid)?;
        if reader.archive().files.len() > crate::archive::MAX_ENTRIES {
            return Err(FormatError::TooLarge("too many entries".into()));
        }

        // STEP 2: 7z may be "solid", so entries are decoded in archive order in one pass.
        let mut pc = PageCollector::new();
        let mut failure: Option<FormatError> = None;
        reader
            .for_each_entries(|entry, data| {
                if entry.is_directory()
                    || !(is_page(entry.name())
                        || entry.name().to_ascii_lowercase().ends_with("comicinfo.xml"))
                {
                    return Ok(true); // the reader skips (but verifies) unread data
                }
                // Read at most limit+1 bytes: the declared size may lie.
                let mut bytes = Vec::new();
                data.take(MAX_PAGE_BYTES + 1).read_to_end(&mut bytes)?;
                let result = if bytes.len() as u64 > MAX_PAGE_BYTES {
                    Err(FormatError::TooLarge(format!(
                        "page {} is too large",
                        entry.name()
                    )))
                } else {
                    pc.add(entry.name(), bytes, out)
                };
                match result {
                    Ok(()) => Ok(true),
                    Err(e) => {
                        failure = Some(e);
                        Ok(false)
                    }
                }
            })
            .map_err(invalid)?;
        if let Some(e) = failure {
            return Err(e);
        }
        pc.finish(src, "CB7", out)
    }
}

// ---------------------------------------------------------------------------
// CBR
// ---------------------------------------------------------------------------

/// RAR-based comics: recognized, but not yet importable.
///
/// The only mature RAR decoder (RARLAB's unRAR source, wrapped by the `unrar`
/// crate) is under a proprietary license that is not on the project's
/// allow-list, so CBR support awaits a licensing decision. Users get a clear
/// message instead of "unsupported format".
pub struct CbrAdapter;

impl FormatAdapter for CbrAdapter {
    fn id(&self) -> &'static str {
        "cbr"
    }
    fn family(&self) -> FormatFamily {
        FormatFamily::Fixed
    }
    fn sniff(&self, head: &[u8], ext: Option<&str>) -> Confidence {
        if head.starts_with(RAR_MAGIC) || ext == Some("cbr") {
            Confidence::High
        } else {
            Confidence::No
        }
    }
    fn inspect(&self, src: &Path) -> Result<Inspection> {
        Ok(Inspection {
            metadata: Metadata {
                title: file_stem(src),
                ..Default::default()
            },
            drm: DrmStatus::None,
        })
    }
    fn convert(&self, src: &Path, _out: &mut JustBookWriter) -> Result<BookContent> {
        // Many ".cbr" files are really ZIPs or 7z archives with the wrong extension.
        let mut head = [0u8; 8];
        let n = std::fs::File::open(src)?.read(&mut head)?;
        if head[..n].starts_with(ZIP_MAGIC) {
            return CbzAdapter.convert(src, _out);
        }
        if head[..n].starts_with(SEVEN_Z_MAGIC) {
            return Cb7Adapter.convert(src, _out);
        }
        Err(FormatError::NotYetSupported(
            "RAR comics (CBR) are not supported yet. Re-packing the file as CBZ (a ZIP of the images) works today.".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_filter() {
        assert!(is_page("Vol 1/001.jpg"));
        assert!(is_page("p2.PNG"));
        assert!(!is_page("__MACOSX/._001.jpg"));
        assert!(!is_page(".hidden/001.jpg"));
        assert!(!is_page("ComicInfo.xml"));
        assert!(!is_page("notes.txt"));
    }
}
