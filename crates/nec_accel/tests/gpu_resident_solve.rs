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

use nec_accel::{
    hardware_adapter_present, solve_hallen_gpu_resident, GpuSolveDeclined, ZSegmentInput,
};
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
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));

    let segs = build_geometry(&deck).expect("geometry should build");
    // The deck's 51 segments, its two free ends refined into thirds (FND-227).
    assert_eq!(segs.len(), 55);
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
        Ok(c) => c,
        // The only decline a gate may skip on — and not on a host that has a GPU,
        // where "no adapter" means the adapter selection itself broke.
        Err(GpuSolveDeclined::NoAdapter) => {
            assert!(
                !pollster::block_on(hardware_adapter_present()),
                "PH7-CHK-003: a hardware adapter is present but the solve found none"
            );
            eprintln!("PH7-CHK-003 gate: no hardware GPU adapter — gate skipped");
            return;
        }
        Err(other) => panic!("PH7-CHK-003: the device solve declined: {other:?}"),
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
        let gpu = match pollster::block_on(solve_hallen_gpu_resident(
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
        )) {
            Ok(gpu) => gpu,
            Err(GpuSolveDeclined::NoAdapter) => {
                assert!(
                    !pollster::block_on(hardware_adapter_present()),
                    "L={len}: a hardware adapter is present but the solve found none"
                );
                eprintln!("FND-158 GPU gate: no hardware GPU adapter — gate skipped");
                return;
            }
            Err(other) => panic!("L={len}: the device solve declined: {other:?}"),
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

/// Feedpoint impedance of an `nseg`-segment λ/2-ish dipole, CPU (f64) and GPU;
/// `None` for the GPU only where no hardware adapter exists.
fn dipole_cpu_gpu(nseg: u32) -> (Complex64, Option<Complex64>) {
    use nec_model::card::{Card, ExCard, GwCard};
    use nec_model::deck::NecDeck;
    use nec_solver::{assemble_z_matrix, build_geometry, build_hallen_rhs, solve_hallen};

    let freq_hz = 14.2e6_f64;
    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: nseg,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    deck.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: nseg / 2 + 1,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    let segs = build_geometry(&deck).expect("geometry");
    let rhs = build_hallen_rhs(&deck, &segs, freq_hz).expect("rhs");
    let ep = vec![(0usize, segs.len() - 1)];
    let feed = (nseg / 2) as usize;
    let one = Complex64::new(1.0, 0.0);
    let cpu = solve_hallen(
        &assemble_z_matrix(&segs, freq_hz),
        &rhs.rhs,
        &rhs.cos_vec,
        &rhs.sin_vec,
        &ep,
        &[],
    )
    .expect("CPU solve");
    let z_inputs: Vec<ZSegmentInput> = segs
        .iter()
        .map(|s| ZSegmentInput {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
            radius: s.radius,
        })
        .collect();
    let gpu = match pollster::block_on(solve_hallen_gpu_resident(
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
    )) {
        Ok(x) => Some(one / x[feed]),
        Err(GpuSolveDeclined::NoAdapter) => {
            assert!(
                !pollster::block_on(hardware_adapter_present()),
                "a hardware adapter is present but the solve found none"
            );
            None
        }
        Err(other) => panic!("{nseg} segments: the device solve declined: {other:?}"),
    };
    (one / cpu.currents[feed], gpu)
}

/// FND-185: 301 segments is where the old normal-equations solve failed even on
/// the AMD device it was built on (residual up to 0.43, negative R), because
/// A = MᴴM squared cond(M) ≈ 2.7e3. Factoring M itself it must solve on the
/// device — not be declined — and land within 2 Ω of the f64 CPU solve.
#[test]
fn a_301_segment_dipole_solves_on_the_device() {
    let (cpu, gpu) = dipole_cpu_gpu(301);
    let Some(gpu) = gpu else {
        eprintln!("FND-185 gate: no hardware GPU adapter — skipped");
        return;
    };
    eprintln!("301 segments: cpu {cpu:.4} gpu {gpu:.4}");
    assert!((gpu - cpu).norm() < 2.0, "cpu {cpu} vs gpu {gpu}");
}

/// FND-185, structurally: the solve shader hands nothing between invocations
/// through storage inside one dispatch — every hand-off is a dispatch boundary.
/// A storage barrier is how that pattern is written, and on an NVIDIA device it
/// did not make a write visible to the rest of the workgroup. This must stay
/// true on devices this repository cannot test.
#[test]
fn the_solve_shader_uses_no_storage_barrier() {
    const SHADER: &str = include_str!("../src/shaders/hallen_lu_solve.wgsl");
    assert!(
        !SHADER.contains("storageBarrier"),
        "hallen_lu_solve.wgsl must not hand data between invocations through storage within a dispatch"
    );
}

/// The triangular solves run one dispatch per column. They were one invocation
/// (`@workgroup_size(1)`) walking the whole factor — 0.73 s per pass at S = 2048,
/// three passes a solve. A serial triangular solve has to read the factor from
/// that one invocation, so no single-invocation entry point may read it; the
/// permutation pass (`cs_permute`) reads only vectors.
#[test]
fn no_single_invocation_entry_point_reads_the_factor() {
    const SHADER: &str = include_str!("../src/shaders/hallen_lu_solve.wgsl");
    let mut checked = 0;
    for (at, _) in SHADER.match_indices("@workgroup_size(1)") {
        let body_start = at + SHADER[at..].find('{').expect("entry body");
        // The entry's body ends at the first line that is exactly "}".
        let body_end = body_start + SHADER[body_start..].find("\n}\n").expect("body end");
        let body = &SHADER[body_start..body_end];
        assert!(
            !body.contains("lu_get("),
            "a single-invocation entry point reads the factor:\n{body}"
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "no single-invocation entry point found — the scan is broken"
    );
}
