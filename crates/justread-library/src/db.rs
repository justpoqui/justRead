// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Schema and migrations.
//!
//! Migrations are plain SQL, applied in order inside a transaction, and the
//! applied version is stored in SQLite's built-in `PRAGMA user_version`
//! (<https://www.sqlite.org/pragma.html#pragma_user_version>). Never edit
//! a released migration — append a new one.

use rusqlite::Connection;

use crate::Result;

/// Ordered migrations. Index + 1 == schema version after applying it.
const MIGRATIONS: &[&str] = &[
    // v1 — Phase 1: books, authors, progress, settings.
    r#"
    CREATE TABLE books (
        id                TEXT PRIMARY KEY,
        title             TEXT NOT NULL,
        sort_title        TEXT NOT NULL,
        authors           TEXT NOT NULL DEFAULT '',   -- display form, "A, B"
        sort_author       TEXT NOT NULL DEFAULT '',
        series            TEXT,
        series_index      REAL,
        language          TEXT,
        publisher         TEXT,
        description       TEXT,
        format            TEXT NOT NULL,
        source_sha256     TEXT NOT NULL UNIQUE,      -- duplicate detection
        file_name         TEXT NOT NULL,
        file_size         INTEGER NOT NULL,
        cover             TEXT,                       -- package-relative path
        converter_version TEXT NOT NULL,
        added_at          INTEGER NOT NULL,           -- unix ms
        last_opened_at    INTEGER
    );
    CREATE TABLE authors (
        id   INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE
    );
    CREATE TABLE book_authors (
        book_id   TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
        author_id INTEGER NOT NULL REFERENCES authors(id),
        position  INTEGER NOT NULL,
        PRIMARY KEY (book_id, author_id)
    );
    CREATE TABLE progress (
        book_id     TEXT PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
        locator     TEXT NOT NULL,
        progression REAL NOT NULL,
        updated_at  INTEGER NOT NULL
    );
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL                           -- JSON
    );
    CREATE INDEX books_sort_title ON books(sort_title);
    CREATE INDEX books_last_opened ON books(last_opened_at);
    "#,
    // v2 — Phase 2: highlights, notes and bookmarks.
    r#"
    CREATE TABLE annotations (
        id          TEXT PRIMARY KEY,
        book_id     TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
        kind        TEXT NOT NULL CHECK (kind IN ('highlight', 'note', 'bookmark')),
        start_loc   TEXT NOT NULL,
        end_loc     TEXT,
        quote       TEXT NOT NULL DEFAULT '',
        note        TEXT NOT NULL DEFAULT '',
        color       TEXT NOT NULL DEFAULT 'yellow',
        chapter     TEXT,                              -- chapter title at creation, for display/export
        sort_spine  INTEGER NOT NULL,                  -- denormalized from start_loc for ORDER BY
        sort_block  INTEGER NOT NULL,
        sort_offset INTEGER NOT NULL,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL
    );
    CREATE INDEX annotations_book ON annotations(book_id, sort_spine, sort_block, sort_offset);
    "#,
];

/// Connection-level settings applied on every open.
pub fn configure(conn: &Connection) -> Result<()> {
    // foreign_keys: needed for ON DELETE CASCADE (off by default in SQLite).
    // WAL: readers don't block the writer; safe for a single-process app.
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;",
    )?;
    Ok(())
}

/// Bring the schema up to the latest version.
pub fn migrate(conn: &Connection) -> Result<()> {
    // STEP 1: where are we?
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    // STEP 2: apply each pending migration atomically with its version bump.
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = i + 1;
        conn.execute_batch(&format!(
            "BEGIN; {sql} PRAGMA user_version = {version}; COMMIT;"
        ))?;
    }
    Ok(())
}

/// Latest schema version known to this build.
pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}

/// Current time in unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
