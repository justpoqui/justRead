// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Highlights, notes and bookmarks (Phase 2).
//!
//! Annotations store *locators*, never page numbers, so they stay attached
//! to the same text in every layout and font size.

use justread_locator::Locator;
use rusqlite::{params, OptionalExtension};

use crate::{
    db, Annotation, AnnotationKind, AnnotationPatch, Library, LibraryError, NewAnnotation, Result,
};

/// Highlight colors the UI offers. Anything else is rejected.
pub const COLORS: &[&str] = &["yellow", "green", "blue", "pink", "purple", "underline"];

const COLUMNS: &str =
    "id, book_id, kind, start_loc, end_loc, quote, note, color, chapter, created_at, updated_at";

impl Library {
    /// Create an annotation after validating it.
    pub fn add_annotation(&self, a: NewAnnotation) -> Result<Annotation> {
        // STEP 1: the book must exist.
        self.get_book(&a.book_id)?;

        // STEP 2: shape rules per kind.
        match a.kind {
            AnnotationKind::Bookmark => {
                if a.end.is_some() {
                    return Err(LibraryError::Invalid(
                        "bookmarks have no end position".into(),
                    ));
                }
            }
            AnnotationKind::Highlight | AnnotationKind::Note => {
                let end = a.end.ok_or_else(|| {
                    LibraryError::Invalid("highlights need an end position".into())
                })?;
                if a.start.compare(&end) != Some(std::cmp::Ordering::Less) {
                    return Err(LibraryError::Invalid(
                        "highlight end must come after its start".into(),
                    ));
                }
            }
        }
        let color = a.color.unwrap_or_else(|| "yellow".to_string());
        if !COLORS.contains(&color.as_str()) {
            return Err(LibraryError::Invalid(format!("unknown color {color}")));
        }

        // STEP 3: insert with denormalized sort keys.
        let (s1, s2, s3) = sort_keys(&a.start);
        let id = uuid::Uuid::new_v4().to_string();
        let now = db::now_ms();
        self.conn().execute(
            "INSERT INTO annotations (id, book_id, kind, start_loc, end_loc, quote, note, color, chapter,
                sort_spine, sort_block, sort_offset, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
            params![
                id,
                a.book_id,
                a.kind.as_str(),
                a.start.to_string(),
                a.end.map(|e| e.to_string()),
                truncate(&a.quote, 4000),
                a.note,
                color,
                a.chapter,
                s1,
                s2,
                s3,
                now
            ],
        )?;
        self.get_annotation(&id)
    }

    /// One annotation.
    pub fn get_annotation(&self, id: &str) -> Result<Annotation> {
        self.conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM annotations WHERE id = ?1"),
                params![id],
                from_row,
            )
            .optional()?
            .ok_or_else(|| LibraryError::NotFound(id.to_string()))
    }

    /// All annotations of a book, in reading order.
    pub fn list_annotations(&self, book_id: &str) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM annotations WHERE book_id = ?1
             ORDER BY sort_spine, sort_block, sort_offset, created_at"
        ))?;
        let rows = stmt.query_map(params![book_id], from_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Change the note text and/or color.
    pub fn update_annotation(&self, id: &str, patch: AnnotationPatch) -> Result<Annotation> {
        // STEP 1: validate.
        let current = self.get_annotation(id)?;
        if let Some(c) = &patch.color {
            if !COLORS.contains(&c.as_str()) {
                return Err(LibraryError::Invalid(format!("unknown color {c}")));
            }
        }
        // STEP 2: a highlight that gains a note becomes a note (and back).
        let note = patch.note.unwrap_or(current.note);
        let kind = match current.kind {
            AnnotationKind::Bookmark => AnnotationKind::Bookmark,
            _ if note.trim().is_empty() => AnnotationKind::Highlight,
            _ => AnnotationKind::Note,
        };
        // STEP 3: write.
        self.conn().execute(
            "UPDATE annotations SET note = ?2, color = ?3, kind = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, note, patch.color.unwrap_or(current.color), kind.as_str(), db::now_ms()],
        )?;
        self.get_annotation(id)
    }

    /// Delete an annotation.
    pub fn delete_annotation(&self, id: &str) -> Result<()> {
        let n = self
            .conn()
            .execute("DELETE FROM annotations WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(LibraryError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

/// Integer sort keys derived from a locator.
fn sort_keys(l: &Locator) -> (i64, i64, i64) {
    match l {
        Locator::Flow(f) => (f.spine as i64, f.block as i64, f.offset as i64),
        Locator::Fixed(p) => (p.page as i64, (p.offset * 1_000_000.0) as i64, 0),
    }
}

/// Cap a string at `max` characters.
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    // Locators were validated on insert; a parse failure means a corrupt row.
    let parse = |s: String, idx: usize| {
        s.parse::<Locator>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
        })
    };
    let kind: String = r.get(2)?;
    Ok(Annotation {
        id: r.get(0)?,
        book_id: r.get(1)?,
        kind: AnnotationKind::parse(&kind).unwrap_or(AnnotationKind::Highlight),
        start: parse(r.get(3)?, 3)?,
        end: match r.get::<_, Option<String>>(4)? {
            Some(s) => Some(parse(s, 4)?),
            None => None,
        },
        quote: r.get(5)?,
        note: r.get(6)?,
        color: r.get(7)?,
        chapter: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}
