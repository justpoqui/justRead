// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Writing a package to disk atomically.
//!
//! Conversion writes into `<final>.partial/`. Only after every file and the
//! manifest are written and validated is the directory renamed to its final
//! name, so a crash or a failed import never leaves a half-written book in
//! the library.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::Manifest;
use crate::paths::normalize_relative;
use crate::reader::validate;
use crate::{Error, Result};

/// Writes one justBook package. Dropping it without calling
/// [`finish`](Self::finish) deletes the partial output.
#[derive(Debug)]
pub struct JustBookWriter {
    final_dir: PathBuf,
    tmp_dir: PathBuf,
    written: HashSet<String>,
    finished: bool,
}

impl JustBookWriter {
    /// Prepare to write a package that will end up at `final_dir`.
    pub fn create(final_dir: &Path) -> Result<Self> {
        // STEP 1: refuse to overwrite an existing package.
        if final_dir.exists() {
            return Err(Error::Invalid(format!(
                "{} already exists",
                final_dir.display()
            )));
        }

        // STEP 2: start from a clean temp dir (a previous crash may have left one).
        let mut tmp = final_dir.as_os_str().to_owned();
        tmp.push(".partial");
        let tmp_dir = PathBuf::from(tmp);
        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir)?;
        }
        fs::create_dir_all(&tmp_dir)?;

        Ok(Self {
            final_dir: final_dir.to_path_buf(),
            tmp_dir,
            written: HashSet::new(),
            finished: false,
        })
    }

    /// Write `bytes` to the package-relative path `rel`. Returns the
    /// normalized path that should be recorded in the manifest.
    pub fn write_file(&mut self, rel: &str, bytes: &[u8]) -> Result<String> {
        // STEP 1: never trust the caller's path.
        let rel = normalize_relative(rel)?;
        if rel == "manifest.json" {
            return Err(Error::UnsafePath(rel));
        }

        // STEP 2: create parent directories and write.
        let target = self.tmp_dir.join(&rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, bytes)?;
        self.written.insert(rel.clone());
        Ok(rel)
    }

    /// Has `rel` already been written?
    pub fn has(&self, rel: &str) -> bool {
        normalize_relative(rel)
            .map(|r| self.written.contains(&r))
            .unwrap_or(false)
    }

    /// Directory currently being written (for adapters that need to peek).
    pub fn working_dir(&self) -> &Path {
        &self.tmp_dir
    }

    /// Validate, write `manifest.json`, and atomically move into place.
    pub fn finish(mut self, manifest: &Manifest) -> Result<PathBuf> {
        // STEP 1: write the manifest (pretty-printed: it is meant to be inspectable).
        let json = serde_json::to_vec_pretty(manifest)?;
        fs::write(self.tmp_dir.join("manifest.json"), json)?;

        // STEP 2: make sure the manifest only references files we wrote.
        let issues = validate(manifest, &self.tmp_dir);
        if !issues.is_empty() {
            return Err(Error::Invalid(issues.join("; ")));
        }

        // STEP 3: rename is atomic on the same filesystem.
        if let Some(parent) = self.final_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&self.tmp_dir, &self.final_dir)?;
        self.finished = true;
        Ok(self.final_dir.clone())
    }
}

impl Drop for JustBookWriter {
    fn drop(&mut self) {
        // Clean up after failed or abandoned conversions.
        if !self.finished {
            let _ = fs::remove_dir_all(&self.tmp_dir);
        }
    }
}
