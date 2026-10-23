// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-284 / FND-285 / FND-298 — the `RP` card as nec2c reads it: by position,
//! mode 0 only, and XNDA's digits split X N D A.
//!
//! References: nec2c 1.3.1, captured 2026-10-23, on the 5 m, 21-segment dipole
//! below at 29.9 MHz (79.004 + j41.448 Ω). nec2c prints gains to 0.01 dB, so a
//! match is within 0.015 dB (its rounding plus 0.01).
use std::process::Command;

mod common;

/// The dipole with `cards` before EX and `rp` as its RP card.
fn dipole(cards: &str, rp: &str) -> String {
    format!(
        "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGE 0\n{cards}\nEX 0 1 11 0 1 0\n\
         FR 0 1 0 0 29.9 0\n{rp}\nEN\n"
    )
}

/// `fnec --solver hallen` on `deck`: Ok(stdout) or Err(stderr).
fn fnec(deck: &str, name: &str) -> Result<String, String> {
    let path = common::TempDeck::new(&format!("fnec_rp_nec2c_{name}.nec"), deck);
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .arg(&path)
        .output()
        .expect("run fnec");
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// The `RADIATION_PATTERN` rows: (θ, φ, total gain dB).
fn pattern(stdout: &str) -> Vec<(f64, f64, f64)> {
    stdout
        .lines()
        .skip_while(|l| *l != "RADIATION_PATTERN")
        .skip(3)
        .map_while(|l| {
            let c: Vec<f64> = l
                .split_whitespace()
                .map(|t| t.parse().ok())
                .collect::<Option<_>>()?;
            (c.len() == 6).then(|| (c[0], c[1], c[2]))
        })
        .collect()
}

fn assert_gains(rows: &[(f64, f64, f64)], want: &[(f64, f64)], what: &str) {
    let got: Vec<(f64, f64)> = rows.iter().map(|r| (r.0, r.2)).collect();
    assert_eq!(got.len(), want.len(), "{what}: {got:?}");
    for (&(t, g), &(wt, wg)) in got.iter().zip(want) {
        assert_eq!(t, wt, "{what}: θ {got:?}");
        assert!(
            (g - wg).abs() <= 0.015,
            "{what}: θ = {t}: {g} dB, nec2c {wg} dB ({got:?})"
        );
    }
}

/// nec2c's θ 0/30/60/90 gains on the unloaded dipole (also its directive gains
/// with a series load at the feed, where the current's shape does not change).
const NEC2C_UNLOADED: [(f64, f64); 4] = [(0.0, -999.99), (30.0, -5.48), (60.0, 0.38), (90.0, 2.16)];

/// FND-284: seven integer fields are `RP I1 NTH NPH XNDA θ0 φ0 Δθ` with Δφ
/// zero; nec2c reads θ 0/30/60/90. fnec read them as the old no-XNDA layout and
/// printed four rows at θ = 1000.
#[test]
fn a_seven_field_integer_rp_is_the_eight_field_grid() {
    let seven = pattern(&fnec(&dipole("", "RP 0 4 1 1000 0 0 30"), "seven").unwrap());
    let eight = pattern(&fnec(&dipole("", "RP 0 4 1 1000 0 0 30 0"), "eight").unwrap());
    assert_eq!(seven, eight);
    assert_gains(&seven, &NEC2C_UNLOADED, "RP 0 4 1 1000 0 0 30");
}

/// FND-298: XNDA's D (tens) digit asks for directive gain: the directivity,
/// without the radiation efficiency. On a 50 Ω series load at the feed nec2c
/// prints directive gains −5.48/0.38/2.16 dB and power gains −7.61/−1.75/0.03
/// (efficiency 61.24 %); on one at segment 5, where the current's shape changes,
/// −5.59/0.32/2.16 against −6.66/−0.75/1.09 (78.20 %). fnec printed power gains
/// for both, D never read.
#[test]
fn the_d_digit_reports_directive_gain_as_nec2c() {
    for (load, directive, power) in [
        (
            "LD 4 1 11 11 50 0",
            NEC2C_UNLOADED,
            [(0.0, -999.99), (30.0, -7.61), (60.0, -1.75), (90.0, 0.03)],
        ),
        (
            "LD 4 1 5 5 50 0",
            [(0.0, -999.99), (30.0, -5.59), (60.0, 0.32), (90.0, 2.16)],
            [(0.0, -999.99), (30.0, -6.66), (60.0, -0.75), (90.0, 1.09)],
        ),
    ] {
        let d = pattern(&fnec(&dipole(load, "RP 0 4 1 0010 0 0 30 0"), "directive").unwrap());
        assert_gains(&d, &directive, &format!("{load}, XNDA 0010"));
        let p = pattern(&fnec(&dipole(load, "RP 0 4 1 0000 0 0 30 0"), "power").unwrap());
        assert_gains(&p, &power, &format!("{load}, XNDA 0000"));
    }
}

/// The D digit belongs to its own card: a power-gain card after a directive one
/// keeps its power gains (the table holds every card's grid in deck order).
#[test]
fn directive_gain_is_per_card() {
    let rows = pattern(
        &fnec(
            &dipole(
                "LD 4 1 11 11 50 0",
                "RP 0 4 1 0010 0 0 30 0\nRP 0 4 1 0000 0 0 30 0",
            ),
            "two_cards",
        )
        .unwrap(),
    );
    assert_eq!(rows.len(), 8, "{rows:?}");
    assert_gains(&rows[..4], &NEC2C_UNLOADED, "the directive card");
    assert_gains(
        &rows[4..],
        &[(0.0, -999.99), (30.0, -7.61), (60.0, -1.75), (90.0, 0.03)],
        "the power-gain card",
    );
}

/// FND-298: every 4nec2 deck writes XNDA 1000, whose X digit only picks nec2c's
/// column labels; fnec read it as "normalize" and printed a NORMALIZED_PATTERN
/// nobody asked for. N (hundreds) is the request.
#[test]
fn the_n_digit_normalizes_and_the_x_digit_does_not() {
    let x = fnec(&dipole("", "RP 0 4 1 1000 0 0 30 0"), "x").unwrap();
    assert!(x.contains("RADIATION_PATTERN") && !x.contains("NORMALIZED_PATTERN"));
    let n = fnec(&dipole("", "RP 0 4 1 0500 0 0 30 0"), "n").unwrap();
    assert!(n.contains("NORMALIZED_PATTERN"));
}

/// FND-285: RP 1 asks for the fields near the ground at RHO (nec2c's `gfld`),
/// and modes 2–6 for patterns over a cliff or a radial screen; fnec solved each
/// as an RP 0 gain pattern, exit 0. Refused by name; RP 0 is the control.
#[test]
fn rp_modes_other_than_zero_are_refused() {
    for (rp, needle) in [
        ("RP 1 3 1 1000 0 0 5 0 1000", "RHO = 1000 m"),
        ("RP 2 4 1 1000 0 0 30 0", "linear cliff"),
        ("RP 4 4 1 1000 0 0 30 0", "radial-wire ground screen"),
    ] {
        let err = fnec(&dipole("", rp), "mode").expect_err(rp);
        assert!(
            err.contains(needle) && err.contains("field 1"),
            "{rp}: {err}"
        );
    }
    assert!(fnec(&dipole("", "RP 0 4 1 1000 0 0 30 0"), "mode0").is_ok());
}

/// D4: the retired 7-field form with a real XNDA, `RP 0 37 73 0.0 0.0 5.0 5.0`,
/// which nec2c refuses and fnec read without XNDA, is refused with its rewrite.
#[test]
fn a_real_xnda_is_refused_with_its_rewrite() {
    let err = fnec(&dipole("", "RP 0 4 1 0.0 0.0 30.0 0.0"), "realxnda").unwrap_err();
    assert!(err.contains("RP 0 4 1 0 0.0 0.0 30.0 0.0"), "{err}");
}
