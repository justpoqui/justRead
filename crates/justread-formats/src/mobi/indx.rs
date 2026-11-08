// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! MOBI/KF8 index records (`INDX` + `TAGX` + `IDXT` + `CNCX`).
//!
//! Indexes hold the NCX (table of contents) and, in KF8, the skeleton and
//! fragment tables needed to rebuild the HTML files. Each entry is a name
//! plus tag → values, where the TAGX table says which tags an entry has
//! (via "control bytes") and how many values each tag carries.

use std::collections::HashMap;

use super::palmdb::{be32, PalmDb};

#[derive(Debug, Clone, Default)]
pub struct IndexEntry {
    pub name: Vec<u8>,
    pub tags: HashMap<u8, Vec<u32>>,
}

impl IndexEntry {
    pub fn tag(&self, t: u8, i: usize) -> Option<u32> {
        self.tags.get(&t).and_then(|v| v.get(i)).copied()
    }
    pub fn name_str(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }
}

/// Forward variable-width integer: 7 bits per byte, last byte has bit 7 set.
pub fn varint(d: &[u8], pos: &mut usize) -> u32 {
    let mut v: u32 = 0;
    while let Some(&b) = d.get(*pos) {
        *pos += 1;
        v = (v << 7) | (b & 0x7F) as u32;
        if b & 0x80 != 0 {
            break;
        }
    }
    v
}

/// Read the index whose header is record `first` (absolute record number).
/// Returns entries and the CNCX string table (offset → text).
pub fn read_index(db: &PalmDb, first: usize) -> (Vec<IndexEntry>, HashMap<u32, String>) {
    let head = db.record(first);
    if head.get(0..4) != Some(b"INDX") {
        return (vec![], HashMap::new());
    }

    // STEP 1: main header — data record count, CNCX count, TAGX table.
    let header_len = be32(head, 4) as usize;
    let data_records = be32(head, 0x18) as usize;
    let cncx_records = be32(head, 0x34) as usize;
    let tagx = parse_tagx(head.get(header_len..).unwrap_or(&[]));

    // STEP 2: entries from each data record.
    let mut entries = Vec::new();
    for r in 1..=data_records.min(1000) {
        let rec = db.record(first + r);
        if rec.get(0..4) != Some(b"INDX") {
            break;
        }
        let idxt = be32(rec, 0x14) as usize;
        let count = be32(rec, 0x18) as usize;
        if rec.get(idxt..idxt + 4) != Some(b"IDXT") {
            continue;
        }
        let offsets: Vec<usize> = (0..count)
            .map(|j| super::palmdb::be16(rec, idxt + 4 + j * 2) as usize)
            .collect();
        for (j, &start) in offsets.iter().enumerate() {
            let end = offsets.get(j + 1).copied().unwrap_or(idxt);
            if let Some(entry) = rec
                .get(start..end.max(start))
                .and_then(|d| parse_entry(d, &tagx))
            {
                entries.push(entry);
            }
        }
    }

    // STEP 3: CNCX records — length-prefixed strings addressed by global offset.
    let mut cncx = HashMap::new();
    for k in 0..cncx_records.min(100) {
        let rec = db.record(first + data_records + 1 + k);
        let mut pos = 0;
        while pos < rec.len() {
            let start = pos;
            let len = varint(rec, &mut pos) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            cncx.insert(
                (k as u32) << 16 | start as u32,
                String::from_utf8_lossy(&rec[pos..pos + len]).into_owned(),
            );
            pos += len;
        }
    }
    (entries, cncx)
}

/// (tag, values per entry, mask, end-of-control-byte flag)
type Tagx = (Vec<(u8, u8, u8, u8)>, usize);

fn parse_tagx(d: &[u8]) -> Tagx {
    if d.get(0..4) != Some(b"TAGX") {
        return (vec![], 0);
    }
    let len = be32(d, 4) as usize;
    let control_bytes = be32(d, 8) as usize;
    let mut tags = Vec::new();
    let mut p = 12;
    while p + 4 <= len.min(d.len()) {
        tags.push((d[p], d[p + 1], d[p + 2], d[p + 3]));
        p += 4;
    }
    (tags, control_bytes)
}

fn parse_entry(d: &[u8], (tagx, control_count): &Tagx) -> Option<IndexEntry> {
    // STEP 1: name.
    let name_len = *d.first()? as usize;
    let name = d.get(1..1 + name_len)?.to_vec();
    let mut pos = 1 + name_len;
    let control = d.get(pos..pos + control_count)?.to_vec();
    pos += control_count;

    // STEP 2: which tags are present, and how their value counts are given.
    enum Count {
        Entries(u32),
        Bytes(u32),
    }
    let mut present = Vec::new();
    let mut cb = 0usize;
    for &(tag, per_entry, mask, end) in tagx {
        if end == 1 {
            cb += 1;
            continue;
        }
        let value = control.get(cb).copied().unwrap_or(0) & mask;
        if value == 0 {
            continue;
        }
        if value == mask && mask.count_ones() > 1 {
            // The value count is too big for the mask: a byte count follows in the data.
            present.push((tag, per_entry, Count::Bytes(varint(d, &mut pos))));
        } else {
            present.push((
                tag,
                per_entry,
                Count::Entries((value >> mask.trailing_zeros()) as u32),
            ));
        }
    }

    // STEP 3: read the values.
    let mut tags = HashMap::new();
    for (tag, per_entry, count) in present {
        let mut values = Vec::new();
        match count {
            Count::Entries(n) => {
                for _ in 0..(n as usize * per_entry as usize).min(10_000) {
                    values.push(varint(d, &mut pos));
                }
            }
            Count::Bytes(total) => {
                let stop = pos + total as usize;
                while pos < stop && pos < d.len() {
                    values.push(varint(d, &mut pos));
                }
            }
        }
        tags.insert(tag, values);
    }
    Some(IndexEntry { name, tags })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints() {
        let d = [0x81, 0x01, 0x82, 0x7F, 0xFF];
        let mut p = 0;
        assert_eq!(varint(&d, &mut p), 1);
        assert_eq!(varint(&d, &mut p), (1 << 7) | 2);
        assert_eq!(varint(&d, &mut p), (0x7F << 7) | 0x7F);
    }
}
