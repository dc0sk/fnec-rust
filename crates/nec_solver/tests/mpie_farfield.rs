// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-007 MPIE Phase C — far-field from the recovered MPIE currents.
//
// The MPIE solve yields nodal basis currents; `segment_currents` recovers the
// per-segment midpoint current (the mean of the touching nodal currents = the
// radiation moment per unit length), and `segments_for_farfield` exports the
// geometry, so the EXISTING radiation-pattern sum (compute_radiation_pattern,
// validated for the Hallén solver to 0.06 dB vs nec2c in PH9-CHK-003) computes
// the pattern with no new far-field code.
//
//   Gate C1 (nec2c/analytic): a free-space λ/2 dipole's pattern peaks broadside
//     (θ=90°) with directivity 2.15 dBi (nec2c reports 2.15 dBi free-space gain),
//     nulls along the wire axis, and follows the analytic cos(½π cosθ)/sinθ shape.
//
//   Gate C2 (symmetry/reciprocity): the pattern is symmetric about broadside
//     (θ ↔ 180°−θ) and independent of azimuth φ for the z-oriented dipole.

use nec_solver::{
    compute_radiation_pattern, segment_currents, segments_for_farfield, solve_mpie,
    solve_mpie_free_space, straight_wire, FarFieldPoint, GroundModel, MpieGeometry,
};

const C0: f64 = 299_792_458.0;
const FREQ: f64 = 14.2e6;

fn dipole_pattern(nseg: usize, points: &[FarFieldPoint]) -> Vec<f64> {
    let lam = C0 / FREQ;
    let half = lam / 4.0;
    let wire = straight_wire([0.0, 0.0, -half], [0.0, 0.0, half], nseg, 0.001);
    let sol = solve_mpie_free_space(&wire, FREQ, nseg / 2 - 1).unwrap();
    let geom = wire.geometry();
    let segs = segments_for_farfield(&geom);
    let ivec = segment_currents(&geom, &sol.basis_currents);
    compute_radiation_pattern(&segs, &ivec, FREQ, points, &GroundModel::FreeSpace)
        .iter()
        .map(|r| r.gain_total_dbi)
        .collect()
}

/// Gate C1: free-space λ/2 dipole peak directivity is 2.15 dBi, broadside, with
/// nulls along the axis and the analytic pattern shape.
#[test]
fn half_wave_dipole_gain_and_shape_match_nec2c() {
    // θ sweep in the φ=0 plane, from axis (0°) to broadside (90°) to axis (180°).
    let points: Vec<FarFieldPoint> = (0..=180)
        .step_by(10)
        .map(|t| FarFieldPoint {
            theta_deg: t as f64,
            phi_deg: 0.0,
        })
        .collect();
    let gains = dipole_pattern(40, &points);

    // Peak directivity 2.15 dBi at broadside (θ=90° is index 9).
    let broadside = gains[9];
    assert!(
        (broadside - 2.15).abs() < 0.1,
        "peak gain {broadside:.3} dBi (expected 2.15)"
    );
    // Broadside is the maximum.
    for (i, &g) in gains.iter().enumerate() {
        assert!(
            g <= broadside + 1e-6,
            "θ index {i} gain {g} exceeds broadside"
        );
    }
    // Deep nulls along the wire axis (θ=0°, 180°).
    assert!(
        gains[0] < -20.0 && gains[18] < -20.0,
        "axis not nulled: {gains:?}"
    );

    // Analytic shape at θ=60°: F = cos(½π cosθ)/sinθ, gain relative to broadside.
    let theta = 60.0_f64.to_radians();
    let f = (0.5 * std::f64::consts::PI * theta.cos()).cos() / theta.sin();
    let rel_analytic_db = 20.0 * f.log10(); // broadside F=1 → 0 dB reference
    let rel_mpie_db = gains[6] - broadside; // index 6 = θ=60°
    assert!(
        (rel_mpie_db - rel_analytic_db).abs() < 0.1,
        "θ=60° relative gain {rel_mpie_db:.3} dB vs analytic {rel_analytic_db:.3} dB"
    );
}

/// Build the symmetric Y (3 arms × 5 m at 120° in the xy-plane) with `nseg_arm`
/// even segments per arm; feed the arm-0 midpoint node.
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
    (
        MpieGeometry {
            nodes,
            segments,
            radius: 0.001,
        },
        nseg_arm / 2,
    )
}

/// Gate C1 (junction pattern): the planar Y-junction radiates broadside to its
/// plane (θ=0°) with a peak directivity of 1.94 dBi (nec2c) and a null in-plane.
#[test]
fn y_junction_pattern_gain_matches_nec2c() {
    let (geom, feed) = build_y(20);
    let sol = solve_mpie(&geom, FREQ, feed).unwrap();
    let segs = segments_for_farfield(&geom);
    let ivec = segment_currents(&geom, &sol.basis_currents);
    let points: Vec<FarFieldPoint> = (0..=180)
        .step_by(10)
        .map(|t| FarFieldPoint {
            theta_deg: t as f64,
            phi_deg: 0.0,
        })
        .collect();
    let gains: Vec<f64> =
        compute_radiation_pattern(&segs, &ivec, FREQ, &points, &GroundModel::FreeSpace)
            .iter()
            .map(|r| r.gain_total_dbi)
            .collect();

    // Peak broadside to the plane (θ=0°, index 0) at nec2c's 1.94 dBi.
    assert!(
        (gains[0] - 1.94).abs() < 0.15,
        "Y peak gain {:.3} dBi (nec2c 1.94)",
        gains[0]
    );
    // In-plane null (θ=90°, index 9) is far below the peak.
    assert!(gains[9] < gains[0] - 10.0, "in-plane not nulled: {gains:?}");
}

/// Gate C2: pattern symmetry about broadside and azimuthal independence.
#[test]
fn dipole_pattern_is_symmetric_and_azimuthally_invariant() {
    // Symmetry θ ↔ 180−θ in the φ=0 plane.
    let sym_pts: Vec<FarFieldPoint> = [30.0, 150.0, 70.0, 110.0]
        .iter()
        .map(|&t| FarFieldPoint {
            theta_deg: t,
            phi_deg: 0.0,
        })
        .collect();
    let g = dipole_pattern(40, &sym_pts);
    assert!(
        (g[0] - g[1]).abs() < 1e-3,
        "θ 30 vs 150: {} vs {}",
        g[0],
        g[1]
    );
    assert!(
        (g[2] - g[3]).abs() < 1e-3,
        "θ 70 vs 110: {} vs {}",
        g[2],
        g[3]
    );

    // Azimuthal invariance: broadside gain equal at φ = 0, 45, 90.
    let az_pts: Vec<FarFieldPoint> = [0.0, 45.0, 90.0]
        .iter()
        .map(|&p| FarFieldPoint {
            theta_deg: 90.0,
            phi_deg: p,
        })
        .collect();
    let ga = dipole_pattern(40, &az_pts);
    assert!(
        (ga[0] - ga[1]).abs() < 1e-3 && (ga[1] - ga[2]).abs() < 1e-3,
        "azimuth-dependent broadside gain: {ga:?}"
    );
}
