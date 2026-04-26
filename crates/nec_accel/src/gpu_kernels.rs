// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! GPU kernel stubs for far-field (radiation pattern) computation.
//!
//! This module provides stub implementations of GPU kernels that perform
//! far-field calculations. Currently implemented on CPU, the structure is
//! designed to be replaced with CUDA/OpenCL code in future versions.
//!
//! ## Architecture
//!
//! Each kernel function follows the pattern:
//! 1. Prepare input data (geometry, currents, frequency)
//! 2. Transfer to GPU memory (stub: no-op)
//! 3. Invoke GPU kernel (stub: CPU computation marked as GPU)
//! 4. Transfer results back (stub: no-op)
//!
//! The stub implementations use the same numerical algorithms as the CPU
//! solvers to ensure parity during development.

use num_complex::Complex64;
use std::f64::consts::PI;

const SPEED_OF_LIGHT: f64 = 299_792_458.0; // m/s
const DB_FACTOR: f64 = 10.0_f64;
const MIN_NORM_PATTERN: f64 = 1e-20;

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

/// Hallen far-field radiation pattern GPU kernel.
///
/// This structure holds the prepared data for a Hallen FR computation
/// ready to be dispatched to GPU (or CPU emulation). In production CUDA/
/// OpenCL, this would manage device memory allocations and kernel invocation.
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

/// Compute far-field point using Hallen FR GPU stub kernel.
///
/// This function computes the complex far-field components (Eθ, Eφ) at a
/// single observation point (θ, φ), then converts to dBi directivity.
/// Currently implemented on CPU; production code will invoke GPU kernel.
///
/// The computation follows the NEC2 standard:
/// ```text
/// F_α(θ,φ) = Σ_n  I_n · (l̂_n · α̂) · Lₙ · exp(jk r̂ · r_mid,n)
/// ```
///
/// # Arguments
/// * `kernel` — prepared GPU kernel with geometry, currents, frequency
/// * `theta_deg` — zenith angle in degrees (0 = +z, 90 = equatorial)
/// * `phi_deg` — azimuth angle in degrees (0 = +x axis)
///
/// # Returns
/// Computed far-field result with directivity in dBi for each polarization.
pub fn compute_hallen_fr_point_stub(
    kernel: &HallenFrGpuKernel,
    theta_deg: f64,
    phi_deg: f64,
) -> GpuFarFieldPoint {
    // Compute unit vectors in spherical coordinates.
    let (r_hat, theta_hat, phi_hat) = unit_vectors(theta_deg, phi_deg);

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

    GpuFarFieldPoint {
        theta_deg,
        phi_deg,
        gain_total_dbi,
        gain_theta_dbi,
        gain_phi_dbi,
        axial_ratio,
    }
}

/// Compute multiple far-field points using Hallen FR GPU stub kernel.
///
/// Processes a batch of (θ, φ) points, returning directivity at each point.
/// This is the typical interface for sweeping radiation patterns.
pub fn compute_hallen_fr_batch_stub(
    kernel: &HallenFrGpuKernel,
    points: &[(f64, f64)],
) -> Vec<GpuFarFieldPoint> {
    points
        .iter()
        .map(|(theta_deg, phi_deg)| compute_hallen_fr_point_stub(kernel, *theta_deg, *phi_deg))
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
        let point_eq = compute_hallen_fr_point_stub(&kernel, 90.0, 0.0);
        assert!(point_eq.gain_total_dbi > -10.0); // rough bound

        // At θ = 0° (pole), dipole should have min gain
        let point_pole = compute_hallen_fr_point_stub(&kernel, 0.0, 0.0);
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
        let results = compute_hallen_fr_batch_stub(&kernel, &points);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].theta_deg, 0.0);
        assert_eq!(results[1].theta_deg, 90.0);
        assert_eq!(results[2].theta_deg, 180.0);
    }
}
