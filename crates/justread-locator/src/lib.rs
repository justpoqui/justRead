// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! # Locators
//!
//! A locator is a reading position that survives any change of layout
//! (single page ↔ spread ↔ scroll), font, size or margin, because it refers
//! to the *content*, never to pixels or page numbers.
//!
//! Two kinds exist (spec: `docs/LOCATOR_SPEC.md`):
//!
//! | Kind  | Text form        | Meaning                                              |
//! |-------|------------------|------------------------------------------------------|
//! | Flow  | `f:3/127:42`     | spine item 3, block `data-jrb="127"`, UTF-16 offset 42 in its `textContent` |
//! | Fixed | `p:12@0.25`      | page 12 (0-based), 25 % down the page (Phase 3)      |
//!
//! The flow form is inspired by EPUB CFI (<https://idpf.org/epub/linking/cfi/>)
//! but much simpler: because every chapter is normalized on import and every
//! block carries a stable id, we need no element-path walking. The same
//! grammar is implemented in `ui/src/lib/reader/locator.ts`; both are tested
//! against `tests/fixtures/locators.json`.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Position inside reflowable (justBook) content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FlowLocator {
    /// Index into `manifest.spine`.
    pub spine: u32,
    /// Value of the `data-jrb` attribute of the innermost enclosing block.
    pub block: u32,
    /// UTF-16 code-unit offset into that block's `textContent`.
    pub offset: u32,
}

/// Position inside fixed-layout content (PDF, comics).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedLocator {
    /// 0-based page index.
    pub page: u32,
    /// Vertical position on the page, 0.0 (top) ..= 1.0 (bottom).
    pub offset: f64,
}

/// Any reading position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Locator {
    Flow(FlowLocator),
    Fixed(FixedLocator),
}

/// Parse failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LocatorError {
    #[error("unknown locator kind in '{0}'")]
    UnknownKind(String),
    #[error("malformed locator '{0}'")]
    Malformed(String),
}

// ---------------------------------------------------------------------------
// Text form
// ---------------------------------------------------------------------------

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Locator::Flow(l) => write!(f, "f:{}/{}:{}", l.spine, l.block, l.offset),
            // Four decimals is sub-pixel on any real screen and keeps strings short.
            Locator::Fixed(l) => write!(f, "p:{}@{}", l.page, format_fraction(l.offset)),
        }
    }
}

/// Format a 0..1 fraction with up to 4 decimals and no trailing zeros.
fn format_fraction(v: f64) -> String {
    let s = format!("{:.4}", v.clamp(0.0, 1.0));
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

impl FromStr for Locator {
    type Err = LocatorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || LocatorError::Malformed(s.to_string());

        // STEP 1: split off the kind prefix.
        let (kind, rest) = s.split_once(':').ok_or_else(bad)?;

        match kind {
            // STEP 2a: flow — "spine/block:offset", all unsigned integers.
            "f" => {
                let (spine, rest) = rest.split_once('/').ok_or_else(bad)?;
                let (block, offset) = rest.split_once(':').ok_or_else(bad)?;
                Ok(Locator::Flow(FlowLocator {
                    spine: parse_u32(spine).ok_or_else(bad)?,
                    block: parse_u32(block).ok_or_else(bad)?,
                    offset: parse_u32(offset).ok_or_else(bad)?,
                }))
            }
            // STEP 2b: fixed — "page@fraction", fraction within 0..=1.
            "p" => {
                let (page, frac) = rest.split_once('@').ok_or_else(bad)?;
                let offset: f64 = frac.parse().map_err(|_| bad())?;
                if !(0.0..=1.0).contains(&offset) || frac.starts_with('+') {
                    return Err(bad());
                }
                Ok(Locator::Fixed(FixedLocator {
                    page: parse_u32(page).ok_or_else(bad)?,
                    offset,
                }))
            }
            _ => Err(LocatorError::UnknownKind(s.to_string())),
        }
    }
}

/// Strict unsigned parse: digits only (no sign, no whitespace), canonical form.
fn parse_u32(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0'))
    {
        return None;
    }
    s.parse().ok()
}

