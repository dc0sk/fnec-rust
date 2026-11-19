// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-328: an `EX` voltage source (type 0 or 5) whose F1/F2 are zero or blank
//! is driven at 1 + j0 V, as NEC-2 drives it, with a warning; fnec's `EX 4`
//! current source at 0 A is refused by name.
//!
//! nec2c 1.3.1 `main.c:561-563` (type 5) and `586-588` (type 0) replace any
//! |V| < 1e-20 with `CPLX_10`; `readmn` reads a missing field as 0. fnec stored
//! the fields as written, so a lone blank source exited 1 with FND-058's
//! "zero-amplitude source" diagnosis and a second, blank source beside a driven
//! one solved silently at 0 V.
//!
//! Anchored on nec2c absolutely (nec2c 1.3.1, the 21-segment 5.282 m dipole at
//! 14.2 MHz of `two_sources_one_segment_nec2c.rs`; a second, parallel dipole 2 m
//! away for the pair):
//!
//! - `EX 0 1 11 0`, `EX 0 1 11 0 0 0`, `EX 0 1 11 0 1e-21 0`: each is nec2c's
//!   1 V deck to every digit — V = 1, I = 9.4662e-3 − j5.4923e-3 A
//!   (|I| 1.0944e-2), Z = 79.034 + j45.856 Ω, power 4.7331e-3 W.
//! - `EX 0 1 11 0 1e-19 0` (negative control): drives 1e-19 V, |I| 1.0944e-21.
//! - `EX 5 1 11 0`: nec2c's `EX 5 … 1 0` deck to every digit,
//!   I = 8.9329e-3 − j5.1167e-3 A (|I| 1.0294e-2).
//! - The pair, `EX 0 1 11 0 1 0` / `EX 0 2 11 0 0 0`: both rows V = 1,
//!   Z = 154.74 + j49.819 Ω — nec2c's deck with both cards at 1 V, to the digit.
//!
//! The bands are inherited from `two_sources_one_segment_nec2c.rs`, not tuned:
//! Z within 2 / 8 Ω, |I| and power within 3 %, EX 5 within 10 % (fnec solves EX 5
//! as EX 0, a documented non-goal, PH8-CHK-003; fnec is +7.8 % here). Each case
//! also asserts what the unfixed reading would give, far outside the band.

mod common;

use std::process::{Command, Output};

/// NEC-2D output needs an executing card; the nec2c reference decks carry it too.
const RP: &str = "RP 0 1 1 1000 90 0 0 0\n";
const GEOM: &str = "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\n";
const PAIR: &str = "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGW 2 21 2 0 -5.282 2 0 5.282 0.001\nGE 0\n";

/// nec2c, the dipole at 1 V.
const NEC2C_ONE_Z: (f64, f64) = (79.034, 45.856);
const NEC2C_ONE_I_MAG: f64 = 1.0944e-2;
const NEC2C_ONE_P_IN: f64 = 4.7331e-3;
/// nec2c, the dipole's EX 5 at 1 V: |8.9329e-3 − j5.1167e-3|.
const NEC2C_EX5_I_MAG: f64 = 1.0294e-2;
/// nec2c, the pair with both sources at 1 V (each row).
const NEC2C_PAIR_Z: (f64, f64) = (154.74, 49.819);

/// The warning's stable words (`ParseError::DefaultVoltage`).
const DRIVES_AT_ONE_VOLT: &str = "drives such a source at 1 + j0 V";

