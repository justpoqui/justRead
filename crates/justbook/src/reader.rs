// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Reading and validating packages that already exist on disk.

use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::{Manifest, TocEntry, SCHEMA_VERSION};
use crate::paths::normalize_relative;
use crate::{Error, Result};

/// Load and version-check `manifest.json` from a package directory.
pub fn load_manifest(package_dir: &Path) -> Result<Manifest> {
    // STEP 1: parse the file.
    let bytes = fs::read(package_dir.join("manifest.json"))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;

    // STEP 2: refuse packages from a newer, unknown schema rather than misreading them.
    if manifest.schema_version > SCHEMA_VERSION {
        return Err(Error::UnsupportedSchema {
            found: manifest.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(manifest)
}

/// Map a package-relative path to an absolute path inside `package_dir`,
/// rejecting traversal. Used by the `justbook://` protocol handler.
pub fn resolve_in_package(package_dir: &Path, rel: &str) -> Result<PathBuf> {
    // STEP 1: lexical check (no "..", no absolute paths).
    let rel = normalize_relative(rel)?;
    let candidate = package_dir.join(&rel);

    // STEP 2: physical check, in case a symlink points outside the package.
    let root = package_dir.canonicalize()?;
    let real = candidate.canonicalize()?;
    if !real.starts_with(&root) {
        return Err(Error::UnsafePath(rel));
    }
    Ok(real)
}

/// Check that a manifest is self-consistent and that the files it lists
/// exist under `dir`. Returns human-readable problems (empty = valid).
pub fn validate(manifest: &Manifest, dir: &Path) -> Vec<String> {
    let mut issues = Vec::new();

    // STEP 1: basic metadata.
    if manifest.metadata.title.trim().is_empty() {
        issues.push("title is empty".to_string());
    }
    if manifest.spine.is_empty() {
        issues.push("spine is empty".to_string());
    }

    // STEP 2: every referenced file must exist.
    let mut check = |what: &str, href: &str| match normalize_relative(href) {
        Ok(p) if dir.join(&p).is_file() => {}
        Ok(p) => issues.push(format!("{what} missing: {p}")),
        Err(_) => issues.push(format!("{what} has unsafe path: {href}")),
    };
    for item in &manifest.spine {
        check("chapter", &item.href);
    }
    for res in &manifest.resources {
        check("resource", &res.href);
    }
    if let Some(cover) = &manifest.cover {
        check("cover", cover);
    }

    // STEP 3: TOC targets must be spine items.
    fn walk(entries: &[TocEntry], m: &Manifest, issues: &mut Vec<String>) {
        for e in entries {
            if let Some(h) = &e.href {
                if m.spine_index_of(h).is_none() {
                    issues.push(format!("toc entry '{}' targets non-spine {h}", e.title));
                }
            }
            walk(&e.children, m, issues);
        }
    }
    walk(&manifest.toc, manifest, &mut issues);
    issues
}
