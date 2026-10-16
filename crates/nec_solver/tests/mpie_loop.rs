// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-007 MPIE Phase B (B3) — closed loops.
//
// A closed loop is a cyclic chain: every node is degree 2, there is no free end,
// and no endpoint (I=0) condition. It therefore falls out of the SAME leg-based
// triangle basis as the open-chain and junction cases with no solver change — the
// property that distinguishes this from the Hallén solver, whose per-wire
// homogeneous basis needed a periodic Green's-function closure that never
// validated (docs/ph9-chk-002-general-junction.md: 1λ loop ≈ 20 − j1210 vs nec2c
// 111 − j146). Here the MPIE reproduces nec2c.
//
//   Gate B3a: a 1λ square loop (4 sides × 0.25λ, fed at a side midpoint) converges
//     toward nec2c under refinement. nec2c (live, 14.2 MHz, a = 1 mm) at 11/21/41
//     seg/side: 111.0 / 110.2 / 109.7 − j146.2; converged (41/side at 27×, EK)
//     108.97 − j146.10, the gate's reference (FND-257). MPIE (concept oracle
//     studies/.../mpie_junction.py + this port), seg/side = 10/20/40:
//     115.2 − j171 / 114.9 − j151 / 113.8 − j148 — R and X both converge toward
//     nec2c (within ~5% at 20 seg/side).
//
//   Gate B3b: a small loop's radiation resistance matches the analytic
//     R_rad = 20π²(C/λ)⁴ (C = circumference). At C = 0.2λ, MPIE R = 0.316 Ω vs
//     analytic 0.316 Ω, with an inductive (positive) reactance.

use nec_solver::{solve_mpie, MpieGeometry};

const C0: f64 = 299_792_458.0;
const FREQ: f64 = 14.2e6;

/// Build a planar square loop (xz-plane) of circumference `circ_lam` wavelengths,
/// `nseg_side` segments per side, as a cyclic geometry. Returns the geometry and
/// the feed node at the midpoint of the bottom side.
fn build_square_loop(circ_lam: f64, nseg_side: usize) -> (MpieGeometry, usize) {
    let lam = C0 / FREQ;
    let a = circ_lam * lam / 4.0; // side length
    let corners = [
        [-a / 2.0, 0.0, -a / 2.0],
        [a / 2.0, 0.0, -a / 2.0],
        [a / 2.0, 0.0, a / 2.0],
        [-a / 2.0, 0.0, a / 2.0],
    ];
    let n = 4 * nseg_side;
    let mut nodes = Vec::with_capacity(n);
    for k in 0..n {
        let s = k as f64 * (4.0 * a) / n as f64;
        let side = (s / a).floor() as usize; // 0..3
        let frac = (s - side as f64 * a) / a;
        let c0 = corners[side];
        let c1 = corners[(side + 1) % 4];
        nodes.push([
            c0[0] + frac * (c1[0] - c0[0]),
            c0[1] + frac * (c1[1] - c0[1]),
            c0[2] + frac * (c1[2] - c0[2]),
        ]);
    }
    let segments = (0..n).map(|k| [k, (k + 1) % n]).collect();
    // Bottom side spans arc [0, a]; its midpoint node is at index nseg_side/2.
    let feed_node = nseg_side / 2;
    (MpieGeometry::uniform(nodes, segments, 0.001), feed_node)
}

fn solve_loop(circ_lam: f64, nseg_side: usize) -> (f64, f64) {
    let (geom, feed) = build_square_loop(circ_lam, nseg_side);
    let z = solve_mpie(&geom, FREQ, feed).unwrap().z_in;
    (z.re, z.im)
}

/// Gate B3a: the 1λ square loop converges to nec2c — the case the Hallén solver's
/// periodic closure could not reproduce.
///
/// The reference is nec2c's converged value, 108.97 − j146.10: the loop at 41
/// segments a side meshed 27× finer (`EK 0`, `scripts/nec2c-converged-series.py`,
/// captured 2026-10-16), where it was nec2c's 109.7 − j146.2 at 41 a side, which
/// moves 55 % of the old 1 % R gate under nec2c's own refinement (FND-257). The
/// MPIE at 10 / 20 / 40 / 80 a side: R 108.87, 109.07, 109.08, 109.04 — settled
/// within 0.1 Ω of nec2c's limit from 10 a side, so R is held to 0.5 % at both
/// meshes, not to a direction: a 0.1 Ω crossing is inside nec2c's own remaining
/// drift. X −146.93, −146.21, −146.05, −146.01: it converges, and its gap must
/// shrink.
#[test]
fn one_wavelength_loop_converges_to_nec2c() {
    let (r10, x10) = solve_loop(1.0, 10);
    let (r20, x20) = solve_loop(1.0, 20);
    let (r_ref, x_ref) = (108.97, -146.10);

    assert!(x10 < 0.0 && x20 < 0.0, "loop X should be capacitive");
    assert!(
        (x20 - x_ref).abs() < (x10 - x_ref).abs() && (x20 - x_ref).abs() / x_ref.abs() < 0.01,
        "X not converging to nec2c's {x_ref}: {x10:.2} then {x20:.2}"
    );
    assert!(
        (r10 - r_ref).abs() / r_ref < 0.005 && (r20 - r_ref).abs() / r_ref < 0.005,
        "R not at nec2c's {r_ref}: {r10:.2} then {r20:.2}"
    );
}

/// Gate B3b: a small loop's radiation resistance matches R_rad = 20π²(C/λ)⁴, and
/// its reactance is inductive (positive) — the small-loop physical signature.
#[test]
fn small_loop_matches_analytic_radiation_resistance() {
    let circ = 0.2_f64;
    let (r, x) = solve_loop(circ, 10);
    let r_analytic = 20.0 * std::f64::consts::PI.powi(2) * circ.powi(4);
    assert!(
        (r - r_analytic).abs() / r_analytic < 0.10,
        "R={r:.5} vs analytic {r_analytic:.5}"
    );
    assert!(x > 0.0, "small loop should be inductive, got X={x:.1}");
}
