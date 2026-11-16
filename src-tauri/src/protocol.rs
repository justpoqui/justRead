// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! The `justbook://` protocol: serves files from book packages to the
//! reader (images, stylesheets, fonts referenced by chapters).
//!
//! URL shape: `justbook://localhost/<book-uuid>/<package path>` — on
//! Windows/Android Tauri exposes custom schemes as
//! `http://justbook.localhost/...` instead; the UI builds URLs with
//! `convertFileSrc(path, "justbook")`, which picks the right form.
//! Docs: <https://docs.rs/tauri/2/tauri/struct.Builder.html#method.register_asynchronous_uri_scheme_protocol>
//!
//! Security: the book id must be a UUID, the path is normalized and
//! canonicalized inside the package (no traversal, no symlink escape), and
//! responses forbid scripts and sniffing.

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::state::AppState;

pub const SCHEME: &str = "justbook";

/// Entry point registered with the Tauri builder.
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    // STEP 0: only the reader's webview may read book files (defence in
    // depth should any other webview ever exist).
    if ctx.webview_label() != "main" {
        responder.respond(
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Vec::new())
                .unwrap(),
        );
        return;
    }

    // STEP 1: copy what we need; the work happens on a blocking thread.
    let books_dir = ctx.app_handle().state::<AppState>().books_dir.clone();
    let path = request.uri().path().to_string();

    std::thread::spawn(move || {
        let response = match serve(&books_dir, &path) {
            Ok((bytes, mime)) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                // Fonts loaded from another origin need CORS to be usable.
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .header("X-Content-Type-Options", "nosniff")
                .header("Content-Security-Policy", "default-src 'none'; img-src justbook: http://justbook.localhost data:; style-src 'unsafe-inline'; script-src 'none'")
                .header(header::CACHE_CONTROL, "private, max-age=3600")
                .body(bytes),
            Err(status) => Response::builder().status(status).body(Vec::new()),
        };
        responder.respond(response.unwrap_or_else(|_| Response::new(Vec::new())));
    });
}

/// Resolve `/<uuid>/<rel>` to file bytes and a MIME type.
fn serve(
    books_dir: &std::path::Path,
    url_path: &str,
) -> Result<(Vec<u8>, &'static str), StatusCode> {
    // STEP 1: decode first (convertFileSrc encodes '/' as %2F), then split
    // into book id and package-relative path. Decoded ".." segments are
    // caught by the normalization in STEP 3.
    let decoded = justbook::paths::percent_decode(url_path);
    let trimmed = decoded.trim_start_matches('/');
    let (id, rel) = trimmed.split_once('/').ok_or(StatusCode::NOT_FOUND)?;
    if !is_uuid(id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let rel = rel.to_string();

    // STEP 2: never serve the manifest or the original file through here.
    if rel == "manifest.json" || rel.starts_with("original/") {
        return Err(StatusCode::FORBIDDEN);
    }

    // STEP 3: safe resolution inside the package, then read.
    let dir = books_dir.join(id);
    let file = justbook::resolve_in_package(&dir, &rel).map_err(|_| StatusCode::NOT_FOUND)?;
    let bytes = std::fs::read(&file).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((bytes, mime_for(&rel)))
}

/// Strict 8-4-4-4-12 hex UUID check (avoids pulling in the uuid crate here).
fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// MIME type by extension (package files only ever have these types).
fn mime_for(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "xhtml" | "html" | "htm" => "application/xhtml+xml",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "pdf" => "application/pdf",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_only_package_files() {
        // STEP 1: a fake package.
        let tmp = std::env::temp_dir().join(format!("jr-proto-{}", std::process::id()));
        let id = "0b5c2f6e-6a43-4c71-9a0e-7f1e7d4b9a10";
        std::fs::create_dir_all(tmp.join(id).join("resources")).unwrap();
        std::fs::write(tmp.join(id).join("resources/a b.png"), b"png").unwrap();
        std::fs::write(tmp.join(id).join("manifest.json"), b"{}").unwrap();
        std::fs::write(tmp.join("secret.txt"), b"s").unwrap();

        // STEP 2: allowed, encoded, forbidden and traversal cases.
        assert_eq!(
            serve(&tmp, &format!("/{id}/resources/a%20b.png")).unwrap(),
            (b"png".to_vec(), "image/png")
        );
        assert_eq!(
            serve(&tmp, &format!("/{id}/manifest.json")),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            serve(&tmp, &format!("/{id}/../secret.txt")),
            Err(StatusCode::NOT_FOUND)
        );
        assert_eq!(
            serve(&tmp, &format!("/{id}/%2e%2e/secret.txt")),
            Err(StatusCode::NOT_FOUND)
        );
        assert_eq!(serve(&tmp, "/not-a-uuid/x"), Err(StatusCode::NOT_FOUND));
        assert_eq!(
            serve(&tmp, &format!("/{id}%2Fresources%2Fa%20b.png"))
                .unwrap()
                .1,
            "image/png"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn uuid_check() {
        assert!(is_uuid("0b5c2f6e-6a43-4c71-9a0e-7f1e7d4b9a10"));
        assert!(!is_uuid("../../etc"));
        assert!(!is_uuid("0b5c2f6e6a434c719a0e7f1e7d4b9a10"));
    }
}
