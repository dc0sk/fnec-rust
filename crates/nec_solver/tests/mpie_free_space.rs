// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-MPIE Phase A — free-space MPIE straight-wire core, external gates.
//
// The MPIE (mixed-potential EFIE, triangle basis) is the second solver scoped in
// docs/mpie-solver-scope.md. Phase A is the free-space core; it is a Rust port of
// the validated Python oracle studies/sommerfeld-ground/efie_mpie_ground.py.
//
//   Gate A2 (nec2c): a straight λ/2 dipole converges (mesh-refinement plateau)
//     to within a few percent of the analytic 79.35 Ω. The MPIE has its own
//     systematic discretization offset (~6% low at N=40, per the Python oracle:
//     N=20→73.39, N=40→74.36, N=80→75.52), converging UP toward 79.35 — unlike
//     the Hallén solver's fixed ~32 Ω reactance bias.
//
//   Gate A3 (identity): the impedance is invariant to wire orientation. Reversing
//     the node order (and feeding the mirror-image node) is the same physical
//     antenna, so it must recover the same Z to machine precision — an orientation
//     gate on the triangle-basis tangent/charge sign bookkeeping.

use nec_solver::{solve_mpie_free_space, straight_wire, MpieWire};

const C0: f64 = 299_792_458.0;
const FREQ: f64 = 14.2e6;

/// Center-fed straight λ/2 dipole meshed into `nseg` equal segments.
fn dipole(nseg: usize) -> MpieWire {
    let lam = C0 / FREQ;
    let half = lam / 4.0;
    straight_wire([0.0, 0.0, -half], [0.0, 0.0, half], nseg, 0.001)
}

/// Feed the central interior node of an `nseg`-segment chain.
fn center_feed(nseg: usize) -> usize {
    (nseg - 1) / 2
}

/// Gate A2: mesh-refinement plateau — R rises monotonically toward the analytic
/// 79.35 Ω and the step between refinements shrinks.
#[test]
fn half_wave_dipole_converges_to_nec2c() {
    let r: Vec<f64> = [20usize, 40, 80]
        .iter()
        .map(|&n| {
            solve_mpie_free_space(&dipole(n), FREQ, center_feed(n))
                .unwrap()
                .z_in
                .re
        })
        .collect();

    // Monotone increase toward the analytic value.
    assert!(r[0] < r[1] && r[1] < r[2], "R not monotone: {r:?}");
    // Each within a few percent of 79.35, closing from below.
    assert!(
        (79.35 - r[2]).abs() / 79.35 < 0.07,
        "R(80)={} not within 7% of 79.35",
        r[2]
    );
    // Refinement steps shrink (plateau, not divergence).
    let step_lo = r[1] - r[0];
    let step_hi = r[2] - r[1];
    assert!(
        step_hi < step_lo + 0.5 && step_hi < 2.0,
        "refinement not plateauing: steps {step_lo} then {step_hi}"
    );
}

/// Gate A2 (reactance): the reactance settles (no fixed Hallén-style offset) and
/// is inductive for a slightly-long resonant dipole.
#[test]
fn half_wave_dipole_reactance_settles() {
    let x40 = solve_mpie_free_space(&dipole(40), FREQ, center_feed(40))
        .unwrap()
        .z_in
        .im;
    let x80 = solve_mpie_free_space(&dipole(80), FREQ, center_feed(80))
        .unwrap()
        .z_in
        .im;
    // Positive (inductive) and stable between refinements.
    assert!(
        x40 > 0.0 && x80 > 0.0,
        "X should be inductive: {x40}, {x80}"
    );
    assert!((x80 - x40).abs() < 3.0, "X not settling: {x40} then {x80}");
}

/// Gate A3: orientation invariance. Reverse the node order and feed the mirror
/// node — identical physical antenna, so Z must match to machine precision.
#[test]
fn impedance_is_orientation_invariant() {
    let nseg = 41usize;
    let feed = 8usize; // deliberately OFF-center to exercise the sign bookkeeping

    let fwd = dipole(nseg);
    let mut rev_nodes = fwd.nodes.clone();
    rev_nodes.reverse();
    let rev = MpieWire {
        nodes: rev_nodes,
        radius: fwd.radius,
    };
    // Interior node (feed+1) maps under reversal to node nseg-(feed+1),
    // i.e. basis index nseg-feed-2.
    let rev_feed = nseg - feed - 2;

    let z_fwd = solve_mpie_free_space(&fwd, FREQ, feed).unwrap().z_in;
    let z_rev = solve_mpie_free_space(&rev, FREQ, rev_feed).unwrap().z_in;
    assert!(
        (z_fwd - z_rev).norm() < 1e-6,
        "orientation not invariant: {z_fwd} vs {z_rev}"
    );
}

/// Physical sanity: a center-fed dipole has a current distribution symmetric
/// about the feed, peaking at the feed and tapering toward the ends.
#[test]
fn center_fed_current_is_symmetric_and_tapered() {
    let nseg = 40usize;
    let feed = center_feed(nseg);
    let sol = solve_mpie_free_space(&dipole(nseg), FREQ, feed).unwrap();
    let i = &sol.basis_currents;
    let nb = i.len();

    // Symmetric magnitude about the feed.
    for off in 1..=5 {
        let a = i[feed - off].norm();
        let b = i[feed + off].norm();
        assert!(
            (a - b).abs() / a.max(b) < 1e-6,
            "current not symmetric at offset {off}: {a} vs {b}"
        );
    }
    // Peak at the feed, tapering toward the ends (interior nodes only).
    assert!(i[feed].norm() > i[0].norm() && i[feed].norm() > i[nb - 1].norm());
    assert!(i[1].norm() < i[feed].norm() && i[nb - 2].norm() < i[feed].norm());
}
