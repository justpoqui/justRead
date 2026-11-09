// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Phase 8 commands: shelves, collections, reading time and statistics.
//! Thin wrappers; validation lives in `justread-library` (shelves.rs, stats.rs).

use justread_library::{
    shelves::Collection,
    stats::{BookStats, ReadingStats},
    BookStatus,
};
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn msg<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn set_book_status(
    state: State<'_, AppState>,
    book_id: String,
    status: BookStatus,
) -> CmdResult<()> {
    state.lib().set_status(&book_id, status).map_err(msg)
}

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>) -> CmdResult<Vec<Collection>> {
    state.lib().list_collections().map_err(msg)
}

#[tauri::command]
pub fn create_collection(state: State<'_, AppState>, name: String) -> CmdResult<Collection> {
    state.lib().create_collection(&name).map_err(msg)
}

#[tauri::command]
pub fn rename_collection(state: State<'_, AppState>, id: String, name: String) -> CmdResult<()> {
    state.lib().rename_collection(&id, &name).map_err(msg)
}

#[tauri::command]
pub fn delete_collection(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.lib().delete_collection(&id).map_err(msg)
}

#[tauri::command]
pub fn set_in_collection(
    state: State<'_, AppState>,
    book_id: String,
    collection_id: String,
    member: bool,
) -> CmdResult<()> {
    state
        .lib()
        .set_in_collection(&book_id, &collection_id, member)
        .map_err(msg)
}

/// Active reading time from the reader (`day` is the reader's local date).
#[tauri::command]
pub fn record_reading(
    state: State<'_, AppState>,
    book_id: String,
    day: String,
    seconds: i64,
    progress: f64,
) -> CmdResult<()> {
    state
        .lib()
        .record_reading(&book_id, &day, seconds, progress)
        .map_err(msg)
}

#[tauri::command]
pub fn reading_stats(
    state: State<'_, AppState>,
    today: String,
    days: u32,
) -> CmdResult<ReadingStats> {
    state.lib().reading_stats(&today, days).map_err(msg)
}

#[tauri::command]
pub fn book_stats(state: State<'_, AppState>, book_id: String) -> CmdResult<BookStats> {
    state.lib().book_stats(&book_id).map_err(msg)
}
