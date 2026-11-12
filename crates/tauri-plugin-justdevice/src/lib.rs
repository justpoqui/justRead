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

// ----- updates (Android) ---------------------------------------------------------
// The HTTP part runs in Kotlin (Android's own TLS); see DevicePlugin.kt.

/// The latest GitHub release of `repo` ("owner/name"), as the API's JSON text.
pub fn latest_release_json<R: Runtime>(app: &AppHandle<R>, repo: &str) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        #[derive(serde::Deserialize)]
        struct Reply {
            json: String,
        }
        #[derive(Serialize)]
        struct Args<'a> {
            repo: &'a str,
        }
        app.state::<Handle<R>>()
            .0
            .run_mobile_plugin::<Reply>("latestRelease", Args { repo })
            .map(|r| r.json)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, repo);
        Err("In-app updates are only available on Android.".into())
    }
}

/// Download a release APK, verify it against its published SHA-256 and open
/// the system installer. Errors with "permission" when the person first has
/// to allow installs from justRead (the settings page has been opened).
pub fn download_and_install<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    apk_url: &str,
    sha256_url: &str,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            repo: &'a str,
            apk_url: &'a str,
            sha256_url: &'a str,
        }
        app.state::<Handle<R>>()
            .0
            .run_mobile_plugin::<serde::de::IgnoredAny>(
                "downloadAndInstall",
                Args {
                    repo,
                    apk_url,
                    sha256_url,
                },
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, repo, apk_url, sha256_url);
        Err("In-app updates are only available on Android.".into())
    }
}
