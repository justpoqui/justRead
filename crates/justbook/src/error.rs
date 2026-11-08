// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Error type shared by everything in the `justbook` crate.

use thiserror::Error;

/// Everything that can go wrong while writing or reading a justBook package.
#[derive(Debug, Error)]
pub enum Error {
    /// Underlying filesystem error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The manifest could not be (de)serialized.
    #[error("manifest JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A path tried to escape the package directory or was otherwise unsafe.
    #[error("unsafe path inside package: {0}")]
    UnsafePath(String),

    /// The package was written by a newer justRead than this one.
    #[error("unsupported justBook schema version {found} (this build supports up to {supported})")]
    UnsupportedSchema { found: u32, supported: u32 },

    /// The package failed validation.
    #[error("invalid package: {0}")]
    Invalid(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
