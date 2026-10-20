// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-271: a Hallén-only geometry refusal names the MPIE the way each run can
// reach it. The refusal text had "`--solver mpie` on a local run" baked in, which
// `--hosts` and `sweep --resonance` refuse as a flag. Each remedy is joined on in
// one place (`GeometryError::with_mpie_remedy`), so each must appear exactly once.

use std::process::Command;

mod common;

/// A half-circle arc fed one segment in from its end: refused by the Hallén
/// geometry (`CurvedForHallen`), solved by the MPIE. Not at the end: the MPIE
/// refuses an end-segment feed (FND-228), which the remedy check does not see
/// (FND-276).
const ARC: &str = "CE\nGA 1 10 0.5 0 180 .001\nGE 0\nEX 0 1 2 0 1 0\nFR 0 1 0 0 30 0\nEN\n";

const CLI: &str = "re-run with `--solver mpie`";
const HOSTS: &str = "run it without `--hosts`, as `fnec --solver mpie`";
const RESONANCE: &str = "solve the deck at the values you want with `fnec --solver mpie`";

fn stderr_of(args: &[&str], deck: &str, name: &str) -> (Option<i32>, String) {
    let deck = common::TempDeck::new(name, deck);
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&deck)
        .output()
        .expect("run fnec");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_local_run_is_told_the_flag_once() {
    let (code, err) = stderr_of(&[], ARC, "fnec-remedy-local.nec");
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("bend at every segment"), "{err}");
    assert_eq!(err.matches(CLI).count(), 1, "{err}");
    assert!(!err.contains("on a local run"), "{err}");
}

/// A sinusoidal run builds the same Hallén geometry, so the same remedy.
#[test]
fn a_sinusoidal_run_is_told_the_flag_once() {
    let (code, err) = stderr_of(&["--solver", "sinusoidal"], ARC, "fnec-remedy-sin.nec");
    assert_eq!(code, Some(1), "{err}");
    assert_eq!(err.matches(CLI).count(), 1, "{err}");
}

/// The MPIE solves the arc: the remedy is true for this deck.
#[test]
fn the_remedy_solves_the_deck() {
    let (code, err) = stderr_of(&["--solver", "mpie"], ARC, "fnec-remedy-mpie.nec");
    assert_eq!(code, Some(0), "{err}");
}

/// A deck the MPIE cannot take (a plane wave) is refused without a remedy.
#[test]
fn a_deck_the_mpie_cannot_take_names_no_remedy() {
    let wave = ARC.replace("EX 0 1 2 0 1 0", "EX 1 1 1 0 0 0");
    let (code, err) = stderr_of(&[], &wave, "fnec-remedy-wave.nec");
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("bend at every segment"), "{err}");
    assert!(!err.contains("MPIE solves this deck"), "{err}");
}

/// RFC 5737 TEST-NET-3: never routed, and the refusal must come before any dial.
#[test]
fn a_distributed_run_is_told_to_run_locally() {
    let hosts = common::TempDeck::new(
        "fnec-remedy-hosts.toml",
        "[[worker]]\nhostname = \"203.0.113.1\"\n",
    );
    let hosts_path = hosts.path().to_str().expect("utf-8 path").to_string();
    let (code, err) = stderr_of(&["--hosts", &hosts_path], ARC, "fnec-remedy-dist.nec");
    assert_eq!(code, Some(1), "{err}");
    assert_eq!(err.matches(HOSTS).count(), 1, "{err}");
    assert!(!err.contains(CLI), "{err}");
    assert!(!err.contains("203.0.113.1"), "{err}");
}

/// `sweep --resonance` takes no `--solver`: it names the MPIE as a separate run.
#[test]
fn a_resonance_search_is_told_to_solve_separately() {
    let toml = "[search]\nvar = \"R\"\nlo = 0.4\nhi = 0.6\ntarget_reactance_ohm = 0.0\n\
                tolerance_ohm = 0.5\nmax_iter = 20\n\n[deck]\ntemplate = \"\"\"\n\
                CE\nGA 1 10 $R 0 180 .001\nGE 0\nEX 0 1 2 0 1 0\nFR 0 1 0 0 30 0\nEN\n\"\"\"\n";
    let file = common::TempDeck::new("fnec-remedy-res.nec.toml", toml);
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["sweep", "--resonance"])
        .arg(file.path())
        .output()
        .expect("run fnec");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{err}");
    assert_eq!(err.matches(RESONANCE).count(), 1, "{err}");
    assert!(!err.contains(CLI), "{err}");
}
