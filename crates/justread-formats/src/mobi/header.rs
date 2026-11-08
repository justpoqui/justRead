// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Record 0 of a MOBI/KF8 section: PalmDOC header, MOBI header and EXTH.
//! Offsets below are from the start of record 0 (the MOBI header starts at 16).

use std::collections::HashMap;

use super::palmdb::{be16, be32};

/// "No index" marker used by MOBI headers.
pub const NONE: u32 = 0xFFFF_FFFF;

#[derive(Debug, Clone, Default)]
pub struct Header {
    pub compression: u32,
    pub text_length: u32,
    pub text_records: u32,
    pub encryption: u32,
    pub has_mobi: bool,
    pub encoding: u32,
    pub version: u32,
    pub full_name: Vec<u8>,
    pub first_image: u32,
    pub huff_offset: u32,
    pub huff_count: u32,
    pub fdst: u32,
    pub extra_flags: u32,
    pub ncx: u32,
    pub frag: u32,
    pub skel: u32,
    /// EXTH records: type → list of raw values.
    pub exth: HashMap<u32, Vec<Vec<u8>>>,
}

impl Header {
    pub fn parse(r0: &[u8]) -> Self {
        let mut h = Header {
            // STEP 1: PalmDOC header (16 bytes).
            compression: be16(r0, 0),
            text_length: be32(r0, 4),
            text_records: be16(r0, 8),
            encryption: be16(r0, 12),
            fdst: NONE,
            ncx: NONE,
            frag: NONE,
            skel: NONE,
            first_image: NONE,
            ..Default::default()
        };
        if r0.get(16..20) != Some(b"MOBI") {
            return h; // plain PalmDOC (TEXtREAd)
        }

        // STEP 2: MOBI header. Fields past the declared length are absent.
        h.has_mobi = true;
        let len = be32(r0, 20) as usize;
        let field = |at: usize| {
            if at + 4 <= 16 + len {
                be32(r0, at)
            } else {
                NONE
            }
        };
        h.encoding = be32(r0, 28);
        h.version = be32(r0, 36);
        let name_off = be32(r0, 84) as usize;
        let name_len = be32(r0, 88) as usize;
        h.full_name = r0
            .get(name_off..name_off + name_len)
            .unwrap_or(&[])
            .to_vec();
        h.first_image = field(108);
        h.huff_offset = field(112);
        h.huff_count = field(116);
        if h.version >= 8 {
            h.fdst = field(192);
        }
        h.extra_flags = if 16 + len >= 0xF4 { be16(r0, 0xF2) } else { 0 };
        h.ncx = field(0xF4);
        if h.version >= 8 {
            h.frag = field(0xF8);
            h.skel = field(0xFC);
        }

        // STEP 3: EXTH block right after the MOBI header (if flagged).
        let exth_flags = be32(r0, 128);
        let at = 16 + len;
        if exth_flags & 0x40 != 0 && r0.get(at..at + 4) == Some(b"EXTH") {
            let count = be32(r0, at + 8) as usize;
            let mut p = at + 12;
            for _ in 0..count.min(4096) {
                let kind = be32(r0, p);
                let size = be32(r0, p + 4) as usize;
                if size < 8 || p + size > r0.len() {
                    break;
                }
                h.exth
                    .entry(kind)
                    .or_default()
                    .push(r0[p + 8..p + size].to_vec());
                p += size;
            }
        }
        h
    }

    /// Decode bytes in the book's text encoding (65001 = UTF-8, else CP1252).
    pub fn decode(&self, b: &[u8]) -> String {
        if self.encoding == 65001 {
            String::from_utf8_lossy(b).into_owned()
        } else {
            encoding_rs::WINDOWS_1252
                .decode_without_bom_handling(b)
                .0
                .into_owned()
        }
    }

    /// EXTH values of a type as strings.
    pub fn exth_strings(&self, kind: u32) -> Vec<String> {
        self.exth
            .get(&kind)
            .map(|v| {
                v.iter()
                    .map(|b| self.decode(b).trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// EXTH value of a type as a big-endian integer.
    pub fn exth_u32(&self, kind: u32) -> Option<u32> {
        self.exth
            .get(&kind)?
            .first()
            .filter(|b| b.len() >= 4)
            .map(|b| be32(b, 0))
    }
}
