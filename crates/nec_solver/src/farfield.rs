// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Far-field radiation-pattern computation.
//!
//! The far-field vector potential (up to the `e^{-jkr}/(4πr)` factor) for a
//! wire MoM structure with pulse-basis currents is:
//!
//! ```text
//! F_α(θ,φ) = Σ_n  I_n · (l̂_n · α̂) · Lₙ · exp(jk r̂ · r_mid,n)
//! ```
//!
//! where α̂ is either θ̂ or φ̂, `r̂` is the unit observation vector, `l̂_n`
//! is the segment unit direction, `Lₙ` is the segment length, `r_mid,n` is
//! the segment midpoint, and `k = 2πf/c`.
//!
//! The radiation intensity pattern is proportional to `|F_θ|² + |F_φ|²`.
//! Directivity (dBi) is obtained by normalising against the total radiated
//! power, integrated numerically over the sphere.

use num_complex::Complex64;
use std::f64::consts::PI;

use crate::geometry::Segment;

const SPEED_OF_LIGHT: f64 = 299_792_458.0; // m/s
/// 4.34294481903... = 10 / ln(10)
const DB_FACTOR: f64 = 10.0_f64;
/// Minimum normalised pattern value before clamping to -999.99 dB.
const MIN_NORM_PATTERN: f64 = 1e-20;

/// Far-field observation angles (both in degrees).
#[derive(Debug, Clone, Copy)]
pub struct FarFieldPoint {
    /// Zenith angle θ (0 = +z axis, 90 = equatorial plane), in degrees.
    pub theta_deg: f64,
    /// Azimuth angle φ (0 = +x axis), in degrees.
    pub phi_deg: f64,
}

