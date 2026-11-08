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
    // justBrowse (granted to the toolbar webview only, except open_browser)
    "take_pending_open",
    "open_browser",
    "browse_navigate",
    "browse_history",
    "browse_reload",
    "browse_allow_http",
    "browse_config",
    "browse_set_search",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
