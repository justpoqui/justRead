// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Data types returned to the UI. All serialize as camelCase JSON.

use justread_locator::Locator;
use serde::{Deserialize, Serialize};

/// A row in the library view.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookSummary {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub language: Option<String>,
    pub format: String,
    /// Package-relative cover path, if the book has one.
    pub cover: Option<String>,
    pub added_at: i64,
    pub last_opened_at: Option<i64>,
    /// 0.0 ..= 1.0; 0 if never opened.
    pub progression: f64,
    pub status: BookStatus,
    pub finished_at: Option<i64>,
    /// Ids of the collections this book is in.
    pub collections: Vec<String>,
}

/// Shelf a book is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BookStatus {
    #[default]
    Unread,
    Reading,
    Finished,
}

impl BookStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Reading => "reading",
            Self::Finished => "finished",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "reading" => Self::Reading,
            "finished" => Self::Finished,
            _ => Self::Unread,
        }
    }
}

/// Columns read by [`summary_from_row`] (keep the two in sync).
pub(crate) const SUMMARY_COLUMNS: &str =
    "b.id, b.title, b.authors, b.series, b.series_index, b.language, \
     b.format, b.cover, b.added_at, b.last_opened_at, COALESCE(p.progression, 0), \
     b.status, b.finished_at, \
     (SELECT group_concat(bc.collection_id, ',') FROM book_collections bc WHERE bc.book_id = b.id)";

pub(crate) fn summary_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<BookSummary> {
    let authors: String = r.get(2)?;
    Ok(BookSummary {
        id: r.get(0)?,
        title: r.get(1)?,
        authors: authors
            .split(", ")
            .filter(|a| !a.is_empty())
            .map(str::to_string)
            .collect(),
        series: r.get(3)?,
        series_index: r.get(4)?,
        language: r.get(5)?,
        format: r.get(6)?,
        cover: r.get(7)?,
        added_at: r.get(8)?,
        last_opened_at: r.get(9)?,
        progression: r.get(10)?,
        status: BookStatus::parse(&r.get::<_, String>(11)?),
        finished_at: r.get(12)?,
        collections: r
            .get::<_, Option<String>>(13)?
            .map(|s| s.split(',').map(str::to_string).collect())
            .unwrap_or_default(),
    })
}

/// Library sort orders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    #[default]
    Title,
    Author,
    Series,
    Recent,
    Progress,
    Added,
}

/// Library list query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub sort: SortKey,
    #[serde(default)]
    pub descending: bool,
    /// Only books on this shelf.
    #[serde(default)]
    pub status: Option<BookStatus>,
    /// Only books in this collection.
    #[serde(default)]
    pub collection: Option<String>,
}

/// Saved reading position.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub locator: Locator,
    pub progression: f64,
    pub updated_at: i64,
}

/// Annotation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnotationKind {
    Highlight,
    Note,
    Bookmark,
}

impl AnnotationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Highlight => "highlight",
            Self::Note => "note",
            Self::Bookmark => "bookmark",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "highlight" => Some(Self::Highlight),
            "note" => Some(Self::Note),
            "bookmark" => Some(Self::Bookmark),
            _ => None,
        }
    }
}

/// A stored highlight, note or bookmark.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: String,
    pub book_id: String,
    pub kind: AnnotationKind,
    pub start: Locator,
    /// End of the highlighted range (exclusive). `None` for bookmarks.
    pub end: Option<Locator>,
    /// The highlighted text (or a snippet near a bookmark).
    pub quote: String,
    pub note: String,
    pub color: String,
    pub chapter: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Input for creating an annotation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAnnotation {
    pub book_id: String,
    pub kind: AnnotationKind,
    pub start: Locator,
    #[serde(default)]
    pub end: Option<Locator>,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub chapter: Option<String>,
}

/// Editable fields of an annotation.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationPatch {
    pub note: Option<String>,
    pub color: Option<String>,
}
