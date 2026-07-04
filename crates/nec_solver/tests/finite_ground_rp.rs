// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH8-CHK-006: radiation pattern over finite ground via the Fresnel
// reflection-coefficient far field.

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, compute_radiation_pattern,
    radiation_efficiency, solve_hallen, wire_endpoints_from_segs, FarFieldPoint, GroundModel,
};

const FREQ_HZ: f64 = 14.2e6;

// Horizontal dipole 10 m above ground, an elevation cut (θ from zenith, φ=0).
fn horizontal_dipole_10m() -> NecDeck {
    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 51,
        start: [-5.282, 0.0, 10.0],
        end: [5.282, 0.0, 10.0],
        radius: 0.001,
    }));
    deck.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: 26,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    deck
}

fn solve_and_pattern(ground: &GroundModel, thetas: &[f64]) -> Vec<f64> {
    let deck = horizontal_dipole_10m();
    let segs = build_geometry(&deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ_HZ, ground);
    let h = build_hallen_rhs(&deck, &segs, FREQ_HZ).expect("rhs");
    let endpoints = wire_endpoints_from_segs(&segs);
    let sol = solve_hallen(&z, &h.rhs, &h.cos_vec, &endpoints, &[]).expect("solve");
    let pts: Vec<FarFieldPoint> = thetas
        .iter()
        .map(|&t| FarFieldPoint {
            theta_deg: t,
            phi_deg: 0.0,
        })
        .collect();
    compute_radiation_pattern(&segs, &sol.currents, FREQ_HZ, &pts, ground)
        .iter()
        .map(|r| r.gain_total_dbi)
        .collect()
}

#[test]
fn finite_ground_high_conductivity_limit_matches_pec() {
    // A finite ground with very high permittivity/conductivity must reproduce the
    // PEC pattern (Γ_v → +1, Γ_h → −1) — the correctness check for the Fresnel
    // far-field convention, independent of any external reference.
    let thetas = [10.0, 30.0, 45.0, 60.0, 80.0];
    let pec = solve_and_pattern(&GroundModel::PerfectConductor, &thetas);
    let near_pec = solve_and_pattern(
        &GroundModel::SimpleFiniteGround {
            eps_r: 1.0e8,
            sigma: 1.0e8,
        },
        &thetas,
    );
    for (i, (&p, &f)) in pec.iter().zip(near_pec.iter()).enumerate() {
        assert!(
            (p - f).abs() < 0.05,
            "θ={}: finite-ground high-σ gain {f:.3} != PEC {p:.3}",
            thetas[i]
        );
    }
}

#[test]
fn finite_ground_pattern_shape_matches_nec2c() {
    // Horizontal dipole 10 m over average ground (ε_r=13, σ=0.005), elevation cut.
    // nec2c total-gain reference (θ = 0..85° in 5° steps); θ=90° is the horizon
    // null. fnec reports directivity (relative to radiated power) while nec2c's
    // gain includes the ground-loss efficiency, so the two differ by a ~constant
    // offset; the design-relevant SHAPE is compared after removing that offset.
    let nec2c: [f64; 18] = [
        -5.02, -4.92, -4.59, -3.98, -3.11, -2.12, -1.19, -0.47, -0.07, -0.06, -0.49, -1.43, -2.94,
        -5.06, -7.75, -10.72, -13.62, -17.78,
    ];
    let thetas: Vec<f64> = (0..18).map(|i| i as f64 * 5.0).collect();
    let fnec = solve_and_pattern(
        &GroundModel::SimpleFiniteGround {
            eps_r: 13.0,
            sigma: 0.005,
        },
        &thetas,
    );

    // Peak must be at θ = 45° (the characteristic 45° elevation lobe).
    let peak_idx = fnec
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(thetas[peak_idx], 45.0, "peak should be at θ=45°");

    // Constant offset (fnec directivity − nec2c gain), estimated at the peak.
    let offset = fnec[9] - nec2c[9];
    let mut max_dev = 0.0f64;
    for i in 0..18 {
        max_dev = max_dev.max((fnec[i] - nec2c[i] - offset).abs());
    }
    println!("offset = {offset:.3} dB, max shape deviation = {max_dev:.3} dB");
    assert!(
        max_dev < 0.3,
        "finite-ground pattern shape deviates from nec2c by {max_dev:.3} dB"
    );
}

