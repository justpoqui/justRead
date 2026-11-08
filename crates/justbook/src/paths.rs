// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Package-relative path handling.
//!
//! Source archives are hostile input: an EPUB can contain entries like
//! `../../etc/passwd` ("zip slip") or absolute paths. Every path that ends
//! up on disk or in the manifest goes through [`normalize_relative`] first.

use crate::{Error, Result};

/// Normalize a `/`-separated relative path:
/// resolves `.` and `..`, collapses duplicate slashes, converts `\` to `/`,
/// and **rejects** anything that would climb above the root or is absolute.
pub fn normalize_relative(path: &str) -> Result<String> {
    // STEP 1: unify separators; Windows-made archives sometimes use '\'.
    let unified = path.replace('\\', "/");

    // STEP 2: absolute paths and drive letters are never allowed.
    if unified.starts_with('/') || unified.chars().nth(1) == Some(':') {
        return Err(Error::UnsafePath(path.to_string()));
    }

    // STEP 3: walk the segments with a stack so `a/b/../c` becomes `a/c`.
    let mut out: Vec<&str> = Vec::new();
    for seg in unified.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                // Popping past the root means the path escapes the package.
                if out.pop().is_none() {
                    return Err(Error::UnsafePath(path.to_string()));
                }
            }
            s => out.push(s),
        }
    }

    // STEP 4: an empty result (e.g. "./") is not a file.
    if out.is_empty() {
        return Err(Error::UnsafePath(path.to_string()));
    }
    Ok(out.join("/"))
}

/// Resolve `reference` (as written inside a document located at `base_file`)
/// to a root-relative path. The fragment and query are NOT included.
///
/// `resolve_reference("OEBPS/text/ch1.xhtml", "../img/a.png")` → `OEBPS/img/a.png`.
pub fn resolve_reference(base_file: &str, reference: &str) -> Result<String> {
    // STEP 1: drop fragment and query; they are handled by the caller.
    let without_frag = reference.split('#').next().unwrap_or("");
    let without_query = without_frag.split('?').next().unwrap_or("");
    let decoded = percent_decode(without_query);

    // STEP 2: an empty reference ("#frag") points at the base document itself.
    if decoded.is_empty() {
        return normalize_relative(base_file);
    }

    // STEP 3: join with the base document's directory and normalize.
    let dir = match base_file.rfind('/') {
        Some(i) => &base_file[..=i],
        None => "",
    };
    normalize_relative(&format!("{dir}{decoded}"))
}

/// Return the fragment of a reference (text after `#`), if any.
pub fn fragment(reference: &str) -> Option<&str> {
    reference
        .split_once('#')
        .map(|(_, f)| f)
        .filter(|f| !f.is_empty())
}

/// Relative path from the directory of `from_file` to `to_file`.
/// Both must already be normalized root-relative paths.
/// `relative_path("content/c1.xhtml", "resources/a.png")` → `../resources/a.png`.
pub fn relative_path(from_file: &str, to_file: &str) -> String {
    // STEP 1: split both into segments; the last segment of `from` is a file.
    let from: Vec<&str> = from_file.split('/').collect();
    let from_dir = &from[..from.len().saturating_sub(1)];
    let to: Vec<&str> = to_file.split('/').collect();

    // STEP 2: find the shared directory prefix.
    let mut common = 0;
    while common < from_dir.len() && common < to.len() - 1 && from_dir[common] == to[common] {
        common += 1;
    }

    // STEP 3: climb out of the remaining `from` dirs, then descend into `to`.
    let mut parts: Vec<&str> = vec![".."; from_dir.len() - common];
    parts.extend_from_slice(&to[common..]);
    parts.join("/")
}

/// Is this reference pointing outside the book (web, mail, …)?
pub fn is_external(reference: &str) -> bool {
    let lower = reference.trim().to_ascii_lowercase();
    // A scheme is letters followed by ':' before any '/', e.g. "https:".
    match lower.find(':') {
        Some(i) => {
            let scheme = &lower[..i];
            !scheme.is_empty()
                && scheme.len() > 1 // single letter would be a Windows drive
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
                && !lower[..i].contains('/')
        }
        None => false,
    }
}

/// Minimal RFC 3986 percent-decoding (invalid sequences are kept verbatim).
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            // Work on bytes so a multi-byte character after '%' can't panic.
            let (h, l) = (bytes[i + 1], bytes[i + 2]);
            if h.is_ascii_hexdigit() && l.is_ascii_hexdigit() {
                let v = (hex_val(h) << 4) | hex_val(l);
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Value of one ASCII hex digit (caller guarantees it is one).
fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_rejects_escapes() {
        assert_eq!(normalize_relative("a/./b//c").unwrap(), "a/b/c");
        assert_eq!(normalize_relative("a/b/../c").unwrap(), "a/c");
        assert_eq!(normalize_relative("a\\b.png").unwrap(), "a/b.png");
        assert!(normalize_relative("../x").is_err());
        assert!(normalize_relative("a/../../x").is_err());
        assert!(normalize_relative("/etc/passwd").is_err());
        assert!(normalize_relative("C:/x").is_err());
        assert!(normalize_relative("./").is_err());
    }

    #[test]
    fn resolves_references() {
        let base = "OEBPS/text/ch1.xhtml";
        assert_eq!(
            resolve_reference(base, "../img/a.png").unwrap(),
            "OEBPS/img/a.png"
        );
        assert_eq!(
            resolve_reference(base, "ch2.xhtml#p3").unwrap(),
            "OEBPS/text/ch2.xhtml"
        );
        assert_eq!(resolve_reference(base, "#top").unwrap(), base);
        assert_eq!(
            resolve_reference(base, "my%20file.xhtml").unwrap(),
            "OEBPS/text/my file.xhtml"
        );
        assert_eq!(resolve_reference("ch.xhtml", "a.css").unwrap(), "a.css");
        assert!(resolve_reference(base, "../../../evil").is_err());
    }

    #[test]
    fn computes_relative_paths() {
        assert_eq!(
            relative_path("content/c1.xhtml", "resources/a.png"),
            "../resources/a.png"
        );
        assert_eq!(
            relative_path("content/c1.xhtml", "content/c2.xhtml"),
            "c2.xhtml"
        );
        assert_eq!(relative_path("a/b/c.css", "a/d/e.png"), "../d/e.png");
        assert_eq!(relative_path("x.css", "img/a.png"), "img/a.png");
    }

    #[test]
    fn detects_external_and_fragments() {
        assert!(is_external("https://example.org"));
        assert!(is_external("mailto:a@b.c"));
        assert!(!is_external("chapter1.xhtml"));
        assert!(!is_external("img/a:b.png"));
        assert_eq!(fragment("a.xhtml#x"), Some("x"));
        assert_eq!(fragment("a.xhtml#"), None);
        assert_eq!(fragment("a.xhtml"), None);
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%E2%82%AC"), "€");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
