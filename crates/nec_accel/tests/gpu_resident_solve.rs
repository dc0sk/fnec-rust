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

/// Whether a HARDWARE adapter is present. `solve_hallen_gpu_resident` returns
/// `None` both when there is no adapter and when its f32 solve fails its own
/// accuracy check, so a gate that read every `None` as "no adapter, skip" passed
/// a shader that had stopped converging: the FND-158 sabotage (sin column
/// zeroed in the shader) failed nothing. With hardware present, `None` is a
/// failure.
fn hardware_adapter_present() -> bool {
    pollster::block_on(nec_accel::wgpu_device::enumerate_compute_adapters())
        .iter()
        .any(|a| a.device_type != "Cpu")
}

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
        theta_inc: 0.0,
        phi_inc: 0.0,
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
    let sol_cpu = solve_hallen(
        &z_cpu,
        &rhs.rhs,
        &rhs.cos_vec,
        &rhs.sin_vec,
        &wire_endpoints,
        &[],
    )
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
        &rhs.sin_vec,
        &wire_endpoints,
        &nec_solver::sin_eligible(&wire_endpoints, &[]),
        &nec_solver::hallen_constraint_rows(
            &wire_endpoints,
            &[],
            &segs.iter().map(|s| s.length).collect::<Vec<_>>(),
        ),
        freq_hz,
    )) {
        Some(c) => c,
        None => {
            assert!(
                !hardware_adapter_present(),
                "PH7-CHK-003: a hardware adapter is present, so None means the device \
                 solve failed, not that it was skipped"
            );
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

/// FND-158: the device must carry the sin homogeneous column too. A centre-fed
/// dipole cannot tell (its sin constant is exactly zero), so these are fed off
/// centre: a 0.55 λ wire, where a cos-only solve misses by ~45 Ω, and an
/// electrically short wire (kL ≈ 0.05), where the sin and cos columns differ in
/// norm by orders of magnitude and only the shader's equilibration keeps the
/// f32 solve conditioned.
#[test]
fn gpu_resident_solve_tracks_the_cpu_on_asymmetric_feeds() {
    use nec_solver::{assemble_z_matrix, build_geometry, build_hallen_rhs, solve_hallen};
    let freq_hz = 14.2e6_f64;
    for (len, nseg, feed, rel_tol) in [(11.6619_f64, 42u32, 11u32, 0.0), (0.168, 21, 6, 0.001)] {
        let deck = nec_parser::parse(&format!(
            "CE\nGW 1 {nseg} 0 0 0 {len} 0 0 .001\nGE\nEX 0 1 {feed} 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n"
        ))
        .expect("deck")
        .deck;
        let segs = build_geometry(&deck).expect("geometry");
        let rhs = build_hallen_rhs(&deck, &segs, freq_hz).expect("rhs");
        let ep = vec![(0usize, segs.len() - 1)];
        let idx = (feed - 1) as usize;
        let cpu = solve_hallen(
            &assemble_z_matrix(&segs, freq_hz),
            &rhs.rhs,
            &rhs.cos_vec,
            &rhs.sin_vec,
            &ep,
            &[],
        )
        .expect("cpu")
        .currents[idx];
        let z_inputs: Vec<ZSegmentInput> = segs
            .iter()
            .map(|s| ZSegmentInput {
                midpoint: s.midpoint,
                direction: s.direction,
                length: s.length,
                radius: s.radius,
            })
            .collect();
        let Some(gpu) = pollster::block_on(solve_hallen_gpu_resident(
            &z_inputs,
            &rhs.rhs,
            &rhs.cos_vec,
            &rhs.sin_vec,
            &ep,
            &nec_solver::sin_eligible(&ep, &[]),
            &nec_solver::hallen_constraint_rows(
                &ep,
                &[],
                &segs.iter().map(|s| s.length).collect::<Vec<_>>(),
            ),
            freq_hz,
        )) else {
            assert!(
                !hardware_adapter_present(),
                "L={len}: a hardware adapter is present, so None means the device solve \
                 failed its accuracy check, not that it was skipped"
            );
            eprintln!("FND-158 GPU gate: no hardware GPU adapter — gate skipped");
            return;
        };
        let one = Complex64::new(1.0, 0.0);
        let (zc, zg) = (one / cpu, one / gpu[idx]);
        let tol = (rel_tol * zc.norm()).max(2.0);
        eprintln!("FND-158 GPU gate L={len}: cpu {zc:.3} gpu {zg:.3} (tol {tol:.3})");
        assert!(
            (zg - zc).norm() <= tol,
            "L={len}: GPU {zg:.3} vs CPU {zc:.3}, tolerance {tol:.3} Ω"
        );
    }
}
