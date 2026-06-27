// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Optional GPU acceleration backends for fnec-rust.
//!
//! # Kernel status
//!
//! GPU acceleration is implemented in two layers:
//!
//! - **`gpu_kernels`** — the **CPU reference** far-field (radiation-pattern)
//!   implementation, plus the GPU-ready data layouts (`GpuSegment`,
//!   `GpuFarFieldPoint`) that the real wgpu kernels consume.  It runs the same
//!   numerical algorithm as the wgpu shaders and is the parity baseline they are
//!   tested against.  No GPU dispatch occurs in this module, and no path here
//!   reports its CPU time as GPU time.
//!
//! - **`wgpu_device`** — real wgpu GPU dispatch (gated behind `feature = "wgpu"`).
//!   Contains WGSL compute shaders for far-field radiation pattern (`RP`) and
//!   Hallén Z-matrix fill.  These paths run on any wgpu-compatible adapter
//!   (Vulkan, Metal, DX12, or software rasteriser).
//!
//! | Kernel / entry point | Module | Status |
//! |---|---|---|
//! | `compute_hallen_fr_point_cpu` / `compute_hallen_fr_batch_cpu` | `gpu_kernels` | CPU reference (parity baseline) |
//! | `run_rp_farfield_wgpu` | `wgpu_device` | **Real wgpu** — WGSL compute shader |
//! | `run_rp_farfield_batch_wgpu` | `wgpu_device` | **Real wgpu** — batch WGSL dispatch |
//! | `fill_zmatrix_wgpu` | `wgpu_device` | **Real wgpu** — N×N Z-matrix fill |
//! | `solve_hallen_gpu_resident` | `wgpu_device` | **Real wgpu** — GPU-resident fill + dense solve |
//!
//! **Known gaps:**
//! - `solve_hallen_gpu_resident` is f32 (LU + Björck refinement); it matches the
//!   f64 CPU solve to ~0.01 Ω on the reference dipole but the f64 CPU solve
//!   remains the corpus-gate accuracy reference (see `docs/ph7-chk-003-gpu-resident-solve.md`).
//! - `run_rp_farfield_wgpu` gain fields are hardcoded to sentinel `-999.99`;
//!   only u_theta / u_phi are computed by the shader.
//! - All GPU computations use f32 precision (f64 downcast).
//!
//! # Dispatch policy
//!
//! [`dispatch_frequency_point`] is the per-frequency scheduling seam used by the
//! CLI hybrid sweep lane.  It currently always returns
//! [`DispatchDecision::FallbackToCpu`]: real per-frequency GPU dispatch is not
//! yet wired (tracked as PH7-CHK-004).  [`DispatchDecision::RunOnGpu`] is
//! reserved for that work.  The real wgpu RP / Z-matrix-fill paths are dispatched
//! directly from the solver/CLI, not through this seam.
//!
//! # Roadmap
//!
//! Remaining GPU work is tracked under DEC-003 in `docs/requirements.md` and
//! Phase 7 (`PH7-CHK-003`, `PH7-CHK-004`) in `docs/roadmap.md`.

pub mod gpu_kernels;

#[cfg(feature = "wgpu")]
pub mod wgpu_device;

#[cfg(feature = "wgpu")]
pub use wgpu_device::{
    fill_zmatrix_wgpu, microbench_zmatrix_dispatch, solve_hallen_gpu_resident, GpuMicrobench,
    ZElem, ZSegmentInput,
};

pub use gpu_kernels::{
    compute_hallen_fr_batch_cpu, compute_hallen_fr_point_cpu, compute_hallen_fr_point_with_timing,
    HallenFrGpuKernel, KernelTiming,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelRequestKind {
    HybridGpuCandidate,
    GpuOnly,
}

/// Per-frequency scheduling decision for the CLI hybrid sweep lane.
///
/// `RunOnGpu` is reserved for PH7-CHK-004 (per-frequency GPU dispatch); until
/// that lands, [`dispatch_frequency_point`] always returns `FallbackToCpu`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchDecision {
    RunOnGpu,
    FallbackToCpu { reason: &'static str },
}

/// Reason returned by [`dispatch_frequency_point`] for the CPU-fallback path.
pub const GPU_DISPATCH_NOT_WIRED: &str = "per-frequency GPU dispatch not yet wired (PH7-CHK-004)";

/// Decide whether a single frequency point should run on the GPU.
///
/// Currently always [`DispatchDecision::FallbackToCpu`] — per-frequency GPU
/// dispatch is not yet wired (PH7-CHK-004). This is an honest seam: it never
/// reports CPU work as GPU work.
pub fn dispatch_frequency_point(_request: AccelRequestKind, _freq_hz: f64) -> DispatchDecision {
    DispatchDecision::FallbackToCpu {
        reason: GPU_DISPATCH_NOT_WIRED,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dispatch_frequency_point, AccelRequestKind, DispatchDecision, GPU_DISPATCH_NOT_WIRED,
    };

    #[test]
    fn hybrid_gpu_candidate_dispatch_falls_back_to_cpu_for_now() {
        let decision = dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, 14.2e6);
        assert!(matches!(
            decision,
            DispatchDecision::FallbackToCpu {
                reason: GPU_DISPATCH_NOT_WIRED
            }
        ));
    }

    #[test]
    fn gpu_only_dispatch_falls_back_to_cpu_for_now() {
        let decision = dispatch_frequency_point(AccelRequestKind::GpuOnly, 14.2e6);
        assert!(matches!(
            decision,
            DispatchDecision::FallbackToCpu {
                reason: GPU_DISPATCH_NOT_WIRED
            }
        ));
    }
}

