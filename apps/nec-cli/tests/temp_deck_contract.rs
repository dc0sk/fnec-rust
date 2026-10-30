// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-319 — `common::TempDeck` owns the uniqueness of its paths.
//!
//! Tests in one binary run in parallel. `loads_config_validation`'s scratch
//! names were keyed on `SystemTime::now()` nanoseconds; two reads returned the
//! same value, one test's load file replaced the other's, and a test that must
//! see a refusal solved a valid load instead (CI, PR #650). The guard now adds
//! the pid and a per-process counter itself and never replaces a file, so a
//! caller's name need not be unique. Its own file, not `common/mod.rs`: a test
//! there would run in every binary that includes the module.

use std::fs;

mod common;

use common::TempDeck;

#[test]
fn two_guards_with_one_name_are_two_files() {
    let a = TempDeck::new("same.nec", "a");
    let b = TempDeck::new("same.nec", "b");
    assert_ne!(a.path(), b.path());
    assert!(
        a.path().to_string_lossy().ends_with("same.nec"),
        "{}",
        a.path().display()
    );
    // The assertion that sees an overwrite: each guard reads its own body.
    assert_eq!(fs::read_to_string(&a).expect("read a"), "a");
    assert_eq!(fs::read_to_string(&b).expect("read b"), "b");
    let b_path = b.path().to_path_buf();
    drop(b);
    assert!(!b_path.exists(), "{}", b_path.display());
    assert_eq!(fs::read_to_string(&a).expect("a survives b's drop"), "a");
}

#[test]
fn a_guard_refuses_to_replace_an_existing_file() {
    let p = std::env::temp_dir().join(format!("fnec-t{}-stale-contract.nec", std::process::id()));
    fs::write(&p, "stale").expect("plant the stale file");
    let err = TempDeck::try_at(p.clone(), "new")
        .err()
        .expect("an existing file is refused");
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
    let kept = fs::read_to_string(&p).expect("read the stale file");
    fs::remove_file(&p).expect("remove the stale file");
    assert_eq!(kept, "stale");
}
