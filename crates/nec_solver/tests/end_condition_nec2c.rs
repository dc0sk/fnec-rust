// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-156 — the Hallén free-end condition, gated against nec2c.
//!
//! The free-end rows used to impose `I = 0` at the end segment's MIDPOINT, half
//! a segment inside each tip, so every wire was modelled one segment short. The
//! error was first-order in the segment length and it was large: 32 Ω of
//! reactance on the corpus dipole, 52 Ω on a coupled pair, 51 Ω on a Yagi. It
//! survived years of tests because every one of them pinned fnec's own output,
//! and the Python "independent" reference shared the same rows.
//!
//! So these gates are external. The numbers are nec2c 1.3.1, captured
//! 2026-09-25 on the geometries below with `XQ` appended. Each band is about 1.5×
//! the corrected residual and far inside the old error, so the old rows fail
//! every one of them — see the `old` figures in each comment.
//!
//! The convergence gate is the one that names the defect class: a first-order
//! error that shrinks as N grows is discretisation; an error that the fix
//! removes wholesale at every N was never discretisation at all.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, first_delta_gap_feedpoint,
    ground_model_from_deck, solve_hallen_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn z_in(deck_text: &str) -> Complex64 {
    let deck = nec_parser::parse(deck_text).expect("deck parses").deck;
    let segs = build_geometry(&deck).expect("geometry builds");
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, FREQ, &[]).expect("solves");
    let ex = first_delta_gap_feedpoint(&deck).expect("a delta gap");
    let idx = segs
        .iter()
        .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        .expect("feed segment");
    Complex64::new(1.0, 0.0) / routed.currents[idx]
}

fn dipole(n: u32) -> String {
    format!(
        "CE\nGW 1 {n} 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 {} 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
        n.div_ceil(2)
    )
}

fn assert_near(what: &str, got: Complex64, want: (f64, f64), tol: (f64, f64)) {
    let (dr, dx) = (got.re - want.0, got.im - want.1);
    assert!(
        dr.abs() < tol.0 && dx.abs() < tol.1,
        "{what}: fnec {:.3} + j{:.3}, nec2c {} + j{}, off by {dr:+.2} / {dx:+.2} \
         (bands ±{} / ±{})",
        got.re,
        got.im,
        want.0,
        want.1,
        tol.0,
        tol.1
    );
}

/// The corpus half-wave dipole. Now 78.83 + j42.44; old 74.24 + j13.90.
#[test]
fn the_half_wave_dipole_tracks_nec2c() {
    assert_near(
        "dipole, N=51",
        z_in(&dipole(51)),
        (79.348, 46.223),
        (1.5, 5.5),
    );
}

/// The reactance gap must SHRINK with N, and stay within a first-order band.
/// Measured: 3.78 Ω at N=51, 2.22 at N=101 (nec2c at the same N each time).
/// Old: 32.3 and 16.6 — first-order too, which is why "it converges" was never
/// evidence that the end rows were right; the constant was.
#[test]
fn the_dipole_reactance_gap_shrinks_with_segment_count() {
    let gap51 = (z_in(&dipole(51)).im - 46.223).abs();
    let gap101 = (z_in(&dipole(101)).im - 46.405).abs();
    assert!(gap51 < 5.5, "N=51 reactance gap {gap51:.2} Ω");
    assert!(gap101 < 3.3, "N=101 reactance gap {gap101:.2} Ω");
    assert!(
        gap101 < 0.75 * gap51,
        "doubling N must shrink the gap: {gap51:.2} -> {gap101:.2} Ω"
    );
}

/// Two parallel half-wave dipoles 1 m apart, one fed: the mutual coupling this
/// finding was first reported against. Now 5.41 + j32.59; old 3.31 − j14.54.
#[test]
fn a_closely_coupled_pair_tracks_nec2c() {
    let deck = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1 0 -5.282 1 0 5.282 0.001\n\
                GE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near("pair at 1 m", z_in(deck), (6.2785, 37.739), (1.5, 7.5));
}

/// `corpus/yagi-5elm-51seg.nec`. Now 8.85 + j49.61; old 30.63 + j5.02 — the
/// resistance was off by a factor of 3.7, on the deck most users would try.
#[test]
fn a_five_element_yagi_tracks_nec2c() {
    let deck = "CE\nGW 1 51 -3.80 0 -5.388 -3.80 0 5.388 0.001\n\
                GW 2 51  0.00 0 -5.282  0.00 0 5.282 0.001\n\
                GW 3 51  2.64 0 -5.176  2.64 0 5.176 0.001\n\
                GW 4 51  5.28 0 -5.092  5.28 0 5.092 0.001\n\
                GW 5 51  8.45 0 -4.965  8.45 0 4.965 0.001\n\
                GE\nEX 0 2 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near("5-element Yagi", z_in(deck), (8.1749, 56.538), (1.5, 10.0));
}

/// FND-159: a merged collinear chain whose end segment is a one-segment `GW`
/// (0.782 m) beside 0.196 m segments. Equal-length extrapolation weights put the
/// reactance 26 Ω off (75.11 + j19.58); the true lengths give 78.13 + j40.07.
/// nec2c 1.3.1: 79.335 + j45.730. The remaining ~6 Ω is the coarse end segment's
/// own pulse-basis residual.
#[test]
fn a_mixed_length_merged_chain_tracks_nec2c() {
    let deck = "CE\nGW 1 1 0 0 -5.282 0 0 -4.5 0.001\nGW 2 50 0 0 -4.5 0 0 5.282 0.001\n\
                GE\nEX 0 2 23 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    let d = nec_parser::parse(deck).expect("deck parses").deck;
    let segs = build_geometry(&d).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&d));
    let routed = solve_hallen_routed(&d, &segs, &mut z, FREQ, &[]).expect("solves");
    let idx = segs
        .iter()
        .position(|s| s.tag == 2 && s.tag_index == 23)
        .expect("feed");
    let zin = Complex64::new(1.0, 0.0) / routed.currents[idx];
    assert_near("mixed-length chain", zin, (79.335, 45.730), (2.0, 8.0));
}
