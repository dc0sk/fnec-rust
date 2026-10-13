// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-215 — every GPU test decides "skip or fail" and "re-run or fail" through
//! `nec_accel::gpu_test`, never by calling the adapter probe ([`PROBE`]) itself.
//!
//! The class had three instances fixed one at a time (#519, #536, #538) before
//! five more were found: each test that asked for the adapter itself either
//! failed outright on a single driver fault (FND-190) or skipped whenever the
//! driver hid the adapter, passing having run nothing (FND-163). This test bans
//! the call from every workspace member's `tests/` directory.
//!
//! What it cannot see: a test that `.expect()`s a GPU result without consulting
//! presence at all. That one fails on a driver fault and is not caught here.

use std::path::{Path, PathBuf};

/// The probe a test may not call. Spelled in two halves so this file does not
/// match itself.
const PROBE: &str = concat!("hardware_adapter", "_present");

/// The one file that may call it: adapter enumeration under concurrency is its
/// subject, not a precondition.
const EXEMPT: &str = "crates/nec_accel/tests/gpu_concurrent_init.rs";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The workspace members, read from the root manifest, so a new member is
/// scanned without editing this test.
fn members() -> Vec<String> {
    let manifest = std::fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");
    let start = manifest.find("members = [").expect("a members list");
    let end = start + manifest[start..].find(']').expect("closed members list");
    manifest[start..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_test_asks_for_the_adapter_itself() {
    let members = members();
    assert!(members.len() >= 9, "the members list parsed to {members:?}");
    let mut files = Vec::new();
    for m in &members {
        rust_files(&root().join(m).join("tests"), &mut files);
    }
    // An empty or mis-rooted scan would pass vacuously.
    assert!(
        files.len() >= 100,
        "only {} test files found under {members:?}",
        files.len()
    );
    let exempt = root().join(EXEMPT);
    assert!(
        files.iter().any(|f| f == &exempt),
        "{EXEMPT} was not among the scanned files"
    );
    let offenders: Vec<String> = files
        .iter()
        .filter(|f| *f != &exempt)
        .filter(|f| {
            std::fs::read_to_string(f)
                .expect("read test file")
                .contains(PROBE)
        })
        .map(|f| f.strip_prefix(root()).unwrap_or(f).display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "these tests call {PROBE} themselves; use \
         nec_accel::gpu_test::{{gpu_expected, gpu_attempt}} (FND-215): {offenders:?}"
    );
}
