// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-194 — a negative `SIG` on a `GN` card is NEC's other form of the ground:
//! `|SIG|` is the imaginary part of the relative permittivity, `εc = εr − j|SIG|`.
//!
//! nec2c 1.3.1 reads `GN 2 0 0 0 13 -5` at 14.2 MHz as `CONDUCTIVITY: 3.950E-03
//! MHOS/METER` (5·ωε₀), and solves this dipole to 73.874 − j20.350 Ω, the same as
//! with `SIG = 0.00395`. fnec clamped it to σ = 0 in the matrix — 70.586 − j26.565,
//! exactly its lossless-ground answer — and used it with the sign flipped in the
//! far field. The gate is the equivalence, through the matrix (the impedance) and
//! the far field (the pattern over ground): fnec's matrix model is not nec2c's,
//! so the absolute value is not what is under test here.

use nec_solver::{
    assemble_z_matrix_with_ground, build_deck_stamps, build_geometry, compute_radiation_pattern,
    ground_model_from_deck, solve_hallen_routed, validate::pre_solve_error, FarFieldPoint,
};
use num_complex::Complex64;

const F: f64 = 14.2e6;
/// ε₀ as `complex_permittivity` has it.
const EPS0: f64 = 8.854_187_817e-12;

/// The feedpoint impedance and the total gain at θ = 60°, φ = 0 for a 10 m
/// horizontal dipole 5 m over `GN 2` with the given `SIG`.
fn dipole_over(sig: f64) -> (Complex64, f64) {
    let text = format!(
        "CE\nGW 1 21 -5 0 5 5 0 5 .001\nGE 1\nGN 2 0 0 0 13 {sig}\nEX 0 1 11 0 1 0\n\
         FR 0 1 0 0 14.2 0\nEN\n"
    );
    let deck = nec_parser::parse(&text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let ground = ground_model_from_deck(&deck);
    assert_eq!(pre_solve_error(&deck, &segs, &ground), None);
    let mut z = assemble_z_matrix_with_ground(&segs, F, &ground);
    let loads = build_deck_stamps(&deck, &segs, F).diagonal;
    let routed = solve_hallen_routed(&deck, &segs, &mut z, F, &loads).expect("solves");
    let feed = segs
        .iter()
        .position(|s| s.tag == 1 && s.tag_index == 11)
        .expect("feed");
    let z_in = Complex64::new(1.0, 0.0) / routed.source_current(feed);
    let pt = FarFieldPoint {
        theta_deg: 60.0,
        phi_deg: 0.0,
    };
    let gain =
        compute_radiation_pattern(&segs, &routed.currents, F, &[pt], &ground)[0].gain_total_dbi;
    (z_in, gain)
}

#[test]
fn a_negative_sigma_solves_as_its_conductivity_equivalent() {
    let (z_neg, g_neg) = dipole_over(-5.0);
    let (z_eq, g_eq) = dipole_over(5.0 * 2.0 * std::f64::consts::PI * F * EPS0);
    let (z_zero, g_zero) = dipole_over(0.0);
    println!("SIG -5: {z_neg:.4} {g_neg:.4} dBi; equivalent: {z_eq:.4} {g_eq:.4}; zero: {z_zero:.4} {g_zero:.4}");
    assert!(
        (z_neg - z_eq).norm() < 1e-6 * z_eq.norm(),
        "matrix: {z_neg} vs {z_eq}"
    );
    assert!(
        (g_neg - g_eq).abs() < 1e-6,
        "far field: {g_neg} vs {g_eq} dBi"
    );
    // And it is not the lossless ground it used to be clamped to.
    assert!(
        (z_neg - z_zero).norm() > 1.0,
        "{z_neg} vs lossless {z_zero}"
    );
}
