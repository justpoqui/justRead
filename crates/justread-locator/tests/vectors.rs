// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Runs the shared locator vectors (also used by the TypeScript tests).

use justread_locator::Locator;
use serde_json::Value;
use std::cmp::Ordering;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/locators.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn valid_vectors_round_trip() {
    for v in vectors()["valid"].as_array().unwrap() {
        // STEP 1: parse, STEP 2: print again — must be byte-identical (canonical form).
        let text = v["text"].as_str().unwrap();
        let loc: Locator = text.parse().unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(loc.to_string(), text);
        // STEP 3: check the decoded fields.
        match loc {
            Locator::Flow(f) => {
                assert_eq!(v["kind"], "flow");
                assert_eq!(f.spine as u64, v["spine"].as_u64().unwrap());
                assert_eq!(f.block as u64, v["block"].as_u64().unwrap());
                assert_eq!(f.offset as u64, v["offset"].as_u64().unwrap());
            }
            Locator::Fixed(p) => {
                assert_eq!(v["kind"], "fixed");
                assert_eq!(p.page as u64, v["page"].as_u64().unwrap());
                assert_eq!(p.offset, v["offset"].as_f64().unwrap());
            }
        }
    }
}

#[test]
fn invalid_vectors_are_rejected() {
    for v in vectors()["invalid"].as_array().unwrap() {
        let text = v.as_str().unwrap();
        assert!(text.parse::<Locator>().is_err(), "should reject {text:?}");
    }
}

#[test]
fn order_vector_is_sorted() {
    let list: Vec<Locator> = vectors()["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().parse().unwrap())
        .collect();
    for pair in list.windows(2) {
        assert_eq!(
            pair[0].compare(&pair[1]),
            Some(Ordering::Less),
            "{} < {}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn serde_uses_text_form() {
    let loc: Locator = "f:1/2:3".parse().unwrap();
    assert_eq!(serde_json::to_string(&loc).unwrap(), "\"f:1/2:3\"");
    let back: Locator = serde_json::from_str("\"f:1/2:3\"").unwrap();
    assert_eq!(back, loc);
}
