// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-007 MPIE Phase B — degree-N junctions, external gates.
//
// The MPIE junction basis makes Kirchhoff's current law automatic (each dipole
// basis carries current in on one arm and out on another), with NO explicit KCL
// row. This is the property the entire-domain Hallén junction prototype could not
// achieve — its radiation resistance DIVERGED to the wrong fixed point (~80 Ω)
// under mesh refinement on the very Y-junction gated here.
//
//   Gate B1 (the headline): a symmetric Y-junction (3 arms × 5 m at 120°, fed at
//     the midpoint of one arm) converges MONOTONICALLY in R toward nec2c's
//     converged 71.5 Ω under mesh refinement, within 5%. nec2c reference (live,
//     14.2 MHz, a = 1 mm) at 11/21/41 seg/arm: 71.78 / 71.60 / 71.50 − j~67.
//     The Python oracle (studies/.../mpie_junction.py) matches: even-mesh
//     feed@0.5 gives R = 68.75 / 69.33 / 69.84 (N = 10/20/40), monotone.
//     (The reactance converges slowly ∝ 1/N — the known delta-gap behavior — but
//     monotonically from below; R is the quantity Hallén got wrong.)
//
//   Gate B4 (structural): KCL is satisfied by basis construction — the degree-3
//     junction node contributes exactly N−1 = 2 bases, so the Y has
//     3·(nseg−1) + 2 bases total.

use nec_solver::{solve_mpie, MpieGeometry};

const C0: f64 = 299_792_458.0;
const FREQ: f64 = 14.2e6;

/// Build the symmetric Y: three 5 m arms at 0°/120°/240° in the xy-plane, all
/// meeting at the origin (node 0). Returns the geometry and the midpoint feed
/// node on arm 0. `nseg_arm` must be even so a node sits at the arm midpoint.
fn build_y(nseg_arm: usize) -> (MpieGeometry, usize) {
    let arm_len = 5.0;
    let dirs = [
        [1.0_f64, 0.0, 0.0],
        [
            (2.0 * std::f64::consts::PI / 3.0).cos(),
            (2.0 * std::f64::consts::PI / 3.0).sin(),
            0.0,
        ],
        [
            (4.0 * std::f64::consts::PI / 3.0).cos(),
            (4.0 * std::f64::consts::PI / 3.0).sin(),
            0.0,
        ],
    ];
    let mut nodes = vec![[0.0, 0.0, 0.0]];
    let mut segments = Vec::new();
    for d in dirs {
        let mut prev = 0usize;
        for i in 1..=nseg_arm {
            let t = i as f64 / nseg_arm as f64 * arm_len;
            nodes.push([d[0] * t, d[1] * t, d[2] * t]);
            let idx = nodes.len() - 1;
            segments.push([prev, idx]);
            prev = idx;
        }
    }
    // Arm-0 nodes are indices 1..=nseg_arm; the midpoint (position 0.5) is index
    // nseg_arm/2.
    let feed_node = nseg_arm / 2;
    (
        MpieGeometry {
            nodes,
            segments,
            radius: 0.001,
        },
        feed_node,
    )
}

fn solve_y(nseg_arm: usize) -> (f64, f64, usize) {
    let (geom, feed) = build_y(nseg_arm);
    let sol = solve_mpie(&geom, FREQ, feed).unwrap();
    (sol.z_in.re, sol.z_in.im, sol.basis_currents.len())
}

/// Gate B1: the Y-junction radiation resistance converges monotonically toward
/// nec2c (71.5 Ω) — the case the entire-domain Hallén prototype diverged on.
#[test]
fn y_junction_resistance_converges_to_nec2c() {
    let (r10, x10, _) = solve_y(10);
    let (r20, x20, _) = solve_y(20);
    let (r40, x40, _) = solve_y(40);

    // Monotone increase in R (Hallén instead diverged upward past 80).
    assert!(
        r10 < r20 && r20 < r40,
        "R not monotone: {r10:.2}, {r20:.2}, {r40:.2}"
    );
    // Within 5% of nec2c's converged 71.5 Ω, closing from below.
    assert!(
        r40 < 71.5 && (71.5 - r40) / 71.5 < 0.05,
        "R(40)={r40:.2} not within 5% below 71.5"
    );
    // Refinement step shrinks (plateau, not divergence).
    assert!((r40 - r20) < 1.0, "R step too large: {}", r40 - r20);

    // Reactance is capacitive and converges from below (less negative as refined).
    assert!(
        x10 < x20 && x20 < x40 && x40 < 0.0,
        "X not converging from below: {x10:.1}, {x20:.1}, {x40:.1}"
    );
}

/// Gate B4: KCL by construction — the degree-3 junction contributes N−1 = 2
/// bases, so the Y has 3·(nseg−1) + 2 bases (3 arms of nseg−1 interior nodes,
/// plus the junction's two arm-pair dipoles).
#[test]
fn y_junction_basis_count_reflects_automatic_kcl() {
    for nseg in [10usize, 20] {
        let (.., nb) = solve_y(nseg);
        assert_eq!(
            nb,
            3 * (nseg - 1) + 2,
            "unexpected basis count for nseg={nseg}"
        );
    }
}

/// Gate B2 (degree-2 bend): an apex-fed inverted-V (two λ/4 arms, 90° included
/// angle) solves to a physical impedance that converges under refinement — the
/// leg-based basis handles the bend via the tangent dot product.
#[test]
fn inverted_v_bend_solves_and_converges() {
    let lam = C0 / FREQ;
    let arm = lam / 4.0;
    // Apex at origin (node 0); arms go down-and-out at ±45° in the xz-plane.
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let build = |nseg: usize| -> (MpieGeometry, usize) {
        let tips = [[arm * s, 0.0, -arm * s], [-arm * s, 0.0, -arm * s]];
        let mut nodes = vec![[0.0, 0.0, 0.0]];
        let mut segments = Vec::new();
        for tip in tips {
            let mut prev = 0usize;
            for i in 1..=nseg {
                let t = i as f64 / nseg as f64;
                nodes.push([tip[0] * t, tip[1] * t, tip[2] * t]);
                let idx = nodes.len() - 1;
                segments.push([prev, idx]);
                prev = idx;
            }
        }
        (
            MpieGeometry {
                nodes,
                segments,
                radius: 0.001,
            },
            0, // apex (degree-2) feed
        )
    };
    let (g1, f1) = build(20);
    let (g2, f2) = build(40);
    let z1 = solve_mpie(&g1, FREQ, f1).unwrap().z_in;
    let z2 = solve_mpie(&g2, FREQ, f2).unwrap().z_in;

    // Physical radiation resistance (a bent λ/2 dipole is tens of ohms).
    assert!(z1.re > 10.0 && z1.re < 90.0, "unphysical R: {}", z1.re);
    // Converging: the refinement step in R is small.
    assert!(
        (z2.re - z1.re).abs() < 3.0,
        "R not converging: {} then {}",
        z1.re,
        z2.re
    );
}
