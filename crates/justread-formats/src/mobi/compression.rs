// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Text decompression for MOBI: PalmDOC LZ77 and Mobipocket HUFF/CDIC,
//! plus removal of the "trailing entries" appended to text records.
//! Format reference: MobileRead wiki, "MOBI" and "PalmDOC" pages.

use crate::{FormatError, Result};

/// Decompress one PalmDOC (LZ77-style) record.
pub fn palmdoc(input: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 2);
    let mut i = 0;
    while i < input.len() {
        let c = input[i];
        i += 1;
        match c {
            // STEP 1: 0x01–0x08 — copy the next c bytes literally.
            1..=8 => {
                let end = (i + c as usize).min(input.len());
                out.extend_from_slice(&input[i..end]);
                i = end;
            }
            // STEP 2: 0x00, 0x09–0x7F — the byte itself.
            0 | 9..=0x7F => out.push(c),
            // STEP 3: 0xC0–0xFF — a space followed by (c ^ 0x80).
            0xC0..=0xFF => {
                out.push(b' ');
                out.push(c ^ 0x80);
            }
            // STEP 4: 0x80–0xBF — back-reference: 11-bit distance, 3-bit length (+3).
            _ => {
                let Some(&c2) = input.get(i) else { break };
                i += 1;
                let pair = ((c as usize) << 8) | c2 as usize;
                let dist = (pair >> 3) & 0x7FF;
                let len = (pair & 7) + 3;
                if dist == 0 || dist > out.len() {
                    continue; // corrupt reference: skip it rather than panic
                }
                // Byte by byte: the source may overlap what we are writing.
                for _ in 0..len {
                    out.push(out[out.len() - dist]);
                }
            }
        }
    }
    out
}

/// Size of the trailing entries at the end of a text record, given the
/// "extra data flags" from the MOBI header.
pub fn trailing_size(data: &[u8], flags: u32) -> usize {
    // STEP 1: every flag bit above bit 0 adds one variable-size entry, whose
    // size is stored backwards at the end of the remaining data.
    let mut num = 0usize;
    let mut f = flags >> 1;
    while f != 0 {
        if f & 1 != 0 {
            num += backward_size(data, data.len().saturating_sub(num));
        }
        f >>= 1;
    }
    // STEP 2: bit 0 — multibyte character overlap: (last byte & 3) + 1 bytes.
    if flags & 1 != 0 {
        if let Some(&b) = data.len().checked_sub(num + 1).and_then(|i| data.get(i)) {
            num += (b & 3) as usize + 1;
        }
    }
    num.min(data.len())
}

/// Variable-width size read backwards from `data[..end]` (7 bits per byte;
/// the byte with the high bit set is the most significant / last read).
fn backward_size(data: &[u8], end: usize) -> usize {
    let mut result = 0usize;
    let mut shift = 0;
    let mut pos = end;
    while pos > 0 {
        let v = data[pos - 1];
        result |= ((v & 0x7F) as usize) << shift;
        shift += 7;
        pos -= 1;
        if v & 0x80 != 0 || shift >= 28 {
            break;
        }
    }
    result
}

/// Mobipocket "HUFF/CDIC" Huffman decompressor.
pub struct Huff {
    /// Per top byte: (code length, terminal?, max code).
    dict1: Vec<(u32, bool, u64)>,
    mincode: [u64; 33],
    maxcode: [u64; 33],
    /// Dictionary entries: (bytes, already fully expanded?).
    dict: Vec<(Vec<u8>, bool)>,
}

impl Huff {
    /// Load the HUFF record and its CDIC records.
    pub fn new(huff: &[u8], cdics: &[&[u8]]) -> Result<Self> {
        let bad = |r: &str| FormatError::Invalid {
            format: "MOBI",
            reason: format!("HUFF: {r}"),
        };
        use super::palmdb::{be16, be32};
        if huff.get(0..4) != Some(b"HUFF") {
            return Err(bad("missing header"));
        }
        // STEP 1: the 256-entry lookup table indexed by the code's top byte.
        let off1 = be32(huff, 8) as usize;
        let off2 = be32(huff, 12) as usize;
        let mut dict1 = Vec::with_capacity(256);
        for i in 0..256 {
            let v = be32(huff, off1 + i * 4);
            let len = v & 0x1F;
            let term = v & 0x80 != 0;
            let max = (v >> 8) as u64;
            if len == 0 {
                return Err(bad("zero code length"));
            }
            let max = if term {
                ((max + 1) << (32 - len)) - 1
            } else {
                max
            };
            dict1.push((len, term, max));
        }
        // STEP 2: canonical code ranges per code length (1..=32).
        let (mut mincode, mut maxcode) = ([0u64; 33], [0u64; 33]);
        for len in 1..=32usize {
            let lo = be32(huff, off2 + (len - 1) * 8) as u64;
            let hi = be32(huff, off2 + (len - 1) * 8 + 4) as u64;
            mincode[len] = lo << (32 - len);
            maxcode[len] = ((hi + 1) << (32 - len)).wrapping_sub(1);
        }
        // STEP 3: dictionary slices from the CDIC records.
        let mut dict = Vec::new();
        for c in cdics {
            if c.get(0..4) != Some(b"CDIC") {
                return Err(bad("missing CDIC"));
            }
            let total = be32(c, 8) as usize;
            let bits = be32(c, 12).min(24);
            let n = (1usize << bits).min(total.saturating_sub(dict.len()));
            for j in 0..n {
                let off = be16(c, 16 + j * 2) as usize;
                let blen = be16(c, 16 + off) as usize;
                let start = 16 + off + 2;
                let slice = c
                    .get(start..start + (blen & 0x7FFF))
                    .unwrap_or(&[])
                    .to_vec();
                dict.push((slice, blen & 0x8000 != 0));
            }
        }
        Ok(Self {
            dict1,
            mincode,
            maxcode,
            dict,
        })
    }

