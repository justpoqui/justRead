// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Library errors. Messages are written for end users: they are shown in
//! the UI as-is.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Format(#[from] justread_formats::FormatError),
    #[error("book package error: {0}")]
    Package(#[from] justbook::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;
