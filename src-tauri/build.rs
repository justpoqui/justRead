// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Build script.
//!
//! Declaring the app's commands in an `AppManifest` makes Tauri generate one
//! permission per command (`allow-<command>`). Commands are then *denied by
//! default* and each window gets only what its capability file grants
//! (`capabilities/*.json`). This is the foundation for justBrowse (Phase 5),
//! whose webview will be granted nothing at all.
//! Docs: <https://v2.tauri.app/security/capabilities/>

/// Every IPC command the reading UI may call.
const COMMANDS: &[&str] = &[
    "supported_extensions",
    "list_books",
    "get_book",
    "import_books",
    "pick_and_import",
    "delete_book",
    "open_book",
    "read_chapter",
    "save_progress",
    "set_cover",
    "get_setting",
    "set_setting",
    "list_annotations",
    "add_annotation",
    "update_annotation",
    "delete_annotation",
    // Phase 7: highlight export (Markdown; share / save / copy)
    "export_annotations",
    "share_annotations",
    "save_annotations",
    // Phase 8: shelves, collections, reading time and stats
    "set_book_status",
    "list_collections",
    "create_collection",
    "rename_collection",
    "delete_collection",
    "set_in_collection",
    "record_reading",
    "reading_stats",
    "book_stats",
    // Reader screen timeout (keep on / dim)
    "set_screen",
    // In-app updates (Android)
    "check_update",
    "install_update",
    "take_pending_open",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
