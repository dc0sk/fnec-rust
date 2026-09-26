// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-158 — the second homogeneous term of Hallén's equation, gated against
//! nec2c on antennas whose current is NOT symmetric about the conductor's
//! midpoint.
//!
//! Along a straight conductor Hallén's homogeneous solution is
//! `C·cos(k·s) + D·sin(k·s)`. fnec carried the cos term alone, which is exact
//! for a centre-fed symmetric dipole and nothing else: a least-squares
//! compromise that read 12% low on an off-centre feed, 10% high on a vertical
//! dipole over ground, and 18× wrong on a dipole beside an offset parasitic.
//! Every corpus gate before this was symmetric, so none could see it.
//!
//! nec2c 1.3.1, captured 2026-09-26. Each band is ~1.5× the corrected residual
//! and far inside the old error (the `was` figure in each comment).

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, first_delta_gap_feedpoint,
    ground_model_from_deck, merged_grouping, solve_hallen_routed, solve_hallen_sinusoidal_basis,
};
use num_complex::Complex64;

const F: f64 = 14.2e6;

fn deck(text: &str) -> (nec_model::deck::NecDeck, Vec<nec_solver::Segment>, usize) {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let ex = first_delta_gap_feedpoint(&deck).expect("feed");
    let idx = segs
        .iter()
        .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        .expect("feed segment");
    (deck, segs, idx)
}

fn z_hallen(text: &str) -> Complex64 {
    let (deck, segs, idx) = deck(text);
    let mut z = assemble_z_matrix_with_ground(&segs, F, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, F, &[]).expect("solves");
    Complex64::new(1.0, 0.0) / routed.source_current(idx)
}

fn assert_near(what: &str, z: Complex64, want: (f64, f64), tol: (f64, f64)) {
    let (dr, dx) = (z.re - want.0, z.im - want.1);
    assert!(
        dr.abs() < tol.0 && dx.abs() < tol.1,
        "{what}: fnec {z:.3}, nec2c {} + j{}, off by {dr:+.2} / {dx:+.2}",
        want.0,
        want.1
    );
}

const OFF_CENTRE: &str =
    "CE\nGW 1 42 0 0 0 11.661903789690601 0 0 .001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

/// 0.55 λ wire fed at a quarter of its length. Was 324.12 + j463.78.
#[test]
fn an_off_centre_feed_tracks_nec2c() {
    assert_near(
        "off-centre",
        z_hallen(OFF_CENTRE),
        (366.93, 483.11),
        (6.0, 15.0),
    );
}

/// The same deck through the sinusoidal basis, which carried the same single
/// column.
#[test]
fn an_off_centre_feed_tracks_nec2c_on_the_sinusoidal_basis() {
    let (d, segs, idx) = deck(OFF_CENTRE);
    let z = assemble_z_matrix_with_ground(&segs, F, &ground_model_from_deck(&d));
    let rhs = build_hallen_rhs(&d, &segs, F).expect("rhs");
    let (ep, j) = merged_grouping(&segs);
    let sol = solve_hallen_sinusoidal_basis(&z, &rhs.rhs, &rhs.cos_vec, &rhs.sin_vec, &ep, &j)
        .expect("solves");
    let zin = Complex64::new(1.0, 0.0) / sol.currents[idx];
    assert_near("off-centre, sinusoidal", zin, (366.93, 483.11), (6.0, 15.0));
}

/// A vertical λ/2 dipole 0.5 m over perfect ground — image theory is exact, so
/// the ground model cannot be blamed. Was 113.63 + j49.80.
#[test]
fn a_vertical_dipole_over_pec_tracks_nec2c() {
    let d = "CE\nGW 1 51 0 0 0.5 0 0 11.064 0.001\nGE 1\nGN 1\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near(
        "vertical over PEC",
        z_hallen(d),
        (103.22, 48.52),
        (2.0, 7.0),
    );
}

/// A driven λ/2 dipole beside a parasitic λ/2 shifted 4 m along its axis: the
/// parasitic's current is strongly asymmetric. Was 459.03 − j89.50 (18× off).
#[test]
fn an_offset_parasitic_tracks_nec2c() {
    let d = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1 0 -1.282 1 0 9.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near(
        "offset parasitic",
        z_hallen(d),
        (25.49, -42.20),
        (3.0, 10.0),
    );
}

/// The same off-centre 0.55 λ geometry as a straight CONDUCTOR PATH (two wires
/// meeting start-to-start), so the path solver's column is gated too. Was
/// 324.14 + j463.82.
#[test]
fn an_off_centre_feed_on_a_conductor_path_tracks_nec2c() {
    let d = "CE\nGW 1 21 0 0 0 0 0 5.831 0.001\nGW 2 21 0 0 0 0 0 -5.831 0.001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near(
        "off-centre path",
        z_hallen(d),
        (366.96, 483.16),
        (6.0, 15.0),
    );
}

/// A current source at the same off-centre feed must price the same port: the
/// current-source shape solves carry the column too, or EX 4 and EX 0 split
/// again (the FND-118 shape).
#[test]
fn a_current_source_at_an_off_centre_feed_matches_the_voltage_drive() {
    let v = z_hallen(OFF_CENTRE);
    let text = OFF_CENTRE.replace("EX 0 1 11 0 1.0 0.0", "EX 4 1 11 0 1.0 0.0");
    let d = nec_parser::parse(&text).expect("parses").deck;
    let segs = build_geometry(&d).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, F, &ground_model_from_deck(&d));
    let routed = solve_hallen_routed(&d, &segs, &mut z, F, &[]).expect("solves");
    let port = routed
        .port_voltage
        .expect("a current source prices the port");
    assert!(
        (port - v).norm() < 1e-6 * v.norm(),
        "EX 4 {port:.4} vs EX 0 {v:.4}"
    );
}
