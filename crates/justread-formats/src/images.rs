// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Small helpers for page images: intrinsic size from file headers (so the
//! reader can lay out pages before any image is decoded) and natural sorting
//! of page file names.

use std::cmp::Ordering;

/// Image types accepted as comic pages, by lower-case extension.
pub const PAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "avif", "bmp"];

/// Lower-case extension of a path, if any.
pub fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

/// Width and height read from the image header, without decoding pixels.
/// Supports PNG, GIF, JPEG and WebP; returns `None` for anything else.
pub fn dimensions(b: &[u8]) -> Option<(u32, u32)> {
    let be16 =
        |i: usize| -> Option<u32> { Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32) };
    let le16 =
        |i: usize| -> Option<u32> { Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32) };
    let be32 =
        |i: usize| -> Option<u32> { Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?)) };

    // STEP 1: PNG — width/height are the first fields of the IHDR chunk.
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?));
    }
    // STEP 2: GIF — logical screen size, little-endian.
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Some((le16(6)?, le16(8)?));
    }
    // STEP 3: WebP — three container variants.
    if b.get(0..4) == Some(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        return match b.get(12..16)? {
            b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            b"VP8L" => {
                let v = u32::from_le_bytes(b.get(21..25)?.try_into().ok()?);
                Some(((v & 0x3fff) + 1, ((v >> 14) & 0x3fff) + 1))
            }
            b"VP8X" => {
                let w = u32::from_le_bytes([*b.get(24)?, *b.get(25)?, *b.get(26)?, 0]) + 1;
                let h = u32::from_le_bytes([*b.get(27)?, *b.get(28)?, *b.get(29)?, 0]) + 1;
                Some((w, h))
            }
            _ => None,
        };
    }
    // STEP 4: JPEG — walk the segments until a start-of-frame marker.
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            // SOF0..SOF15 except DHT (C4), JPG (C8) and DAC (CC) carry the frame size.
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                return Some((be16(i + 7)?, be16(i + 5)?));
            }
            // Markers without a length field.
            if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) || marker == 0xFF
            {
                i += if marker == 0xFF { 1 } else { 2 };
                continue;
            }
            i += 2 + be16(i + 2)? as usize;
        }
    }
    None
}

/// "Natural" order: `page2` before `page10`, case-insensitive.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b), // equal ignoring case: fall back to exact order
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                // STEP 1: compare whole digit runs as numbers (leading zeros ignored).
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut s = String::new();
                    while let Some(ch) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(ch);
                        it.next();
                    }
                    s
                };
                let (n, m) = (take(&mut x), take(&mut y));
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let ord = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                // STEP 2: everything else compares case-insensitively.
                let ord = c.to_lowercase().cmp(d.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec![
            "p10.jpg", "p2.jpg", "P1.jpg", "p02b.jpg", "a/p3.jpg", "p002.jpg",
        ];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            vec!["a/p3.jpg", "P1.jpg", "p002.jpg", "p2.jpg", "p02b.jpg", "p10.jpg"]
        );
    }

    #[test]
    fn header_sizes() {
        // PNG: signature + IHDR (len, type, w=300, h=450)
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&300u32.to_be_bytes());
        png.extend_from_slice(&450u32.to_be_bytes());
        assert_eq!(dimensions(&png), Some((300, 450)));
        // GIF
        let gif = [b'G', b'I', b'F', b'8', b'9', b'a', 0x2C, 0x01, 0xC2, 0x01];
        assert_eq!(dimensions(&gif), Some((300, 450)));
        // JPEG: SOI, APP0 (len 4), SOF0 with h=450 w=300
        let jpg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0xC2,
            0x01, 0x2C, 0x03,
        ];
        assert_eq!(dimensions(&jpg), Some((300, 450)));
        assert_eq!(dimensions(b"not an image"), None);
        assert_eq!(extension("dir/Page.JPG").as_deref(), Some("jpg"));
    }
}
