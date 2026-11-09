// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Phase 8: shelves (Unread / Reading / Finished) and user collections.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::{db, BookStatus, Library, LibraryError, Result};

/// A user collection with its number of books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub count: i64,
}

/// Collection names: trimmed, 1–60 characters, no control characters.
fn clean_name(name: &str) -> Result<String> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() || n.chars().count() > 60 || n.chars().any(char::is_control) {
        return Err(LibraryError::Invalid(
            "A collection name needs 1 to 60 characters.".into(),
        ));
    }
    Ok(n)
}

impl Library {
    // ----- shelves ---------------------------------------------------------

    /// Put a book on a shelf. Finished records when; other shelves clear it.
    pub fn set_status(&self, book_id: &str, status: BookStatus) -> Result<()> {
        let finished_at = (status == BookStatus::Finished).then(db::now_ms);
        let n = self.conn().execute(
            "UPDATE books SET status = ?2,
                    finished_at = CASE WHEN ?2 = 'finished' THEN COALESCE(finished_at, ?3) ELSE NULL END
             WHERE id = ?1",
            params![book_id, status.as_str(), finished_at],
        )?;
        if n == 0 {
            return Err(LibraryError::NotFound(book_id.to_string()));
        }
        Ok(())
    }

    // ----- collections -----------------------------------------------------

    /// All collections, alphabetically, with book counts.
    pub fn list_collections(&self) -> Result<Vec<Collection>> {
        let mut stmt = self.conn().prepare(
            "SELECT c.id, c.name, (SELECT COUNT(*) FROM book_collections bc WHERE bc.collection_id = c.id)
             FROM collections c ORDER BY c.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Collection {
                id: r.get(0)?,
                name: r.get(1)?,
                count: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Create a collection (names are unique, ignoring case).
    pub fn create_collection(&self, name: &str) -> Result<Collection> {
        let name = clean_name(name)?;
        let exists: Option<String> = self
            .conn()
            .query_row(
                "SELECT name FROM collections WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = exists {
            return Err(LibraryError::Invalid(format!(
                "There is already a collection called “{existing}”."
            )));
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.conn().execute(
            "INSERT INTO collections (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, db::now_ms()],
        )?;
        Ok(Collection { id, name, count: 0 })
    }

    pub fn rename_collection(&self, id: &str, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        let clash: Option<String> = self
            .conn()
            .query_row(
                "SELECT id FROM collections WHERE name = ?1 AND id <> ?2",
                params![name, id],
                |r| r.get(0),
            )
            .optional()?;
        if clash.is_some() {
            return Err(LibraryError::Invalid(format!(
                "There is already a collection called “{name}”."
            )));
        }
        let n = self.conn().execute(
            "UPDATE collections SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        if n == 0 {
            return Err(LibraryError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Delete a collection. Its books stay in the library.
    pub fn delete_collection(&self, id: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM collections WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Add a book to a collection or take it out.
    pub fn set_in_collection(
        &self,
        book_id: &str,
        collection_id: &str,
        member: bool,
    ) -> Result<()> {
        if member {
            // Foreign keys reject unknown books/collections.
            self.conn()
                .execute(
                    "INSERT OR IGNORE INTO book_collections (book_id, collection_id) VALUES (?1, ?2)",
                    params![book_id, collection_id],
                )
                .map_err(|_| LibraryError::NotFound(format!("{book_id} / {collection_id}")))?;
        } else {
            self.conn().execute(
                "DELETE FROM book_collections WHERE book_id = ?1 AND collection_id = ?2",
                params![book_id, collection_id],
            )?;
        }
        Ok(())
    }
}
