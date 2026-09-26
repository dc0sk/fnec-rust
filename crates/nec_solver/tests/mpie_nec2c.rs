// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-157 — the MPIE solver against nec2c, through the session every frontend
//! uses.
//!
//! MPIE integrated the reduced kernel's self and adjacent terms with a 6-point
//! rule, which cannot resolve the kernel's peak (width ~ the wire radius, 1 mm on
//! a 200 mm segment). It read 6% low on a dipole, 35 Ω off in R on a Yagi, and
//! 260 Ω off in X on a Y-junction — the topology MPIE exists for. The static part
//! is now integrated exactly. nec2c 1.3.1, captured 2026-09-26.

use nec_solver::{build_geometry, ground_model_from_deck, solve_mpie_session};
use num_complex::Complex64;

const F: f64 = 14.2e6;

fn z_mpie(text: &str, feed: (u32, u32)) -> Complex64 {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let currents =
        solve_mpie_session(&deck, &segs, &ground_model_from_deck(&deck), F).expect("solves");
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Complex64::new(1.0, 0.0) / currents[idx]
}

fn assert_near(what: &str, z: Complex64, want: (f64, f64), tol: (f64, f64)) {
    assert!(
        (z.re - want.0).abs() < tol.0 && (z.im - want.1).abs() < tol.1,
        "{what}: MPIE {z:.3}, nec2c {} + j{}",
        want.0,
        want.1
    );
}

/// Was 74.44 + j41.75.
#[test]
fn a_half_wave_dipole_tracks_nec2c() {
    let d = "CE\nGW 1 41 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nEX 0 1 21 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near("dipole", z_mpie(d, (1, 21)), (79.107, 45.047), (0.8, 1.0));
}

/// `corpus/yagi-5elm-51seg.nec`. Was 43.47 + j43.77 — R 5× the truth.
#[test]
fn a_five_element_yagi_tracks_nec2c() {
    let d = "CE\nGW 1 51 -3.80 0 -5.388 -3.80 0 5.388 0.001\nGW 2 51  0.00 0 -5.282  0.00 0 5.282 0.001\n\
             GW 3 51  2.64 0 -5.176  2.64 0 5.176 0.001\nGW 4 51  5.28 0 -5.092  5.28 0 5.092 0.001\n\
             GW 5 51  8.45 0 -4.965  8.45 0 4.965 0.001\nGE 0\nEX 0 2 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near("Yagi", z_mpie(d, (2, 26)), (8.1749, 56.538), (0.5, 1.5));
}

/// A degree-3 Y-junction — the case `--solver mpie` is recommended for. Was
/// 63.67 − j322.20: 260 Ω of reactance off.
#[test]
fn a_y_junction_tracks_nec2c() {
    let d = "CE\nGW 1 20 0 0 0 5 0 0 0.001\nGW 2 20 0 0 0 -2.5 4.330127 0 0.001\n\
             GW 3 20 0 0 0 -2.5 -4.330127 0 0.001\nGE 0\nEX 0 1 10 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near(
        "Y-junction",
        z_mpie(d, (1, 10)),
        (67.215, -63.033),
        (2.5, 2.5),
    );
}
