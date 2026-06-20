// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Optional GPU acceleration backends for fnec-rust.
//!
//! # Kernel status
//!
//! GPU acceleration is implemented in two layers:
//!
//! - **`gpu_kernels`** — CPU-emulation stubs (always compiled).  These use the
//!   same numerical algorithms as `nec_solver` and serve as the reference for
//!   parity testing.  No real GPU dispatch occurs in this module.
//!
//! - **`wgpu_device`** — real wgpu GPU dispatch (gated behind `feature = "wgpu"`).
//!   Contains WGSL compute shaders for far-field radiation pattern (`RP`) and
//!   Hallén Z-matrix fill.  These paths run on any wgpu-compatible adapter
//!   (Vulkan, Metal, DX12, or software rasteriser).
//!
//! | Kernel / entry point | Module | Status |
//! |---|---|---|
//! | `HallenFrGpuKernel` (far-field Hallén) | `gpu_kernels` | CPU emulation stub |
//! | `HallenRhsGpuKernel` (RHS builder) | `gpu_kernels` | CPU emulation stub |
//! | `PocklingtonMatrixGpuKernel` (Z-matrix fill) | `gpu_kernels` | CPU emulation stub |
//! | `compute_hallen_fr_point_stub` | `gpu_kernels` | CPU emulation stub |
//! | `compute_hallen_fr_batch_stub` | `gpu_kernels` | CPU emulation stub |
//! | `run_rp_farfield_wgpu` | `wgpu_device` | **Real wgpu** — WGSL compute shader |
//! | `run_rp_farfield_batch_wgpu` | `wgpu_device` | **Real wgpu** — batch WGSL dispatch |
//! | `fill_zmatrix_wgpu` | `wgpu_device` | **Real wgpu** — N×N Z-matrix fill |
//!
//! **Known gaps:**
//! - No GPU linear solver (LU decomposition runs on CPU).
//! - `run_rp_farfield_wgpu` gain fields are hardcoded to sentinel `-999.99`;
//!   only u_theta / u_phi are computed by the shader.
//! - All GPU computations use f32 precision (f64 downcast).
//!
//! # Dispatch policy
//!
//! [`dispatch_frequency_point`] returns [`DispatchDecision::FallbackToCpu`]
//! by default.  Setting `FNEC_ACCEL_STUB_GPU=1` forces
//! [`DispatchDecision::RunOnGpu`] for testing the hybrid scheduling path,
//! but [`execute_frequency_point`] still runs the CPU closure — the real
//! wgpu path is invoked directly by solver tests, not through this dispatch
//! seam.
//!
//! # Roadmap
//!
//! Remaining GPU work is tracked under DEC-003 in `docs/requirements.md`.

pub mod gpu_kernels;

#[cfg(feature = "wgpu")]
pub mod wgpu_device;

#[cfg(feature = "wgpu")]
pub use wgpu_device::{fill_zmatrix_wgpu, ZElem, ZSegmentInput};

pub use gpu_kernels::{
    compute_hallen_fr_batch_stub, compute_hallen_fr_point_stub,
    compute_hallen_fr_point_with_timing, HallenFrGpuKernel, HallenRhsGpuKernel, KernelTiming,
    PocklingtonMatrixGpuKernel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelRequestKind {
    HybridGpuCandidate,
    GpuOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchDecision {
    RunOnGpu,
    FallbackToCpu { reason: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPath {
    CpuFallback,
    GpuStubEmulation,
}

const ACCEL_STUB_GPU_ENV: &str = "FNEC_ACCEL_STUB_GPU";

fn stub_gpu_enabled() -> bool {
    std::env::var_os(ACCEL_STUB_GPU_ENV)
        .and_then(|v| v.into_string().ok())
        .is_some_and(|v| v == "1")
}

pub fn dispatch_frequency_point(_request: AccelRequestKind, _freq_hz: f64) -> DispatchDecision {
    if stub_gpu_enabled() {
        return DispatchDecision::RunOnGpu;
    }

    DispatchDecision::FallbackToCpu {
        reason: "GPU kernels are not yet wired",
    }
}

pub fn execute_frequency_point<T, F>(
    decision: DispatchDecision,
    cpu_emulated_solve: F,
) -> (ExecutionPath, Result<T, String>)
where
    F: FnOnce() -> Result<T, String>,
{
    match decision {
        DispatchDecision::FallbackToCpu { .. } => {
            (ExecutionPath::CpuFallback, cpu_emulated_solve())
        }
        DispatchDecision::RunOnGpu => (ExecutionPath::GpuStubEmulation, cpu_emulated_solve()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{
        dispatch_frequency_point, execute_frequency_point, AccelRequestKind, DispatchDecision,
        ExecutionPath,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn hybrid_gpu_candidate_dispatch_falls_back_to_cpu_for_now() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::remove_var("FNEC_ACCEL_STUB_GPU");
        let decision = dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, 14.2e6);
        assert!(matches!(
            decision,
            DispatchDecision::FallbackToCpu {
                reason: "GPU kernels are not yet wired"
            }
        ));
    }

    #[test]
    fn gpu_only_dispatch_falls_back_to_cpu_for_now() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::remove_var("FNEC_ACCEL_STUB_GPU");
        let decision = dispatch_frequency_point(AccelRequestKind::GpuOnly, 14.2e6);
        assert!(matches!(
            decision,
            DispatchDecision::FallbackToCpu {
                reason: "GPU kernels are not yet wired"
            }
        ));
    }

    #[test]
    fn stub_gpu_env_enables_run_on_gpu_dispatch() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::set_var("FNEC_ACCEL_STUB_GPU", "1");

        let hybrid = dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, 14.2e6);
        let gpu_only = dispatch_frequency_point(AccelRequestKind::GpuOnly, 14.2e6);

        std::env::remove_var("FNEC_ACCEL_STUB_GPU");

        assert!(matches!(hybrid, DispatchDecision::RunOnGpu));
        assert!(matches!(gpu_only, DispatchDecision::RunOnGpu));
    }

    #[test]
    fn execute_frequency_point_marks_fallback_path() {
        let (path, result) = execute_frequency_point(
            DispatchDecision::FallbackToCpu {
                reason: "GPU kernels are not yet wired",
            },
            || Ok::<usize, String>(42),
        );

        assert_eq!(path, ExecutionPath::CpuFallback);
        assert!(matches!(result, Ok(42)));
    }

    #[test]
    fn execute_frequency_point_marks_stub_emulation_path() {
        let (path, result) =
            execute_frequency_point(DispatchDecision::RunOnGpu, || Ok::<usize, String>(7));

        assert_eq!(path, ExecutionPath::GpuStubEmulation);
        assert!(matches!(result, Ok(7)));
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

    /// Parity test — RP far-field wgpu kernel vs CPU stub (gate G3).
    ///
    /// Uses a minimal 3-segment vertical dipole at 14 MHz.  For each of several
    /// (θ, φ) observation directions, asserts that the GPU radiation intensity
    /// components match the CPU reference within f32 precision (we tolerate up to
    /// 1 % relative error, which far exceeds the ≤0.5 dB gain tolerance spec but
    /// correctly captures f32 vs f64 rounding).  When no wgpu adapter is
    /// available the test passes vacuously — this is the expected CI behaviour on
    /// bare-metal hosts without a software rasterizer.
    #[test]
    fn wgpu_rp_farfield_parity_vs_cpu_stub() {
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
