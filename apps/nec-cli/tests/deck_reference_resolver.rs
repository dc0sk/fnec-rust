// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! A deck reference `(tag, segment)` becomes a segment in one place:
//! `nec_solver::find_deck_segment`.
//!
//! Nineteen sites spelled the lookup inline — `s.tag == t && s.tag_index == n` —
//! across the solver, the worker, the CLI, the GUI and fnec_py. A change to what
//! a reference may name (FND-135 renumbered GM copies; free-end refinement adds
//! segments no card may address, FND-227) then had nineteen places to be made.
//! This scan refuses a new inline copy anywhere in the product's sources, test
//! modules included, so the next one is caught at the gate rather than in a
//! wrong answer.

use std::path::{Path, PathBuf};

/// An equality test on `tag_index` — the inline lookup. Range tests (`>=`, `<=`,
/// the `LD` segment range) are not lookups and are not matched.
fn inline_lookups(src: &str) -> Vec<usize> {
    let is_lookup = |line: &str| {
        let code: String = line
            .split("//")
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        // `x.tag_index == n`, or `n == x.tag_index` on the right-hand side.
        code.contains("tag_index==")
            || code.split("==").skip(1).any(|rhs| {
                let operand: String = rhs
                    .trim_start_matches('&')
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                    .collect();
                operand.ends_with("tag_index")
            })
    };
    src.lines()
        .enumerate()
        .filter(|(_, l)| is_lookup(l))
        .map(|(i, _)| i + 1)
        .collect()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read source dir") {
        let p = entry.expect("dir entry").path();
        if p.is_dir() {
            rust_sources(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// The scan must see what it is looking for: a planted copy in either spelling
/// is found, a range test and a comment are not.
#[test]
fn the_scan_finds_an_inline_lookup() {
    let planted = "let i = segs.iter().position(|s| s.tag == t && s.tag_index == n);\n\
                   let j = segs.iter().position(|s| n == s.tag_index);\n\
                   let ok = s.tag_index >= first && s.tag_index <= last;\n\
                   // s.tag_index == n in a comment\n";
    assert_eq!(inline_lookups(planted), vec![1, 2]);
}

#[test]
fn every_deck_reference_goes_through_find_deck_segment() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in ["crates", "apps", "bindings"] {
        let d = root.join(dir);
        let mut all = Vec::new();
        rust_sources(&d, &mut all);
        // `src/` trees only: integration tests and examples are not the product.
        files.extend(
            all.into_iter()
                .filter(|p| p.components().any(|c| c.as_os_str() == "src")),
        );
    }
    assert!(
        files.len() > 50,
        "scanned only {} files — the walk is broken",
        files.len()
    );
    let resolver = root.join("crates/nec_solver/src/geometry.rs");
    let mut found = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).expect("read source");
        let mut lines = inline_lookups(&src);
        if f.canonicalize().ok() == resolver.canonicalize().ok() {
            // The resolver's own comparison is the one allowed.
            let at = src
                .lines()
                .position(|l| l.contains("pub fn find_deck_segment("))
                .expect("find_deck_segment is in geometry.rs")
                + 1;
            lines.retain(|&l| !(at..at + 8).contains(&l));
        }
        found.extend(lines.into_iter().map(|l| format!("{}:{l}", f.display())));
    }
    assert!(
        found.is_empty(),
        "inline (tag, segment) lookups — use nec_solver::find_deck_segment:\n{}",
        found.join("\n")
    );
}
