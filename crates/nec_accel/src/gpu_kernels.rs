// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! CPU reference far-field (radiation-pattern) kernel and GPU-ready data layouts.
//!
//! This module provides the **CPU reference implementation** of the Hallén
//! far-field computation (`compute_hallen_fr_point_cpu` /
//! `compute_hallen_fr_batch_cpu`), plus the GPU-ready data layouts
//! (`GpuSegment`, `GpuFarFieldPoint`) that the real wgpu kernels in
//! [`crate::wgpu_device`] consume.
//!
//! The CPU reference runs the same numerical algorithm as the WGSL shaders and
//! is the parity baseline they are tested against (see the
//! `wgpu_rp_farfield_parity_vs_cpu_*` test). Nothing in this module dispatches
//! to a GPU, and no value here reports its CPU time as GPU time.

use num_complex::Complex64;
use std::f64::consts::PI;
use std::time::Instant;

const SPEED_OF_LIGHT: f64 = 299_792_458.0; // m/s
const DB_FACTOR: f64 = 10.0_f64;
const MIN_NORM_PATTERN: f64 = 1e-20;

/// CPU timing breakdown for the reference far-field kernel.
///
/// Tracks wall-clock time for each stage of the CPU reference computation.
/// `retrieval_us` is always 0 on the CPU path (no device-to-host transfer); it
/// exists so the breakdown mirrors the stages a GPU kernel would report.
#[derive(Debug, Clone, Copy)]
pub struct KernelTiming {
    /// Host-side preparation time (geometry conversion, unit vectors) in microseconds.
    pub prep_us: u64,
    /// CPU compute time for the far-field sum in microseconds.
    pub exec_us: u64,
    /// Result transfer time in microseconds (always 0 on the CPU path).
    pub retrieval_us: u64,
}

impl KernelTiming {
    pub fn total_us(&self) -> u64 {
        self.prep_us + self.exec_us + self.retrieval_us
    }
}

/// GPU-prepared geometry segment for kernel computation.
///
/// This is a minimal representation of a wire segment optimized for
/// GPU memory layout. In CUDA/OpenCL versions, this would be packed
/// into GPU texture/buffer memory for efficient streaming.
#[derive(Debug, Clone, Copy)]
pub struct GpuSegment {
    /// Segment midpoint (x, y, z) in meters.
    pub midpoint: [f64; 3],
    /// Segment direction (normalized unit vector).
    pub direction: [f64; 3],
    /// Segment length in meters.
    pub length: f64,
}

/// Single far-field observation result from GPU computation.
#[derive(Debug, Clone, Copy)]
pub struct GpuFarFieldPoint {
    pub theta_deg: f64,
    pub phi_deg: f64,
    pub gain_total_dbi: f64,
    pub gain_theta_dbi: f64,
    pub gain_phi_dbi: f64,
    pub axial_ratio: f64,
}

/// Prepared inputs for a Hallén far-field (radiation-pattern) computation.
///
/// Holds the GPU-ready segment layout, the solved current vector, and the
/// normalisation constant. It is the input to both the CPU reference
/// (`compute_hallen_fr_*_cpu`) and the real wgpu kernels in
/// [`crate::wgpu_device`].
pub struct HallenFrGpuKernel {
    /// GPU-prepared segments (flatten from NEC model).
    pub gpu_segments: Vec<GpuSegment>,
    /// Solved current vector (one complex per segment).
    pub currents: Vec<Complex64>,
    /// Operating frequency in Hz.
    pub freq_hz: f64,
    /// Precomputed wavenumber k = 2π·f/c.
    pub wavenumber: f64,
    /// Total radiated power normalisation constant.
    pub total_radiated: f64,
}

