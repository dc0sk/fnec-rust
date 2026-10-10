// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-172, FND-175 — one rule for "straight".
//!
//! The collinear merge and `ConductorPath::is_trivial` each decided whether two
//! wire cards form one straight conductor, by different rules: the merge needed
//! the direction within 1e-6 rad and equal radii, `is_trivial` the direction
//! within 4.5e-5 rad and no radius at all. A path between the two was called
//! trivial, then not merged, and took the plain basis's pairwise junction row:
//! garbage, silently. Expectations are nec2c 1.3.1, captured 2026-09-28.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck, solve_hallen_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn z_in(geometry: &str, feed: (u32, u32)) -> Complex64 {
    let text = format!(
        "CE\n{geometry}GE 0\nEX 0 {} {} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
        feed.0, feed.1
    );
    let deck = nec_parser::parse(&text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, FREQ, &[]).expect("solves");
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Complex64::new(1.0, 0.0) / routed.source_current(idx)
}

fn rel(a: Complex64, b: Complex64) -> f64 {
    (a - b).norm() / b.norm()
}

/// A 10 m wire at 63°, split into two cards with 4-decimal coordinates: a kink of
/// 2.7e-5 rad, inside the old window. It was 0.321 − j930.33; it must be the same
/// antenna as the wire written as one card, and track nec2c (119.79 − j71.13).
#[test]
fn a_wire_split_with_rounded_coordinates_is_one_wire() {
    let split = z_in(
        "GW 1 25 0 0 0 2.2361 0 4.4721 .001\nGW 2 25 2.2361 0 4.4721 4.4721 0 8.9443 .001\n",
        (1, 13),
    );
    let one = z_in("GW 1 50 0 0 0 4.4721 0 8.9443 .001\n", (1, 13));
    assert!(
        rel(split, one) < 1e-4,
        "split {split:.4} vs one card {one:.4}"
    );
    let nec2c = Complex64::new(119.79, -71.13);
    assert!(rel(split, nec2c) < 0.06, "{split:.3} vs nec2c {nec2c}");
}

/// A stepped-radius collinear element (4 mm / 8 mm / 4 mm), fed in the thick
/// middle. The merge refuses unequal radii, so it is a conductor path, not one
/// wire; it was −7.35 − j1181 on the plain junction rows. Two meshes, converging.
///
/// Against nec2c on a 3× mesh (the feed on segment 3k − 1, the same point),
/// captured 2026-10-10 — the finest mesh on which the thick segments stay at
/// least 3 radii long. On a fat wire nec2c's reactance keeps rising with the mesh
/// (31.98 / 37.10 / 39.13 / 40.84 at 1 / 3 / 5 / 9 × the coarse deck) until the
/// thin-wire kernel breaks down (27× reads 99.99 − j2.97 on the fine deck); since
/// FND-227 refined the free ends fnec sits inside that band (76.02 + j39.74,
/// 76.31 + j40.56), so nec2c on the deck's own mesh was no longer a reference.
#[test]
fn a_stepped_radius_element_tracks_nec2c() {
    let element = |a: u32, b: u32| {
        format!(
            "GW 1 {a} 0 0 -5.282 0 0 -1.5 .004\nGW 2 {b} 0 0 -1.5 0 0 1.5 .008\nGW 3 {a} 0 0 1.5 0 0 5.282 .004\n"
        )
    };
    let coarse = rel(
        z_in(&element(15, 21), (2, 11)),
        Complex64::new(76.877, 37.098),
    );
    let fine = rel(
        z_in(&element(30, 41), (2, 21)),
        Complex64::new(76.865, 39.710),
    );
    println!(
        "stepped radius: {:.2} % -> {:.2} %",
        coarse * 100.0,
        fine * 100.0
    );
    assert!(fine < 0.05, "{:.2} % from nec2c", fine * 100.0);
    assert!(fine < coarse, "the error must shrink with refinement");
}
