// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-007 MPIE Phase D — Sommerfeld ground IN the Z-matrix.
//
// This is the phase that retires the third deferred Phase-9 frontier: correct
// near-ground CURRENTS/patterns/gain (Sommerfeld Level 2), not just the feedpoint
// Z that Level 1 already corrected. The MPIE keeps the scalar potential explicit,
// so adding the reflected vector- and scalar-potential kernels (G_A, G_Φ) to the
// Z-fill puts the surface wave into the current solution itself.
//
// The Python oracle studies/sommerfeld-ground/efie_mpie_ground.py reproduces
// nec2c GN2 to ~5% on R AND X; this port must match it. nec2c refs (14.2 MHz,
// εr=13, σ=0.005, horizontal λ/2 dipole):
//   free space : 78.85 + j44.70
//   PEC  0.05λ :  6.16 + j38.18   (GN1 — image cancellation)
//   GN2  0.05λ : 67.26 + j52.61
//   GN2  0.025λ: 87.81 + j68.64
// Oracle (N=40): free 74.36+j41.36, PEC 5.87+j34.11, GN2 64.00+j49.18 / 83.46+j66.26.
//
//   Gate D1: horizontal λ/2 dipole over GN2 at 0.05λ and 0.025λ — feed Z matches
//     the oracle (and hence nec2c to ~5-8%), and the ground pulls R well below the
//     74 Ω free-space value (a real ground effect, not a small perturbation).
//   Gate D2: PEC image cancellation (R → ~6 at 0.05λ); non-horizontal geometry is
//     rejected (Phase D boundary — arbitrary orientation is Phase E).

use nec_solver::{solve_mpie_ground, straight_wire, GroundModel, MpieError};

const C0: f64 = 299_792_458.0;
const FREQ: f64 = 14.2e6;

/// Horizontal λ/2 dipole along x at height `h_lam` (wavelengths), 40 segments.
fn horizontal_dipole(h_lam: f64) -> (nec_solver::MpieGeometry, usize) {
    let lam = C0 / FREQ;
    let half = lam / 4.0;
    let h = h_lam * lam;
    let wire = straight_wire([-half, 0.0, h], [half, 0.0, h], 40, 0.001);
    (wire.geometry(), 20)
}

fn gn2() -> GroundModel {
    GroundModel::SimpleFiniteGround {
        eps_r: 13.0,
        sigma: 0.005,
    }
}

/// Gate D1: horizontal dipole over GN2 — the feed Z tracks the oracle/nec2c and
/// the ground substantially lowers R from the 74 Ω free-space value.
#[test]
fn horizontal_dipole_gn2_matches_oracle() {
    // 0.05λ: oracle 64.00 + j49.18, nec2c 67.26 + j52.61.
    let (g05, f05) = horizontal_dipole(0.05);
    let z05 = solve_mpie_ground(&g05, FREQ, f05, &gn2()).unwrap();
    assert!(
        (z05.z_in.re - 64.0).abs() < 2.0 && (z05.z_in.im - 49.18).abs() < 2.0,
        "GN2 0.05λ Z={} (oracle 64.00+j49.18)",
        z05.z_in
    );
    // Within ~8% of nec2c on both parts.
    assert!(
        (z05.z_in.re - 67.26).abs() / 67.26 < 0.08,
        "R vs nec2c: {}",
        z05.z_in.re
    );
    assert!(
        (z05.z_in.im - 52.61).abs() / 52.61 < 0.08,
        "X vs nec2c: {}",
        z05.z_in.im
    );
    // The ground is a strong effect: R well below the 74 Ω free-space value.
    assert!(
        z05.z_in.re < 70.0,
        "ground effect too weak: R={}",
        z05.z_in.re
    );

    // 0.025λ: oracle 83.46 + j66.26, nec2c 87.81 + j68.64.
    let (g025, f025) = horizontal_dipole(0.025);
    let z025 = solve_mpie_ground(&g025, FREQ, f025, &gn2()).unwrap();
    assert!(
        (z025.z_in.re - 83.46).abs() < 2.5 && (z025.z_in.im - 66.26).abs() < 2.5,
        "GN2 0.025λ Z={} (oracle 83.46+j66.26)",
        z025.z_in
    );
    assert!(
        (z025.z_in.re - 87.81).abs() / 87.81 < 0.08,
        "R vs nec2c: {}",
        z025.z_in.re
    );
}

/// Gate D2: PEC image cancellation — a horizontal dipole 0.05λ over a perfect
/// conductor has its radiation resistance collapse toward the oracle's 5.87 Ω
/// (nec2c GN1 6.16 Ω), the horizontal-over-ground signature.
#[test]
fn pec_image_cancellation() {
    let (g, f) = horizontal_dipole(0.05);
    let z = solve_mpie_ground(&g, FREQ, f, &GroundModel::PerfectConductor).unwrap();
    assert!(
        z.z_in.re < 10.0 && (z.z_in.re - 5.87).abs() < 1.5,
        "PEC 0.05λ R={} (oracle 5.87, nec2c GN1 6.16)",
        z.z_in.re
    );
}

/// Gate D2 (boundary): a non-horizontal wire over ground is rejected — arbitrary
/// orientation is Phase E (the full reflected dyadic).
#[test]
fn vertical_wire_over_ground_is_rejected() {
    let lam = C0 / FREQ;
    // Vertical λ/2 dipole with its base above ground.
    let wire = straight_wire(
        [0.0, 0.0, 0.1 * lam],
        [0.0, 0.0, 0.1 * lam + lam / 2.0],
        40,
        0.001,
    );
    let geom = wire.geometry();
    assert!(matches!(
        solve_mpie_ground(&geom, FREQ, 20, &gn2()),
        Err(MpieError::UnsupportedGround)
    ));
}