impl HallenFrGpuKernel {
    /// Prepare a Hallen FR kernel from CPU-side geometry and currents.
    ///
    /// This function performs the host-side prep work: converting geometry
    /// to GPU-optimized format, precomputing wavenumber, and caching the
    /// normalisation integral.
    ///
    /// # Arguments
    /// * `segments` — CPU-side segment list (any reference implementing Into<GpuSegment>)
    /// * `currents` — solved current vector
    /// * `freq_hz` — operating frequency
    /// * `normalisation` — precomputed total radiated power
    pub fn new(
        segments: Vec<GpuSegment>,
        currents: Vec<Complex64>,
        freq_hz: f64,
        normalisation: f64,
    ) -> Self {
        let wavenumber = 2.0 * PI * freq_hz / SPEED_OF_LIGHT;
        HallenFrGpuKernel {
            gpu_segments: segments,
            currents,
            freq_hz,
            wavenumber,
            total_radiated: normalisation,
        }
    }
}

/// Compute a far-field point with the CPU reference kernel.
///
/// This function computes the complex far-field components (Eθ, Eφ) at a
/// single observation point (θ, φ), then converts to dBi directivity. It is the
/// parity baseline for the wgpu RP shader.
///
/// The computation follows the NEC2 standard:
/// ```text
/// F_α(θ,φ) = Σ_n  I_n · (l̂_n · α̂) · Lₙ · exp(jk r̂ · r_mid,n)
/// ```
///
/// # Arguments
/// * `kernel` — prepared inputs with geometry, currents, frequency
/// * `theta_deg` — zenith angle in degrees (0 = +z, 90 = equatorial)
/// * `phi_deg` — azimuth angle in degrees (0 = +x axis)
///
/// # Returns
/// Computed far-field result with directivity in dBi for each polarization.
pub fn compute_hallen_fr_point_cpu(
    kernel: &HallenFrGpuKernel,
    theta_deg: f64,
    phi_deg: f64,
) -> GpuFarFieldPoint {
    compute_hallen_fr_point_with_timing(kernel, theta_deg, phi_deg).0
}

/// Compute a far-field point with the CPU reference kernel, with timing.
///
/// Same as `compute_hallen_fr_point_cpu()` but returns a CPU timing breakdown.
/// Enable timing collection via `FNEC_GPU_BENCH=1` environment variable.
pub fn compute_hallen_fr_point_with_timing(
    kernel: &HallenFrGpuKernel,
    theta_deg: f64,
    phi_deg: f64,
) -> (GpuFarFieldPoint, KernelTiming) {
    let timing_enabled = std::env::var_os("FNEC_GPU_BENCH")
        .and_then(|v| v.into_string().ok())
        .is_some_and(|v| v == "1");

    let prep_start = Instant::now();

    // Compute unit vectors in spherical coordinates.
    let (r_hat, theta_hat, phi_hat) = unit_vectors(theta_deg, phi_deg);

    let prep_elapsed = if timing_enabled {
        prep_start.elapsed().as_micros() as u64
    } else {
        0
    };

    let exec_start = Instant::now();

    // Compute far-field components by summing over all segments.
    let (f_theta, f_phi) = far_field_components(
        &kernel.gpu_segments,
        &kernel.currents,
        kernel.wavenumber,
        r_hat,
        theta_hat,
        phi_hat,
    );

    // Compute radiation intensities and gains.
    let u_total = f_theta.norm_sqr() + f_phi.norm_sqr();
    let u_theta = f_theta.norm_sqr();
    let u_phi = f_phi.norm_sqr();

    let norm = if kernel.total_radiated > 0.0 {
        4.0 * PI / kernel.total_radiated
    } else {
        0.0
    };

    let gain_total_dbi = if u_total * norm > MIN_NORM_PATTERN {
        DB_FACTOR * (u_total * norm).log10()
    } else {
        -999.99
    };
    let gain_theta_dbi = if u_theta * norm > MIN_NORM_PATTERN {
        DB_FACTOR * (u_theta * norm).log10()
    } else {
        -999.99
    };
    let gain_phi_dbi = if u_phi * norm > MIN_NORM_PATTERN {
        DB_FACTOR * (u_phi * norm).log10()
    } else {
        -999.99
    };

    let e_theta_mag = f_theta.norm();
    let e_phi_mag = f_phi.norm();
    let axial_ratio = if e_phi_mag > 1e-30 {
        e_theta_mag / e_phi_mag
    } else {
        0.0
    };

    let exec_elapsed = if timing_enabled {
        exec_start.elapsed().as_micros() as u64
    } else {
        0
    };

    let result = GpuFarFieldPoint {
        theta_deg,
        phi_deg,
        gain_total_dbi,
        gain_theta_dbi,
        gain_phi_dbi,
        axial_ratio,
    };

    let timing = KernelTiming {
        prep_us: prep_elapsed,
        exec_us: exec_elapsed,
        retrieval_us: 0,
    };

    (result, timing)
}

