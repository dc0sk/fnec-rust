// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! MoM impedance matrix assembly.
//!
//! Implements the Pocklington EFIE with pulse basis functions and point
//! matching.  The double-derivative (∂²K/∂l_i∂l_j) term is handled via
//! integration by parts, leaving one line integral per element (evaluated
//! with 4-point Gauss-Legendre quadrature) plus two endpoint evaluations.
//!
//! The thin-wire reduced kernel is applied to diagonal (self) elements:
//!   R_eff = sqrt(R² + a²)   where a is the wire radius.
//! Off-diagonal elements use the exact distance R.

use num_complex::Complex64;

use crate::geometry::Segment;

// ---------------------------------------------------------------------------
// Physical constants (SI)
// ---------------------------------------------------------------------------

const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7; // H/m
const C0: f64 = 299_792_458.0; // m/s

// ---------------------------------------------------------------------------
// 4-point Gauss-Legendre quadrature nodes and weights on [-1, 1]
// ---------------------------------------------------------------------------

const GL_N: [f64; 4] = [
    -0.861_136_311_594_953,
    -0.339_981_043_584_856,
    0.339_981_043_584_856,
    0.861_136_311_594_953,
];
const GL_W: [f64; 4] = [
    0.347_854_845_137_454,
    0.652_145_154_862_626,
    0.652_145_154_862_626,
    0.347_854_845_137_454,
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Dense N×N complex impedance matrix (row-major).
pub struct ZMatrix {
    /// Dimension (number of segments).
    pub n: usize,
    data: Vec<Complex64>,
}

impl ZMatrix {
    /// Allocate an N×N zero matrix.
    pub fn new(n: usize) -> Self {
        Self {
            n,
            data: vec![Complex64::new(0.0, 0.0); n * n],
        }
    }

    /// Get element at (row, col).
    pub fn get(&self, row: usize, col: usize) -> Complex64 {
        self.data[row * self.n + col]
    }

    fn set(&mut self, row: usize, col: usize, val: Complex64) {
        self.data[row * self.n + col] = val;
    }

    /// Write an element — exposed for use by unit tests in sibling modules.
    #[cfg(test)]
    pub fn set_test(&mut self, row: usize, col: usize, val: Complex64) {
        self.set(row, col, val);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Assemble the N×N impedance matrix Z for `segs` at frequency `freq_hz`.
///
/// The matrix is computed element-by-element using the Pocklington EFIE.
/// No ground plane or loading is applied.
pub fn assemble_z_matrix(segs: &[Segment], freq_hz: f64) -> ZMatrix {
    let n = segs.len();
    let mut z = ZMatrix::new(n);

    let omega = 2.0 * std::f64::consts::PI * freq_hz;
    let k = omega / C0;
    // Prefactor: jωμ₀ / (4π)
    let pre = Complex64::new(0.0, omega * MU0 / (4.0 * std::f64::consts::PI));

    for i in 0..n {
        for j in 0..n {
            z.set(i, j, elem(&segs[i], &segs[j], k, pre, i == j));
        }
    }

    z
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Compute the (i, j) matrix element.
///
/// Z_ij = jωμ₀/(4π) * { k²·cos(α)·∫K dl  +  [∂K/∂l_i]_ends }
///
/// where K(R) = exp(−jkR)/R and the endpoint term comes from integrating
/// ∂²K/∂l_i∂l_j by parts along segment j.
fn elem(obs: &Segment, src: &Segment, k: f64, pre: Complex64, is_self: bool) -> Complex64 {
    let cos_alpha = dot3(obs.direction, src.direction);
    let half = src.length * 0.5;

    // --- Gauss-Legendre integral ∫K(R) dl ----------------------------------
    let mut int_k = Complex64::new(0.0, 0.0);
    for m in 0..4 {
        let u = GL_N[m];
        // Source point along segment j at parameter u ∈ [-1, 1]
        let r_src = mad3(src.midpoint, u * half, src.direction);
        let rho = sub3(obs.midpoint, r_src);
        let r2 = dot3(rho, rho);
        // Thin-wire reduced kernel for self element: R_eff² = R² + a²
        let r_eff = if is_self {
            (r2 + src.radius * src.radius).sqrt()
        } else {
            r2.sqrt()
        };
        int_k += GL_W[m] * green_k(r_eff, k);
    }
    // Change of variables: dl = (L_j/2) du
    int_k *= half;

    // --- Endpoint terms [∂K/∂l_i] at u = ±1 --------------------------------
    let r_plus = mad3(src.midpoint, half, src.direction); // endpoint u = +1
    let r_minus = mad3(src.midpoint, -half, src.direction); // endpoint u = −1

    let ep_plus = endpoint_contribution(obs.midpoint, r_plus, obs.direction, k);
    let ep_minus = endpoint_contribution(obs.midpoint, r_minus, obs.direction, k);

    pre * (int_k * (k * k * cos_alpha) + (ep_plus - ep_minus))
}

/// K(R) = exp(−jkR) / R  — free-space scalar Green's function.
fn green_k(r: f64, k: f64) -> Complex64 {
    Complex64::new(0.0, -k * r).exp() / r
}

/// Evaluate ∂K/∂l_i = K′(R) · (ρ̂ · t̂_i) at a single source endpoint.
///
/// K′(R) = d/dR [e^{−jkR}/R] = −(jkR + 1) e^{−jkR} / R²
fn endpoint_contribution(p: [f64; 3], r_end: [f64; 3], t_obs: [f64; 3], k: f64) -> Complex64 {
    let rho = sub3(p, r_end); // ρ = observation − source endpoint
    let r = mag3(rho);
    if r < 1e-15 {
        return Complex64::new(0.0, 0.0);
    }
    let cos_i = dot3(rho, t_obs) / r; // ρ̂ · t̂_i
    let kprime = -(Complex64::new(0.0, k * r) + 1.0) * Complex64::new(0.0, -k * r).exp() / (r * r);
    kprime * cos_i
}

// --- 3-vector helpers -------------------------------------------------------

/// a + s·b
fn mad3(a: [f64; 3], s: f64, b: [f64; 3]) -> [f64; 3] {
    [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2]]
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn mag3(v: [f64; 3]) -> f64 {
    dot3(v, v).sqrt()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Segment;

    /// Build a test segment from midpoint + direction.
    fn make_seg(
        tag: u32,
        idx: u32,
        global: usize,
        midpoint: [f64; 3],
        direction: [f64; 3],
        length: f64,
        radius: f64,
    ) -> Segment {
        let half = length * 0.5;
        Segment {
            tag,
            tag_index: idx,
            global_index: global,
            start: mad3(midpoint, -half, direction),
            end: mad3(midpoint, half, direction),
            midpoint,
            direction,
            length,
            radius,
        }
    }

    const FREQ: f64 = 28.0e6; // Hz
    const SEG_LEN: f64 = 5.354 / 11.0; // ~0.487 m — one segment of 11-seg dipole
    const RADIUS: f64 = 0.001;
    const DIR_Z: [f64; 3] = [0.0, 0.0, 1.0];

    /// Z_11 self-impedance: must be finite, non-zero, Re > 0 (radiation resistance).
    #[test]
    fn self_impedance_is_finite_and_positive_real() {
        let s = make_seg(1, 1, 0, [0.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_z_matrix(&[s], FREQ);
        let z11 = z.get(0, 0);
        assert!(z11.re.is_finite(), "Z11.re not finite");
        assert!(z11.im.is_finite(), "Z11.im not finite");
        assert!(z11.re > 0.0, "Z11.re should be positive, got {}", z11.re);
        assert!(z11.im != 0.0, "Z11.im should be non-zero");
    }

    /// Reciprocity: Z_ij = Z_ji (Pocklington EFIE is reciprocal).
    #[test]
    fn reciprocity_two_collinear_segments() {
        let s0 = make_seg(1, 1, 0, [0.0, 0.0, -SEG_LEN * 0.5], DIR_Z, SEG_LEN, RADIUS);
        let s1 = make_seg(1, 2, 1, [0.0, 0.0, SEG_LEN * 0.5], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_z_matrix(&[s0, s1], FREQ);
        let z01 = z.get(0, 1);
        let z10 = z.get(1, 0);
        assert!(
            (z01.re - z10.re).abs() < 1e-6,
            "Z01.re={:.8} ≠ Z10.re={:.8}",
            z01.re,
            z10.re
        );
        assert!(
            (z01.im - z10.im).abs() < 1e-6,
            "Z01.im={:.8} ≠ Z10.im={:.8}",
            z01.im,
            z10.im
        );
    }

    /// Identical segments far apart should have the same self-impedance.
    #[test]
    fn identical_segments_have_equal_self_impedance() {
        // Place second segment 100 m away so mutual coupling is negligible
        let s0 = make_seg(1, 1, 0, [0.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let s1 = make_seg(2, 1, 1, [100.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_z_matrix(&[s0, s1], FREQ);
        assert!(
            (z.get(0, 0).re - z.get(1, 1).re).abs() < 1e-6,
            "Z00={} vs Z11={}",
            z.get(0, 0),
            z.get(1, 1)
        );
        assert!(
            (z.get(0, 0).im - z.get(1, 1).im).abs() < 1e-6,
            "Z00={} vs Z11={}",
            z.get(0, 0),
            z.get(1, 1)
        );
    }

    /// Mutual impedance should decay with increasing separation.
    #[test]
    fn mutual_impedance_decays_with_distance() {
        let s0 = make_seg(1, 1, 0, [0.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let s_near = make_seg(2, 1, 1, [0.5, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let s_far = make_seg(3, 1, 2, [5.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_z_matrix(&[s0, s_near, s_far], FREQ);
        let near_mag = z.get(0, 1).norm();
        let far_mag = z.get(0, 2).norm();
        assert!(
            near_mag > far_mag,
            "|Z01|={near_mag:.6} should be > |Z02|={far_mag:.6}"
        );
    }

    /// Matrix dimensions must match segment count.
    #[test]
    fn matrix_dimensions_match_segment_count() {
        let segs: Vec<Segment> = (0..5)
            .map(|i| {
                make_seg(
                    1,
                    i + 1,
                    i as usize,
                    [0.0, 0.0, (i as f64) * SEG_LEN],
                    DIR_Z,
                    SEG_LEN,
                    RADIUS,
                )
            })
            .collect();
        let z = assemble_z_matrix(&segs, FREQ);
        assert_eq!(z.n, 5);
    }
}