fn run(name: &str, geom: &str, cards: &str, args: &[&str]) -> Output {
    let deck = common::TempDeck::new(
        &format!("fnd328_{name}_{}.nec", std::process::id()),
        &format!("CM {name}\nCE\n{geom}{cards}FR 0 1 0 0 14.2 0\n{RP}EN\n"),
    );
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--exec", "cpu"])
        .args(args)
        .arg(&*deck)
        .output()
        .expect("run fnec")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn ok(name: &str, o: &Output) -> String {
    assert!(o.status.success(), "{name}: fnec failed:\n{}", stderr(o));
    stdout(o)
}

/// A text section's numeric rows, up to the blank line.
fn section(report: &str, name: &str) -> Vec<Vec<f64>> {
    let start = report
        .find(&format!("\n{name}\n"))
        .unwrap_or_else(|| panic!("no {name} section in:\n{report}"));
    report[start + name.len() + 2..]
        .lines()
        .take_while(|l| !l.is_empty())
        // Header lines (`TAG SEG …`, `N_SOURCES 1`) are not all numbers.
        .filter(|l| l.split_whitespace().all(|c| c.parse::<f64>().is_ok()))
        .map(|l| l.split_whitespace().map(|c| c.parse().unwrap()).collect())
        .collect()
}

/// `|I|` of the CURRENTS row for `tag` / `seg`, unrounded (FEEDPOINTS prints six
/// decimals, which reads a 1e-21 A current as 0).
fn i_mag_at(report: &str, tag: f64, seg: f64) -> f64 {
    section(report, "CURRENTS")
        .into_iter()
        .find(|r| (r[0], r[1]) == (tag, seg))
        .unwrap_or_else(|| panic!("no CURRENTS row {tag} {seg}:\n{report}"))[4]
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn assert_z(r: &[f64], z: (f64, f64), what: &str) {
    assert!(
        (r[6] - z.0).abs() < 2.0 && (r[7] - z.1).abs() < 8.0,
        "{what}: Z {} {:+} against nec2c {z:?}",
        r[6],
        r[7]
    );
}

/// A lone blank or zero EX 0 is nec2c's 1 V source (today: exit 1). The report,
/// the SOURCES table and NEC-2D output all carry the 1 V; the echo of the card
/// keeps it as written, as nec2c's does.
#[test]
fn a_blank_ex0_is_driven_at_one_volt_as_nec2c() {
    for (name, ex) in [("blank", "EX 0 1 11 0\n"), ("zero", "EX 0 1 11 0 0 0\n")] {
        let out = run(name, GEOM, ex, &[]);
        let report = ok(name, &out);
        let rows = section(&report, "FEEDPOINTS");
        assert_eq!(rows.len(), 1, "{name}:\n{report}");
        assert_eq!((rows[0][2], rows[0][3]), (1.0, 0.0), "{name}: V\n{report}");
        assert_z(&rows[0], NEC2C_ONE_Z, name);
        let i = i_mag_at(&report, 1.0, 11.0);
        assert!(rel(i, NEC2C_ONE_I_MAG) < 0.03, "{name}: |I| {i}");
        let src = section(&report, "SOURCES");
        assert_eq!(
            (src[0][4], src[0][5]),
            (1.0, 0.0),
            "{name}: SOURCES\n{report}"
        );
        let err = stderr(&out);
        assert!(
            err.contains("line 5: EX 0 names no voltage") && err.contains(DRIVES_AT_ONE_VOLT),
            "{name}: the warning:\n{err}"
        );
    }

    let out = run(
        "blank_nec2d",
        GEOM,
        "EX 0 1 11 0\n",
        &["--output-format", "nec2d"],
    );
    let nec2d = ok("blank_nec2d", &out);
    let p: f64 = nec2d
        .lines()
        .find(|l| l.contains("INPUT POWER   ="))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_end_matches("WATTS").trim().parse().unwrap())
        .unwrap_or_else(|| panic!("no INPUT POWER line:\n{nec2d}"));
    assert!(rel(p, NEC2C_ONE_P_IN) < 0.03, "INPUT POWER {p}");
    assert!(
        nec2d.contains("EX   0     1    11     0  0.00000E+00  0.00000E+00"),
        "the card echo keeps the blank voltage, as nec2c's does:\n{nec2d}"
    );
}

/// A blank EX 5 is nec2c's 1 V EX 5 (today: exit 1).
#[test]
fn a_blank_ex5_is_driven_at_one_volt_as_nec2c() {
    let out = run("ex5", GEOM, "EX 5 1 11 0\n", &[]);
    let report = ok("ex5", &out);
    let rows = section(&report, "FEEDPOINTS");
    assert_eq!((rows[0][2], rows[0][3]), (1.0, 0.0), "V\n{report}");
    let i = i_mag_at(&report, 1.0, 11.0);
    assert!(rel(i, NEC2C_EX5_I_MAG) < 0.10, "|I| {i}");
    assert!(stderr(&out).contains("EX 5 names no voltage"));
}

/// The silent case: a blank source beside a driven one. nec2c drives both at
/// 1 V; fnec drove the second at 0 V, a different antenna, with exit 0 and no
/// word — one dipole driven beside a parasitic one, whose row 1 is far below
/// nec2c's R.
#[test]
fn a_blank_second_source_is_driven_at_one_volt_as_nec2c() {
    let out = run("pair", PAIR, "EX 0 1 11 0 1 0\nEX 0 2 11 0 0 0\n", &[]);
    let report = ok("pair", &out);
    let rows = section(&report, "FEEDPOINTS");
    assert_eq!(rows.len(), 2, "two driven segments:\n{report}");
    for r in &rows {
        assert_eq!((r[2], r[3]), (1.0, 0.0), "V\n{report}");
        assert_z(r, NEC2C_PAIR_Z, "pair");
    }
    assert!(stderr(&out).contains("line 7: EX 0 names no voltage"));
}

/// At nec2c's threshold: 1e-21 V is 1 V, 1e-19 V is driven as written.
#[test]
fn the_threshold_is_nec2cs() {
    let out = run("tiny", GEOM, "EX 0 1 11 0 1e-21 0\n", &[]);
    let report = ok("tiny", &out);
    let i = i_mag_at(&report, 1.0, 11.0);
    assert!(rel(i, NEC2C_ONE_I_MAG) < 0.03, "1e-21 V: |I| {i}");
    assert!(stderr(&out).contains(DRIVES_AT_ONE_VOLT));

    // Negative control: nec2c drives 1e-19 V as written, and so does fnec.
    let out = run("small", GEOM, "EX 0 1 11 0 1e-19 0\n", &[]);
    let report = ok("small", &out);
    let i = i_mag_at(&report, 1.0, 11.0);
    assert!(rel(i, NEC2C_ONE_I_MAG * 1e-19) < 0.03, "1e-19 V: |I| {i}");
    assert!(
        !stderr(&out).contains(DRIVES_AT_ONE_VOLT),
        "{}",
        stderr(&out)
    );
}

/// fnec's EX 4 at 0 A: refused by name before a solve, not blamed on the
/// frequency after one. A written current still solves.
#[test]
fn a_zero_current_ex4_is_refused_by_name() {
    let out = run("ex4", GEOM, "EX 4 1 11 0 0 0\n", &[]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let err = stderr(&out);
    assert!(
        err.contains("EX 4 names no current") && !err.contains("frequency"),
        "{err}"
    );
    let out = run("ex4_ok", GEOM, "EX 4 1 11 0 0.01 0\n", &[]);
    ok("ex4_ok", &out);
}
