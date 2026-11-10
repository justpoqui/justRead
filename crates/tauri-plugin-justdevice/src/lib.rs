// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! justRead's own Android integrations (Kotlin in `android/`):
//!
//! * **share** — the system share sheet ("send to any app", Phase 7).
//!   A small Kotlin class (`android/`) writes the text to a file in
//!   the app's cache, exposes it through the plugin's own FileProvider, and
//!   starts `ACTION_SEND` with both the file and the text, so apps that take
//!   files and apps that take text both work.
//!   Other platforms return an error; the UI uses the web Share API
//!   there (iOS, Windows, macOS) or offers Save / Copy (Linux).
//! * **screen** — the reader's own screen timeout: keep the screen on while
//!   reading and dim the real backlight. Elsewhere a no-op (the UI uses the
//!   web Wake Lock API and a dark overlay).

use serde::Serialize;
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};

/// What to share.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRequest {
    /// Title of the chooser and the message subject (e.g. for email).
    pub title: String,
    /// The text itself (also the file's content).
    pub text: String,
    /// File name of the attached copy, e.g. `Book — highlights.md`.
    pub file_name: String,
    /// MIME type of the attached file.
    pub mime_type: String,
}

#[cfg(target_os = "android")]
struct Handle<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// STEP 1: install the plugin (registers the Kotlin class on Android).
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("justdevice")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle = _api.register_android_plugin("app.justread.device", "DevicePlugin")?;
                _app.manage(Handle(handle));
            }
            Ok(())
        })
        .build()
}

/// Is a native share sheet available through this plugin?
pub fn supported() -> bool {
    cfg!(target_os = "android")
}

/// STEP 2: open the share sheet. Returns once the sheet is shown.
pub fn share<R: Runtime>(app: &AppHandle<R>, req: ShareRequest) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<Handle<R>>();
        handle
            .0
            .run_mobile_plugin::<serde::de::IgnoredAny>("share", req)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, req);
        Err("This device has no share sheet here; use Save or Copy.".into())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenRequest {
    keep_on: bool,
    dim: bool,
}

/// Keep the screen on and/or dim it (Android). Elsewhere a no-op returning Ok.
pub fn screen<R: Runtime>(app: &AppHandle<R>, keep_on: bool, dim: bool) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        app.state::<Handle<R>>()
            .0
            .run_mobile_plugin::<serde::de::IgnoredAny>("screen", ScreenRequest { keep_on, dim })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, ScreenRequest { keep_on, dim });
        Ok(())
    }
}