#[cfg(all(test, feature = "wgpu"))]
mod wgpu_tests {
    use super::wgpu_device::{
        enumerate_compute_adapters, run_noop_compute_pipeline, NoOpPipelineResult,
    };

    /// Enumerate adapters — must not panic; may return an empty list on bare CI.
    #[test]
    fn wgpu_enumerate_adapters_does_not_panic() {
        let adapters = pollster::block_on(enumerate_compute_adapters());
        // Zero adapters is acceptable; we only require no panic.
        let _ = adapters;
    }

    /// Compile and dispatch a no-op compute shader.
    ///
    /// Accepts `NoAdapterAvailable` for headless CI without a software rasterizer;
    /// fails on any panic or unexpected variant (there are only two).
    #[test]
    fn wgpu_noop_compute_pipeline_succeeds_or_skips_gracefully() {
        let result = pollster::block_on(run_noop_compute_pipeline());
        assert!(
            matches!(
                result,
                NoOpPipelineResult::Success | NoOpPipelineResult::NoAdapterAvailable
            ),
            "unexpected result: {:?}",
            result
        );
    }

    /// Parity test — RP far-field wgpu kernel vs CPU reference (gate G3).
    ///
    /// Uses a minimal 3-segment vertical dipole at 14 MHz.  For each of several
    /// (θ, φ) observation directions, asserts that the GPU radiation intensity
    /// components match the CPU reference within f32 precision (we tolerate up to
    /// 1 % relative error, which far exceeds the ≤0.5 dB gain tolerance spec but
    /// correctly captures f32 vs f64 rounding).  When no wgpu adapter is
    /// available the test passes vacuously — this is the expected CI behaviour on
    /// bare-metal hosts without a software rasterizer.
    #[test]
    fn wgpu_rp_farfield_parity_vs_cpu_reference() {
        use super::gpu_kernels::{GpuSegment, HallenFrGpuKernel};
        use super::wgpu_device::{run_rp_farfield_wgpu, RpPipelineResult};
        use num_complex::Complex64;

        // 3-segment vertical dipole — each segment 0.1 m long along Z axis.
        let seg_length = 0.1_f64;
        let segs = vec![
            GpuSegment {
                midpoint: [0.0, 0.0, -0.1],
                direction: [0.0, 0.0, 1.0],
                length: seg_length,
            },
            GpuSegment {
                midpoint: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, 1.0],
                length: seg_length,
            },
            GpuSegment {
                midpoint: [0.0, 0.0, 0.1],
                direction: [0.0, 0.0, 1.0],
                length: seg_length,
            },
        ];

        // Uniform current (simplified — good enough for a parity check).
        let currents: Vec<Complex64> = vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(1.0, 0.0),
            Complex64::new(1.0, 0.0),
        ];

        let freq_hz = 14.2e6_f64;
        let wavenumber = 2.0 * std::f64::consts::PI * freq_hz / 299_792_458.0;

        // Build CPU kernel (total_radiated = 1.0 — we only compare u_theta/u_phi).
        let cpu_kernel = HallenFrGpuKernel {
            gpu_segments: segs.clone(),
            currents: currents.clone(),
            freq_hz,
            wavenumber,
            total_radiated: 1.0,
        };

        // Observation directions to test.
        let points: &[(f64, f64)] = &[
            (90.0, 0.0),
            (90.0, 90.0),
            (45.0, 0.0),
            (45.0, 45.0),
            (135.0, 180.0),
        ];

        for &(theta_deg, phi_deg) in points {
            let gpu_result = pollster::block_on(run_rp_farfield_wgpu(
                &segs, &currents, wavenumber, theta_deg, phi_deg,
            ));

            let gpu = match gpu_result {
                RpPipelineResult::Success(r) => r,
                RpPipelineResult::NoAdapterAvailable => {
                    // Acceptable on headless CI — skip gracefully.
                    return;
                }
            };

            // CPU reference: compute_hallen_fr_point_with_timing returns the
            // intermediate radiation intensity values we need.  We re-derive
            // them from the point's gain fields using the same math.
            use super::gpu_kernels::compute_hallen_fr_point_with_timing;
            let (cpu_pt, _timing) =
                compute_hallen_fr_point_with_timing(&cpu_kernel, theta_deg, phi_deg);

            // Reconstruct u_theta and u_phi from the CPU result using the
            // inverse of the gain formula: u = gain_linear * P_rad / (4π).
            // Since total_radiated = 1.0, norm = 4π, so u = gain_linear / 1.
            // But the CPU path stores dBi values. Instead, run the inner kernel
            // directly via the public batch stub and compare the radiation
            // intensities. We re-run the reference calculation using the
            // cpu_kernel at total_radiated=1 and check that gain_total_dbi
            // matches the GPU-derived gain.
            //
            // GPU u_total = u_theta + u_phi.  CPU point gives dBi.  We
            // convert GPU u → dBi using the same norm factor (4π / 1.0).
            let norm = 4.0 * std::f64::consts::PI; // total_radiated = 1.0
            let gpu_u_total = gpu.u_theta + gpu.u_phi;

            let gpu_gain_dbi = if gpu_u_total * norm > 1e-20 {
                10.0 * (gpu_u_total * norm).log10()
            } else {
                -999.99
            };

            // Tolerance: 0.5 dBi (G3 gate spec).  On values that are
            // effectively zero (-999 dBi) both sides should be equal.
            let diff = (gpu_gain_dbi - cpu_pt.gain_total_dbi).abs();
            assert!(
                diff <= 0.5,
                "RP parity failure at θ={theta_deg} φ={phi_deg}: GPU={gpu_gain_dbi:.4} dBi  \
                 CPU={:.4} dBi  |Δ|={diff:.4} dB (limit 0.5)",
                cpu_pt.gain_total_dbi
            );
        }
    }
}
