// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! # justRead library
//!
//! Everything the app stores lives on the device, under one data directory:
//!
//! ```text
//! <data dir>/
//!   library.sqlite3     books, progress, annotations, settings
//!   books/<uuid>/       one justBook package per book
//! ```
//!
//! No network, no accounts, no telemetry.

pub mod annotations;
pub mod dates;
pub mod db;
pub mod error;
pub mod export;
pub mod import;
pub mod models;
pub mod settings;
pub mod shelves;
pub mod stats;

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

pub use error::{LibraryError, Result};
pub use import::ImportOutcome;
pub use models::*;

/// Handle to the on-device library.
pub struct Library {
    conn: Connection,
    books_dir: PathBuf,
}

impl Library {
    /// Open (creating if needed) the library stored in `data_dir`.
    pub fn open(data_dir: &Path) -> Result<Self> {
        // STEP 1: directories.
        std::fs::create_dir_all(data_dir.join("books"))?;
        // STEP 2: database + migrations.
        let conn = Connection::open(data_dir.join("library.sqlite3"))?;
        db::configure(&conn)?;
        db::migrate(&conn)?;
        Ok(Self {
            conn,
            books_dir: data_dir.join("books"),
        })
    }

    /// In-memory database with packages in `books_dir` (tests).
    pub fn open_in_memory(books_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(books_dir)?;
        let conn = Connection::open_in_memory()?;
        db::configure(&conn)?;
        db::migrate(&conn)?;
        Ok(Self {
            conn,
            books_dir: books_dir.to_path_buf(),
        })
    }

    /// Directory of the package for `book_id`. The id is validated so it
    /// can't be abused for path traversal by the protocol handler.
    pub fn package_dir(&self, book_id: &str) -> Result<PathBuf> {
        if uuid::Uuid::parse_str(book_id).is_err() {
            return Err(LibraryError::NotFound(book_id.to_string()));
        }
        Ok(self.books_dir.join(book_id))
    }

    /// Load the manifest of a book.
    pub fn manifest(&self, book_id: &str) -> Result<justbook::Manifest> {
        Ok(justbook::load_manifest(&self.package_dir(book_id)?)?)
    }

