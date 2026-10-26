// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// The NEC-2D report over a finite ground.
//
// FND-286: `--ground-solver sommerfeld` corrects the feedpoint impedance alone (a
// one-port ΔZ on the reflection-coefficient solve); NEC-2's feed row derives Z
// from V and I, and the currents, power and pattern are the uncorrected solve's.
// No NEC-2D block of that run can be self-consistent, so `--output-format nec2d`
// refuses the request — and only the request: the MPIE (which puts the surface
// wave into the whole solve), free space and the plain run still print. A text run
// that applied the correction says once that its V and I are not the corrected
// solve's (decision D1).
//
// FND-303: the report's ENVIRONMENT block echoes the ground the solve uses — a
// negative σ is NEC-2's ε'' form, which nec2c prints as 3.950E-03 S/m and
// 13 − j5 for `GN 0 0 0 0 13 -5` at 14.2 MHz.
//
// nec2c 1.3.1 references, run on the decks below (GN 0 swapped for GN 2 for the
// Sommerfeld truth):
//   GN 2 0 0 0 13 0.005, dipole 0.528 m up, 14.2 MHz: Z = 87.797 + j68.626
//   GN 0 0 0 0 13 -5:    CONDUCTIVITY 3.950E-03, COMPLEX DIELECTRIC CONSTANT 1.3000E+01-5.0000E+00j
//   GN 0 0 0 0 13 0.005: CONDUCTIVITY 5.000E-03, COMPLEX DIELECTRIC CONSTANT 1.3000E+01-6.3296E+00j

use std::process::{Command, Output};

mod common;

/// A λ/2 dipole 0.025 λ over a finite ground (`som_low`, FND-286's deck).
const SOM_LOW: &str = "CE\nGW 1 21 -5.278 0 0.528 5.278 0 0.528 0.001\nGE 0\n\
                       GN 0 0 0 0 13 0.005\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\n\
                       RP 0 3 1 1000 0 0 30 0\nEN\n";
/// The same dipole in free space.
const FREE: &str = "CE\nGW 1 21 -5.278 0 0.528 5.278 0 0.528 0.001\nGE 0\n\
                    EX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nRP 0 3 1 1000 0 0 30 0\nEN\n";
/// The same wire bent into an apex-fed inverted V: the correction declines it.
const BENT: &str = "CE\nGW 1 10 -3.732 0 1.056 0 0 4.788 0.001\n\
                    GW 2 10 0 0 4.788 3.732 0 1.056 0.001\nGE 0\nGN 0 0 0 0 13 0.005\n\
                    EX 0 1 10 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
/// `SOM_LOW` at three frequencies, for "once per run".
const SOM_SWEEP: &str = "CE\nGW 1 21 -5.278 0 0.528 5.278 0 0.528 0.001\nGE 0\n\
                         GN 0 0 0 0 13 0.005\nEX 0 1 11 0 1 0\nFR 0 3 0 0 14.0 0.1\nEN\n";

const NEC2C_GN2: (f64, f64) = (87.797, 68.626);
const INFO: &str = "info: --ground-solver sommerfeld: Z includes the surface-wave correction";

fn fnec(test: &str, deck: &str, args: &[&str]) -> Output {
    let deck = common::TempDeck::new(
        &format!("fnec-nec2d-ground-{test}-{}.nec", std::process::id()),
        deck,
    );
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--exec", "cpu"])
        .args(args)
        .arg(deck.path())
        .output()
        .expect("runs fnec")
}

/// The numbers of a NEC-2D line; Fortran's `E` fields run together when the next
/// is negative, so a sign after a digit starts a new number.
fn fortran_numbers(line: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut tok = String::new();
    let mut prev = ' ';
    for c in line.chars() {
        let split = c.is_whitespace() || ((c == '-' || c == '+') && prev.is_ascii_digit());
        if split && !tok.is_empty() {
            out.push(tok.parse().unwrap_or(f64::NAN));
            tok.clear();
        }
        if !c.is_whitespace() {
            tok.push(c);
        }
        prev = c;
    }
    if !tok.is_empty() {
        out.push(tok.parse().unwrap_or(f64::NAN));
    }
    out
}

/// The NEC-2D feed row of segment `1 11`: V, I and Z as complex pairs.
fn nec2d_feed(stdout: &str) -> [(f64, f64); 3] {
    let row = stdout
        .lines()
        .skip_while(|l| !l.contains("ANTENNA INPUT PARAMETERS"))
        .map(fortran_numbers)
        .find(|n| n.len() >= 8 && n[0] == 1.0 && n[1] == 11.0)
        .unwrap_or_else(|| panic!("no feed row:\n{stdout}"));
    [(row[2], row[3]), (row[4], row[5]), (row[6], row[7])]
}

fn within(got: (f64, f64), want: (f64, f64), rel: f64) -> bool {
    let d = ((got.0 - want.0).powi(2) + (got.1 - want.1).powi(2)).sqrt();
    d <= rel * (want.0.powi(2) + want.1.powi(2)).sqrt()
}

fn text(out: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// FND-286: the request is refused before anything is solved — no partial block.
#[test]
fn nec2d_refuses_the_sommerfeld_correction_over_a_finite_ground() {
    let out = fnec(
        "refused",
        SOM_LOW,
        &["--ground-solver", "sommerfeld", "--output-format", "nec2d"],
    );
    let (stdout, stderr) = text(&out);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stdout.is_empty(), "a refused run printed:\n{stdout}");
    assert!(stderr.contains("FND-286"), "{stderr}");
    assert!(
        stderr.contains("--solver mpie"),
        "the remedy is named: {stderr}"
    );
}

