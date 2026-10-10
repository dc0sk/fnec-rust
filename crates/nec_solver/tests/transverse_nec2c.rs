// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-162 stage 4, FND-171, FND-174 — the transverse-divergence term from
//! sources that share no node with the observation wire: other wires, and the
//! ground images.
//!
//! Hallén's rows couple two segments through `cos α` alone, and lose the part of
//! `∇·A` a non-parallel source contributes. Perpendicular wires did not couple at
//! all; a 45° dipole over perfect ground solved to its free-space value, to every
//! digit; an inverted-V over ground drifted away from nec2c as the mesh was
//! refined. The corner term of FND-162 stage 1b is the missing piece and needs no
//! shared node, so it now takes every non-parallel source, images included.
//!
//! Expectations are nec2c 1.3.1, captured 2026-09-29. Each deck is gated at two
//! meshes and its error must shrink. The kill criteria were fixed by the design
//! review (Fable) before the code was written.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck, solve_hallen_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

/// The routed solve of a deck body (geometry, GE/GN, EX): segments and currents.
fn solve(body: &str) -> (Vec<nec_solver::Segment>, Vec<Complex64>) {
    let deck = nec_parser::parse(&format!("CE\n{body}FR 0 1 0 0 14.2 0\nEN\n"))
        .expect("parses")
        .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, FREQ, &[]).expect("solves");
    (segs, routed.currents)
}

fn z_in(body: &str, feed: (u32, u32)) -> Complex64 {
    let (segs, currents) = solve(body);
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Complex64::new(1.0, 0.0) / currents[idx]
}

fn rel(a: Complex64, b: Complex64) -> f64 {
    (a - b).norm() / b.norm()
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

/// FND-171. A 45° dipole 10 m over perfect ground. Before: 79.505 + j46.847 at
/// 41 segments, the free-space value to every digit, and 5.2 % → 7.4 % off nec2c,
/// diverging. Kill criterion: under 3.5 % at 81, and shrinking.
#[test]
fn a_slanted_dipole_over_ground_sees_its_image() {
    let dipole = |n: u32| {
        format!(
            "GW 1 {n} -3.7477 0 6.2523 3.7477 0 13.7477 .001\nGE 1\nGN 1\nEX 0 1 {} 0 1 0\n",
            n / 2 + 1
        )
    };
    let e41 = rel(z_in(&dipole(41), (1, 21)), Complex64::new(78.285, 42.395));
    let e81 = rel(z_in(&dipole(81), (1, 41)), Complex64::new(78.440, 42.591));
    assert_converges("45° dipole over GN 1", e41, e81, 0.035);
}

/// FND-174. An apex-fed 90° inverted-V, apex 10 m over perfect ground: the corner
/// term now takes the images too. Before: 8.4 → 11.4 → 13.0 %, diverging. Kill
/// criterion: under 5 % at 81 per arm, and shrinking. And it must equal the same
/// antenna with its image written out as wires in free space, driven −1 V: the
/// image term IS the doubled problem. Against nec2c on a finer mesh (27× at 41,
/// 9× at 81 per arm; the apex feed on the same point), captured 2026-10-10: once
/// FND-227 refined the free ends fnec outran nec2c on the deck's own mesh
/// (3.35 → 3.47 %, not shrinking); against the finer mesh 2.04 → 1.25 %.
#[test]
fn an_inverted_v_over_ground_converges_and_equals_its_explicit_image() {
    let arms = |n: u32| {
        format!("GW 1 {n} 0 0 10 -3.7477 0 6.2523 .001\nGW 2 {n} 0 0 10 3.7477 0 6.2523 .001\n")
    };
    let over = |n: u32| format!("{}GE 1\nGN 1\nEX 0 1 1 0 1 0\n", arms(n));
    let e41 = rel(z_in(&over(41), (1, 1)), Complex64::new(53.401, 9.52));
    let e81 = rel(z_in(&over(81), (1, 1)), Complex64::new(53.345, 9.50));
    assert_converges("inverted-V over GN 1", e41, e81, 0.05);

    let explicit = format!(
        "{}GW 3 21 0 0 -10 -3.7477 0 -6.2523 .001\nGW 4 21 0 0 -10 3.7477 0 -6.2523 .001\n\
         GE 0\nEX 0 1 1 0 1 0\nEX 0 3 1 0 -1 0\n",
        arms(21)
    );
    let (with_ground, as_wires) = (z_in(&over(21), (1, 1)), z_in(&explicit, (1, 1)));
    assert!(
        rel(with_ground, as_wires) < 1e-9,
        "ground {with_ground} vs explicit image {as_wires}"
    );
}

/// FND-162 stage 4. A horizontal wire centred 0.7 m over the top of a fed
/// vertical dipole. Before: exactly zero current on it. Kill criteria: its
/// current within 8 % of nec2c at 41 (relative to nec2c's peak), shrinking; and
/// antisymmetric — the centre current under 1e-3 of the peak — at 0.7 m and at
/// 5 cm, where a quadrature graded only toward section ends broke it.
#[test]
fn perpendicular_wires_couple_and_stay_antisymmetric() {
    let deck = |n: u32, gap: f64| {
        format!(
            "GW 1 {n} 0 0 -5 0 0 5 .001\nGW 2 {n} -5 0 {z} 5 0 {z} .001\nGE 0\nEX 0 1 {} 0 1 0\n",
            n / 2 + 1,
            z = 5.0 + gap
        )
    };
    let wire2 = |n: u32, gap: f64| -> Vec<Complex64> {
        let (segs, currents) = solve(&deck(n, gap));
        // nec2c numbers the deck's segments (FND-227).
        segs.iter()
            .zip(&currents)
            .filter(|(s, _)| s.tag == 2 && s.is_deck_segment())
            .map(|(_, &i)| i)
            .collect()
    };
    // nec2c at the quarter points of wire 2, and its peak |I|.
    let err = |got: &[Complex64], peak: f64, refs: &[(usize, f64, f64)]| {
        refs.iter()
            .map(|&(seg, re, im)| (got[seg - 1] - Complex64::new(re, im)).norm() / peak)
            .fold(0.0, f64::max)
    };
    let e21 = err(
        &wire2(21, 0.7),
        3.543803e-4,
        &[(6, -3.3699e-4, -1.0965e-4), (16, 3.3699e-4, 1.0965e-4)],
    );
    let e41 = err(
        &wire2(41, 0.7),
        3.561313e-4,
        &[(11, -3.3896e-4, -1.0796e-4), (31, 3.3896e-4, 1.0796e-4)],
    );
    assert_converges("wire over a vertical dipole", e21, e41, 0.08);

    for gap in [0.7, 0.05] {
        for n in [21u32, 41] {
            let w = wire2(n, gap);
            let peak = w.iter().map(|c| c.norm()).fold(0.0, f64::max);
            let centre = w[n as usize / 2].norm();
            assert!(peak > 0.0, "gap {gap} N={n}: the wire must couple at all");
            assert!(
                centre < 1e-3 * peak,
                "gap {gap} N={n}: centre {centre:.3e} vs peak {peak:.3e}"
            );
        }
    }
}
