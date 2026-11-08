// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Palm Database container (the outer layer of MOBI/AZW/PRC/PDB files).
//!
//! Layout: 78-byte header (name, …, type/creator at 60, record count at 76),
//! then one 8-byte entry per record (offset, attributes/uid). Record `i`
//! spans from its offset to the next record's offset (or end of file).

use crate::{FormatError, Result};

pub struct PalmDb {
    pub data: Vec<u8>,
    /// `TEXtREAd`, `BOOKMOBI`, …
    pub kind: [u8; 8],
    ranges: Vec<(usize, usize)>,
}

impl PalmDb {
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let bad = |r: &str| FormatError::Invalid {
            format: "MOBI",
            reason: r.to_string(),
        };
        // STEP 1: header.
        if data.len() < 78 {
            return Err(bad("file too short"));
        }
        let mut kind = [0u8; 8];
        kind.copy_from_slice(&data[60..68]);
        let count = u16::from_be_bytes([data[76], data[77]]) as usize;
        if count == 0 || 78 + count * 8 > data.len() {
            return Err(bad("bad record table"));
        }
        // STEP 2: record offsets must be increasing and inside the file.
        let offsets: Vec<usize> = (0..count)
            .map(|i| u32::from_be_bytes(data[78 + i * 8..82 + i * 8].try_into().unwrap()) as usize)
            .collect();
        let mut ranges = Vec::with_capacity(count);
        for i in 0..count {
            let start = offsets[i];
            let end = offsets.get(i + 1).copied().unwrap_or(data.len());
            if start > end || end > data.len() {
                return Err(bad("record offsets out of range"));
            }
            ranges.push((start, end));
        }
        Ok(Self { data, kind, ranges })
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Record `i`, or an empty slice if it doesn't exist.
    pub fn record(&self, i: usize) -> &[u8] {
        self.ranges
            .get(i)
            .map(|&(s, e)| &self.data[s..e])
            .unwrap_or(&[])
    }
}

/// Big-endian readers that return 0 when out of bounds (damaged files).
pub fn be16(d: &[u8], at: usize) -> u32 {
    d.get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]) as u32)
        .unwrap_or(0)
}

pub fn be32(d: &[u8], at: usize) -> u32 {
    d.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0)
}
