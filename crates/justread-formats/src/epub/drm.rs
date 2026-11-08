// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! DRM detection — *detection only*. justRead never attempts to remove DRM.
//!
//! Signals (see EPUB 3.3 §4.2.6.3 "encryption.xml" and vendor docs):
//! * `META-INF/rights.xml`           → Adobe ADEPT
//! * `META-INF/sinf.xml`             → Apple FairPlay
//! * `META-INF/license.lcpl`         → Readium LCP
//! * `META-INF/encryption.xml` listing any algorithm other than the two
//!   font-obfuscation algorithms → some form of encryption
//!
//! Font obfuscation (IDPF and Adobe) is *not* DRM: the spec defines it so
//! reading systems can open the fonts. Those entries are returned so the
//! fonts can be de-obfuscated (see `fonts.rs`).

use std::collections::HashMap;

use justbook::paths::{normalize_relative, percent_decode};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::archive::SafeArchive;
use super::opf::attr;
use crate::DrmStatus;

/// IDPF font obfuscation algorithm URI (EPUB 3.3 §4.4).
pub const IDPF_OBFUSCATION: &str = "http://www.idpf.org/2008/embedding";
/// Adobe font obfuscation algorithm URI.
pub const ADOBE_OBFUSCATION: &str = "http://ns.adobe.com/pdf/enc#RC";

/// Result of examining `META-INF`.
#[derive(Debug, Default)]
pub struct EncryptionInfo {
    /// `Some(scheme)` if the book is DRM-protected.
    pub drm: Option<String>,
    /// Obfuscated fonts: container path → algorithm URI.
    pub obfuscated: HashMap<String, String>,
}

impl EncryptionInfo {
    pub fn status(&self) -> DrmStatus {
        match &self.drm {
            Some(s) => DrmStatus::Protected(s.clone()),
            None => DrmStatus::None,
        }
    }
}

/// Inspect the archive for DRM and obfuscated fonts.
pub fn examine(archive: &mut SafeArchive) -> EncryptionInfo {
    let mut info = EncryptionInfo::default();

    // STEP 1: vendor marker files are conclusive on their own.
    for (file, scheme) in [
        ("META-INF/rights.xml", "Adobe ADEPT"),
        ("META-INF/sinf.xml", "Apple FairPlay"),
        ("META-INF/license.lcpl", "Readium LCP"),
    ] {
        if archive.has(file) {
            info.drm = Some(scheme.to_string());
            return info;
        }
    }

    // STEP 2: encryption.xml — classify each EncryptedData entry.
    let Ok(xml) = archive.read_text("META-INF/encryption.xml") else {
        return info;
    };
    let mut r = Reader::from_str(&xml);
    let mut algorithm: Option<String> = None;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                "EncryptedData" => algorithm = None,
                "EncryptionMethod" => algorithm = attr(&e, "Algorithm"),
                "CipherReference" => {
                    let uri = attr(&e, "URI").unwrap_or_default();
                    let alg = algorithm.clone().unwrap_or_default();
                    if alg == IDPF_OBFUSCATION || alg == ADOBE_OBFUSCATION {
                        // URIs are relative to the container root.
                        if let Ok(p) = normalize_relative(&percent_decode(&uri)) {
                            info.obfuscated.insert(p, alg);
                        }
                    } else {
                        info.drm = Some("encrypted content".to_string());
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    info
}
