// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Domain blocklist: a host is blocked if it, or any parent domain, is listed.

use std::collections::HashSet;

/// The bundled list (see blocklist.txt for its origin and license).
const BUNDLED: &str = include_str!("blocklist.txt");

pub struct Blocklist {
    domains: HashSet<String>,
}

impl Blocklist {
    pub fn bundled() -> Self {
        Self::parse(BUNDLED)
    }

    pub fn parse(text: &str) -> Self {
        let domains = text
            .lines()
            .map(|l| {
                l.split('#')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches("*.")
                    .to_ascii_lowercase()
            })
            .filter(|l| !l.is_empty())
            .collect();
        Self { domains }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.domains.len()
    }

    /// `ads.example.com` is blocked by an `example.com` entry; `notexample.com` is not.
    pub fn is_blocked(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let mut rest = host.as_str();
        loop {
            if self.domains.contains(rest) {
                return true;
            }
            match rest.split_once('.') {
                Some((_, parent)) if parent.contains('.') || self.domains.contains(parent) => {
                    rest = parent
                }
                _ => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        let b =
            Blocklist::parse("# c\ndoubleclick.net\nconnect.facebook.net  # pixel\n*.hotjar.com\n");
        assert!(b.is_blocked("doubleclick.net"));
        assert!(b.is_blocked("stats.g.doubleclick.net"));
        assert!(b.is_blocked("CONNECT.facebook.net."));
        assert!(!b.is_blocked("facebook.net"));
        assert!(!b.is_blocked("www.facebook.com"));
        assert!(b.is_blocked("script.hotjar.com"));
        assert!(!b.is_blocked("notdoubleclick.net"));
        assert!(Blocklist::bundled().len() > 100);
        assert!(!Blocklist::bundled().is_blocked("standardebooks.org"));
        assert!(!Blocklist::bundled().is_blocked("www.gutenberg.org"));
    }
}
