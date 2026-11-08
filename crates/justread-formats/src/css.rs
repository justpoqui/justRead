// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Stylesheet scrubbing.
//!
//! Book CSS is kept (publishers' typography is part of the book), but some
//! constructs are removed because they reach the network or break the
//! reader:
//!
//! * remote `url(...)` and remote `@import` — justRead is offline-only;
//! * `expression(...)`, `behavior:` and `-moz-binding` — legacy script hooks;
//! * `position: fixed` — would pin content over every page.
//!
//! This is a small hand-written scanner, not a full CSS parser: it only needs
//! to understand comments, strings and `url(...)` to make safe edits.

/// Scrub a whole stylesheet (or the contents of a `<style>` element).
pub fn scrub_stylesheet(css: &str) -> String {
    // STEP 1: remove comments so they can't hide anything from later steps.
    let no_comments = strip_comments(css);
    // STEP 2: drop remote @import rules entirely.
    let no_imports = strip_remote_imports(&no_comments);
    // STEP 3: neutralize remote urls and script hooks.
    scrub_declarations(&no_imports)
}

/// Scrub a `style="..."` attribute value.
pub fn scrub_inline(style: &str) -> String {
    scrub_declarations(&strip_comments(style))
}

/// Remove `/* ... */` comments, leaving string contents untouched.
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match quote {
            // Inside a string: copy verbatim until the closing quote.
            Some(q) => {
                out.push(c);
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                out.push(c);
            }
            None if c == '/' && chars.peek() == Some(&'*') => {
                // Skip to the end of the comment (or of the input).
                chars.next();
                let mut prev = '\0';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
                out.push(' ');
            }
            None => out.push(c),
        }
    }
    out
}

/// Remove `@import` rules whose target is remote.
fn strip_remote_imports(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = find_ascii_ci(rest, "@import") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').map(|e| e + 1).unwrap_or(tail.len());
        let rule = &tail[..end];
        if !is_remote(rule) {
            out.push_str(rule);
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Replace remote `url(...)`s and strip script-capable constructs.
fn scrub_declarations(css: &str) -> String {
    // STEP 1: rewrite url(...) tokens.
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = find_ascii_ci(rest, "url(") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(')').map(|e| e + 1).unwrap_or(tail.len());
        let token = &tail[..end];
        if is_remote(token) {
            out.push_str("url(\"\")");
        } else {
            out.push_str(token);
        }
        rest = &tail[end..];
    }
    out.push_str(rest);

    // STEP 2: neutralize legacy script hooks and fixed positioning.
    let mut s = out;
    for (needle, replacement) in [
        ("expression(", "invalid("),
        ("behavior:", "x-removed:"),
        ("-moz-binding", "x-removed"),
        ("javascript:", "removed:"),
    ] {
        s = replace_ascii_ci(&s, needle, replacement);
    }
    s = replace_fixed_position(&s);
    s
}

/// `position: fixed` (any spacing/case) → `position: static`.
fn replace_fixed_position(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = find_ascii_ci(rest, "position") {
        let (before, tail) = rest.split_at(i + "position".len());
        out.push_str(before);
        let trimmed = tail.trim_start();
        if let Some(after_colon) = trimmed.strip_prefix(':') {
            let value = after_colon.trim_start();
            if value.len() >= 5 && value[..5].eq_ignore_ascii_case("fixed") {
                out.push_str(": static");
                rest = &value[5..];
                continue;
            }
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// Does a CSS fragment reference something outside the book?
fn is_remote(fragment: &str) -> bool {
    let l = fragment.to_ascii_lowercase();
    l.contains("http:")
        || l.contains("https:")
        || l.contains("//")
        || l.contains("ftp:")
        || l.contains("javascript:")
}

/// ASCII case-insensitive `find`.
fn find_ascii_ci(hay: &str, needle: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// ASCII case-insensitive replace-all.
fn replace_ascii_ci(hay: &str, needle: &str, with: &str) -> String {
    let mut out = String::with_capacity(hay.len());
    let mut rest = hay;
    while let Some(i) = find_ascii_ci(rest, needle) {
        out.push_str(&rest[..i]);
        out.push_str(with);
        rest = &rest[i + needle.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_remote_and_dangerous_content() {
        let css = r#"@import url("https://evil.example/x.css");
@import "local.css";
/* url(http://hidden) */ p { background: URL(http://x/y.png); color: red }
.a { background: url('img/ok.png') } .b { width: expression(alert(1)); behavior: url(x.htc) }
.c { position :  FIXED; top: 0 }"#;
        let out = scrub_stylesheet(css);
        assert!(!out.contains("evil"));
        assert!(out.contains("@import \"local.css\";"));
        assert!(!out.contains("hidden"));
        assert!(out.contains("background: url(\"\")"));
        assert!(out.contains("url('img/ok.png')"));
        assert!(!out.to_lowercase().contains("expression("));
        assert!(!out.contains("behavior:"));
        assert!(out.contains("position: static"));
    }

    #[test]
    fn comments_inside_strings_are_kept() {
        assert_eq!(
            strip_comments(r#"a { content: "/* x */" }"#),
            r#"a { content: "/* x */" }"#
        );
    }

    #[test]
    fn inline_styles() {
        assert_eq!(
            scrub_inline("color: red; background: url(https://t/p.gif)"),
            "color: red; background: url(\"\")"
        );
    }
}
