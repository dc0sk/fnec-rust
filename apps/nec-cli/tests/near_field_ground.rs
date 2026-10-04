// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-201 — near fields over ground.
//!
//! `NE`/`NH` summed the solved currents of the real segments only, so over any
//! ground the field the ground returns was missing: a vertical dipole over PEC
//! read |Ez| 0.0234 V/m 30 m out where nec2c reads 0.0387, |Ex| 9 m up 91 % low,
//! and a ground-mounted monopole lost its whole image half — exit 0, no warning.
//! Over PEC the images are now summed; over a finite ground, where the reflected
//! field is not modelled, the CLI says so.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-nfground-{name}-{n}"), body)
}

fn run(deck: &common::TempDeck) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .arg(deck)
        .output()
        .expect("run fnec");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "fnec failed:\n{stderr}");
    (String::from_utf8_lossy(&out.stdout).into_owned(), stderr)
}

/// The rows of one near-field section, as `(x, y, z, [re, im] × 3)`.
fn section(stdout: &str, name: &str) -> Vec<[f64; 9]> {
    let mut rows = Vec::new();
    let mut inside = false;
    for line in stdout.lines() {
        if line == name {
            inside = true;
            continue;
        }
        if inside {
            let v: Vec<f64> = line
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            if v.len() == 9 {
                rows.push(v.try_into().unwrap());
            } else if line.is_empty() {
                break;
            }
        }
    }
    assert!(!rows.is_empty(), "no {name} rows in:\n{stdout}");
    rows
}

/// Over PEC a deck's field above the plane is the free-space field of the deck
/// plus its image, so each PEC deck here is checked against that image written
/// out as wires in free space — no reference needed:
///
/// - a λ/4 monopole, whose double is the dipole of it and its image, driven
///   across both centre segments (before the fix its field was the upper half's
///   alone);
/// - a horizontal dipole 3 m up, whose image carries the reversed horizontal
///   current, i.e. an antiphase dipole at z = −3 (this one pins the image's
///   horizontal sign, which the vertical deck cannot see).
#[test]
fn a_deck_over_pec_has_the_near_field_of_its_free_space_double() {
    const MONO_GRID: &str = "NE 0 2 1 2 3 0 2 4 0 7\nNH 0 2 1 2 3 0 2 4 0 7\n";
    const HORIZ_GRID: &str = "NE 0 2 2 2 2 0 1 6 4 5\nNH 0 2 2 2 2 0 1 6 4 5\n";
    let cases = [
        (
            "monopole",
            format!("CE\nGW 1 20 0 0 0 0 0 5 0.001\nGE 1\nGN 1\nEX 0 1 1 0 1 0\nFR 0 1 0 0 14.2 0\n{MONO_GRID}EN\n"),
            format!("CE\nGW 1 40 0 0 -5 0 0 5 0.001\nGE 0\nEX 0 1 20 0 1 0\nEX 0 1 21 0 1 0\nFR 0 1 0 0 14.2 0\n{MONO_GRID}EN\n"),
        ),
        (
            "horizontal dipole",
            format!("CE\nGW 1 21 -5.28 0 3 5.28 0 3 0.001\nGE 1\nGN 1\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\n{HORIZ_GRID}EN\n"),
            format!("CE\nGW 1 21 -5.28 0 3 5.28 0 3 0.001\nGW 2 21 -5.28 0 -3 5.28 0 -3 0.001\nGE 0\nEX 0 1 11 0 1 0\nEX 0 2 11 0 -1 0\nFR 0 1 0 0 14.2 0\n{HORIZ_GRID}EN\n"),
        ),
    ];
    for (label, pec, double) in cases {
        let (m, _) = run(&tmp("pec.nec", &pec));
        let (d, _) = run(&tmp("double.nec", &double));
        for name in ["NEAR_FIELD", "NEAR_H_FIELD"] {
            let (rm, rd) = (section(&m, name), section(&d, name));
            assert_eq!(rm.len(), rd.len(), "{label} {name}: row counts differ");
            for (a, b) in rm.iter().zip(&rd) {
                let scale = b[3..].iter().fold(0.0_f64, |s, v| s.max(v.abs()));
                for c in 3..9 {
                    assert!(
                        (a[c] - b[c]).abs() <= 1e-6 * scale,
                        "{label} {name} at ({}, {}, {}): over PEC {a:?} vs double {b:?}",
                        a[0],
                        a[1],
                        a[2]
                    );
                }
            }
        }
    }
}

/// A vertical dipole 5–15 m over PEC, 30 m out, against nec2c's NEAR ELECTRIC
/// FIELDS magnitudes. Before the fix fnec was 51 % low in |Ez| at z = 1 and 91 %
/// low in |Ex| at z = 9; the free-space deck's model error here is ~4 %.
#[test]
fn a_dipole_over_pec_has_nec2c_s_near_field() {
    let d = tmp(
        "vert.nec",
        "CE\nGW 1 21 0 0 5 0 0 15 0.001\nGE 1\nGN 1\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nNE 0 1 1 3 30 0 1 0 0 4\nEN\n",
    );
    // (z, |Ex|, |Ez|) from nec2c.
    const NEC2C: [(f64, f64, f64); 3] = [
        (1.0, 1.5929e-3, 4.3041e-2),
        (5.0, 7.0440e-3, 3.8729e-2),
        (9.0, 9.4274e-3, 3.0515e-2),
    ];
    let (out, stderr) = run(&d);
    assert!(
        !stderr.contains("near fields over a finite ground"),
        "PEC is not a finite ground:\n{stderr}"
    );
    let rows = section(&out, "NEAR_FIELD");
    for (z, ex, ez) in NEC2C {
        let r = rows
            .iter()
            .find(|r| (r[2] - z).abs() < 1e-9)
            .unwrap_or_else(|| panic!("no row at z = {z}"));
        let got_ex = r[3].hypot(r[4]);
        let got_ez = r[7].hypot(r[8]);
        for (what, got, want) in [("|Ex|", got_ex, ex), ("|Ez|", got_ez, ez)] {
            let rel = (got - want).abs() / want;
            assert!(
                rel < 0.06,
                "{what} at z = {z}: fnec {got:.4e} vs nec2c {want:.4e} ({:.1} %)",
                100.0 * rel
            );
        }
    }
}

/// Over a finite ground the reflected field is not in the sum, and the user is
/// told so rather than handed the numbers silently.
#[test]
fn near_fields_over_a_finite_ground_carry_a_caveat() {
    let d = tmp(
        "gn2.nec",
        "CE\nGW 1 21 0 0 5 0 0 15 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nNE 0 1 1 1 30 0 5 0 0 0\nEN\n",
    );
    let (_, stderr) = run(&d);
    assert!(
        stderr.contains("near fields over a finite ground omit the field the ground reflects"),
        "no finite-ground near-field caveat:\n{stderr}"
    );
}
