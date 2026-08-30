// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-126 / FND-127 — a solve that did not converge must not be reported as an
//! answer, on any drive.
//!
//! The guard existed, and the sibling path had none. `FeedpointError::
//! NonFiniteCurrent` fires inside `feedpoint_impedance`, which a plane-wave
//! receive deck never reaches because it has no feedpoint. Measured on the tree
//! before the fix, one deck with one card changed:
//!
//! | drive | radius | result |
//! |-------|--------|--------|
//! | `EX 0` driven  | `0.0` | exit 1, "the solved current at the feedpoint is not a finite number" |
//! | `EX 1` receive | `0.0` | **exit 0**, 51 rows of `NaN NaN NaN NaN`, zero warnings |
//!
//! The receive-pattern path was worse still: it reduces each incidence angle
//! with `fold(0.0f64, f64::max)`, and `f64::max` returns the other operand when
//! one side is NaN — so a fully diverged solve printed as a −999.99 dB null
//! rather than as NaN, indistinguishable from a genuine radiation null.

use std::process::Command;

/// A zero wire radius: a plausible deck typo, not an exotic input, and already
/// a hard error on the driven path.
const DEGENERATE: &str = "0.0";
const HEALTHY: &str = "0.001";

fn deck(radius: &str, extra: &str) -> String {
    format!("CE\nGW 1 51 0 0 -5.282 0 0 5.282 {radius}\nGE\n{extra}\nFR 0 1 0 0 14.2 0.0\nEN\n")
}

fn run(deck_text: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!(
        "fnec-nonfinite-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck_text).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The defect itself: a receive deck must not answer with NaN and exit 0.
#[test]
fn a_diverged_receive_solve_is_refused_rather_than_printed_as_nan() {
    let (ok, stdout, stderr) = run(&deck(DEGENERATE, "EX 1 1 26 0 1.0 0.0"));
    assert!(!ok, "a diverged receive solve must not exit 0:\n{stdout}");
    assert!(
        !stdout.contains("NaN"),
        "no NaN may reach the report:\n{stdout}"
    );
    assert!(
        stderr.contains("did not converge"),
        "the refusal must say why: {stderr}"
    );
}

/// The sibling that always worked. Without this, a change that refused every
/// receive deck would pass the test above.
#[test]
fn the_driven_sibling_still_refuses_the_same_input() {
    let (ok, _, stderr) = run(&deck(DEGENERATE, "EX 0 1 26 0 1.0 0.0"));
    assert!(!ok, "the driven path must keep refusing: {stderr}");
}

/// FND-127: the receive-pattern reduction turned NaN into a plausible number.
/// A pattern row of −999.99 dB reads as a null, so a diverged solve was not
/// merely unreported — it was reported as physics.
#[test]
fn a_diverged_receive_pattern_is_refused_not_folded_into_a_null() {
    let (ok, stdout, stderr) = run(&deck(
        DEGENERATE,
        "EX 1 1 26 0 1.0 0.0\nRP 0 5 1 1000 0.0 0.0 20.0 0.0",
    ));
    assert!(!ok, "a diverged receive pattern must not exit 0:\n{stdout}");
    assert!(
        !stdout.contains("-999.99"),
        "a diverged solve must not print as a radiation null:\n{stdout}"
    );
    assert!(
        stderr.contains("did not converge"),
        "the refusal must say why: {stderr}"
    );
}

/// Negative control: an ordinary receive deck still solves and still patterns.
/// The three tests above are all satisfied by refusing everything.
#[test]
fn an_ordinary_receive_deck_is_unaffected() {
    let (ok, stdout, stderr) = run(&deck(
        HEALTHY,
        "EX 1 1 26 0 1.0 0.0\nRP 0 5 1 1000 0.0 0.0 20.0 0.0",
    ));
    assert!(ok, "a healthy receive deck must still solve: {stderr}");
    assert!(!stdout.contains("NaN"), "{stdout}");
    assert!(
        stdout.contains("RECEIVE") || stdout.contains("PATTERN"),
        "the pattern must still be produced:\n{stdout}"
    );
}
