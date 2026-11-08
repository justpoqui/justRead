// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! The Tauri shell. Deliberately thin: all real logic lives in the
//! `justbook`, `justread-formats` and `justread-library` crates; this crate
//! only wires them to the webview (commands + the `justbook://` protocol).

mod commands;
mod protocol;
mod state;

use tauri::Manager;

/// Application entry point (desktop `main` and, later, mobile).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // STEP 1: only the dialog plugin — used for the "Import" file picker.
        .plugin(tauri_plugin_dialog::init())
        // STEP 2: serve book packages to the reader iframe.
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        // STEP 3: open the library in the per-user app data directory.
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = state::AppState::open(&data_dir)?;
            app.manage(state);
            Ok(())
        })
        // STEP 4: the command surface (each one is also allow-listed in capabilities/main.json).
        .invoke_handler(tauri::generate_handler![
            commands::supported_extensions,
            commands::list_books,
            commands::get_book,
            commands::import_books,
            commands::delete_book,
            commands::open_book,
            commands::read_chapter,
            commands::save_progress,
            commands::set_cover,
            commands::get_setting,
            commands::set_setting,
            commands::list_annotations,
            commands::add_annotation,
            commands::update_annotation,
            commands::delete_annotation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running justRead");
}
