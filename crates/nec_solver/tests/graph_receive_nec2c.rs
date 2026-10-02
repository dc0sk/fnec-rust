// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-162 stage 5 — the plane-wave RECEIVE solve on junctions and loops.
//!
//! A plane wave is a delta gap on every segment, `V_p = E_t(p)·Δl_p`, so the
//! section-graph system takes it as it takes a feed; the plan builds that system
//! once and reuses it for every incidence direction. Junction and loop decks lit
//! by a plane wave were refused (`JunctionedGeometryNotSupported`).
//!
//! Every expectation is nec2c 1.3.1, captured 2026-10-02: the peak |I| and the
//! currents at a handful of segments (the feed corner, a mid-run, the node). The
//! whole current table agrees too (max |ΔI| / peak at 21 → 41 per wire: loop
//! broadside 0.78 → 0.48 %, at 45° 0.64 → 0.42 %; T broadside 0.94 → 0.55 %, at
//! 45° 0.82 → 0.56 %). Each deck is gated at two meshes and must converge.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck,
    solve_hallen_planewave_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn receive(geometry: &str, wave: &str) -> Vec<Complex64> {
    let deck = nec_parser::parse(&format!(
        "CE\n{geometry}GE 0\n{wave}FR 0 1 0 0 14.2 0\nEN\n"
    ))
    .expect("parses")
    .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    solve_hallen_planewave_routed(&deck, &segs, &z, FREQ).expect("receive solve")
}

/// Max over the pinned segments (1-based) of |ΔI| / peak |I_nec2c|.
fn err(got: &[Complex64], peak: f64, refs: &[(usize, f64, f64)]) -> f64 {
    refs.iter()
        .map(|&(seg, re, im)| (got[seg - 1] - Complex64::new(re, im)).norm() / peak)
        .fold(0.0, f64::max)
}

fn assert_converges(label: &str, coarse: f64, fine: f64, bound: f64) {
    println!("{label}: {:.2} % -> {:.2} %", coarse * 100.0, fine * 100.0);
    assert!(
        fine < bound,
        "{label}: {:.2} % at the finer mesh",
        fine * 100.0
    );
    assert!(
        fine < coarse,
        "{label}: must shrink, {coarse:.4} -> {fine:.4}"
    );
}

fn square_loop(n: u32) -> String {
    let a = 5.278;
    format!(
        "GW 1 {n} 0 0 0 {a} 0 0 .001\nGW 2 {n} {a} 0 0 {a} 0 {a} .001\n\
         GW 3 {n} {a} 0 {a} 0 0 {a} .001\nGW 4 {n} 0 0 {a} 0 0 0 .001\n"
    )
}

/// A 1 λ square loop in the x–z plane, lit broadside (θ = 90°, φ = 90°).
/// Kill criterion: under 1 % at 41, shrinking. Measured 0.55 → 0.34 % on these
/// segments.
#[test]
fn a_square_loop_receives_like_nec2c() {
    let wave = "EX 1 1 1 0 90 90 0\n";
    let e21 = err(
        &receive(&square_loop(21), wave),
        5.031_719e-2,
        &[
            (1, 2.096_900e-2, 2.683_200e-2),
            (11, 3.757_000e-11, 7.630_300e-12),
            (21, -2.096_900e-2, -2.683_200e-2),
            (22, -2.263_100e-2, -2.895_000e-2),
            (53, -3.753_000e-11, -7.629_100e-12),
            (84, 2.263_100e-2, 2.895_000e-2),
        ],
    );
    let e41 = err(
        &receive(&square_loop(41), wave),
        5.031_120e-2,
        &[
            (1, 2.137_700e-2, 2.736_200e-2),
            (21, 2.145_400e-11, 4.335_800e-12),
            (41, -2.137_700e-2, -2.736_200e-2),
            (42, -2.222_800e-2, -2.844_800e-2),
            (103, -2.151_600e-11, -4.476_700e-12),
            (164, 2.222_800e-2, 2.844_800e-2),
        ],
    );
    assert_converges("loop, broadside", e21, e41, 0.01);
}

/// A T (4 m stem, 3 m bar halves) lit at θ = 45°: every arm sees the field, and
/// the node splits the received current three ways. Kill criterion: under 1 % at
/// 41, shrinking. Measured 0.82 → 0.56 %.
#[test]
fn a_t_receives_like_nec2c() {
    let tee = |n: u32| {
        format!(
            "GW 1 {n} 0 0 0 0 0 4 .001\nGW 2 {n} 0 0 4 -3 0 4 .001\nGW 3 {n} 0 0 4 3 0 4 .001\n"
        )
    };
    let wave = "EX 1 1 1 0 45 0 0\n";
    let e21 = err(
        &receive(&tee(21), wave),
        5.530_196e-3,
        &[
            (1, 9.733_200e-5, -2.987_300e-4),
            (11, 1.443_900e-3, -3.777_200e-3),
            (21, 1.719_900e-3, -4.481_800e-3),
            (22, 3.225_400e-3, -4.492_200e-3),
            (43, -1.564_700e-3, 8.379_500e-5),
            (63, -1.232_500e-4, 2.556_000e-5),
        ],
    );
    let e41 = err(
        &receive(&tee(41), wave),
        5.554_170e-3,
        &[
            (1, 5.442_700e-5, -1.677_800e-4),
            (21, 1.447_800e-3, -3.783_100e-3),
            (41, 1.714_900e-3, -4.476_000e-3),
            (42, 3.240_600e-3, -4.510_800e-3),
            (83, -1.554_400e-3, 7.018_500e-5),
            (123, -6.977_600e-5, 1.438_800e-5),
        ],
    );
    assert_converges("T at 45°", e21, e41, 0.01);
}

/// A loaded receive deck on the graph is refused, not solved with its loads in
/// the wrong form (the routed solve stamps them for the plain basis).
#[test]
fn a_loaded_plane_wave_junction_deck_is_refused() {
    let deck = nec_parser::parse(&format!(
        "CE\n{}GE 0\nLD 0 2 5 5 50 0 0\nEX 1 1 1 0 45 0 0\nFR 0 1 0 0 14.2 0\nEN\n",
        square_loop(21)
    ))
    .expect("parses")
    .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let err = solve_hallen_planewave_routed(&deck, &segs, &z, FREQ).expect_err("refused");
    assert!(err.to_string().contains("LD loads"), "{err}");
}