// Locators travel to the UI and the database as their text form.
impl Serialize for Locator {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Locator {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

impl Locator {
    /// Reading-order comparison. Returns `None` when comparing a flow
    /// locator with a fixed one (they never belong to the same book).
    ///
    /// Note: blocks are numbered in document (pre-)order, so text that sits
    /// in an outer block *after* a nested inner block compares as earlier
    /// than the inner block. This only affects text-after-nested-block
    /// edge cases and is acceptable for "furthest read" and sorting.
    pub fn compare(&self, other: &Locator) -> Option<Ordering> {
        match (self, other) {
            (Locator::Flow(a), Locator::Flow(b)) => Some(a.cmp(b)),
            (Locator::Fixed(a), Locator::Fixed(b)) => {
                Some(a.page.cmp(&b.page).then(a.offset.total_cmp(&b.offset)))
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

/// Size of one spine item, as recorded in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpineStats {
    pub chars: u64,
    pub blocks: u32,
}

/// Estimate whole-book progression (0.0 ..= 1.0) of a flow locator.
///
/// Chapters are weighted by their text length; inside a chapter we
/// interpolate by block index, which is a good approximation without
/// needing per-block sizes.
pub fn progression(spine: &[SpineStats], loc: &FlowLocator) -> f64 {
    // STEP 1: total text; an empty book is "at the start".
    let total: u64 = spine.iter().map(|s| s.chars).sum();
    if total == 0 || spine.is_empty() {
        return 0.0;
    }

    // STEP 2: everything before the current chapter has been read.
    let idx = (loc.spine as usize).min(spine.len() - 1);
    let before: u64 = spine[..idx].iter().map(|s| s.chars).sum();

    // STEP 3: fraction of the current chapter, by block position.
    let here = spine[idx];
    let frac = if here.blocks <= 1 {
        0.0
    } else {
        loc.block as f64 / here.blocks as f64
    };
    let within = here.chars as f64 * frac.clamp(0.0, 1.0);

    ((before as f64 + within) / total as f64).clamp(0.0, 1.0)
}

/// Inverse of [`progression`]: an approximate locator for a fraction of the
/// book (used when the user drags the progress slider).
pub fn locate_progression(spine: &[SpineStats], fraction: f64) -> FlowLocator {
    // STEP 1: target position in characters.
    let total: u64 = spine.iter().map(|s| s.chars).sum();
    let target = (fraction.clamp(0.0, 1.0) * total as f64) as u64;

    // STEP 2: walk chapters until the target falls inside one.
    let mut acc = 0u64;
    for (i, s) in spine.iter().enumerate() {
        if target < acc + s.chars || i == spine.len() - 1 {
            // STEP 3: interpolate a block index inside that chapter.
            let into = target.saturating_sub(acc) as f64 / s.chars.max(1) as f64;
            let block = ((into * s.blocks as f64) as u32).min(s.blocks.saturating_sub(1));
            return FlowLocator {
                spine: i as u32,
                block,
                offset: 0,
            };
        }
        acc += s.chars;
    }
    FlowLocator {
        spine: 0,
        block: 0,
        offset: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(spine: u32, block: u32, offset: u32) -> Locator {
        Locator::Flow(FlowLocator {
            spine,
            block,
            offset,
        })
    }

    #[test]
    fn formats_and_parses() {
        assert_eq!(flow(3, 127, 42).to_string(), "f:3/127:42");
        assert_eq!("f:3/127:42".parse::<Locator>().unwrap(), flow(3, 127, 42));
        let fixed = Locator::Fixed(FixedLocator {
            page: 12,
            offset: 0.25,
        });
        assert_eq!(fixed.to_string(), "p:12@0.25");
        assert_eq!("p:12@0.25".parse::<Locator>().unwrap(), fixed);
        assert_eq!(
            Locator::Fixed(FixedLocator {
                page: 0,
                offset: 0.0
            })
            .to_string(),
            "p:0@0"
        );
        assert_eq!(
            Locator::Fixed(FixedLocator {
                page: 0,
                offset: 1.0
            })
            .to_string(),
            "p:0@1"
        );
    }

    #[test]
    fn ordering() {
        assert_eq!(flow(1, 5, 0).compare(&flow(2, 0, 0)), Some(Ordering::Less));
        assert_eq!(
            flow(1, 5, 9).compare(&flow(1, 5, 3)),
            Some(Ordering::Greater)
        );
        assert_eq!(flow(1, 5, 3).compare(&flow(1, 5, 3)), Some(Ordering::Equal));
        let p = Locator::Fixed(FixedLocator {
            page: 1,
            offset: 0.5,
        });
        assert_eq!(p.compare(&flow(0, 0, 0)), None);
    }

    #[test]
    fn progression_is_weighted_by_chapter_size() {
        let spine = [
            SpineStats {
                chars: 100,
                blocks: 10,
            },
            SpineStats {
                chars: 300,
                blocks: 4,
            },
        ];
        let at = |s, b| {
            progression(
                &spine,
                &FlowLocator {
                    spine: s,
                    block: b,
                    offset: 0,
                },
            )
        };
        assert_eq!(at(0, 0), 0.0);
        assert_eq!(at(0, 5), 50.0 / 400.0);
        assert_eq!(at(1, 0), 0.25);
        assert_eq!(at(1, 2), 250.0 / 400.0);
        assert_eq!(at(9, 99), 1.0); // out of range clamps
        assert_eq!(
            progression(
                &[],
                &FlowLocator {
                    spine: 0,
                    block: 0,
                    offset: 0
                }
            ),
            0.0
        );
    }

    #[test]
    fn locate_progression_inverts_progression() {
        let spine = [
            SpineStats {
                chars: 100,
                blocks: 10,
            },
            SpineStats {
                chars: 300,
                blocks: 4,
            },
        ];
        assert_eq!(
            locate_progression(&spine, 0.0),
            FlowLocator {
                spine: 0,
                block: 0,
                offset: 0
            }
        );
        assert_eq!(
            locate_progression(&spine, 0.125),
            FlowLocator {
                spine: 0,
                block: 5,
                offset: 0
            }
        );
        assert_eq!(
            locate_progression(&spine, 0.625),
            FlowLocator {
                spine: 1,
                block: 2,
                offset: 0
            }
        );
        assert_eq!(
            locate_progression(&spine, 1.0),
            FlowLocator {
                spine: 1,
                block: 3,
                offset: 0
            }
        );
    }
}