    /// List books with optional search and sorting.
    pub fn list_books(&self, q: &BookQuery) -> Result<Vec<BookSummary>> {
        // STEP 1: the ORDER BY clause comes from a fixed whitelist, never from user text.
        let dir = if q.descending { "DESC" } else { "ASC" };
        let order = match q.sort {
            SortKey::Title => format!("b.sort_title {dir}"),
            SortKey::Author => format!("b.sort_author {dir}, b.sort_title ASC"),
            SortKey::Series => {
                format!("b.series IS NULL, b.series {dir}, b.series_index {dir}, b.sort_title ASC")
            }
            SortKey::Recent => {
                format!("b.last_opened_at IS NULL, b.last_opened_at {dir}, b.added_at DESC")
            }
            SortKey::Progress => format!("COALESCE(p.progression, 0) {dir}, b.sort_title ASC"),
            SortKey::Added => format!("b.added_at {dir}"),
        };

        // STEP 2: search matches title, authors or series (case-insensitive for ASCII).
        let pattern = q
            .search
            .as_deref()
            .map(|s| format!("%{}%", escape_like(s.trim())))
            .unwrap_or_else(|| "%".into());
        // Shelf and collection filters are bound parameters (NULL = no filter).
        let sql = format!(
            "SELECT {cols} FROM books b LEFT JOIN progress p ON p.book_id = b.id
             WHERE (b.title LIKE ?1 ESCAPE '\\' OR b.authors LIKE ?1 ESCAPE '\\' OR COALESCE(b.series, '') LIKE ?1 ESCAPE '\\')
               AND (?2 IS NULL OR b.status = ?2)
               AND (?3 IS NULL OR EXISTS (SELECT 1 FROM book_collections bc
                                          WHERE bc.book_id = b.id AND bc.collection_id = ?3))
             ORDER BY {order}",
            cols = models::SUMMARY_COLUMNS
        );

        // STEP 3: run.
        let mut stmt = self.conn.prepare(&sql)?;
        let status = q.status.map(|s| s.as_str());
        let rows = stmt.query_map(
            params![pattern, status, q.collection],
            models::summary_from_row,
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// One book by id.
    pub fn get_book(&self, book_id: &str) -> Result<BookSummary> {
        let sql = format!(
            "SELECT {} FROM books b LEFT JOIN progress p ON p.book_id = b.id WHERE b.id = ?1",
            models::SUMMARY_COLUMNS
        );
        self.conn
            .query_row(&sql, params![book_id], models::summary_from_row)
            .optional()?
            .ok_or_else(|| LibraryError::NotFound(book_id.to_string()))
    }

    /// Remove a book, its progress/annotations (cascade) and its package.
    pub fn delete_book(&mut self, book_id: &str) -> Result<()> {
        let dir = self.package_dir(book_id)?;
        let n = self
            .conn
            .execute("DELETE FROM books WHERE id = ?1", params![book_id])?;
        if n == 0 {
            return Err(LibraryError::NotFound(book_id.to_string()));
        }
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Store a cover image for a book that has none yet (PDF covers are
    /// rendered by the UI from page 1). Only PNG/JPEG/WebP are accepted.
    pub fn set_cover(&self, book_id: &str, bytes: &[u8]) -> Result<String> {
        // STEP 1: validate type and size — this comes from the webview.
        let ext = if bytes.starts_with(b"\x89PNG") {
            "png"
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            "jpg"
        } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            "webp"
        } else {
            return Err(LibraryError::Invalid(
                "cover must be PNG, JPEG or WebP".into(),
            ));
        };
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(LibraryError::Invalid("cover image is too large".into()));
        }

        // STEP 2: write the file and update manifest + row.
        let dir = self.package_dir(book_id)?;
        let mut manifest = justbook::load_manifest(&dir)?;
        let href = format!("cover.{ext}");
        std::fs::write(dir.join(&href), bytes)?;
        manifest.cover = Some(href.clone());
        justbook::update_manifest(&dir, &manifest)?;
        self.conn.execute(
            "UPDATE books SET cover = ?2 WHERE id = ?1",
            params![book_id, href],
        )?;
        Ok(href)
    }

    /// Record that a book was opened (drives "recently read"); an unread
    /// book moves to the Reading shelf.
    pub fn mark_opened(&self, book_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE books SET last_opened_at = ?2,
                    status = CASE status WHEN 'unread' THEN 'reading' ELSE status END
             WHERE id = ?1",
            params![book_id, db::now_ms()],
        )?;
        Ok(())
    }

    /// Save the reading position.
    pub fn save_progress(
        &self,
        book_id: &str,
        locator: &justread_locator::Locator,
        progression: f64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO progress (book_id, locator, progression, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(book_id) DO UPDATE SET locator = excluded.locator,
               progression = excluded.progression, updated_at = excluded.updated_at",
            params![book_id, locator.to_string(), progression.clamp(0.0, 1.0), db::now_ms()],
        )?;
        Ok(())
    }

    /// Last saved reading position, if any.
    pub fn progress(&self, book_id: &str) -> Result<Option<Progress>> {
        let row = self
            .conn
            .query_row(
                "SELECT locator, progression, updated_at FROM progress WHERE book_id = ?1",
                params![book_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        // A corrupt locator string is treated as "no position" rather than an error.
        Ok(row.and_then(|(l, p, u)| {
            Some(Progress {
                locator: l.parse().ok()?,
                progression: p,
                updated_at: u,
            })
        }))
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub(crate) fn books_dir(&self) -> &Path {
        &self.books_dir
    }
}

/// Escape `%`, `_` and `\` for a LIKE pattern.
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
