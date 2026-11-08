// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Pure decisions for justBrowse (unit-tested): what the address bar means,
//! which navigations are allowed, and which search templates are valid.

use std::collections::HashSet;

use tauri::Url;

/// Default search: DuckDuckGo (no account, no tracking by design).
pub const DEFAULT_SEARCH: &str = "https://duckduckgo.com/?q={q}";

/// Turn address-bar input into a URL: a full URL, a bare domain, or a search.
pub fn resolve_input(input: &str, search_template: &str) -> Option<Url> {
    let t = input.trim();
    if t.is_empty() {
        return None;
    }
    // STEP 1: explicit http(s) URLs.
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Url::parse(t).ok();
    }
    // STEP 2: something that looks like a host ("standardebooks.org", "example.com/path").
    let host = t.split(['/', '?', '#']).next().unwrap_or("");
    let looks_like_host = !t.contains(char::is_whitespace)
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || "-.:".contains(c));
    if looks_like_host {
        return Url::parse(&format!("https://{t}")).ok(); // HTTPS first, always
    }
    // STEP 3: everything else is a search.
    Url::parse(&search_template.replace("{q}", &encode_query(t))).ok()
}

/// What to do with a top-level navigation in the content webview.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Plain HTTP to a host the user hasn't allowed: ask first.
    HttpWarning,
    /// Never allowed (local files, app schemes, javascript:, data: …).
    Block,
}

pub fn navigation(url: &Url, http_allowed: &HashSet<String>) -> Decision {
    match url.scheme() {
        "https" => Decision::Allow,
        "http" => {
            let host = url.host_str().unwrap_or("").to_ascii_lowercase();
            if http_allowed.contains(&host) {
                Decision::Allow
            } else {
                Decision::HttpWarning
            }
        }
        "about" if url.as_str() == "about:blank" => Decision::Allow,
        // blob: is how pages hand files to the download machinery.
        "blob" => Decision::Allow,
        // file:, data:, javascript:, justbook:, tauri:, ipc: … are never navigable.
        _ => Decision::Block,
    }
}

/// A search template must be an https URL with exactly one `{q}`.
pub fn valid_search_template(t: &str) -> bool {
    t.starts_with("https://")
        && t.matches("{q}").count() == 1
        && Url::parse(&t.replace("{q}", "test")).is_ok()
}

/// Percent-encode a search query for a URL query component.
pub fn encode_query(q: &str) -> String {
    let mut out = String::with_capacity(q.len() * 3);
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_bar() {
        let r = |s| resolve_input(s, DEFAULT_SEARCH).map(|u| u.to_string());
        assert_eq!(
            r("standardebooks.org").as_deref(),
            Some("https://standardebooks.org/")
        );
        assert_eq!(
            r("gutenberg.org/ebooks/1342").as_deref(),
            Some("https://gutenberg.org/ebooks/1342")
        );
        assert_eq!(
            r("http://example.com/a").as_deref(),
            Some("http://example.com/a")
        );
        assert_eq!(
            r("pride and prejudice epub").as_deref(),
            Some("https://duckduckgo.com/?q=pride+and+prejudice+epub")
        );
        assert_eq!(
            r("c++ & rust").as_deref(),
            Some("https://duckduckgo.com/?q=c%2B%2B+%26+rust")
        );
        assert_eq!(r("  "), None);
        // A self-hosted SearXNG template works the same way.
        assert_eq!(
            resolve_input("moby dick", "https://searx.example/search?q={q}")
                .unwrap()
                .as_str(),
            "https://searx.example/search?q=moby+dick"
        );
    }

    #[test]
    fn navigation_policy() {
        let mut allowed = HashSet::new();
        let d = |u: &str, a: &HashSet<String>| navigation(&Url::parse(u).unwrap(), a);
        assert_eq!(d("https://standardebooks.org", &allowed), Decision::Allow);
        assert_eq!(d("http://example.com", &allowed), Decision::HttpWarning);
        allowed.insert("example.com".into());
        assert_eq!(d("http://example.com/x", &allowed), Decision::Allow);
        assert_eq!(d("http://other.com", &allowed), Decision::HttpWarning);
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hi",
            "justbook://localhost/x",
            "tauri://localhost",
            "ipc://localhost/x",
        ] {
            assert_eq!(d(bad, &allowed), Decision::Block, "{bad}");
        }
        assert_eq!(d("about:blank", &allowed), Decision::Allow);
    }

    #[test]
    fn search_templates() {
        assert!(valid_search_template(DEFAULT_SEARCH));
        assert!(valid_search_template(
            "https://searx.example.org/search?q={q}&language=en"
        ));
        assert!(!valid_search_template("http://searx.example.org/?q={q}"));
        assert!(!valid_search_template("https://example.org/search"));
        assert!(!valid_search_template("https://example.org/?q={q}&r={q}"));
        assert!(!valid_search_template("javascript:{q}"));
    }
}
