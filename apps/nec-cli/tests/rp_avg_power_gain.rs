// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-004: RP XNDA `A` digit — average power gain. The solid-angle-weighted
// mean gain over the pattern region equals the radiation efficiency over the full
// sphere (≈1 for a lossless antenna). Validated against nec2c, which prints
// "AVERAGE POWER GAIN: 9.9795E-01" for the free-space λ/2 dipole below.

use std::path::PathBuf;
use std::process::Command;

fn run_fnec(deck: &str, name: &str) -> String {
    let path = std::env::temp_dir().join(format!("fnec_apg_{name}.nec"));
    std::fs::write(&path, deck).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .unwrap();
    assert!(out.status.success(), "fnec failed for {name}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const DIPOLE: &str =
    "CM dipole\nCE\nGW 1 21 0 0 -5.28 0 0 5.28 0.001\nGE 0\nFR 0 1 0 0 14.2 0\nEX 0 1 11 0 1.0 0.0\n";

fn avg_power_gain(stdout: &str) -> Option<f64> {
    stdout
        .lines()
        .find(|l| l.trim_start().starts_with("AVERAGE_POWER_GAIN"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|t| t.parse().ok())
}

/// A full-sphere RP with the `A` digit set emits AVERAGE_POWER_GAIN ≈ 1
/// (nec2c: 0.998 for this lossless free-space dipole).
#[test]
fn average_power_gain_matches_nec2c() {
    // RP 0 NTHETA NPHI XNDA=1002 θ0 φ0 Δθ Δφ — full sphere, A digit = 2.
    let deck = format!("{DIPOLE}RP 0 19 37 1002 0 0 10 10\nEN\n");
    let apg = avg_power_gain(&run_fnec(&deck, "sphere")).expect("AVERAGE_POWER_GAIN line");
    assert!(
        (apg - 0.998).abs() / 0.998 < 0.02,
        "average power gain {apg} not within 2% of nec2c 0.998"
    );
}

/// Without the `A` digit (XNDA=1000), no AVERAGE_POWER_GAIN line is emitted.
#[test]
fn no_a_digit_no_average_power_gain() {
    let deck = format!("{DIPOLE}RP 0 19 37 1000 0 0 10 10\nEN\n");
    assert!(
        avg_power_gain(&run_fnec(&deck, "noa")).is_none(),
        "AVERAGE_POWER_GAIN must not appear without the XNDA A digit"
    );
}
