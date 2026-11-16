// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! In-app updates (Android).
//!
//! Builds are published as GitHub Releases of the public justRead repository
//! (see docs/RELEASES.md), each with `justRead.apk` and `justRead.apk.sha256`.
//! `check_update` asks GitHub for the latest release and compares versions;
//! `install_update` downloads the APK found by the last check, verifies its
//! checksum and opens Android's installer, which also refuses any APK not
//! signed with the same key as the installed app. The webview never supplies
//! a URL: the repository is fixed here and the asset URLs come from GitHub.
//! The only network use is these requests; nothing about the reader is sent.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

/// Where releases are published.
pub const REPO: &str = "justpoqui/justRead";
const APK_NAME: &str = "justRead.apk";
const SHA_NAME: &str = "justRead.apk.sha256";

type CmdResult<T> = Result<T, String>;

/// The update found by the last check, kept here so the webview can't swap it.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<(String, String)>>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// This build's version.
    pub current: String,
    /// In-app updates work on this platform.
    pub supported: bool,
    /// A newer release, if any.
    pub available: Option<Release>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub published_at: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// "v0.2.15" / "android-v0.2.15" / "0.2.15" → [0, 2, 15]. None if not a version.
pub fn parse_version(s: &str) -> Option<Vec<u64>> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let core = s[start..].split(['-', '+']).next()?;
    let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty() && p.len() <= 4)
}

/// Is `candidate` a newer version than `current`?
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => {
            let n = a.len().max(b.len());
            let pad = |v: &Vec<u64>| {
                (0..n)
                    .map(|i| v.get(i).copied().unwrap_or(0))
                    .collect::<Vec<_>>()
            };
            pad(&a) > pad(&b)
        }
        _ => false,
    }
}

/// Only plain download URLs of this repository's own releases are accepted
/// (no `..`, encoded dots, queries or fragments that could lead elsewhere).
fn is_release_url(url: &str) -> bool {
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    let rest = url.strip_prefix(&prefix).unwrap_or("");
    !rest.is_empty()
        && !url.contains("..")
        && !url.to_ascii_lowercase().contains("%2e")
        && !url.contains(['?', '#', '\\', '@'])
        && rest.split('/').count() == 2
}

/// Pick the newer release and its two assets out of GitHub's JSON.
fn evaluate(json: &str, current: &str) -> CmdResult<Option<(Release, String, String)>> {
    let r: GhRelease =
        serde_json::from_str(json).map_err(|e| format!("Unexpected answer from GitHub: {e}"))?;
    if !is_newer(&r.tag_name, current) {
        return Ok(None);
    }
    let url = |name: &str| {
        r.assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
            .filter(|u| is_release_url(u))
    };
    let (Some(apk), Some(sha)) = (url(APK_NAME), url(SHA_NAME)) else {
        return Ok(None); // a release without an Android build
    };
    let version = parse_version(&r.tag_name)
        .map(|v| v.iter().map(u64::to_string).collect::<Vec<_>>().join("."))
        .unwrap_or_default();
    let notes: String = r.body.unwrap_or_default().chars().take(2000).collect();
    Ok(Some((
        Release {
            version,
            notes,
            published_at: r.published_at,
        },
        apk,
        sha,
    )))
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult<UpdateInfo> {
    let current = app.package_info().version.to_string();
    let supported = cfg!(target_os = "android");
    if !supported {
        return Ok(UpdateInfo {
            current,
            supported,
            available: None,
        });
    }
    let json = tauri_plugin_justdevice::latest_release_json(&app, REPO)?;
    let found = evaluate(&json, &current)?;
    let pending = app.state::<PendingUpdate>();
    *pending.0.lock().unwrap() = found
        .as_ref()
        .map(|(_, apk, sha)| (apk.clone(), sha.clone()));
    Ok(UpdateInfo {
        current,
        supported,
        available: found.map(|(r, _, _)| r),
    })
}

#[tauri::command]
pub async fn install_update(app: AppHandle, pending: State<'_, PendingUpdate>) -> CmdResult<()> {
    let found = pending.0.lock().unwrap().clone();
    let Some((apk, sha)) = found else {
        return Err("No update to install. Check for updates first.".into());
    };
    tauri_plugin_justdevice::download_and_install(&app, REPO, &apk, &sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert_eq!(parse_version("android-v0.2.15"), Some(vec![0, 2, 15]));
        assert_eq!(parse_version("v1.0.0-beta.1"), Some(vec![1, 0, 0]));
        assert_eq!(parse_version("nightly"), None);
        assert!(is_newer("v0.2.15", "0.2.9"));
        assert!(is_newer("0.3", "0.2.99"));
        assert!(!is_newer("v0.2.9", "0.2.15"));
        assert!(!is_newer("v0.2.15", "0.2.15"));
        assert!(!is_newer("junk", "0.2.0"));
    }

    #[test]
    fn evaluates_github_releases() {
        let json = r#"{"tag_name":"android-v0.2.20","body":"Swipe fixes","published_at":"2026-11-12T10:00:00Z",
          "assets":[{"name":"justRead.apk","browser_download_url":"https://github.com/justpoqui/justRead/releases/download/android-v0.2.20/justRead.apk"},
                    {"name":"justRead.apk.sha256","browser_download_url":"https://github.com/justpoqui/justRead/releases/download/android-v0.2.20/justRead.apk.sha256"}]}"#;
        let (r, apk, sha) = evaluate(json, "0.2.11").unwrap().unwrap();
        assert_eq!(r.version, "0.2.20");
        assert_eq!(r.notes, "Swipe fixes");
        assert!(apk.ends_with("/justRead.apk") && sha.ends_with(".sha256"));
        assert!(
            evaluate(json, "0.2.20").unwrap().is_none(),
            "same version: nothing to do"
        );
        let no_apk = r#"{"tag_name":"v9.0.0","assets":[]}"#;
        assert!(evaluate(no_apk, "0.2.0").unwrap().is_none());
        assert!(evaluate("not json", "0.2.0").is_err());
    }

    #[test]
    fn only_own_release_urls() {
        let ok =
            format!("https://github.com/{REPO}/releases/download/android-v0.2.20/justRead.apk");
        assert!(is_release_url(&ok));
        for bad in [
            format!("https://github.com/{REPO}/releases/download/../../../evil/x/justRead.apk"),
            format!("https://github.com/{REPO}/releases/download/%2E%2E/x/justRead.apk"),
            format!("https://github.com/{REPO}/releases/download/v1/justRead.apk?x=1"),
            "https://github.com/someone/else/releases/download/v1/justRead.apk".to_string(),
            "http://github.com/justpoqui/justRead/releases/download/v1/justRead.apk".to_string(),
            format!("https://github.com/{REPO}/releases/download/a/b/justRead.apk"),
        ] {
            assert!(!is_release_url(&bad), "{bad}");
        }
    }
}
