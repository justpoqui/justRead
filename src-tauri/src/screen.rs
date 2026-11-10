// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! The reader's screen timeout (keep on / dim). Android does it natively
//! through our plugin; elsewhere this is a no-op and the UI uses the web
//! Wake Lock API plus a dark overlay.

use tauri::AppHandle;

#[tauri::command]
pub fn set_screen(app: AppHandle, keep_on: bool, dim: bool) -> Result<(), String> {
    tauri_plugin_justdevice::screen(&app, keep_on, dim)
}
