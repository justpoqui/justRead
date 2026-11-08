// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

// Phones and tablets use a native browser screen instead (planned, see ADR 0008);
// until then the window/webview code below is desktop-only and unused there.
#![cfg_attr(mobile, allow(dead_code, unused_imports))]

//! justBrowse — a privacy-hardened browser for finding free books.
//!
//! One window ("browse") holds two webviews:
//!
//! * `browse-chrome`  — our own toolbar page (address bar, buttons). Its
//!   capability (capabilities/browse.json) grants only the browse commands.
//! * `browse-content` — the remote web page. **No capability names it**, so
//!   it has zero access to IPC commands; it runs in an incognito session
//!   (nothing persists, nothing shared with the reader), and all its traffic
//!   goes through the local [`proxy`] (blocklist, HTTPS-only, no local hosts).
//!
//! Downloads of book files are handed to the normal import pipeline.
//! Multi-webview windows need Tauri's `unstable` feature on desktop; mobile
//! uses a different design (see docs/adr/0008-justbrowse.md).

pub mod blocklist;
pub mod policy;
pub mod proxy;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, WebviewUrl, WindowEvent,
};

use self::policy::{Decision, DEFAULT_SEARCH};
use self::proxy::Gate;
use crate::state::AppState;

pub const WINDOW: &str = "browse";
pub const CHROME: &str = "browse-chrome";
pub const CONTENT: &str = "browse-content";
/// Height of the toolbar webview (logical px).
#[cfg(desktop)]
const BAR: f64 = 96.0;

/// Browser state shared by commands and webview callbacks.
#[derive(Default)]
pub struct BrowseState {
    gate: OnceLock<Arc<Gate>>,
    proxy: OnceLock<std::net::SocketAddr>,
    /// url → where we told the webview to save it (macOS doesn't report the path).
    downloads: Mutex<HashMap<String, PathBuf>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageState {
    url: String,
    loading: bool,
    secure: bool,
    blocked: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadStatus {
    status: &'static str, // started | imported | duplicate | rejected | failed
    name: String,
    message: String,
}

#[cfg(desktop)]
/// Start the proxy once per app run.
fn ensure_proxy(app: &AppHandle) -> Result<std::net::SocketAddr, String> {
    let st = app.state::<BrowseState>();
    if let Some(addr) = st.proxy.get() {
        return Ok(*addr);
    }
    // Local test servers are only reachable in debug builds, on explicit request.
    let allow_loopback = cfg!(debug_assertions)
        && std::env::var("JUSTREAD_BROWSE_ALLOW_LOOPBACK").as_deref() == Ok("1");
    let handle = app.clone();
    let gate = Arc::new(Gate::new(
        allow_loopback,
        Box::new(move |_host| {
            let blocked = handle
                .state::<BrowseState>()
                .gate
                .get()
                .map(|g| g.blocked.load(Ordering::Relaxed))
                .unwrap_or(0);
            let _ = handle.emit_to(CHROME, "browse-blocked", blocked);
        }),
    ));
    let addr =
        tauri::async_runtime::block_on(proxy::start(gate.clone())).map_err(|e| e.to_string())?;
    let _ = st.gate.set(gate);
    let _ = st.proxy.set(addr);
    Ok(addr)
}

fn gate(app: &AppHandle) -> Option<Arc<Gate>> {
    app.state::<BrowseState>().gate.get().cloned()
}

#[cfg(desktop)]
/// Open (or focus) the browser window.
pub fn open(app: &AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_window(WINDOW) {
        return w.set_focus().map_err(|e| e.to_string());
    }
    let proxy = ensure_proxy(app)?;
    let err = |e: tauri::Error| e.to_string();

    // STEP 1: the window and its size.
    let window = WindowBuilder::new(app, WINDOW)
        .title("justBrowse")
        .inner_size(1100.0, 800.0)
        .min_inner_size(600.0, 400.0)
        .build()
        .map_err(err)?;
    let scale = window.scale_factor().map_err(err)?;
    let size = window.inner_size().map_err(err)?.to_logical::<f64>(scale);

    // STEP 2: the toolbar — a local app page with a narrow capability.
    let chrome = WebviewBuilder::new(CHROME, WebviewUrl::App("browse.html".into()));

    // STEP 3: the content webview — remote pages, no IPC, incognito, proxied.
    let nav_app = app.clone();
    let win_app = app.clone();
    let dl_app = app.clone();
    let load_app = app.clone();
    let content = WebviewBuilder::new(
        CONTENT,
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .incognito(true)
    .general_autofill_enabled(false)
    .proxy_url(format!("http://{proxy}").parse().unwrap())
    .on_navigation(move |url| navigation_allowed(&nav_app, url))
    .on_new_window(move |url, _features| {
        // Popups open in place (subject to the same navigation rules).
        let app = win_app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(w) = app.get_webview(CONTENT) {
                let _ = w.navigate(url);
            }
        });
        NewWindowResponse::Deny
    })
    .on_download(move |_w, event| on_download(&dl_app, event))
    .on_page_load(move |_w, payload| {
        let blocked = gate(&load_app)
            .map(|g| g.blocked.load(Ordering::Relaxed))
            .unwrap_or(0);
        let url = payload.url();
        let state = PageState {
            url: url.to_string(),
            loading: matches!(payload.event(), PageLoadEvent::Started),
            secure: url.scheme() == "https",
            blocked,
        };
        let _ = load_app.emit_to(CHROME, "browse-state", state);
    });

    // STEP 4: place both and keep them laid out on resize.
    window
        .add_child(
            chrome,
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(size.width, BAR),
        )
        .map_err(err)?;
    let chrome = app.get_webview(CHROME).ok_or("toolbar webview missing")?;
    let content = window
        .add_child(
            content,
            LogicalPosition::new(0.0, BAR),
            LogicalSize::new(size.width, (size.height - BAR).max(1.0)),
        )
        .map_err(err)?;
    block_third_party_cookies(&content);
    pack_linux(&chrome, &content);
    // GTK packs child webviews into a container that initially shares the space
    // evenly, ignoring the sizes given to add_child — so apply the layout
    // explicitly now, again once the window is mapped, and on every resize.
    layout(app);
    let later = app.clone();
    tauri::async_runtime::spawn(async move {
        for ms in [50, 250, 1000] {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            let app = later.clone();
            let _ = later.run_on_main_thread(move || layout(&app));
        }
    });
    let layout_app = app.clone();
    window.on_window_event(move |e| {
        if let WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } = e {
            layout(&layout_app);
        }
    });
    Ok(())
}

