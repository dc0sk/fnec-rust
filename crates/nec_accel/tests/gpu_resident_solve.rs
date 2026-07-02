// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Gate PH7-CHK-003: GPU-resident Hallén solve (fill + normal-equations solve
//! entirely on the device) end-to-end parity test.
//!
//! Builds a 51-segment half-wave dipole at 14 MHz and solves it two ways:
//!   1. all-CPU reference (`assemble_z_matrix` + f64 `solve_hallen`)
//!   2. GPU-resident (`solve_hallen_gpu_resident`: fill Z on GPU, solve the
//!      regularized normal-equations system on the GPU, only the current vector
//!      returns to the host)
//!
//! The feedpoint impedance from the f32 GPU-resident solve must agree with the
//! f64 CPU reference within ±2 Ω on R and X — the established GPU-path tolerance
//! (the f64 CPU solve remains the 0.05 Ω corpus-gate reference).
//!
//! Skips vacuously when no wgpu adapter is available.

use nec_accel::{solve_hallen_gpu_resident, ZSegmentInput};
use num_complex::Complex64;

fn build_dipole() -> (
    Vec<nec_solver::Segment>,
    nec_solver::HallenRhs,
    Vec<(usize, usize)>,
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

#[test]
fn gpu_resident_hallen_solve_within_2_ohm_of_cpu() {
    use nec_solver::{assemble_z_matrix, solve_hallen};

    let freq_hz = 14.0e6_f64;
    let (segs, rhs, wire_endpoints) = build_dipole();
    let n = segs.len();
    let feed_seg = n / 2;

    // --- CPU reference (f64) ---
    let z_cpu = assemble_z_matrix(&segs, freq_hz);
    let sol_cpu = solve_hallen(&z_cpu, &rhs.rhs, &rhs.cos_vec, &wire_endpoints, &[])
        .expect("CPU solve should succeed");
    let i_cpu = sol_cpu.currents[feed_seg];
    assert!(i_cpu.norm() > 1e-30, "CPU feedpoint current is ~zero");
    let z_cpu_imp = Complex64::new(1.0, 0.0) / i_cpu;

    // --- GPU-resident (f32, fill + solve on device) ---
    let z_inputs: Vec<ZSegmentInput> = segs
        .iter()
        .map(|s| ZSegmentInput {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
            radius: s.radius,
        })
        .collect();

    let gpu_solution = match pollster::block_on(solve_hallen_gpu_resident(
        &z_inputs,
        &rhs.rhs,
        &rhs.cos_vec,
        &wire_endpoints,
        &[],
        freq_hz,
    )) {
        Some(c) => c,
        None => {
            eprintln!(
                "PH7-CHK-003 gate: no hardware GPU adapter — gate skipped (software fallback)"
            );
            return;
        }
    };

    // Full solution is length S = N + W; currents are the first N entries.
    assert!(
        gpu_solution.len() >= n,
        "expected at least N solution entries"
    );
    let i_gpu = gpu_solution[feed_seg];
    assert!(i_gpu.norm() > 1e-30, "GPU feedpoint current is ~zero");
    let z_gpu_imp = Complex64::new(1.0, 0.0) / i_gpu;

    let delta_r = (z_gpu_imp.re - z_cpu_imp.re).abs();
    let delta_x = (z_gpu_imp.im - z_cpu_imp.im).abs();

    eprintln!(
        "PH7-CHK-003 gate: Z_cpu=({:.3}+j{:.3}) Ω  Z_gpu=({:.3}+j{:.3}) Ω  ΔR={:.4}  ΔX={:.4}",
        z_cpu_imp.re, z_cpu_imp.im, z_gpu_imp.re, z_gpu_imp.im, delta_r, delta_x
    );

    const TOL_OHM: f64 = 2.0;
    assert!(
        delta_r <= TOL_OHM,
        "PH7-CHK-003: feedpoint resistance delta {delta_r:.4} Ω > {TOL_OHM} Ω limit"
    );
    assert!(
        delta_x <= TOL_OHM,
        "PH7-CHK-003: feedpoint reactance delta {delta_x:.4} Ω > {TOL_OHM} Ω limit"
    );
}
