// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Gate G7 (PH5-CHK-007): GPU Z-matrix fill + CPU Hallén solve end-to-end test.
//!
//! Builds a 51-segment half-wave dipole at 14 MHz, fills the Hallén A-matrix on
//! the GPU via `fill_zmatrix_wgpu`, constructs a `ZMatrix` from the result, then
//! runs the CPU `solve_hallen` to obtain segment currents.  The feedpoint
//! impedance computed from the GPU-filled solution must agree with the
//! all-CPU reference within ±2 Ω on both resistance and reactance.
//!
//! The test passes vacuously (prints a skip notice) when no wgpu adapter is
//! available, matching the pattern used by other GPU gate tests in this crate.

use nec_accel::{fill_zmatrix_wgpu, ZSegmentInput};
use nec_solver::ZMatrix;
use num_complex::Complex64;

// ---------------------------------------------------------------------------
// Shared geometry builder
// ---------------------------------------------------------------------------

/// Build a 51-segment half-wave dipole deck and return segments + hallen RHS.
fn build_dipole() -> (
    Vec<nec_solver::Segment>,
    nec_solver::HallenRhs,
    Vec<(usize, usize)>, // wire_endpoints
) {
    use nec_model::card::{Card, ExCard, GwCard};
    use nec_model::deck::NecDeck;
    use nec_solver::{build_geometry, build_hallen_rhs};

    let freq_hz = 14.0e6_f64;
    let half = 5.35_f64;
    let radius = 0.001_f64;

    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 51,
        start: [0.0, 0.0, -half],
        end: [0.0, 0.0, half],
        radius,
    }));
    // Delta-gap voltage source at segment 26 (1-indexed = segment 25 0-indexed).
    deck.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: 26,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
    }));

    let segs = build_geometry(&deck).expect("geometry should build");
    assert_eq!(segs.len(), 51);

    let rhs = build_hallen_rhs(&deck, &segs, freq_hz).expect("rhs should build");

    let wire_endpoints = vec![(0usize, segs.len() - 1)];

    (segs, rhs, wire_endpoints)
}

// ---------------------------------------------------------------------------
// Gate G7 test
// ---------------------------------------------------------------------------

/// Gate G7: end-to-end GPU Z-matrix fill + CPU Hallén solve must yield
/// feedpoint impedance within ±2 Ω (R and X) of the all-CPU reference.
///
/// Skips vacuously when no wgpu adapter is available (e.g. bare-metal CI).
#[test]
fn gpu_hallen_path_feedpoint_impedance_within_2_ohm_of_cpu() {
    use nec_solver::{assemble_z_matrix, solve_hallen};

    let freq_hz = 14.0e6_f64;
    let (segs, rhs, wire_endpoints) = build_dipole();
    let n = segs.len();

    // --- CPU reference ---
    let z_cpu = assemble_z_matrix(&segs, freq_hz);
    let sol_cpu = solve_hallen(&z_cpu, &rhs.rhs, &rhs.cos_vec, &wire_endpoints, &[])
        .expect("CPU solve should succeed");
    let feed_seg = n / 2; // segment 25 (0-indexed)
    let i_cpu = sol_cpu.currents[feed_seg];
    let v_feed = rhs.rhs[feed_seg];
    // Z_cpu ≈ V / I (the delta-gap excitation contributes via the rhs)
    let z_cpu_imp = if i_cpu.norm() > 1e-30 {
        // For Hallen formulation the impressed voltage is implicit in the rhs.
        // We approximate impedance as Re(V_source) / I.  V_source = 1 V.
        Complex64::new(1.0, 0.0) / i_cpu
    } else {
        panic!("CPU feedpoint current is effectively zero — something is wrong");
    };
    let _ = v_feed; // intentionally unused; kept for documentation

    // --- GPU fill ---
    let z_inputs: Vec<ZSegmentInput> = segs
        .iter()
        .map(|s| ZSegmentInput {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
            radius: s.radius,
        })
        .collect();

    let gpu_result = pollster::block_on(fill_zmatrix_wgpu(&z_inputs, freq_hz));

    let Some(gpu_elems) = gpu_result else {
        eprintln!("G7 gate: no hardware GPU adapter — gate skipped (software fallback)");
        return;
    };

    // Build ZMatrix from GPU elements (f32 → f64).
    let flat: Vec<Complex64> = gpu_elems
        .iter()
        .map(|e| Complex64::new(e.re as f64, e.im as f64))
        .collect();
    let z_gpu = ZMatrix::from_flat(n, flat);

    // --- GPU-filled solve ---
    let sol_gpu = solve_hallen(&z_gpu, &rhs.rhs, &rhs.cos_vec, &wire_endpoints, &[])
        .expect("GPU-matrix solve should succeed");

    let i_gpu = sol_gpu.currents[feed_seg];
    let z_gpu_imp = if i_gpu.norm() > 1e-30 {
        Complex64::new(1.0, 0.0) / i_gpu
    } else {
        panic!("GPU feedpoint current is effectively zero");
    };

    let delta_r = (z_gpu_imp.re - z_cpu_imp.re).abs();
    let delta_x = (z_gpu_imp.im - z_cpu_imp.im).abs();

    eprintln!(
        "G7 gate: Z_cpu=({:.3}+j{:.3}) Ω  Z_gpu=({:.3}+j{:.3}) Ω  ΔR={:.4}  ΔX={:.4}",
        z_cpu_imp.re, z_cpu_imp.im, z_gpu_imp.re, z_gpu_imp.im, delta_r, delta_x
    );

    const TOL_OHM: f64 = 2.0;
    assert!(
        delta_r <= TOL_OHM,
        "G7: feedpoint resistance delta {delta_r:.4} Ω > {TOL_OHM} Ω limit"
    );
    assert!(
        delta_x <= TOL_OHM,
        "G7: feedpoint reactance delta {delta_x:.4} Ω > {TOL_OHM} Ω limit"
    );
}
