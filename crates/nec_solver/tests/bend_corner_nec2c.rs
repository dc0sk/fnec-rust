// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-162 stage 1b — Hallén on bent conductors, gated against nec2c.
//!
//! Hallén's rows hold the tangential vector potential. At a bend that is
//! incomplete: the transverse divergence from the other, non-parallel section is
//! missing, and a 90° L came out 45 % off nec2c with the bend rows alone. The
//! corner term restores it. Every expectation here is nec2c 1.3.1, captured
//! 2026-09-27, and each deck is gated at two meshes so the error must SHRINK
//! with refinement — one mesh can agree by cancellation (the apex-fed inverted-V
//! did, at 0.7 %, and diverged under refinement without the term).

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck, solve_hallen_routed,
};
use num_complex::Complex64;

fn z_in(geometry: &str, feed: (u32, u32)) -> Complex64 {
    let text = format!(
        "CE\n{geometry}GE 0\nEX 0 {} {} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
        feed.0, feed.1
    );
    let deck = nec_parser::parse(&text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, 14.2e6, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, 14.2e6, &[]).expect("solves");
    assert!(routed.route.paths, "a bent conductor takes the path basis");
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Complex64::new(1.0, 0.0) / routed.source_current(idx)
}

fn err(z: Complex64, nec2c: Complex64) -> f64 {
    (z - nec2c).norm() / nec2c.norm()
}

/// A 90° L fed mid-arm. Bend rows alone: 84.6 − j50.2 (45 % off). With the
/// corner term: 5.2 % at 21 segments per arm, 3.1 % at 41.
#[test]
fn a_right_angle_bend_converges_to_nec2c() {
    let l = |n: u32| format!("GW 1 {n} 0 0 0 0 0 5 .001\nGW 2 {n} 0 0 5 5 0 5 .001\n");
    let e21 = err(z_in(&l(21), (1, 11)), Complex64::new(60.537, -118.240));
    let e41 = err(z_in(&l(41), (1, 21)), Complex64::new(60.385, -117.700));
    assert!(e41 < 0.04, "L, 41 per arm: {:.2} % from nec2c", e41 * 100.0);
    assert!(
        e41 < e21,
        "the error must shrink with refinement: {:.2} % -> {:.2} %",
        e21 * 100.0,
        e41 * 100.0
    );
}

/// Two bends: the middle section is referenced at one node, so its corner
/// integral enters the OTHER node's potential row — a branch a single-bend deck
/// never reaches. nec2c 14.39 − j219.22 (11 per wire), 14.31 − j218.18 (21).
#[test]
fn a_two_bend_u_tracks_nec2c() {
    let u = |n: u32| {
        format!("GW 1 {n} 0 0 0 0 0 3 .001\nGW 2 {n} 0 0 3 3 0 3 .001\nGW 3 {n} 3 0 3 3 0 0 .001\n")
    };
    let e11 = err(z_in(&u(11), (2, 6)), Complex64::new(14.39, -219.22));
    let e21 = err(z_in(&u(21), (2, 11)), Complex64::new(14.31, -218.18));
    assert!(
        e21 < 0.02,
        "U, 21 per wire: {:.2} % from nec2c",
        e21 * 100.0
    );
    assert!(
        e21 < e11,
        "the error must shrink with refinement: {:.2} % -> {:.2} %",
        e11 * 100.0,
        e21 * 100.0
    );
}

/// An inverted-V fed off its apex, at 0.36 of an arm: segment 8 of 21 and 23 of
/// 63, the same point (`scripts/nec2c-fine-mesh.py`'s mapping), so the two meshes
/// solve one antenna and the error is measured on it twice. The reference is
/// nec2c's own converged value, 933.23 + j892.42 at 81× (1701 segments an arm,
/// `EK 0`; `scripts/nec2c-converged-series.py --ek --to 81`, captured
/// 2026-10-16): its 1 / 3 / 9 / 27 / 81 × R is 853.71, 896.05, 916.08, 926.52,
/// 933.23, X 889.25 … 892.42, about 12 Ω (0.9 %) still to move in R, a third of
/// the gate. fnec: 3.2 % at 21 per arm, 1.6 % at 63.
///
/// FND-258: this fed segment 5 of 21 and 10 of 41 — centres at 0.214 and 0.232 of
/// the arm, two antennas whose nec2c values differ by 770 Ω in X — each against
/// its own same-mesh nec2c, which then moved 17.5 % on refinement against a 7 %
/// gate (FND-257). Not that feed point again: near 0.21 of the arm the V's current
/// is near a minimum, |Z| ≈ 3.6 kΩ, and nec2c's R there has not settled at 81×
/// (steps 329, 148, 75, 48 Ω, the ratio rising).
#[test]
fn an_off_apex_inverted_v_converges_to_nec2c() {
    let v = |n: u32| format!("GW 1 {n} -5 0 0 0 0 3 .001\nGW 2 {n} 0 0 3 5 0 0 .001\n");
    let nec2c = Complex64::new(933.23, 892.42);
    let e21 = err(z_in(&v(21), (1, 8)), nec2c);
    let e63 = err(z_in(&v(63), (1, 23)), nec2c);
    assert!(
        e63 < 0.03,
        "inverted-V, 63 per arm: {:.2} % from nec2c",
        e63 * 100.0
    );
    assert!(
        e63 < e21,
        "the error must shrink with refinement: {:.2} % -> {:.2} %",
        e21 * 100.0,
        e63 * 100.0
    );
}

/// FND-302: the residual describes the system the bent-path solve solved — the
/// corner term included. The same solve, with the corner dropped from the
/// residual's inputs, must read large, so this fixture's corner is not
/// negligible and the first bound cannot pass by the term being ~0.
#[test]
fn the_residual_includes_the_corner_term_the_solve_adds() {
    let text = "CE\nGW 1 21 0 0 0 0 0 5 .001\nGW 2 21 0 0 5 5 0 5 .001\nGE 0\n\
                EX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, 14.2e6, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, 14.2e6, &[]).expect("solves");
    assert!(routed.route.paths, "a bent conductor takes the path basis");
    let ri = routed
        .residual_inputs
        .clone()
        .expect("a delta-gap solve has residual inputs");
    assert!(!ri.corner.is_empty(), "a bend carries corner entries");
    let (_, rel) = routed.residual.expect("the session computes the residual");
    assert!(rel < 1e-9, "residual of the solved system: {rel:e}");
    assert_eq!(routed.residual, Some(ri.residual(&z, &routed.currents)));
    let mut no_corner = ri;
    no_corner.corner.clear();
    let (_, without) = no_corner.residual(&z, &routed.currents);
    assert!(
        without > 1e-2,
        "without the corner the residual must be large: {without:e}"
    );
}
