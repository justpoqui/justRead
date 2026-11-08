// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! "Open with justRead": files handed to the app by the operating system.
//!
//! * Windows / Linux: the file paths arrive as command-line arguments.
//! * macOS / iOS / Android: they arrive as URLs in `RunEvent::Opened`
//!   (`file://…`, or `content://…` on Android).
//!
//! They are queued here; the UI takes the queue when it starts and listens
//! for `open-files` while running, then imports and opens the book.
//! File associations themselves are declared in tauri.conf.json.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime, State, Url};

/// Files waiting for the UI.
#[derive(Default)]
pub struct PendingOpen(pub Mutex<Vec<String>>);

/// Command-line arguments that name existing files (Windows/Linux "Open with").
pub fn from_args(args: impl Iterator<Item = String>) -> Vec<String> {
    args.skip(1)
        .filter(|a| !a.starts_with('-') && Path::new(a).is_file())
        .collect()
}

/// Queue files and tell a running UI about them.
pub fn push<R: Runtime>(app: &AppHandle<R>, files: Vec<String>) {
    if files.is_empty() {
        return;
    }
    app.state::<PendingOpen>().0.lock().unwrap().extend(files);
    let _ = app.emit_to("main", "open-files", ());
}

/// UI: take (and clear) the queue.
#[tauri::command]
pub fn take_pending_open(state: State<'_, PendingOpen>) -> Vec<String> {
    std::mem::take(&mut *state.0.lock().unwrap())
}

/// Turn a path or URI from the OS/file picker into a local file we can import.
/// Returns the path and whether it is a temporary copy to delete afterwards.
pub fn materialize<R: Runtime>(app: &AppHandle<R>, input: &str) -> Result<(PathBuf, bool), String> {
    // STEP 1: plain paths (desktop pickers, command line).
    let lower = input.to_ascii_lowercase();
    if !lower.starts_with("file://") && !lower.starts_with("content://") {
        return Ok((PathBuf::from(input), false));
    }
    let url = Url::parse(input).map_err(|e| e.to_string())?;

    // STEP 2: file:// URLs are local paths (macOS/iOS hand us these).
    #[cfg(not(target_os = "android"))]
    if url.scheme() == "file" {
        if let Ok(p) = url.to_file_path() {
            if p.is_file() {
                return Ok((p, false));
            }
        }
    }

    // STEP 3: everything else (Android content://, iOS security-scoped URLs) is
    // copied through the fs plugin into a private folder, keeping the file name
    // because some formats are recognized by their extension.
    use tauri_plugin_fs::{FsExt, OpenOptions};
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("incoming");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(file_name_from_uri(&url));
    let mut opts = OpenOptions::new();
    opts.read(true);
    let mut src = app
        .fs()
        .open(url, opts)
        .map_err(|e| format!("could not read the file: {e}"))?;
    let mut dst = std::fs::File::create(&target).map_err(|e| e.to_string())?;
    std::io::copy(&mut src, &mut dst).map_err(|e| e.to_string())?;
    Ok((target, true))
}

/// A safe file name from a URI's last segment. Android document URIs often
/// encode the path in it (`…/document/primary%3ADownload%2Fbook.epub`).
pub fn file_name_from_uri(url: &Url) -> String {
    let last = url
        .path_segments()
        .and_then(|mut s| s.next_back().map(str::to_string))
        .unwrap_or_default();
    let decoded = justbook::paths::percent_decode(&last);
    let name = decoded.rsplit(['/', ':', '\\']).next().unwrap_or("");
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " ._-()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.trim_matches(['.', ' ']).to_string();
    if clean.is_empty() {
        "book".into()
    } else {
        clean.chars().take(120).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_names() {
        let n = |s: &str| file_name_from_uri(&Url::parse(s).unwrap());
        assert_eq!(n("content://com.android.providers.downloads.documents/document/primary%3ADownload%2FMoby%20Dick.epub"), "Moby Dick.epub");
        assert_eq!(n("content://media/external/file/1234"), "1234");
        assert_eq!(n("file:///home/a/My%20Book.fb2"), "My Book.fb2");
        assert_eq!(n("content://x/..%2F..%2Fetc%2Fpasswd"), "passwd");
        assert_eq!(n("content://x/"), "book");
    }

    #[test]
    fn args_only_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("b.epub");
        std::fs::write(&f, b"x").unwrap();
        let args = vec![
            "justread".into(),
            "--flag".into(),
            f.to_string_lossy().into_owned(),
            "/nope/missing.epub".into(),
        ];
        assert_eq!(
            from_args(args.into_iter()),
            vec![f.to_string_lossy().into_owned()]
        );
    }
}
