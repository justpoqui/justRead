// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! A ZIP reader with safety limits.
//!
//! EPUBs and CBZs are ZIP files and must be treated as hostile: they can declare
//! absurd sizes, contain millions of entries, or decompress to gigabytes
//! ("zip bombs"). Every read goes through [`SafeArchive`], which enforces
//! hard limits and never extracts anything to disk by entry name.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use crate::{FormatError, Result};

/// Maximum number of entries in one archive.
pub const MAX_ENTRIES: usize = 20_000;
/// Maximum decompressed size of a single entry.
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
/// Maximum total bytes decompressed from one archive during one conversion.
pub const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// ZIP archive wrapper; see module docs.
pub struct SafeArchive {
    zip: ZipArchive<File>,
    /// Format name used in error messages ("EPUB", "CBZ").
    format: &'static str,
    /// lower-case name → real name, for case-insensitive fallback lookups.
    by_lower: HashMap<String, String>,
    /// real name → entry index.
    by_name: HashMap<String, usize>,
    /// Bytes decompressed so far.
    total_read: u64,
}

impl SafeArchive {
    /// Open `path` and index its entries. `format` labels error messages.
    pub fn open(path: &Path, format: &'static str) -> Result<Self> {
        // STEP 1: parse the central directory.
        let file = File::open(path)?;
        let invalid = |reason: String| FormatError::Invalid { format, reason };
        let mut zip = ZipArchive::new(file).map_err(|e| invalid(e.to_string()))?;
        if zip.len() > MAX_ENTRIES {
            return Err(FormatError::TooLarge(format!("{} entries", zip.len())));
        }

        // STEP 2: build lookup tables (skip directories and undecodable names).
        let mut by_lower = HashMap::new();
        let mut by_name = HashMap::new();
        for i in 0..zip.len() {
            let entry = zip.by_index_raw(i).map_err(|e| invalid(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            if let Ok(name) = entry.name() {
                let name = name.into_owned();
                by_lower
                    .entry(name.to_lowercase())
                    .or_insert_with(|| name.clone());
                by_name.insert(name, i);
            }
        }
        Ok(Self {
            zip,
            format,
            by_lower,
            by_name,
            total_read: 0,
        })
    }

    /// Real entry name for `name` (exact match first, then case-insensitive;
    /// some EPUBs reference files with the wrong case).
    pub fn find(&self, name: &str) -> Option<String> {
        if self.by_name.contains_key(name) {
            return Some(name.to_string());
        }
        self.by_lower.get(&name.to_lowercase()).cloned()
    }

    /// Does an entry called `name` exist?
    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// All file entry names.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.by_name.keys()
    }

    /// Read a whole entry, enforcing the size limits.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        // STEP 1: locate the entry.
        let format = self.format;
        let invalid = |reason: String| FormatError::Invalid { format, reason };
        let real = self
            .find(name)
            .ok_or_else(|| invalid(format!("missing entry {name}")))?;
        let idx = self.by_name[&real];
        let entry = self.zip.by_index(idx).map_err(|e| invalid(e.to_string()))?;

        // STEP 2: read at most limit+1 bytes — the declared size may lie.
        let budget = MAX_ENTRY_BYTES.min(MAX_TOTAL_BYTES.saturating_sub(self.total_read));
        let mut buf = Vec::with_capacity(entry.size().min(budget) as usize);
        entry
            .take(budget + 1)
            .read_to_end(&mut buf)
            .map_err(|e| invalid(e.to_string()))?;
        if buf.len() as u64 > budget {
            return Err(FormatError::TooLarge(format!(
                "entry {real} decompresses past the limit"
            )));
        }

        // STEP 3: account for it in the per-archive budget.
        self.total_read += buf.len() as u64;
        Ok(buf)
    }

    /// Read an entry as text (BOM/declaration-aware decoding).
    pub fn read_text(&mut self, name: &str) -> Result<String> {
        Ok(crate::decode_text(&self.read(name)?))
    }
}