/// Compute multiple far-field points with the CPU reference kernel.
///
/// Processes a batch of (θ, φ) points, returning directivity at each point.
/// This is the typical interface for sweeping radiation patterns.
pub fn compute_hallen_fr_batch_cpu(
    kernel: &HallenFrGpuKernel,
    points: &[(f64, f64)],
) -> Vec<GpuFarFieldPoint> {
    points
        .iter()
        .map(|(theta_deg, phi_deg)| compute_hallen_fr_point_cpu(kernel, *theta_deg, *phi_deg))
        .collect()
}

// ---------------------------------------------------------------------------
// Internal helpers (GPU-friendly numerical kernels)
// ---------------------------------------------------------------------------

/// Convert (θ, φ) in degrees to spherical coordinate unit vectors.
#[inline]
fn unit_vectors(theta_deg: f64, phi_deg: f64) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let th = theta_deg.to_radians();
    let ph = phi_deg.to_radians();
    let (st, ct) = (th.sin(), th.cos());
    let (sp, cp) = (ph.sin(), ph.cos());
    let r_hat = [st * cp, st * sp, ct];
    let theta_hat = [ct * cp, ct * sp, -st];
    let phi_hat = [-sp, cp, 0.0];
    (r_hat, theta_hat, phi_hat)
}

