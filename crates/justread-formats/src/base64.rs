// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Minimal standard base64 decoder (RFC 4648 §4) for embedded images in
//! FB2 files. Whitespace is ignored; decoding stops at the first padding or
//! invalid character, so damaged data yields as much as can be recovered.

pub fn decode(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in input.bytes() {
        // STEP 1: map one character to its 6-bit value.
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62, // also accept the URL-safe alphabet
            b'/' | b'_' => 63,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => break, // '=' padding or garbage ends the data
        } as u32;
        // STEP 2: emit a byte whenever 8 bits are available.
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes() {
        assert_eq!(super::decode("aGVsbG8="), b"hello");
        assert_eq!(super::decode("aGVs\n bG8gd29y\tbGQ="), b"hello world");
        assert_eq!(super::decode(""), b"");
        assert_eq!(super::decode("YQ"), b"a");
    }
}
