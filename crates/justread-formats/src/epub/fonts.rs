// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Font de-obfuscation (not DRM — a spec-defined reversible XOR so fonts
//! can't be trivially extracted and reused outside the book).
//!
//! * IDPF (EPUB 3.3 §4.4, <https://www.w3.org/TR/epub-33/#sec-font-obfuscation>):
//!   key = SHA-1 of the unique identifier with whitespace removed;
//!   the first 1040 bytes are XORed with the 20-byte key, cyclically.
//! * Adobe: key = the 16 bytes of the `urn:uuid:` identifier; the first
//!   1024 bytes are XORed with it, cyclically.

use sha1::{Digest, Sha1};

use super::drm::{ADOBE_OBFUSCATION, IDPF_OBFUSCATION};

/// Undo obfuscation in place. Returns `false` if no key could be derived.
pub fn deobfuscate(
    data: &mut [u8],
    algorithm: &str,
    identifiers: &[String],
    unique: Option<&str>,
) -> bool {
    match algorithm {
        IDPF_OBFUSCATION => {
            // STEP 1: key from the unique identifier minus XML whitespace.
            let Some(uid) = unique else { return false };
            let cleaned: String = uid
                .chars()
                .filter(|c| !matches!(c, ' ' | '\t' | '\r' | '\n'))
                .collect();
            let key = Sha1::digest(cleaned.as_bytes());
            // STEP 2: XOR the header.
            xor(data, &key, 1040);
            true
        }
        ADOBE_OBFUSCATION => {
            // STEP 1: find a urn:uuid identifier (unique one first).
            let candidates = unique
                .into_iter()
                .chain(identifiers.iter().map(String::as_str));
            let Some(key) = candidates.filter_map(uuid_bytes).next() else {
                return false;
            };
            // STEP 2: XOR the header.
            xor(data, &key, 1024);
            true
        }
        _ => false,
    }
}

/// XOR the first `len` bytes of `data` with `key`, repeating the key.
fn xor(data: &mut [u8], key: &[u8], len: usize) {
    for (i, b) in data.iter_mut().take(len).enumerate() {
        *b ^= key[i % key.len()];
    }
}

/// Parse `urn:uuid:xxxxxxxx-xxxx-...` (or a bare UUID) into 16 bytes.
fn uuid_bytes(id: &str) -> Option<Vec<u8>> {
    let hex: String = id
        .trim()
        .trim_start_matches("urn:uuid:")
        .chars()
        .filter(|c| *c != '-')
        .collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..16)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idpf_round_trip() {
        // Obfuscation is its own inverse: applying it twice restores the data.
        let original: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        let mut data = original.clone();
        let uid = Some(" urn:uuid:abc \n");
        assert!(deobfuscate(&mut data, IDPF_OBFUSCATION, &[], uid));
        assert_ne!(data[..1040], original[..1040]);
        assert_eq!(data[1040..], original[1040..]); // only the header changes
        assert!(deobfuscate(
            &mut data,
            IDPF_OBFUSCATION,
            &[],
            Some("urn:uuid:abc")
        ));
        assert_eq!(data, original);
    }

    #[test]
    fn adobe_needs_uuid() {
        let mut data = vec![0u8; 1100];
        assert!(!deobfuscate(
            &mut data,
            ADOBE_OBFUSCATION,
            &["isbn:123".into()],
            None
        ));
        let ids = vec!["urn:uuid:00112233-4455-6677-8899-aabbccddeeff".to_string()];
        assert!(deobfuscate(&mut data, ADOBE_OBFUSCATION, &ids, None));
        assert_eq!(&data[..3], &[0x00, 0x11, 0x22]);
        assert_eq!(data[1024], 0);
    }
}
