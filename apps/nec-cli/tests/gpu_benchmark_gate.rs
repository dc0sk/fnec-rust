// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Gate G5 (PH5-CHK-005): the GPU far-field (RP) kernel must not be more than
//! 50% slower than the CPU far-field on the large RP grid (37×73 = 2701 points).
//!
//! **In-process, device initialised once** (FND-165). This gate used to time two
//! whole `fnec` processes, `--exec gpu` against `--exec cpu`, and skip whenever
//! stderr contained any of three strings. Every `--exec gpu` run prints two of
//! them — the scheduling-seam warning and the GPU-resident solve warning — and
//! the diagnostics label contains the third, so it skipped on every run on every
//! host and never enforced anything. It could not honestly have done otherwise:
//! the 51-segment deck takes the GPU-resident SOLVE, then measured at 0.04x–0.48x
//! the CPU (a crossover, near 500 segments for one point, only since FND-185), and even at 15 segments (no resident solve) each process pays wgpu's
//! device start-up, ~100 ms, which put the GPU run at 1.63x the CPU on its own.
//!
//! What G5's documentation always said it measured is the RP far-field kernel,
//! so that is what it times now: the same currents and the same 2701 points,
//! through `run_rp_farfield_batch_wgpu` (one warm-up call pays the device
//! start-up) and through `compute_radiation_pattern`, best of several runs each.
//! It skips only where there is no hardware adapter; on a host with one, a
//! kernel that returns nothing is a failure (FND-163).

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, compute_radiation_pattern,
    ground_model_from_deck, integrate_radiated_power, rp_card_points, solve_hallen_routed,
};
use std::time::Instant;

const REPS: usize = 7;

#[test]
fn gpu_rp_kernel_not_more_than_50_percent_slower_than_cpu() {
    let text = include_str!("../../../corpus/dipole-freesp-rp-large-grid.nec");
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let ground = ground_model_from_deck(&deck);
    let f = 14.2e6;
    let mut z = assemble_z_matrix_with_ground(&segs, f, &ground);
    let currents = solve_hallen_routed(&deck, &segs, &mut z, f, &[])
        .expect("solves")
        .currents;

    let points = rp_card_points(37, 73, 0.0, 0.0, 5.0, 5.0);
    assert_eq!(points.len(), 2701);
    let tuples: Vec<(f64, f64)> = points.iter().map(|p| (p.theta_deg, p.phi_deg)).collect();
    let gpu_segs: Vec<_> = segs
        .iter()
        .map(|s| nec_accel::kernel_reference::GpuSegment {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
        })
        .collect();
    let k = 2.0 * std::f64::consts::PI * f / 299_792_458.0;
    let total = integrate_radiated_power(&segs, &currents, f, false);
    let gpu_run = || {
        pollster::block_on(nec_accel::wgpu_device::run_rp_farfield_batch_wgpu(
            &gpu_segs, &currents, k, total, &tuples,
        ))
    };

    // Warm-up: pays the device start-up, which is not the kernel's cost.
    if gpu_run().is_none() {
        assert!(
            !pollster::block_on(nec_accel::hardware_adapter_present()),
            "G5: a hardware adapter is present but the RP kernel returned nothing"
        );
        eprintln!("G5 gate: no hardware GPU adapter — skipped");
        return;
    }

    let best = |f: &mut dyn FnMut()| {
        (0..REPS)
            .map(|_| {
                let t = Instant::now();
                f();
                t.elapsed().as_micros()
            })
            .min()
            .expect("REPS > 0")
    };
    let gpu_us = best(&mut || {
        assert!(gpu_run().is_some(), "the RP kernel failed after warm-up");
    });
    let cpu_us = best(&mut || {
        let _ = compute_radiation_pattern(&segs, &currents, f, &points, &ground);
    });
    let ratio = gpu_us as f64 / cpu_us as f64;
    eprintln!("G5 gate: RP kernel {gpu_us} µs vs CPU {cpu_us} µs (ratio {ratio:.3})");
    assert!(
        ratio <= 1.5,
        "G5: the GPU RP kernel took {gpu_us} µs against the CPU's {cpu_us} µs ({ratio:.2}x > 1.5x)"
    );
}