    /// Decompress one text record.
    pub fn unpack(&mut self, data: &[u8]) -> Vec<u8> {
        self.unpack_depth(data, 0)
    }

    fn unpack_depth(&mut self, data: &[u8], depth: usize) -> Vec<u8> {
        let mut out = Vec::new();
        if depth > 32 {
            return out; // malicious self-referencing dictionary
        }
        // STEP 1: a 64-bit window over the input, refilled 32 bits at a time.
        let mut bits_left = data.len() as i64 * 8;
        let mut buf = data.to_vec();
        buf.extend_from_slice(&[0u8; 8]);
        let read64 = |p: usize| u64::from_be_bytes(buf[p..p + 8].try_into().unwrap());
        let mut pos = 0usize;
        let mut x = read64(0);
        let mut n: i64 = 32;
        loop {
            if n <= 0 {
                pos += 4;
                if pos + 8 > buf.len() {
                    break;
                }
                x = read64(pos);
                n += 32;
            }
            // STEP 2: find the code length and the dictionary index.
            let code = (x >> n) & 0xFFFF_FFFF;
            let (mut len, term, mut max) = self.dict1[(code >> 24) as usize];
            if !term {
                while (len as usize) < 32 && code < self.mincode[len as usize] {
                    len += 1;
                }
                max = self.maxcode[len as usize];
            }
            n -= len as i64;
            bits_left -= len as i64;
            if bits_left < 0 {
                break;
            }
            let r = (max.wrapping_sub(code) >> (32 - len)) as usize;
            let Some((slice, expanded)) = self.dict.get(r).cloned() else {
                break;
            };
            // STEP 3: entries may themselves be compressed; expand once and memoize.
            let piece = if expanded {
                slice
            } else {
                let full = self.unpack_depth(&slice, depth + 1);
                self.dict[r] = (full.clone(), true);
                full
            };
            out.extend_from_slice(&piece);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palmdoc_cases() {
        // literals, a literal run, space+char, and an overlapping back-reference
        let input = [b'a', b'b', 2, 0x90, 0x91, 0xE1, 0x80, 0x2F];
        // 0x80,0x2F → dist = (0x802F >> 3) & 0x7FF = 5, len = 7 + 3 = 10
        let out = palmdoc(&input);
        assert_eq!(&out[..6], b"ab\x90\x91 a");
        assert_eq!(out.len(), 6 + 10);
        assert_eq!(out[6], out[1]); // copied from 5 bytes back
        assert_eq!(palmdoc(&[0x80]), b""); // truncated reference is ignored
    }

    #[test]
    fn trailing_entries() {
        // multibyte flag only: last byte & 3 = 1 → strip 2 bytes
        assert_eq!(trailing_size(b"hello\x00\x01", 1), 2);
        // one extra entry (bit 1): size 3 stored backwards as 0x83
        assert_eq!(trailing_size(b"textxx\x83", 2), 3);
        // both
        assert_eq!(trailing_size(b"text\x00xx\x83", 3), 4);
    }

    #[test]
    fn huffman_two_symbol_code() {
        // STEP 1: HUFF with every top byte mapping to a terminal 1-bit code.
        // maxcode_raw = 1 → bit '1' selects entry 0 ("A"), bit '0' selects entry 1 ("B").
        let mut huff = b"HUFF".to_vec();
        huff.extend_from_slice(&24u32.to_be_bytes());
        huff.extend_from_slice(&24u32.to_be_bytes()); // dict1 offset
        huff.extend_from_slice(&(24u32 + 1024).to_be_bytes()); // min/max table offset
        huff.extend_from_slice(&[0u8; 8]);
        for _ in 0..256 {
            huff.extend_from_slice(&((1u32 << 8) | 0x80 | 1).to_be_bytes());
        }
        huff.extend_from_slice(&[0u8; 256]);
        // STEP 2: one CDIC with two literal entries.
        let mut cdic = b"CDIC".to_vec();
        cdic.extend_from_slice(&16u32.to_be_bytes());
        cdic.extend_from_slice(&2u32.to_be_bytes());
        cdic.extend_from_slice(&1u32.to_be_bytes()); // 1 bit → 2 entries
        cdic.extend_from_slice(&4u16.to_be_bytes());
        cdic.extend_from_slice(&7u16.to_be_bytes());
        cdic.extend_from_slice(&(0x8000u16 | 1).to_be_bytes());
        cdic.push(b'A');
        cdic.extend_from_slice(&(0x8000u16 | 1).to_be_bytes());
        cdic.push(b'B');
        let mut h = Huff::new(&huff, &[&cdic]).unwrap();
        // STEP 3: bits 1,0,0,1,1,0,1,0 → A B B A A B A B
        assert_eq!(h.unpack(&[0b1001_1010]), b"ABBAABAB");
    }
}
