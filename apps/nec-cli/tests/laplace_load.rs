// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! End-to-end tests for the fnec-specific Laplace-domain load
//! (`--loads-config`, BL-IMPR-016). A rational `Z(s) = N(s)/D(s)` load must
//! reproduce the equivalent built-in RLC load to numerical tolerance.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

/// Write a uniquely-named temp file that deletes itself when the test ends.
///
/// It used to return a bare path and leave the file behind; one session of
/// repeated `cargo test --workspace` runs left hundreds of `fnec-laplace-*`
/// files in the system temp directory.
fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-laplace-{name}-{n}"), body)
}

fn feedpoint_z(stdout: &str) -> (f64, f64) {
    for line in stdout.lines() {
        let c: Vec<&str> = line.split_whitespace().collect();
        if c.len() == 8 && c[0].parse::<usize>().is_ok() && c[1].parse::<usize>().is_ok() {
            return (c[6].parse().unwrap(), c[7].parse().unwrap());
        }
    }
    panic!("no feedpoint row in:\n{stdout}");
}

fn run(extra: &[&str], deck: &common::TempDeck) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .args(extra)
        .arg(deck)
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .output()
        .expect("run fnec")
}

const DIPOLE: &str =
    "CE\nGW 1 51 0 0 -5.0 0 0 5.0 0.001\nGE\n{LD}EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn laplace_rl_load_matches_equivalent_ld2_load() {
    // Deck A: a series R+L load via the built-in LD 2 card on segment 20.
    let deck_a = tmp(
        "a.nec",
        &DIPOLE.replace("{LD}", "LD 2 1 20 20 150.0 2e-6 0.0\n"),
    );
    let out_a = run(&[], &deck_a);
    assert!(out_a.status.success());
    let (ra, xa) = feedpoint_z(&String::from_utf8_lossy(&out_a.stdout));

    // Deck B: the same geometry, no LD; the identical load as a Laplace load
    // Z(s) = 150 + 2e-6·s  ->  N = [150, 2e-6], D = [1].
    let deck_b = tmp("b.nec", &DIPOLE.replace("{LD}", ""));
    let cfg = tmp("loads.toml", "[[laplace_load]]\ntag = 1\nseg_first = 20\nnumerator = [150.0, 2.0e-6]\ndenominator = [1.0]\n");
    let out_b = run(&["--loads-config", cfg.to_str().unwrap()], &deck_b);
    assert!(
        out_b.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out_b.stderr)
    );
    let (rb, xb) = feedpoint_z(&String::from_utf8_lossy(&out_b.stdout));

    // The two loads are identical, so the feedpoint impedance must match.
    assert!(
        (ra - rb).abs() < 1e-3 && (xa - xb).abs() < 1e-3,
        "Laplace load Z=({rb},{xb}) != LD2 load Z=({ra},{xa})"
    );

    // Sanity: the load actually moved the impedance (vs the bare dipole).
    let deck_bare = tmp("bare.nec", &DIPOLE.replace("{LD}", ""));
    let (r0, _) = feedpoint_z(&String::from_utf8_lossy(&run(&[], &deck_bare).stdout));
    assert!((ra - r0).abs() > 1e-3, "load had no effect on R");
}

#[test]
fn laplace_load_rejected_on_mpie_path() {
    let deck = tmp("mpie.nec", &DIPOLE.replace("{LD}", ""));
    let cfg = tmp(
        "mpie-loads.toml",
        "[[laplace_load]]\ntag = 1\nseg_first = 26\nnumerator = [50.0]\ndenominator = [1.0]\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "mpie", "--loads-config", cfg.to_str().unwrap()])
        .arg(&deck)
        .output()
        .expect("run fnec");
    assert!(
        !out.status.success(),
        "mpie + laplace loads should be rejected"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Laplace loads") && stderr.contains("mpie"),
        "expected an mpie-rejection message, got: {stderr}"
    );
}