/// Far-field result for one (θ, φ) point.
#[derive(Debug, Clone, Copy)]
pub struct FarFieldResult {
    pub theta_deg: f64,
    pub phi_deg: f64,
    /// Total directivity (dBi).
    pub gain_total_dbi: f64,
    /// Theta-polarised (vertical) component directivity (dBi).
    pub gain_theta_dbi: f64,
    /// Phi-polarised (horizontal) component directivity (dBi).
    pub gain_phi_dbi: f64,
    /// Axial ratio |E_θ| / |E_φ|  (clamped to 0 when |E_φ| ≈ 0).
    pub axial_ratio: f64,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert (θ, φ) in degrees to the three spherical-coordinate unit vectors.
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

#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Compute the complex far-field θ and φ components for one observation
/// direction.  Returns `(F_θ, F_φ)` — the normalised far-field sums.
fn far_field_components(
    segs: &[Segment],
    i_vec: &[Complex64],
    k: f64,
    theta_deg: f64,
    phi_deg: f64,
) -> (Complex64, Complex64) {
    let (r_hat, theta_hat, phi_hat) = unit_vectors(theta_deg, phi_deg);

    let mut f_theta = Complex64::new(0.0, 0.0);
    let mut f_phi = Complex64::new(0.0, 0.0);

    for (n, seg) in segs.iter().enumerate() {
        let i_n = i_vec[n];
        let phase_arg = k * dot3(seg.midpoint, r_hat);
        // exp(j * phase_arg)
        let phase = Complex64::new(phase_arg.cos(), phase_arg.sin());
        let weighted = i_n * (seg.length * phase);
        // Project segment direction onto theta_hat and phi_hat.
        let proj_theta = dot3(seg.direction, theta_hat);
        let proj_phi = dot3(seg.direction, phi_hat);
        f_theta += weighted * proj_theta;
        f_phi += weighted * proj_phi;
    }

    (f_theta, f_phi)
}

// ---------------------------------------------------------------------------
// Normalisation integral
// ---------------------------------------------------------------------------

/// Compute the total radiated power proportionality constant by integrating
/// `(|F_θ|² + |F_φ|²) sin θ` over the sphere using a trapezoidal rule on a
/// 1° × 1° grid.
///
/// Returns the normalisation denominator `∫ U dΩ` (in sr × [|F|²] units).
fn integrate_over_sphere(segs: &[Segment], i_vec: &[Complex64], k: f64) -> f64 {
    // 1° resolution: θ ∈ [0°, 180°], φ ∈ [0°, 360°)
    let n_theta = 181usize;
    let n_phi = 360usize;
    let d_theta = PI / (n_theta as f64 - 1.0);
    let d_phi = 2.0 * PI / n_phi as f64;

    let mut total = 0.0_f64;

    for it in 0..n_theta {
        let theta_deg = it as f64;
        let th = theta_deg.to_radians();
        let sin_theta = th.sin();
        if sin_theta == 0.0 {
            // θ = 0° or θ = 180°: singular edge, pattern vanishes for dipole.
            continue;
        }
        // θ weight (trapezoidal: half weight for endpoints)
        let w_theta = if it == 0 || it == n_theta - 1 {
            0.5 * d_theta
        } else {
            d_theta
        };

        for ip in 0..n_phi {
            let phi_deg = ip as f64;
            let (f_t, f_p) = far_field_components(segs, i_vec, k, theta_deg, phi_deg);
            let u = f_t.norm_sqr() + f_p.norm_sqr();
            total += u * sin_theta * w_theta * d_phi;
        }
    }

    total
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute the far-field radiation pattern at a set of (θ, φ) points.
///
/// The returned [`FarFieldResult`] slice has the same length as `points`.
///
/// # Arguments
/// * `segs`     — flat segment list from [`crate::geometry::build_geometry`].
/// * `i_vec`    — solved current vector (one entry per segment).
/// * `freq_hz`  — operating frequency in Hz.
/// * `points`   — observation directions (θ, φ in degrees).
pub fn compute_radiation_pattern(
    segs: &[Segment],
    i_vec: &[Complex64],
    freq_hz: f64,
    points: &[FarFieldPoint],
) -> Vec<FarFieldResult> {
    let lambda = SPEED_OF_LIGHT / freq_hz;
    let k = 2.0 * PI / lambda;

    let total_radiated = integrate_over_sphere(segs, i_vec, k);

    points
        .iter()
        .map(|pt| {
            let (f_t, f_p) = far_field_components(segs, i_vec, k, pt.theta_deg, pt.phi_deg);
            let u_total = f_t.norm_sqr() + f_p.norm_sqr();
            let u_theta = f_t.norm_sqr();
            let u_phi = f_p.norm_sqr();

            let norm = if total_radiated > 0.0 {
                4.0 * PI / total_radiated
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

            let e_theta_mag = f_t.norm();
            let e_phi_mag = f_p.norm();
            let axial_ratio = if e_phi_mag > 1e-30 {
                e_theta_mag / e_phi_mag
            } else {
                0.0
            };

            FarFieldResult {
                theta_deg: pt.theta_deg,
                phi_deg: pt.phi_deg,
                gain_total_dbi,
                gain_theta_dbi,
                gain_phi_dbi,
                axial_ratio,
            }
        })
        .collect()
}

/// Build the list of (θ, φ) sample points described by an RP card.
///
/// The NEC RP card grid is:
///   θ ∈ [θ₀, θ₀ + (Nθ−1)·Δθ]  (Nθ points)
///   φ ∈ [φ₀, φ₀ + (Nφ−1)·Δφ]  (Nφ points)
///
/// The full set is the Cartesian product, row-major in φ (outer) then θ
/// (inner) — consistent with the NEC2 printout order.
pub fn rp_card_points(
    n_theta: u32,
    n_phi: u32,
    theta0: f64,
    phi0: f64,
    d_theta: f64,
    d_phi: f64,
) -> Vec<FarFieldPoint> {
    let nt = n_theta.max(1) as usize;
    let np = n_phi.max(1) as usize;
    let mut pts = Vec::with_capacity(nt * np);
    for ip in 0..np {
        let phi = phi0 + ip as f64 * d_phi;
        for it in 0..nt {
            let theta = theta0 + it as f64 * d_theta;
            pts.push(FarFieldPoint {
                theta_deg: theta,
                phi_deg: phi,
            });
        }
    }
    pts
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Segment;
    use num_complex::Complex64;

    /// Build a simple z-axis λ/2 dipole segment list (single segment, centred
    /// at origin).  In the far-field this is the textbook Hertzian-dipole
    /// limit: maximum gain at θ = 90°, zero at θ = 0° / 180°.
    fn hertzian_segment(length: f64) -> (Vec<Segment>, Vec<Complex64>) {
        let half = length / 2.0;
        let seg = Segment {
            tag: 1,
            tag_index: 1,
            global_index: 0,
            start: [0.0, 0.0, -half],
            end: [0.0, 0.0, half],
            midpoint: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            length,
            radius: 1e-4,
        };
        let i = vec![Complex64::new(1.0, 0.0)];
        (vec![seg], i)
    }

    #[test]
    fn hertzian_dipole_max_gain_at_equator() {
        let (segs, i_vec) = hertzian_segment(0.01); // very short → Hertzian limit
        let freq_hz = 14.2e6;
        let pts = vec![
            FarFieldPoint {
                theta_deg: 90.0,
                phi_deg: 0.0,
            },
            FarFieldPoint {
                theta_deg: 0.0,
                phi_deg: 0.0,
            },
        ];
        let results = compute_radiation_pattern(&segs, &i_vec, freq_hz, &pts);

        // θ = 90° → maximum, should be close to Hertzian-dipole directivity ≈ 1.76 dBi.
        assert!(
            results[0].gain_total_dbi > 1.0 && results[0].gain_total_dbi < 3.0,
            "θ=90° gain = {} dBi, expected ≈ 1.76 dBi",
            results[0].gain_total_dbi
        );
        // θ = 0° → null
        assert!(
            results[1].gain_total_dbi < -10.0,
            "θ=0° gain = {} dBi, expected deep null",
            results[1].gain_total_dbi
        );
    }

    #[test]
    fn hertzian_dipole_only_theta_polarised() {
        let (segs, i_vec) = hertzian_segment(0.01);
        let pts = vec![FarFieldPoint {
            theta_deg: 90.0,
            phi_deg: 0.0,
        }];
        let results = compute_radiation_pattern(&segs, &i_vec, 14.2e6, &pts);

        // z-axis dipole at equator: all energy in E_θ, zero in E_φ.
        assert!(
            results[0].gain_phi_dbi < -10.0,
            "phi component should be negligible: {} dBi",
            results[0].gain_phi_dbi
        );
        assert!(
            results[0].gain_theta_dbi > 0.0,
            "theta component should be positive: {} dBi",
            results[0].gain_theta_dbi
        );
    }

    #[test]
    fn rp_card_points_count() {
        let pts = rp_card_points(3, 4, 0.0, 0.0, 45.0, 90.0);
        assert_eq!(pts.len(), 12); // 3 × 4
    }

    #[test]
    fn rp_card_points_values() {
        let pts = rp_card_points(2, 2, 10.0, 20.0, 30.0, 40.0);
        // outer loop = phi, inner = theta
        assert!((pts[0].theta_deg - 10.0).abs() < 1e-10);
        assert!((pts[0].phi_deg - 20.0).abs() < 1e-10);
        assert!((pts[1].theta_deg - 40.0).abs() < 1e-10);
        assert!((pts[1].phi_deg - 20.0).abs() < 1e-10);
        assert!((pts[2].theta_deg - 10.0).abs() < 1e-10);
        assert!((pts[2].phi_deg - 60.0).abs() < 1e-10);
    }
}
