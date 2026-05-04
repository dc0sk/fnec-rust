// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Gate G6 (PH5-CHK-006): GPU Z-matrix fill parity test.
//!
//! Builds the Hallén A-matrix for a 51-segment half-wave dipole at 14 MHz
//! on the GPU via `fill_zmatrix_wgpu` and compares every element against the
//! CPU reference (`assemble_z_matrix`).
//!
//! Acceptance: relative error ≤ 1×10⁻⁴ for every element.  (GPU computes in
//! f32; CPU in f64, so full double precision is not expected, but 4-digit
//! agreement is required.)
//!
//! The test passes vacuously (prints a skip notice) when no wgpu adapter is
//! available, matching the pattern used by other GPU gate tests in this crate.

use nec_accel::fill_zmatrix_wgpu;
use nec_accel::ZSegmentInput;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a 51-segment dipole at 14 MHz and return its CPU Z-matrix alongside
/// the corresponding `ZSegmentInput` slice ready for the GPU path.
fn build_test_inputs() -> (Vec<ZSegmentInput>, Vec<[f64; 2]>, usize, f64) {
    use nec_model::card::{Card, GwCard};
    use nec_model::deck::NecDeck;
    use nec_solver::{assemble_z_matrix, build_geometry};

    let freq_hz = 14.0e6_f64;
    // Half-wave dipole: total length ≈ λ/2 ≈ 10.7 m
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

    let segs = build_geometry(&deck).expect("geometry should build");
    assert_eq!(segs.len(), 51);

    // CPU reference matrix (f64)
    let z_cpu = assemble_z_matrix(&segs, freq_hz);
    let n = segs.len();
    let cpu_flat: Vec<[f64; 2]> = (0..n)
        .flat_map(|i| {
            (0..n)
                .map(|j| z_cpu.get(i, j))
                .map(|c| [c.re, c.im])
                .collect::<Vec<_>>()
        })
        .collect();

    // Convert segments to ZSegmentInput
    let gpu_segs: Vec<ZSegmentInput> = segs
        .iter()
        .map(|s| ZSegmentInput {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
            radius: s.radius,
        })
        .collect();

    (gpu_segs, cpu_flat, n, freq_hz)
}

// ---------------------------------------------------------------------------
// Gate G6 parity test
// ---------------------------------------------------------------------------

/// Gate G6: GPU Z-matrix fill must match CPU within 1×10⁻⁴ relative error.
///
/// Skips vacuously when no wgpu adapter is available (e.g. bare-metal CI).
#[test]
fn gpu_zmatrix_fill_matches_cpu_within_1e4_relative() {
    let (gpu_segs, cpu_flat, n, freq_hz) = build_test_inputs();

    let gpu_result = pollster::block_on(fill_zmatrix_wgpu(&gpu_segs, freq_hz));

    let Some(gpu_flat) = gpu_result else {
        eprintln!("G6 gate: no hardware GPU adapter — parity gate skipped (software fallback)");
        return;
    };

    assert_eq!(
        gpu_flat.len(),
        n * n,
        "GPU result length mismatch: expected {}, got {}",
        n * n,
        gpu_flat.len()
    );

    const REL_TOL: f64 = 1.0e-4;
    let mut max_err: f64 = 0.0;
    let mut worst = (0usize, 0usize, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);

    for idx in 0..(n * n) {
        let [cpu_re, cpu_im] = cpu_flat[idx];
        let gpu_re = gpu_flat[idx].re as f64;
        let gpu_im = gpu_flat[idx].im as f64;

        let abs_cpu = (cpu_re * cpu_re + cpu_im * cpu_im).sqrt();
        let diff_re = (gpu_re - cpu_re).abs();
        let diff_im = (gpu_im - cpu_im).abs();
        let abs_diff = (diff_re * diff_re + diff_im * diff_im).sqrt();

        let rel_err = if abs_cpu > 1e-30 {
            abs_diff / abs_cpu
        } else {
            abs_diff // near-zero: use absolute
        };

        if rel_err > max_err {
            max_err = rel_err;
            let i = idx / n;
            let j = idx % n;
            worst = (i, j, cpu_re, cpu_im, gpu_re, gpu_im);
        }
    }

    let (wi, wj, cpu_re, cpu_im, gpu_re, gpu_im) = worst;
    eprintln!(
        "G6 gate: n={n}  max_rel_err={max_err:.2e}  limit={REL_TOL:.0e}  \
         worst=[{wi},{wj}] cpu=({cpu_re:.6e},{cpu_im:.6e}) gpu=({gpu_re:.6e},{gpu_im:.6e})"
    );

    assert!(
        max_err <= REL_TOL,
        "G6 parity failure: max relative error {max_err:.3e} > limit {REL_TOL:.0e} at [{wi},{wj}]"
    );
}
