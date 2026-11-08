// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! Developer tool: import books into a library directory from the command line.
//!
//! ```text
//! cargo run -p justread-library --example import -- <data-dir> <file>...
//! ```
//! Prints one JSON line per file. Useful for preparing test libraries.

use std::path::PathBuf;

use justread_formats::Registry;
use justread_library::{ImportOutcome, Library};

fn main() {
    // STEP 1: arguments.
    let mut args = std::env::args().skip(1);
    let Some(data_dir) = args.next() else {
        eprintln!("usage: import <data-dir> <file>...");
        std::process::exit(2);
    };

    // STEP 2: open the library and import each file.
    let mut lib = Library::open(&PathBuf::from(data_dir)).expect("open library");
    let registry = Registry::with_defaults();
    for file in args {
        match lib.import(&PathBuf::from(&file), &registry) {
            Ok(ImportOutcome::Imported { book, warnings }) => {
                println!(
                    "{{\"file\":{file:?},\"id\":{:?},\"title\":{:?},\"warnings\":{}}}",
                    book.id,
                    book.title,
                    warnings.len()
                )
            }
            Ok(ImportOutcome::Duplicate { book }) => {
                println!("{{\"file\":{file:?},\"duplicate\":{:?}}}", book.id)
            }
            Err(e) => println!("{{\"file\":{file:?},\"error\":{:?}}}", e.to_string()),
        }
    }
}
