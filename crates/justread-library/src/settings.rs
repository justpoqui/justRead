// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Key/value settings stored as JSON. The UI owns the shape of each value;
//! the backend only guarantees that what it stores is valid JSON.

use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use crate::{Library, LibraryError, Result};

impl Library {
    /// Read one setting.
    pub fn get_setting(&self, key: &str) -> Result<Option<Value>> {
        let raw: Option<String> = self
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
    }

    /// Write one setting.
    pub fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        // Keys are short identifiers like "reader.typography".
        if key.is_empty()
            || key.len() > 64
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        {
            return Err(LibraryError::Invalid(format!("bad settings key {key:?}")));
        }
        // Settings are small JSON objects; refuse anything huge (the webview is untrusted).
        let text = value.to_string();
        if text.len() > 256 * 1024 {
            return Err(LibraryError::Invalid("setting value is too large".into()));
        }
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, text],
        )?;
        Ok(())
    }
}
