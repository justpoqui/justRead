// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! IPC commands for the reading UI.
//!
//! Conventions:
//! * every command returns `Result<T, String>`; the string is a user-facing
//!   message shown as-is by the UI;
//! * anything slow (imports) runs on a blocking thread so the webview stays
//!   responsive;
//! * inputs are validated here or in the library crate — the webview is
//!   treated as untrusted.

use justbook::Manifest;
use justread_library::{
    import::is_drm_error, Annotation, AnnotationPatch, BookQuery, BookSummary, ImportOutcome,
    NewAnnotation, Progress,
};
use justread_locator::Locator;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn msg<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// File extensions accepted by the import dialog.
#[tauri::command]
pub fn supported_extensions(state: State<'_, AppState>) -> Vec<&'static str> {
    state.registry.extensions()
}

#[tauri::command]
pub fn list_books(state: State<'_, AppState>, query: BookQuery) -> CmdResult<Vec<BookSummary>> {
    state.lib().list_books(&query).map_err(msg)
}

#[tauri::command]
pub fn get_book(state: State<'_, AppState>, id: String) -> CmdResult<BookSummary> {
    state.lib().get_book(&id).map_err(msg)
}

#[tauri::command]
pub fn delete_book(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.lib().delete_book(&id).map_err(msg)
}

/// Result of importing one file.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub path: String,
    pub outcome: Option<ImportOutcome>,
    pub error: Option<String>,
    /// True when the file was refused because it is DRM-protected.
    pub drm: bool,
}

/// Import files dropped on the window or handed over by "Open with".
/// Only paths the person chose are accepted (see `open_files::GrantedPaths`).
#[tauri::command]
pub async fn import_books(app: AppHandle, paths: Vec<String>) -> CmdResult<Vec<ImportResult>> {
    import_paths(app, paths).await
}

/// The Import button: the file picker is opened here, by Rust, so the
/// webview never chooses which files are read. Empty if cancelled.
#[tauri::command]
pub async fn pick_and_import(app: AppHandle) -> CmdResult<Vec<ImportResult>> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let extensions = app.state::<AppState>().registry.extensions();
    let picked = app
        .dialog()
        .file()
        .add_filter("Books", &extensions)
        .blocking_pick_files()
        .unwrap_or_default();
    let paths: Vec<String> = picked
        .into_iter()
        .map(|p| match p {
            FilePath::Path(p) => p.to_string_lossy().into_owned(),
            FilePath::Url(u) => u.to_string(),
        })
        .collect();
    crate::open_files::grant(&app, &paths);
    import_paths(app, paths).await
}

async fn import_paths(app: AppHandle, paths: Vec<String>) -> CmdResult<Vec<ImportResult>> {
    // STEP 1: conversion is CPU/disk heavy — keep it off the async runtime.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        paths
            .into_iter()
            .map(|path| {
                // STEP 1b: only files the person chose.
                if !crate::open_files::take_grant(&app, &path) {
                    return ImportResult {
                        path,
                        outcome: None,
                        drm: false,
                        error: Some(
                            "Choose the file with Import, or drop it on the window.".into(),
                        ),
                    };
                }
                // STEP 2: one file at a time; a failure never aborts the batch.
                // Paths or URIs (Android content://, iOS/macOS file://) are accepted.
                let (file, temporary) = match crate::open_files::materialize(&app, &path) {
                    Ok(f) => f,
                    Err(e) => {
                        return ImportResult {
                            path,
                            outcome: None,
                            drm: false,
                            error: Some(e),
                        }
                    }
                };
                let result = state.lib().import(&file, &state.registry);
                if temporary {
                    let _ = std::fs::remove_file(&file);
                }
                match result {
                    Ok(outcome) => ImportResult {
                        path,
                        outcome: Some(outcome),
                        error: None,
                        drm: false,
                    },
                    Err(e) => ImportResult {
                        path,
                        outcome: None,
                        drm: is_drm_error(&e),
                        error: Some(e.to_string()),
                    },
                }
            })
            .collect()
    })
    .await
    .map_err(msg)
}

/// Everything the reader needs to open a book.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedBook {
    pub book: BookSummary,
    pub manifest: Manifest,
    pub progress: Option<Progress>,
}

#[tauri::command]
pub fn open_book(state: State<'_, AppState>, id: String) -> CmdResult<OpenedBook> {
    let lib = state.lib();
    let manifest = lib.manifest(&id).map_err(msg)?;
    lib.mark_opened(&id).map_err(msg)?;
    Ok(OpenedBook {
        book: lib.get_book(&id).map_err(msg)?,
        manifest,
        progress: lib.progress(&id).map_err(msg)?,
    })
}

/// Return the XHTML of one chapter. Only spine documents can be read.
#[tauri::command]
pub fn read_chapter(
    state: State<'_, AppState>,
    book_id: String,
    href: String,
) -> CmdResult<String> {
    // STEP 1: the href must be a spine item of this book (no arbitrary file reads).
    let (manifest, dir) = {
        let lib = state.lib();
        (
            lib.manifest(&book_id).map_err(msg)?,
            lib.package_dir(&book_id).map_err(msg)?,
        )
    };
    let index = manifest
        .spine_index_of(&href)
        .ok_or_else(|| format!("not a chapter of this book: {href}"))?;
    // STEP 2: read it from inside the package.
    let path = justbook::resolve_in_package(&dir, &manifest.spine[index].href).map_err(msg)?;
    std::fs::read_to_string(path).map_err(msg)
}

/// Store a cover rendered by the UI (PDFs carry no cover image).
#[tauri::command]
pub fn set_cover(state: State<'_, AppState>, book_id: String, bytes: Vec<u8>) -> CmdResult<String> {
    state.lib().set_cover(&book_id, &bytes).map_err(msg)
}

#[tauri::command]
pub fn save_progress(
    state: State<'_, AppState>,
    book_id: String,
    locator: String,
    progression: f64,
) -> CmdResult<()> {
    let loc: Locator = locator.parse().map_err(msg)?;
    state
        .lib()
        .save_progress(&book_id, &loc, progression)
        .map_err(msg)
}

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> CmdResult<Option<Value>> {
    state.lib().get_setting(&key).map_err(msg)
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: Value) -> CmdResult<()> {
    state.lib().set_setting(&key, &value).map_err(msg)
}

#[tauri::command]
pub fn list_annotations(state: State<'_, AppState>, book_id: String) -> CmdResult<Vec<Annotation>> {
    state.lib().list_annotations(&book_id).map_err(msg)
}

#[tauri::command]
pub fn add_annotation(
    state: State<'_, AppState>,
    annotation: NewAnnotation,
) -> CmdResult<Annotation> {
    state.lib().add_annotation(annotation).map_err(msg)
}

#[tauri::command]
pub fn update_annotation(
    state: State<'_, AppState>,
    id: String,
    patch: AnnotationPatch,
) -> CmdResult<Annotation> {
    state.lib().update_annotation(&id, patch).map_err(msg)
}

#[tauri::command]
pub fn delete_annotation(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.lib().delete_annotation(&id).map_err(msg)
}
