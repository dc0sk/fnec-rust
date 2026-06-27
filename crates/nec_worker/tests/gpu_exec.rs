// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH7-CHK-004: worker-level GPU execution tests.
//!
//! 1. A worker asked to use the GPU on an in-class deck runs the GPU-resident
//!    solve when a wgpu adapter is present (and matches the CPU solve within
//!    2 Ω); falls back to CPU with a note otherwise.
//! 2. A worker asked to use the GPU on an out-of-class (loaded) deck
//!    deterministically falls back to CPU and stays correct.

use nec_worker::solve::{solve_deck_at_frequency, solve_deck_at_frequency_with_exec};

const DIPOLE_FREESP: &str = include_str!("../../../corpus/dipole-freesp-51seg.nec");
const DIPOLE_LOADED: &str = include_str!("../../../corpus/dipole-ld-loaded-51seg.nec");

#[test]
fn gpu_capable_node_uses_gpu_when_adapter_present() {
    let freq = 14.2e6;
    let cpu = solve_deck_at_frequency(DIPOLE_FREESP, freq, "hallen").expect("cpu solve");
    let gpu = solve_deck_at_frequency_with_exec(DIPOLE_FREESP, freq, "hallen", "gpu")
        .expect("gpu-requested solve");

    eprintln!(
        "PH7-CHK-004: exec_used={}  Z_cpu=({:.3}+j{:.3})  Z=({:.3}+j{:.3})",
        gpu.exec_used, cpu.impedance_re, cpu.impedance_im, gpu.impedance_re, gpu.impedance_im
    );

    if gpu.exec_used == "gpu" {
        // GPU adapter present: the GPU-resident solve must match CPU within 2 Ω.
        assert!(
            (gpu.impedance_re - cpu.impedance_re).abs() <= 2.0
                && (gpu.impedance_im - cpu.impedance_im).abs() <= 2.0,
            "GPU node impedance differs from CPU by >2 Ω"
        );
    } else {
        // No adapter — graceful CPU fallback; result must still be the CPU answer.
        assert_eq!(gpu.exec_used, "cpu");
        assert!((gpu.impedance_re - cpu.impedance_re).abs() < 1e-9);
        eprintln!("PH7-CHK-004: no wgpu adapter — worker fell back to CPU (expected on bare CI)");
    }
}

#[test]
fn loaded_deck_out_of_class_falls_back_to_cpu() {
    let freq = 14.2e6;
    let cpu = solve_deck_at_frequency(DIPOLE_LOADED, freq, "hallen").expect("cpu solve");
    // exec=gpu, but the LD card stamps the host matrix → out of the GPU-resident
    // supported class → deterministic CPU fallback regardless of adapter.
    let gpu = solve_deck_at_frequency_with_exec(DIPOLE_LOADED, freq, "hallen", "gpu")
        .expect("gpu-requested solve");

    assert_eq!(
        gpu.exec_used, "cpu",
        "a loaded deck must fall back to the CPU solve"
    );
    assert!((gpu.impedance_re - cpu.impedance_re).abs() < 1e-9);
    assert!((gpu.impedance_im - cpu.impedance_im).abs() < 1e-9);
}
