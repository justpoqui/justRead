// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! The import pipeline: file → validated → justBook package → library row.
//!
//! ```text
//! hash ─▶ duplicate? ─▶ detect format ─▶ convert (DRM check inside)
//!      ─▶ copy original ─▶ finish package (atomic) ─▶ insert row (transaction)
//! ```
//! If inserting the row fails, the freshly written package is removed so
//! disk and database never disagree.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use justbook::{JustBookWriter, Manifest, SourceInfo, SCHEMA_VERSION};
use justread_formats::{Registry, CONVERTER_VERSION};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{db, BookSummary, Library, LibraryError, Result};

/// What happened to one imported file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ImportOutcome {
    /// Newly added.
    Imported {
        book: BookSummary,
        warnings: Vec<String>,
    },
    /// The exact same file is already in the library.
    Duplicate { book: BookSummary },
}

impl Library {
    /// Import one file.
    pub fn import(&mut self, src: &Path, registry: &Registry) -> Result<ImportOutcome> {
        // STEP 1: content hash — identical files are imported once.
        let sha256 = hash_file(src)?;
        let existing: Option<String> = self
            .conn()
            .query_row(
                "SELECT id FROM books WHERE source_sha256 = ?1",
                params![sha256],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(ImportOutcome::Duplicate {
                book: self.get_book(&id)?,
            });
        }

        // STEP 2: pick the adapter.
        let adapter = registry.detect(src)?;

        // STEP 3: convert into a temp package (removed automatically on error).
        let id = uuid::Uuid::new_v4().to_string();
        let dest = self.books_dir().join(&id);
        let mut writer = JustBookWriter::create(&dest)?;
        let content = adapter.convert(src, &mut writer)?;

        // STEP 4: keep the untouched original for future re-conversion, unless the
        // package already holds it losslessly (PDFs, comic pages).
        let file_name = safe_file_name(src);
        let file_size = std::fs::metadata(src)?.len();
        if content.keep_original {
            writer.write_file(&format!("original/{file_name}"), &std::fs::read(src)?)?;
        }

        // STEP 5: final manifest; `finish` validates and renames atomically.
        let manifest = Manifest {
            schema_version: SCHEMA_VERSION,
            id: id.clone(),
            layout: content.layout,
            metadata: content.metadata,
            spine: content.spine,
            toc: content.toc,
            resources: content.resources,
            cover: content.cover,
            source: SourceInfo {
                format: adapter.id().to_string(),
                file_name: file_name.clone(),
                sha256: sha256.clone(),
                converter_version: CONVERTER_VERSION.to_string(),
            },
            fixed: content.fixed,
        };
        writer.finish(&manifest)?;

        // STEP 6: database row; roll the package back if this fails.
        if let Err(e) = self.insert_book(&manifest, file_size) {
            let _ = std::fs::remove_dir_all(&dest);
            return Err(e);
        }
        Ok(ImportOutcome::Imported {
            book: self.get_book(&id)?,
            warnings: content.warnings,
        })
    }

    /// Insert the books/authors rows for a finished package.
    fn insert_book(&mut self, m: &Manifest, file_size: u64) -> Result<()> {
        let md = &m.metadata;
        let tx = self.conn_mut().transaction()?;

        // STEP 1: the book itself.
        tx.execute(
            "INSERT INTO books (id, title, sort_title, authors, sort_author, series, series_index,
                language, publisher, description, format, source_sha256, file_name, file_size,
                cover, converter_version, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                m.id,
                md.title,
                sort_title(&md.title),
                md.authors.join(", "),
                md.authors
                    .first()
                    .map(|a| sort_author(a))
                    .unwrap_or_default(),
                md.series,
                md.series_index,
                md.language,
                md.publisher,
                md.description,
                m.source.format,
                m.source.sha256,
                m.source.file_name,
                file_size as i64,
                m.cover,
                m.source.converter_version,
                db::now_ms(),
            ],
        )?;

        // STEP 2: normalized author rows (for future author browsing).
        for (pos, name) in md.authors.iter().enumerate() {
            tx.execute(
                "INSERT OR IGNORE INTO authors (name) VALUES (?1)",
                params![name],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO book_authors (book_id, author_id, position)
                 SELECT ?1, id, ?3 FROM authors WHERE name = ?2",
                params![m.id, name, pos as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Streaming SHA-256 of a file, hex encoded.
pub fn hash_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// File name safe to store inside the package.
fn safe_file_name(p: &Path) -> String {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("book");
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " ._-()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim_matches('.').is_empty() {
        "book".into()
    } else {
        cleaned
    }
}

/// Lower-case title without a leading English article, for sorting.
pub fn sort_title(title: &str) -> String {
    let l = title.trim().to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = l.strip_prefix(article) {
            return rest.trim_start().to_string();
        }
    }
    l
}

/// "Jane Q. Doe" → "doe jane q." (unless it already looks like "Doe, Jane").
pub fn sort_author(name: &str) -> String {
    let n = name.trim();
    if n.contains(',') {
        return n.to_lowercase();
    }
    match n.rsplit_once(' ') {
        Some((first, last)) => format!("{} {}", last, first).to_lowercase(),
        None => n.to_lowercase(),
    }
}

/// Helper for callers that only need to know whether an error is a DRM refusal.
pub fn is_drm_error(e: &LibraryError) -> bool {
    matches!(
        e,
        LibraryError::Format(justread_formats::FormatError::DrmProtected(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_keys() {
        assert_eq!(sort_title("The Hobbit"), "hobbit");
        assert_eq!(sort_title("A Tale"), "tale");
        assert_eq!(sort_title("Anathem"), "anathem");
        assert_eq!(sort_author("Jane Q. Doe"), "doe jane q.");
        assert_eq!(sort_author("Doe, Jane"), "doe, jane");
        assert_eq!(sort_author("Homer"), "homer");
    }

    #[test]
    fn file_names() {
        assert_eq!(
            safe_file_name(Path::new("/x/My Book (2).epub")),
            "My Book (2).epub"
        );
        assert_eq!(safe_file_name(Path::new("/x/a:b?.txt")), "a_b_.txt");
    }
}