/// Negative control: the MPIE puts the surface wave into the whole solve, so its
/// NEC-2D block is self-consistent (Z = V/I) and matches nec2c's Sommerfeld truth.
#[test]
fn nec2d_under_the_mpie_carries_the_surface_wave_and_is_not_refused() {
    let out = fnec(
        "mpie",
        SOM_LOW,
        &[
            "--solver",
            "mpie",
            "--ground-solver",
            "sommerfeld",
            "--output-format",
            "nec2d",
        ],
    );
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let [v, i, z] = nec2d_feed(&stdout);
    // Z = V / I, to the E12.5 print precision.
    let den = i.0 * i.0 + i.1 * i.1;
    let vi = ((v.0 * i.0 + v.1 * i.1) / den, (v.1 * i.0 - v.0 * i.1) / den);
    assert!(within(z, vi, 1e-4), "Z {z:?} is not V/I {vi:?}");
    // Measured 87.411 + j68.002 (0.7 % from nec2c GN 2).
    assert!(
        within(z, NEC2C_GN2, 0.01),
        "Z {z:?} vs nec2c GN 2 {NEC2C_GN2:?}"
    );
}

/// Negative control: over free space there is nothing to correct, so the flag is
/// not a request and the report prints.
#[test]
fn nec2d_with_the_flag_over_free_space_is_not_refused() {
    let out = fnec(
        "free",
        FREE,
        &["--ground-solver", "sommerfeld", "--output-format", "nec2d"],
    );
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert!(!stderr.contains("FND-286"), "{stderr}");
    let _ = nec2d_feed(&stdout);
}

/// Negative control: the plain run prints the reflection-coefficient solve. A
/// self-pin (fnec's normal-incidence Γ is not nec2c GN 0's angle-dependent one,
/// the known GN 0 "Partial" row) — what it proves is that the run is not refused
/// and is the rcm solve, V/I = Z.
#[test]
fn nec2d_without_the_flag_prints_the_rcm_solve() {
    let out = fnec("plain", SOM_LOW, &["--output-format", "nec2d"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let [_, _, z] = nec2d_feed(&stdout);
    assert!(within(z, (30.7018, 34.1489), 1e-4), "Z {z:?}");
}

/// D1 of FND-286: a run whose Z carries the correction says, once, that V, I, the
/// currents and the pattern do not; a declined request and an rcm run do not say it.
#[test]
fn an_applied_correction_says_once_that_v_and_i_are_the_rcm_solves() {
    let (stdout, stderr) = text(&fnec(
        "info-applied",
        SOM_LOW,
        &["--ground-solver", "sommerfeld"],
    ));
    assert_eq!(stderr.matches(INFO).count(), 1, "{stderr}");
    // The row it describes: Z corrected (within 1.5 % of nec2c GN 2), I as solved.
    let row: Vec<f64> = stdout
        .lines()
        .skip_while(|l| !l.starts_with("TAG SEG V_RE"))
        .nth(1)
        .expect("feed row")
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect();
    assert!(within((row[6], row[7]), NEC2C_GN2, 0.015), "{row:?}");

    let (_, sweep) = text(&fnec(
        "info-sweep",
        SOM_SWEEP,
        &["--ground-solver", "sommerfeld"],
    ));
    assert_eq!(
        sweep.matches(INFO).count(),
        1,
        "once per run, not per point:\n{sweep}"
    );

    let (_, declined) = text(&fnec(
        "info-declined",
        BENT,
        &["--ground-solver", "sommerfeld"],
    ));
    assert!(declined.contains("declined this deck"), "{declined}");
    assert!(!declined.contains(INFO), "{declined}");

    let (_, rcm) = text(&fnec("info-rcm", SOM_LOW, &[]));
    assert!(!rcm.contains(INFO), "{rcm}");
}

/// The ENVIRONMENT block's conductivity and complex dielectric constant.
fn environment(stdout: &str) -> (f64, (f64, f64)) {
    let after = |key: &str| {
        let l = stdout
            .lines()
            .find(|l| l.contains(key))
            .unwrap_or_else(|| panic!("no {key}:\n{stdout}"));
        fortran_numbers(&l[l.find('=').expect("=") + 1..])
    };
    let sigma = after("CONDUCTIVITY=");
    let eps = after("COMPLEX DIELECTRIC CONSTANT=");
    (sigma[0], (eps[0], eps[1]))
}

/// FND-303: a negative σ is echoed as the ground the solve uses, as nec2c prints
/// it; a positive σ (the control) is unchanged.
#[test]
fn nec2d_echoes_a_negative_sigma_ground_as_nec2c_does() {
    let neg = SOM_LOW.replace("GN 0 0 0 0 13 0.005", "GN 0 0 0 0 13 -5");
    let out = fnec("negsig", &neg, &["--output-format", "nec2d"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let (sigma, eps) = environment(&stdout);
    assert!((sigma - 3.950e-3).abs() < 5e-7, "conductivity {sigma}");
    assert!(within(eps, (13.0, -5.0), 1e-5), "εc {eps:?}");

    let out = fnec("possig", SOM_LOW, &["--output-format", "nec2d"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let (sigma, eps) = environment(&stdout);
    assert!((sigma - 5.0e-3).abs() < 5e-7, "conductivity {sigma}");
    // nec2c's 59.96 for 1/(2π·c·ε0) puts its ε'' at 6.3296; the exact value is
    // 6.3293 — 5e-5 apart, inside this bound.
    assert!(within(eps, (13.0, -6.3296), 1e-4), "εc {eps:?}");
}
