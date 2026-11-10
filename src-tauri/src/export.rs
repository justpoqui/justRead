// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Phase 7: exporting a book's highlights, notes and bookmarks as Markdown
//! so they can go to any app.
//!
//! * `export_annotations` returns the text (Copy, and the web Share API on
//!   iOS / Windows / macOS);
//! * `share_annotations` opens the Android share sheet (our own plugin);
//! * `save_annotations` shows a Save dialog *from Rust* and writes the file:
//!   the destination is chosen by the user in the OS dialog, never passed in
//!   by the webview, so a compromised page can't write anywhere it likes.

use std::io::Write;

use justread_library::{db::now_ms, export};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedAnnotations {
    pub title: String,
    pub file_name: String,
    pub markdown: String,
    /// True where `share_annotations` opens a native share sheet (Android).
    pub native_share: bool,
}

/// STEP 1: build the document from the library (shared by all three commands).
fn build(state: &AppState, book_id: &str) -> CmdResult<ExportedAnnotations> {
    let lib = state.lib();
    let book = lib.get_book(book_id).map_err(|e| e.to_string())?;
    let list = lib.list_annotations(book_id).map_err(|e| e.to_string())?;
    Ok(ExportedAnnotations {
        markdown: export::annotations_markdown(&book, &list, &export::iso_date(now_ms())),
        file_name: export::export_file_name(&book.title),
        title: format!("{} — highlights", book.title),
        native_share: tauri_plugin_justdevice::supported(),
    })
}

#[tauri::command]
pub fn export_annotations(
    state: State<'_, AppState>,
    book_id: String,
) -> CmdResult<ExportedAnnotations> {
    build(&state, &book_id)
}

/// STEP 2: Android share sheet ("send to any app").
#[tauri::command]
pub async fn share_annotations(app: AppHandle, book_id: String) -> CmdResult<()> {
    let doc = build(&app.state::<AppState>(), &book_id)?;
    tauri_plugin_justdevice::share(
        &app,
        tauri_plugin_justdevice::ShareRequest {
            title: doc.title,
            text: doc.markdown,
            file_name: doc.file_name,
            // text/plain reaches the most apps; the .md name tells them it's Markdown.
            mime_type: "text/plain".into(),
        },
    )
}

/// STEP 3: Save as file. Returns the saved file's name, or None if cancelled.
/// Async so the blocking dialog runs off the main thread (as the plugin requires).
#[tauri::command]
pub async fn save_annotations(app: AppHandle, book_id: String) -> CmdResult<Option<String>> {
    let doc = build(&app.state::<AppState>(), &book_id)?;
    let Some(target) = app
        .dialog()
        .file()
        .set_file_name(&doc.file_name)
        .add_filter("Markdown", &["md"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    match target {
        // Desktop: a normal path.
        FilePath::Path(path) => {
            std::fs::write(&path, doc.markdown.as_bytes())
                .map_err(|e| format!("Could not save: {e}"))?;
            Ok(Some(
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(doc.file_name),
            ))
        }
        // Android/iOS: a content:// or security-scoped URL, written through the fs plugin.
        FilePath::Url(url) => {
            use tauri_plugin_fs::{FsExt, OpenOptions};
            let mut opts = OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            let mut file = app
                .fs()
                .open(FilePath::Url(url), opts)
                .map_err(|e| format!("Could not save: {e}"))?;
            file.write_all(doc.markdown.as_bytes())
                .map_err(|e| format!("Could not save: {e}"))?;
            Ok(Some(doc.file_name))
        }
    }
}
