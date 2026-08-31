// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-002 (current-source junction, CLI wiring): a junctioned antenna driven by
// an EX-type-4 current source now solves through the CLI on continuous conductor
// paths — it no longer fails fast on degree-2 junctioned geometry. Gated by the
// internal-consistency check: the reported current-source feedpoint Z (= V/i0) must
// match the voltage-source feedpoint Z on the same junctioned geometry.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(deck: &str, name: &str) -> (String, bool) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-{name}-{now}.nec"));
    fs::write(&path, deck).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .arg(&path)
        .current_dir(&root)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.success(),
    )
}

fn first_feedpoint_impedance(stdout: &str) -> (f64, f64) {
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 8 || cols[0] == "TAG" {
            continue;
        }
        if cols[0].parse::<usize>().is_err() || cols[1].parse::<usize>().is_err() {
            continue;
        }
        let z_re = cols[6].parse::<f64>().expect("Z_RE");
        let z_im = cols[7].parse::<f64>().expect("Z_IM");
        return (z_re, z_im);
    }
    panic!("no feedpoint rows in stdout:\n{stdout}");
}

// Split λ/2 dipole: two 26-seg arms both starting at the origin (start-to-start,
// one arm reversed) — a degree-2 junction the per-wire current source rejects. Fed
// at the join (wire 1 seg 1). Voltage-source vs current-source (i0 = 1 A) decks.
const V_SRC: &str = "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
const I_SRC: &str = "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 4 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

fn first_feedpoint_current(stdout: &str) -> (f64, f64) {
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 8 || cols[0] == "TAG" {
            continue;
        }
        if cols[0].parse::<usize>().is_err() || cols[1].parse::<usize>().is_err() {
            continue;
        }
        return (
            cols[4].parse::<f64>().expect("I_RE"),
            cols[5].parse::<f64>().expect("I_IM"),
        );
    }
    panic!("no feedpoint rows in stdout:\n{stdout}");
}

// The same half-wave dipole written as two COLLINEAR cards, current-source fed at
// the join. Distinct from the start-to-start pair above: that one is a degree-2
// junction and routes to the conductor-path solver, while a collinear split is a
// trivial path and lands on the per-wire solver — which is where FND-048 lived.
const COLLINEAR_I_SRC: &str = "GW 1 26 0 0 -5.282 0 0 0 0.001\nGW 2 25 0 0 0 0 0 5.282 0.001\nGE 0\nEX 4 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
const COLLINEAR_V_SRC: &str = "GW 1 26 0 0 -5.282 0 0 0 0.001\nGW 2 25 0 0 0 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

/// FND-048: a current source on a collinear-split wire delivered half its current.
///
/// `solve_hallen_current_source` pins `I = 0` at the first and last segment of
/// every entry in the endpoint list it is given, and this path handed it the raw
/// per-`GW` list — so the join carried a spurious zero. With the source sitting on
/// it, the solver was asked for `I = 1` and `I = 0` at the same segment and least
/// squares split the difference: **36.953 + j7.013 Ω at I = 0.5 A**, exit 0, no
/// warning, against 74.228 + j13.897 for the identical single-wire geometry.
///
/// The current assertion is the sharper of the two. An impedance can be wrong for
/// many reasons; a current source that does not deliver its own stated current has
/// violated the boundary condition the user wrote down.
#[test]
fn a_collinear_split_current_source_delivers_the_current_it_was_given() {
    let (out_i, ok_i) = run(COLLINEAR_I_SRC, "collinear-i");
    assert!(ok_i, "collinear-split current source must solve:\n{out_i}");

    let (i_re, i_im) = first_feedpoint_current(&out_i);
    let i_mag = (i_re * i_re + i_im * i_im).sqrt();
    assert!(
        (i_mag - 1.0).abs() < 1e-6,
        "EX 4 asked for 1 A; the solve delivered {i_mag} A"
    );

    // And the impedance must agree with the voltage-source solve of the same
    // geometry, which has always merged collinear joins.
    let (out_v, ok_v) = run(COLLINEAR_V_SRC, "collinear-v");
    assert!(ok_v, "voltage-source reference must solve");
    let (zi_re, zi_im) = first_feedpoint_impedance(&out_i);
    let (zv_re, zv_im) = first_feedpoint_impedance(&out_v);
    let rel = ((zi_re - zv_re).hypot(zi_im - zv_im)) / zv_re.hypot(zv_im);
    assert!(
        rel < 5e-3,
        "current-source Z {zi_re}+j{zi_im} disagrees with voltage-source \
         {zv_re}+j{zv_im} on the same geometry (rel {rel:.3e})"
    );
}

#[test]
fn junctioned_current_source_solves_and_matches_voltage_source() {
    let (out_i, ok_i) = run(I_SRC, "csjunc-i");
    assert!(
        ok_i && out_i.contains("FEEDPOINTS"),
        "junctioned current source must now solve through the CLI (was fail-fast); got:\n{out_i}"
    );
    let (out_v, ok_v) = run(V_SRC, "csjunc-v");
    assert!(ok_v, "voltage-source reference deck must solve");

    let (zi_re, zi_im) = first_feedpoint_impedance(&out_i);
    let (zv_re, zv_im) = first_feedpoint_impedance(&out_v);
    println!("Z(current source) = {zi_re:.4} + j{zi_im:.4}");
    println!("Z(voltage source) = {zv_re:.4} + j{zv_im:.4}");

    // Impedance is a property of the antenna, independent of drive type.
    let num = ((zi_re - zv_re).powi(2) + (zi_im - zv_im).powi(2)).sqrt();
    let den = (zv_re.powi(2) + zv_im.powi(2)).sqrt();
    let rel = num / den;
    assert!(
        rel < 5e-3,
        "junctioned current-source Z ({zi_re:.3}+j{zi_im:.3}) != voltage-source Z \
         ({zv_re:.3}+j{zv_im:.3}) (rel {rel:.2e})"
    );
    // Positive radiation resistance (was garbage / fail-fast before the path solve).
    assert!(
        zi_re > 0.0,
        "current-source resistance must be positive, got {zi_re}"
    );
}
