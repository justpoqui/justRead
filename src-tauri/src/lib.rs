// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! The Tauri shell. Deliberately thin: all real logic lives in the
//! `justbook`, `justread-formats` and `justread-library` crates; this crate
//! only wires them to the webview (commands + the `justbook://` protocol).

mod browse;
mod commands;
mod export;
mod open_files;
mod protocol;
mod screen;
mod state;
mod stats;
mod updater;

use tauri::Manager;

/// Application entry point (desktop `main` and, later, mobile).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // STEP 1: the dialog plugin (Import picker, Save) and the fs plugin, used
        // only from Rust. No webview is granted any dialog or fs permission.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Our Android integrations: share sheet, screen keep-on/dim (no JS commands of its own).
        .plugin(tauri_plugin_justdevice::init())
        .manage(open_files::PendingOpen::default())
        .manage(open_files::GrantedPaths::default())
        // Files dropped on the window were chosen by the person: they may be imported.
        .on_webview_event(|webview, event| {
            if let tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                let files: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                open_files::grant(webview.app_handle(), &files);
            }
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                let files: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                open_files::grant(window.app_handle(), &files);
            }
        })
        .manage(browse::BrowseState::default())
        .manage(updater::PendingUpdate::default())
        // STEP 2: serve book packages to the reader iframe.
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        // STEP 3: open the library in the per-user app data directory.
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = state::AppState::open(&data_dir)?;
            app.manage(state);
            // Windows/Linux "Open with": files are passed as arguments.
            open_files::push(app.handle(), open_files::from_args(std::env::args()));
            Ok(())
        })
        // STEP 4: the command surface (each one is also allow-listed in capabilities/main.json).
        .invoke_handler(tauri::generate_handler![
            commands::supported_extensions,
            commands::list_books,
            commands::get_book,
            commands::import_books,
            commands::pick_and_import,
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
            export::export_annotations,
            export::share_annotations,
            export::save_annotations,
            stats::set_book_status,
            stats::list_collections,
            stats::create_collection,
            stats::rename_collection,
            stats::delete_collection,
            stats::set_in_collection,
            stats::record_reading,
            stats::reading_stats,
            stats::book_stats,
            screen::set_screen,
            updater::check_update,
            updater::install_update,
            browse::open_browser,
            browse::browse_navigate,
            browse::browse_history,
            browse::browse_reload,
            browse::browse_allow_http,
            browse::browse_config,
            browse::browse_set_search,
            open_files::take_pending_open,
        ])
        .build(tauri::generate_context!())
        .expect("error while building justRead")
        .run(|_app, _event| {
            // macOS/iOS/Android "Open with": files arrive as URLs while running.
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
            if let tauri::RunEvent::Opened { urls } = _event {
                open_files::push(_app, urls.iter().map(|u| u.to_string()).collect());
            }
        });
}