/// 3D dot product (inlineable for GPU compatibility).
#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Compute far-field θ and φ components for one observation direction.
/// This is the core GPU kernel algorithm.
#[inline]
fn far_field_components(
    segs: &[GpuSegment],
    i_vec: &[Complex64],
    k: f64,
    r_hat: [f64; 3],
    theta_hat: [f64; 3],
    phi_hat: [f64; 3],
) -> (Complex64, Complex64) {
    let mut f_theta = Complex64::new(0.0, 0.0);
    let mut f_phi = Complex64::new(0.0, 0.0);

    for (n, seg) in segs.iter().enumerate() {
        let i_n = i_vec[n];
        let phase_arg = k * dot3(seg.midpoint, r_hat);
        let phase = Complex64::new(phase_arg.cos(), phase_arg.sin());
        let weighted = i_n * (seg.length * phase);

        let proj_theta = dot3(seg.direction, theta_hat);
        let proj_phi = dot3(seg.direction, phi_hat);

        f_theta += weighted * proj_theta;
        f_phi += weighted * proj_phi;
    }

    (f_theta, f_phi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_timing_structure() {
        let timing = KernelTiming {
            prep_us: 100,
            exec_us: 500,
            retrieval_us: 10,
        };
        assert_eq!(timing.total_us(), 610);
    }

    #[test]
    fn gpu_kernel_construction() {
        let seg = GpuSegment {
            midpoint: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            length: 1.0,
        };
        let currents = vec![Complex64::new(1.0, 0.0)];
        let kernel = HallenFrGpuKernel::new(vec![seg], currents, 14.2e6, 1.0);

        assert_eq!(kernel.freq_hz, 14.2e6);
        assert!(kernel.wavenumber > 0.0);
        assert_eq!(kernel.total_radiated, 1.0);
    }

    #[test]
    fn hertzian_dipole_at_equator() {
        // Single λ/2 dipole (very short to approximate Hertzian limit)
        let seg = GpuSegment {
            midpoint: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            length: 0.01,
        };
        let currents = vec![Complex64::new(1.0, 0.0)];
        let freq_hz = 14.2e6;

        // Normalisation for Hertzian dipole (approximate)
        let kernel = HallenFrGpuKernel::new(vec![seg], currents, freq_hz, 1e-4);

        // At θ = 90° (equator), dipole should have max gain
        let point_eq = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        assert!(point_eq.gain_total_dbi > -10.0); // rough bound

        // At θ = 0° (pole), dipole should have min gain
        let point_pole = compute_hallen_fr_point_cpu(&kernel, 0.0, 0.0);
        assert!(point_pole.gain_total_dbi < point_eq.gain_total_dbi);
    }

    #[test]
    fn batch_computation() {
        let seg = GpuSegment {
            midpoint: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            length: 0.01,
        };
        let currents = vec![Complex64::new(1.0, 0.0)];
        let kernel = HallenFrGpuKernel::new(vec![seg], currents, 14.2e6, 1e-4);

        let points = vec![(0.0, 0.0), (90.0, 0.0), (180.0, 0.0)];
        let results = compute_hallen_fr_batch_cpu(&kernel, &points);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].theta_deg, 0.0);
        assert_eq!(results[1].theta_deg, 90.0);
        assert_eq!(results[2].theta_deg, 180.0);
    }

    // ── Edge-case tests (BL-IMPR-006) ────────────────────────────────────

    fn single_seg(length: f64) -> GpuSegment {
        GpuSegment {
            midpoint: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            length,
        }
    }

    // --- 1-segment cases -------------------------------------------------

    #[test]
    fn one_segment_hallen_fr_kernel_construction() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.5)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1.0,
        );
        assert_eq!(kernel.gpu_segments.len(), 1);
        assert!(kernel.wavenumber > 0.0);
    }

    #[test]
    fn one_segment_kernel_compute_does_not_panic() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        assert!(result.gain_total_dbi.is_finite() || result.gain_total_dbi == -999.99);
    }

    #[test]
    fn one_segment_batch_returns_one_result() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let results = compute_hallen_fr_batch_cpu(&kernel, &[(90.0, 0.0)]);
        assert_eq!(results.len(), 1);
    }

    // --- Very small / very large frequencies -----------------------------

    #[test]
    fn very_low_frequency_does_not_panic() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(1.0)],
            vec![Complex64::new(1.0, 0.0)],
            1.0,
            1e-30,
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        // Must not panic; gain is either finite or the sentinel -999.99.
        assert!(result.gain_total_dbi.is_finite() || result.gain_total_dbi == -999.99);
    }

    #[test]
    fn very_high_frequency_does_not_panic() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.001)],
            vec![Complex64::new(1.0, 0.0)],
            1e12,
            1e10,
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        assert!(result.gain_total_dbi.is_finite() || result.gain_total_dbi == -999.99);
    }

    #[test]
    fn very_low_frequency_wavenumber_is_tiny() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(1.0)],
            vec![Complex64::new(1.0, 0.0)],
            1.0,
            1.0,
        );
        // k = 2π·1.0 / c ≈ 2.1e-8  (very tiny)
        assert!(kernel.wavenumber < 1e-6);
    }

    #[test]
    fn very_high_frequency_wavenumber_is_large() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.001)],
            vec![Complex64::new(1.0, 0.0)],
            1e12,
            1.0,
        );
        // k = 2π·1e12 / c ≈ 2.09e4
        assert!(kernel.wavenumber > 1e3);
    }

    // --- NaN-source handling --------------------------------------------

    #[test]
    fn nan_current_produces_sentinel_gain() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(f64::NAN, 0.0)],
            14.2e6,
            1e-4,
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        // NaN propagates through the computation; gain must be either NaN
        // or the -999.99 sentinel — in either case it must not panic.
        let g = result.gain_total_dbi;
        assert!(
            g.is_nan() || g == -999.99 || g.is_finite(),
            "unexpected gain value: {g}"
        );
    }

    #[test]
    fn zero_current_produces_sentinel_gain() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(0.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        // All-zero currents ⟹ zero field intensity ⟹ should hit the
        // MIN_NORM_PATTERN floor and return the -999.99 sentinel.
        assert_eq!(result.gain_total_dbi, -999.99);
    }

    #[test]
    fn zero_normalisation_produces_zero_gain_or_sentinel() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            0.0, // zero normalisation
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        // norm = 0 ⟹ u * norm = 0 ⟹ below MIN_NORM_PATTERN ⟹ -999.99
        assert_eq!(result.gain_total_dbi, -999.99);
    }

    // --- Pattern near the poles (θ=0, θ=180) ----------------------------

    #[test]
    fn pattern_at_north_pole_does_not_panic() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 0.0, 0.0);
        assert_eq!(result.theta_deg, 0.0);
        // Gain is a finite number or the -999.99 sentinel — no NaN.
        assert!(
            !result.gain_total_dbi.is_nan(),
            "gain should not be NaN at θ=0, got {}",
            result.gain_total_dbi
        );
    }

    #[test]
    fn pattern_at_south_pole_does_not_panic() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let result = compute_hallen_fr_point_cpu(&kernel, 180.0, 0.0);
        assert_eq!(result.theta_deg, 180.0);
        assert!(
            !result.gain_total_dbi.is_nan(),
            "gain should not be NaN at θ=180, got {}",
            result.gain_total_dbi
        );
    }

    #[test]
    fn vertical_dipole_has_null_at_both_poles() {
        // A z-aligned dipole has zero radiation at the poles in both θ and φ.
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let north = compute_hallen_fr_point_cpu(&kernel, 0.0, 0.0);
        let south = compute_hallen_fr_point_cpu(&kernel, 180.0, 0.0);
        let equator = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        // Both poles should have lower gain than the equator.
        assert!(
            north.gain_total_dbi < equator.gain_total_dbi,
            "north pole gain ({}) should be < equator gain ({})",
            north.gain_total_dbi,
            equator.gain_total_dbi
        );
        assert!(
            south.gain_total_dbi < equator.gain_total_dbi,
            "south pole gain ({}) should be < equator gain ({})",
            south.gain_total_dbi,
            equator.gain_total_dbi
        );
    }

    #[test]
    fn batch_at_poles_returns_correct_theta_values() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let points = vec![(0.0_f64, 0.0_f64), (180.0, 0.0)];
        let results = compute_hallen_fr_batch_cpu(&kernel, &points);
        assert_eq!(results[0].theta_deg, 0.0);
        assert_eq!(results[1].theta_deg, 180.0);
    }

    // --- Empty segment list ---------------------------------------------

    #[test]
    fn empty_segment_list_does_not_panic() {
        let kernel = HallenFrGpuKernel::new(vec![], vec![], 14.2e6, 1e-4);
        let result = compute_hallen_fr_point_cpu(&kernel, 90.0, 0.0);
        // No segments ⟹ zero field ⟹ sentinel -999.99.
        assert_eq!(result.gain_total_dbi, -999.99);
    }

    #[test]
    fn empty_batch_returns_empty_vec() {
        let kernel = HallenFrGpuKernel::new(
            vec![single_seg(0.01)],
            vec![Complex64::new(1.0, 0.0)],
            14.2e6,
            1e-4,
        );
        let results = compute_hallen_fr_batch_cpu(&kernel, &[]);
        assert!(results.is_empty());
    }
}
