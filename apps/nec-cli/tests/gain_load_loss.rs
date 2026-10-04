// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-200 — a lossy load costs gain whatever the ground.
//!
//! The pattern's directivity becomes gain through the radiation efficiency
//! `η = P_radiated / P_input`, and that correction used to be applied over a
//! lossy finite ground only: free space and PEC were "lossless". A lossy `LD`
//! load is not, so a λ/2 dipole with 100 Ω at its feed printed 2.17 dBi in free
//! space against nec2c's −1.20 (efficiency 45.89 %), exit 0. Pinned here against
//! nec2c absolutely, at two meshes, in free space and over PEC — with the
//! unloaded deck as the control the correction must leave alone, and the same
//! load from `--loads-config`, which must cost exactly what the card does.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-gainloss-{name}-{n}"), body)
}

/// The total gain at `theta` from the `RADIATION_PATTERN` table.
fn gain_at(extra: &[&str], deck: &common::TempDeck, theta: f64) -> f64 {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .args(extra)
        .arg(deck)
        .output()
        .expect("run fnec");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut in_table = false;
    for line in stdout.lines() {
        if line == "RADIATION_PATTERN" {
            in_table = true;
            continue;
        }
        let c: Vec<&str> = line.split_whitespace().collect();
        if in_table && c.len() >= 3 {
            if let (Ok(t), Ok(g)) = (c[0].parse::<f64>(), c[2].parse::<f64>()) {
                if (t - theta).abs() < 1e-9 {
                    return g;
                }
            }
        }
    }
    panic!("no pattern row at theta {theta} in:\n{stdout}");
}

/// A λ/2 dipole at 299.79 MHz with `segs` segments, fed at its centre: in free
/// space along z (read at θ = 90°), or horizontal at 0.5 m over PEC (θ = 45°).
fn deck(segs: u32, pec: bool, load: bool) -> (String, f64) {
    let feed = segs.div_ceil(2);
    let (geo, ground, theta) = if pec {
        (
            format!("GW 1 {segs} -0.25 0 0.5 0.25 0 0.5 .001"),
            "GE 1\nGN 1",
            45.0,
        )
    } else {
        (format!("GW 1 {segs} 0 0 -0.25 0 0 0.25 .001"), "GE 0", 90.0)
    };
    let ld = if load {
        format!("LD 4 1 {feed} {feed} 100 0\n")
    } else {
        String::new()
    };
    (
        format!(
            "CE\n{geo}\n{ground}\n{ld}EX 0 1 {feed} 0 1 0\nFR 0 1 0 0 299.79 0\nRP 0 1 1 1000 {theta} 0 0 0\nEN\n"
        ),
        theta,
    )
}

/// nec2c's gain (dBi) for each deck, from its `RADIATION PATTERNS` table, with
/// its EFFICIENCY line for the loaded ones: free space 45.89 % / 46.15 %, PEC
/// 43.65 % / 43.84 % at 21 / 41 segments.
const NEC2C: [(u32, bool, bool, f64); 8] = [
    (21, false, false, 2.18),
    (21, false, true, -1.20),
    (21, true, false, 2.34),
    (21, true, true, -1.26),
    (41, false, false, 2.18),
    (41, false, true, -1.18),
    (41, true, false, 2.34),
    (41, true, true, -1.24),
];

#[test]
fn a_lossy_load_costs_gain_in_free_space_and_over_pec_as_in_nec2c() {
    let mut loaded_err = std::collections::HashMap::new();
    for (segs, pec, load, want) in NEC2C {
        let (text, theta) = deck(segs, pec, load);
        let d = tmp("d.nec", &text);
        let got = gain_at(&[], &d, theta);
        let err = (got - want).abs();
        eprintln!("{segs} segs, pec {pec}, load {load}: fnec {got:.4} dBi, nec2c {want:.2}");
        // nec2c prints two decimals; the unloaded deck is the control the
        // correction must not move, the loaded one was ~3.4 dB high before.
        let tol = if load { 0.10 } else { 0.03 };
        assert!(
            err <= tol,
            "{segs} segs, pec {pec}, load {load}: fnec {got} dBi vs nec2c {want} (|Δ| {err:.3} > {tol})"
        );
        if load {
            loaded_err.insert((segs, pec), err);
        }
    }
    // Converging on nec2c, not resting on a lucky offset.
    for pec in [false, true] {
        assert!(
            loaded_err[&(41, pec)] < loaded_err[&(21, pec)],
            "pec {pec}: the loaded error does not shrink with the mesh: {} at 21, {} at 41",
            loaded_err[&(21, pec)],
            loaded_err[&(41, pec)]
        );
    }
}

/// The same 100 Ω from `--loads-config` costs exactly what the `LD` card does:
/// the decision reads the power the solve loses, not the card that spelled it.
#[test]
fn a_laplace_load_costs_the_same_gain_as_its_ld_twin() {
    for pec in [false, true] {
        let (with_ld, theta) = deck(21, pec, true);
        let (bare, _) = deck(21, pec, false);
        let d_ld = tmp("ld.nec", &with_ld);
        let d_bare = tmp("bare.nec", &bare);
        let cfg = tmp(
            "l.toml",
            "[[laplace_load]]\ntag = 1\nseg_first = 11\nseg_last = 11\nnumerator = [100.0]\ndenominator = [1.0]\n",
        );
        let g_ld = gain_at(&[], &d_ld, theta);
        let g_lap = gain_at(&["--loads-config", cfg.to_str().unwrap()], &d_bare, theta);
        assert!(
            (g_ld - g_lap).abs() < 1e-3,
            "pec {pec}: LD {g_ld} dBi vs --loads-config {g_lap} dBi"
        );
        assert!(
            g_ld < 0.0,
            "pec {pec}: the loaded gain {g_ld} dBi is not reduced"
        );
    }
}

/// The `RP` A digit's average power gain is built from the same rows, so it is
/// the radiation efficiency: nec2c prints 4.5797E-01 for the loaded free-space
/// dipole (EFFICIENCY 45.89 %). It read ~1 while the load loss was dropped.
#[test]
fn the_average_power_gain_of_a_loaded_dipole_is_its_efficiency() {
    let d = tmp(
        "avg.nec",
        "CE\nGW 1 21 0 0 -0.25 0 0 0.25 .001\nGE 0\nLD 4 1 11 11 100 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 299.79 0\nRP 0 19 37 1001 0 0 10 10\nEN\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .arg(&d)
        .output()
        .expect("run fnec");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let avg: f64 = stdout
        .lines()
        .find_map(|l| l.strip_prefix("AVERAGE_POWER_GAIN "))
        .unwrap_or_else(|| panic!("no AVERAGE_POWER_GAIN in:\n{stdout}"))
        .trim()
        .parse()
        .expect("a number");
    assert!(
        (avg - 0.45797).abs() < 0.01,
        "average power gain {avg} vs nec2c 0.45797"
    );
}