#[test]
fn finite_ground_has_horizon_null() {
    // Any ground plane nulls the horizon (θ=90°) and below.
    let g = solve_and_pattern(
        &GroundModel::SimpleFiniteGround {
            eps_r: 13.0,
            sigma: 0.005,
        },
        &[90.0, 120.0],
    );
    assert!(
        g[0] < -100.0,
        "θ=90° should be the horizon null, got {}",
        g[0]
    );
    assert!(g[1] < -100.0, "below horizon should be null, got {}", g[1]);
}

// ── Absolute gain over ground via radiation efficiency (PH9-CHK-003) ─────────

fn input_power(
    deck: &NecDeck,
    ground: &GroundModel,
) -> (Vec<num_complex::Complex64>, f64, Vec<nec_solver::Segment>) {
    let segs = build_geometry(deck).unwrap();
    let z = assemble_z_matrix_with_ground(&segs, FREQ_HZ, ground);
    let h = build_hallen_rhs(deck, &segs, FREQ_HZ).unwrap();
    let ep = wire_endpoints_from_segs(&segs);
    let sol = solve_hallen(&z, &h.rhs, &h.cos_vec, &ep, &[]).unwrap();
    let i_feed = sol.currents[25];
    let p_in = 0.5 * (num_complex::Complex64::new(1.0, 0.0) * i_feed.conj()).re;
    (sol.currents, p_in, segs)
}

fn free_dipole() -> NecDeck {
    let mut d = NecDeck::new();
    d.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 51,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    d.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: 26,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    d
}

#[test]
fn radiation_efficiency_is_unity_for_lossless() {
    // Free-space and PEC ground are lossless → efficiency ≈ 1 (so gain == directivity).
    for g in [GroundModel::FreeSpace, GroundModel::PerfectConductor] {
        let (i, p_in, segs) = input_power(&free_dipole(), &g);
        let eta = radiation_efficiency(&segs, &i, FREQ_HZ, &g, p_in);
        assert!(
            (eta - 1.0).abs() < 0.01,
            "lossless efficiency {eta:.4} should be ≈ 1 for {g:?}"
        );
    }
}

#[test]
fn finite_ground_absolute_gain_matches_nec2c() {
    // gain = directivity + 10·log10(η). Over average ground the ground-loss
    // efficiency (~0.74) converts fnec's directivity to gain that matches nec2c's
    // ABSOLUTE gain (not just the shape). nec2c total gain, θ = 0..85°.
    let nec2c: [f64; 18] = [
        -5.02, -4.92, -4.59, -3.98, -3.11, -2.12, -1.19, -0.47, -0.07, -0.06, -0.49, -1.43, -2.94,
        -5.06, -7.75, -10.72, -13.62, -17.78,
    ];
    let g = GroundModel::SimpleFiniteGround {
        eps_r: 13.0,
        sigma: 0.005,
    };
    let deck = horizontal_dipole_10m();
    let (i, p_in, _segs) = input_power(&deck, &g);
    let eta = radiation_efficiency(&build_geometry(&deck).unwrap(), &i, FREQ_HZ, &g, p_in);
    let gain_offset = 10.0 * eta.log10();
    let thetas: Vec<f64> = (0..18).map(|k| k as f64 * 5.0).collect();
    let directivity = solve_and_pattern(&g, &thetas);
    let mut max_dev = 0.0f64;
    for k in 0..18 {
        let gain = directivity[k] + gain_offset;
        max_dev = max_dev.max((gain - nec2c[k]).abs());
    }
    println!("efficiency = {eta:.4}, gain offset = {gain_offset:.3} dB, max |gain − nec2c| = {max_dev:.3} dB");
    assert!(
        max_dev < 0.15,
        "finite-ground absolute gain deviates from nec2c by {max_dev:.3} dB"
    );
}
