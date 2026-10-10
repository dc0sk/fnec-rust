// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-164 — the Hallén solve's Tikhonov term must not bias the answer.
//!
//! The normal equations carried an absolute `λ = 1e-8`, whose weight relative
//! to a column depended on that column's scale. On an electrically tiny dipole
//! the homogeneous `sin` column is small, and the term moved R by 4.7 %.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, find_deck_segment, ground_model_from_deck,
    solve_hallen_routed,
};
use num_complex::Complex64;

/// A 2 cm dipole at 14.2 MHz (kL ≈ 0.006).
///
/// Two anchors. The λ → 0 limit of this solve, measured by sweeping the relative
/// weight from 1e-6 to 1e-16, is R = 1.5021e-4 Ω; the old absolute term gave
/// 1.57e-4, 4.7 % off it, so the 1 % band fails on the old code. Since FND-227
/// refined the free ends the mesh is different and so is its limit: swept again
/// over 1e-12 / 1e-14 / 1e-16 / 1e-18, R = 1.5324e-4 / 1.5323e-4 / 1.5323e-4 /
/// 1.5323e-4. And the
/// textbook short-dipole value `20 π² (L/λ)²` = 1.77e-4 Ω keeps the pin from
/// being only fnec's own number: a thin-wire MoM on 21 segments sits ~15 % under
/// that triangular-current idealisation.
#[test]
fn a_tiny_dipole_is_not_biased_by_the_regularisation() {
    let text =
        "CE\nGW 1 21 0 0 -0.01 0 0 0.01 0.0001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let f = 14.2e6;
    let mut z = assemble_z_matrix_with_ground(&segs, f, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, f, &[]).expect("solves");
    // nec2c numbers the deck's segments (FND-227).
    let feed = find_deck_segment(&segs, 1, 11).expect("feed");
    let zin = Complex64::new(1.0, 0.0) / routed.source_current(feed);

    let limit = 1.5323e-4;
    assert!(
        (zin.re - limit).abs() < 0.01 * limit,
        "R = {:.4e}, the unregularised limit is {limit:.4e}",
        zin.re
    );
    let lambda = 299_792_458.0 / f;
    let textbook = 20.0 * std::f64::consts::PI.powi(2) * (0.02 / lambda).powi(2);
    assert!(
        (zin.re / textbook - 1.0).abs() < 0.2,
        "R = {:.4e} against 20π²(L/λ)² = {textbook:.4e}",
        zin.re
    );
}