#[cfg(desktop)]
/// Linux: Tauri places child webviews in the window's vertical `gtk::Box`,
/// where explicit bounds are ignored and space is shared evenly. Give the
/// toolbar a fixed height and let the content take the remaining space.
fn pack_linux(chrome: &tauri::Webview, content: &tauri::Webview) {
    #[cfg(target_os = "linux")]
    for (webview, is_toolbar) in [(chrome, true), (content, false)] {
        let _ = webview.with_webview(move |pw| {
            use gtk::prelude::{BoxExt, Cast, WidgetExt};
            let w = pw.inner();
            if is_toolbar {
                w.set_size_request(-1, BAR as i32);
            }
            if let Some(parent) = w.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) {
                parent.set_child_packing(&w, !is_toolbar, true, 0, gtk::PackType::Start);
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (chrome, content);
}

#[cfg(desktop)]
/// Toolbar on top (BAR px), web content filling the rest.
fn layout(app: &AppHandle) {
    let Some(w) = app.get_window(WINDOW) else {
        return;
    };
    let Ok(px) = w.inner_size() else { return };
    let s = px.to_logical::<f64>(w.scale_factor().unwrap_or(1.0));
    if let Some(c) = app.get_webview(CHROME) {
        let _ = c.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = c.set_size(LogicalSize::new(s.width, BAR));
    }
    if let Some(c) = app.get_webview(CONTENT) {
        let _ = c.set_position(LogicalPosition::new(0.0, BAR));
        let _ = c.set_size(LogicalSize::new(s.width, (s.height - BAR).max(1.0)));
    }
}

#[cfg(desktop)]
/// Refuse third-party cookies in the content webview.
///
/// * Linux (WebKitGTK): set explicitly on the webview's (ephemeral) cookie manager.
/// * macOS/iOS (WKWebView): WebKit blocks third-party cookies by default (ITP).
/// * Windows (WebView2): relies on the InPrivate session; to be verified on Windows.
fn block_third_party_cookies(webview: &tauri::Webview) {
    #[cfg(target_os = "linux")]
    let _ = webview.with_webview(|pw| {
        use webkit2gtk::{CookieAcceptPolicy, CookieManagerExt, WebViewExt, WebsiteDataManagerExt};
        if let Some(cookies) = pw
            .inner()
            .website_data_manager()
            .and_then(|m| m.cookie_manager())
        {
            cookies.set_accept_policy(CookieAcceptPolicy::NoThirdParty);
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = webview;
}

#[cfg(desktop)]
/// Top-level navigation policy (see policy::navigation).
fn navigation_allowed(app: &AppHandle, url: &Url) -> bool {
    let allowed = gate(app)
        .and_then(|g| g.http_allowed.lock().ok().map(|s| s.clone()))
        .unwrap_or_default();
    match policy::navigation(url, &allowed) {
        Decision::Allow => true,
        Decision::HttpWarning => {
            let _ = app.emit_to(CHROME, "browse-http-warning", url.to_string());
            false
        }
        Decision::Block => false,
    }
}

#[cfg(desktop)]
/// Downloads: save into a private folder, then import books into the library.
fn on_download(app: &AppHandle, event: DownloadEvent<'_>) -> bool {
    let st = app.state::<BrowseState>();
    match event {
        DownloadEvent::Requested { url, destination } => {
            // STEP 1: a private, app-owned folder (never the user's Downloads).
            let Ok(dir) = app
                .path()
                .app_cache_dir()
                .map(|d| d.join("browse-downloads"))
            else {
                return false;
            };
            if std::fs::create_dir_all(&dir).is_err() {
                return false;
            }
            let name = file_name_for(&url);
            let path = unique(&dir, &name);
            *destination = path.clone();
            st.downloads.lock().unwrap().insert(url.to_string(), path);
            let _ = app.emit_to(
                CHROME,
                "browse-download",
                DownloadStatus {
                    status: "started",
                    name,
                    message: String::new(),
                },
            );
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            // STEP 2: find the file and import it off the UI thread.
            let path = path
                .filter(|p| p.is_file())
                .or_else(|| st.downloads.lock().unwrap().remove(url.as_str()));
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let Some(path) = path else { return };
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("download")
                    .to_string();
                let status = if !success {
                    DownloadStatus {
                        status: "failed",
                        name,
                        message: "The download did not complete.".into(),
                    }
                } else {
                    import_download(&app, &path, name)
                };
                let _ = std::fs::remove_file(&path); // the library keeps its own copy
                let _ = app.emit_to(CHROME, "browse-download", status);
            });
            true
        }
        _ => true,
    }
}

#[cfg(desktop)]
fn import_download(app: &AppHandle, path: &std::path::Path, name: String) -> DownloadStatus {
    let state = app.state::<AppState>();
    // Files that aren't books (no adapter recognizes them) are discarded.
    if state.registry.detect(path).is_err() {
        return DownloadStatus {
            status: "rejected",
            name,
            message: "Not a book format justRead can open; the file was discarded.".into(),
        };
    }
    let result = state.lib().import(path, &state.registry);
    match result {
        Ok(justread_library::ImportOutcome::Imported { book, .. }) => {
            let _ = app.emit_to("main", "library-changed", ());
            DownloadStatus {
                status: "imported",
                name,
                message: format!("“{}” was added to your library.", book.title),
            }
        }
        Ok(justread_library::ImportOutcome::Duplicate { book }) => DownloadStatus {
            status: "duplicate",
            name,
            message: format!("“{}” is already in your library.", book.title),
        },
        Err(e) => DownloadStatus {
            status: "failed",
            name,
            message: e.to_string(),
        },
    }
}

/// A safe file name from the URL's last path segment.
fn file_name_for(url: &Url) -> String {
    let last = url
        .path_segments()
        .and_then(|mut s| s.next_back().map(str::to_string))
        .unwrap_or_default();
    let decoded = justbook::paths::percent_decode(&last);
    let clean: String = decoded
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
        "download".into()
    } else {
        clean.chars().take(120).collect()
    }
}

fn unique(dir: &std::path::Path, name: &str) -> PathBuf {
    let mut p = dir.join(name);
    let mut n = 1;
    while p.exists() {
        p = dir.join(format!("{n}-{name}"));
        n += 1;
    }
    p
}

fn content(app: &AppHandle) -> Result<tauri::Webview, String> {
    app.get_webview(CONTENT)
        .ok_or_else(|| "the browser is not open".to_string())
}

fn search_template(state: &AppState) -> String {
    state
        .lib()
        .get_setting("browse.search")
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|t| policy::valid_search_template(t))
        .unwrap_or_else(|| DEFAULT_SEARCH.to_string())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Reader window: open justBrowse.
#[tauri::command]
pub fn open_browser(app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    return open(&app);
    // Phones/tablets have no multi-webview windows; a native browser screen is planned
    // (docs/adr/0008-justbrowse.md).
    #[cfg(mobile)]
    {
        let _ = app;
        Err("justBrowse is not available on phones and tablets yet.".into())
    }
}

/// Toolbar: go to a URL or search for the text. Returns the resolved URL so
/// the address bar can show it immediately, even if the site is slow.
#[tauri::command]
pub fn browse_navigate(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
) -> Result<String, String> {
    let url = policy::resolve_input(&input, &search_template(&state))
        .ok_or("Type an address or something to search for.")?;
    content(&app)?
        .navigate(url.clone())
        .map_err(|e| e.to_string())?;
    Ok(url.to_string())
}

/// Toolbar: back (-1) / forward (1).
#[tauri::command]
pub fn browse_history(app: AppHandle, step: i32) -> Result<(), String> {
    let step = step.clamp(-1, 1);
    content(&app)?
        .eval(format!("history.go({step})"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn browse_reload(app: AppHandle) -> Result<(), String> {
    content(&app)?
        .eval("location.reload()")
        .map_err(|e| e.to_string())
}

/// Toolbar: the user accepted the "not encrypted" warning for this site.
#[tauri::command]
pub fn browse_allow_http(app: AppHandle, url: String) -> Result<(), String> {
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    if url.scheme() != "http" {
        return Err("only http:// addresses need this".into());
    }
    let host = url.host_str().ok_or("no host")?.to_ascii_lowercase();
    gate(&app)
        .ok_or("the browser is not open")?
        .http_allowed
        .lock()
        .unwrap()
        .insert(host);
    content(&app)?.navigate(url).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseConfig {
    search: String,
    blocked: u64,
    blocklist_size: usize,
}

#[tauri::command]
pub fn browse_config(app: AppHandle, state: State<'_, AppState>) -> BrowseConfig {
    let g = gate(&app);
    BrowseConfig {
        search: search_template(&state),
        blocked: g
            .as_ref()
            .map(|g| g.blocked.load(Ordering::Relaxed))
            .unwrap_or(0),
        blocklist_size: g.map(|g| g.blocklist.len()).unwrap_or(0),
    }
}

/// Toolbar: change the search engine (any https URL template with `{q}`, e.g. SearXNG).
#[tauri::command]
pub fn browse_set_search(state: State<'_, AppState>, template: String) -> Result<(), String> {
    if !policy::valid_search_template(&template) {
        return Err("The search address must start with https:// and contain {q} once.".into());
    }
    state
        .lib()
        .set_setting("browse.search", &serde_json::Value::String(template))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_names() {
        let u = |s: &str| Url::parse(s).unwrap();
        assert_eq!(
            file_name_for(&u("https://x.org/ebooks/1342.epub3.images")),
            "1342.epub3.images"
        );
        assert_eq!(
            file_name_for(&u("https://x.org/a/My%20Book%20(1).epub")),
            "My Book (1).epub"
        );
        assert_eq!(
            file_name_for(&u("https://x.org/../../etc/passwd")),
            "passwd"
        );
        assert_eq!(file_name_for(&u("https://x.org/")), "download");
        // Encoded separators can never produce a path.
        for nasty in [
            "https://x.org/..%2F..%2Fx.epub",
            "https://x.org/a%5C..%5Cb.epub",
            "https://x.org/.hidden",
        ] {
            let n = file_name_for(&u(nasty));
            assert!(
                !n.contains('/') && !n.contains('\\') && !n.starts_with('.'),
                "{nasty} → {n}"
            );
        }
    }
}
