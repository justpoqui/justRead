// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Shared application state.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use justread_formats::Registry;
use justread_library::Library;

/// Managed by Tauri; one per app.
pub struct AppState {
    /// SQLite connections are not `Sync`, so access is serialized. All
    /// operations are short except imports, which run off the UI thread.
    library: Mutex<Library>,
    pub registry: Registry,
    /// `<data dir>/books`, used by the protocol handler without locking.
    pub books_dir: PathBuf,
}

impl AppState {
    pub fn open(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            library: Mutex::new(Library::open(data_dir)?),
            registry: Registry::with_defaults(),
            books_dir: data_dir.join("books"),
        })
    }

    /// Lock the library. A poisoned lock (a panic mid-operation) is
    /// recovered: SQLite transactions keep the data consistent anyway.
    pub fn lib(&self) -> MutexGuard<'_, Library> {
        self.library.lock().unwrap_or_else(|p| p.into_inner())
    }
}
