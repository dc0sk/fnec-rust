// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Integration tests for EP-4 DeckValidator CLI integration (PH4-CHK-005).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_temp_deck(prefix: &str, body: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-{prefix}-{now}.nec"));
    fs::write(&path, body).expect("failed to write temporary deck");
    path
}

const DECK_NO_EX: &str = "\
CM Deck without EX card
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
FR 0 1 0 0 14.0 0.0
EN
";

const DECK_WITH_EX: &str = "\
CM Normal dipole
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.0 0.0
EN
";

#[test]
fn validator_warning_emitted_for_deck_without_ex_card() {
    let path = write_temp_deck("no-ex", DECK_NO_EX);
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("failed to run fnec");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[validator]"),
        "expected [validator] tag in stderr, got: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("no ex card")
            || stderr.to_lowercase().contains("no feedpoint"),
        "expected 'no EX card' or 'no feedpoint' warning in stderr: {stderr}"
    );
}

#[test]
fn no_validator_warning_for_well_formed_deck() {
    let path = write_temp_deck("with-ex", DECK_WITH_EX);
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("failed to run fnec");
    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[validator]"),
        "unexpected [validator] warning for valid deck: {stderr}"
    );
}

#[test]
fn validator_warning_does_not_prevent_exit_success_for_warning_level() {
    // Warning-level diagnostics must not cause a non-zero exit code.
    let path = write_temp_deck("no-ex-exit", DECK_NO_EX);
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("failed to run fnec");
    // A warning (not an error) — we expect either success or FAILURE depending
    // on whether the deck produces output. The key invariant is that the warning
    // tag appears in stderr. Exit code may be 0 (empty report) or non-zero only
    // for solver errors, not for validator warnings.
    let stderr = String::from_utf8_lossy(&output.stderr);
    // If it does exit non-zero, it should not be solely due to the validator warning.
    // For a deck with no EX but valid geometry + FR, fnec emits an empty report and exits 0.
    assert!(
        output.status.success(),
        "fnec should exit 0 for validator warnings (not errors): {stderr}"
    );
}

#[test]
fn corpus_deck_does_not_trigger_validator_warning() {
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = corpus_root.join("corpus/dipole-freesp-51seg.nec");
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&deck_path)
        .output()
        .expect("failed to run fnec");
    assert!(
        output.status.success(),
        "fnec failed on corpus deck: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[validator]"),
        "unexpected [validator] tag for corpus deck: {stderr}"
    );
}
