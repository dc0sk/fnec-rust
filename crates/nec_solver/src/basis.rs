// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Current-basis utilities.
//!
//! This module provides a continuity-enforcing transform from internal node
//! unknowns to per-segment currents for a single linear wire chain.
//!
//! For N segments, we define N-1 internal-node unknowns `a` and map them to
//! segment currents `I` via:
//!
//! I_n = a_{n+1} - a_n
//!
//! with boundary values a_0 = 0 and a_N = 0.
//!
//! This enforces endpoint current zeros by construction and is a practical
//! stepping stone toward rooftop/sinusoidal-like continuity behavior.

use num_complex::Complex64;

/// Dense real transform T with shape (n_segments, n_basis).
///
/// Mapping: I_seg = T * a_basis
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuityTransform {
    /// Number of wire segments.
    pub n_segments: usize,
    /// Number of continuity-basis unknowns.
    pub n_basis: usize,
    /// Row-major dense transform data.
    pub t: Vec<Vec<f64>>,
}

/// Dense real transform with overlapping sinusoidal shape functions for a
/// single chain.
///
/// Mapping: I_seg = T * a_basis, where each basis coefficient contributes to
/// the two adjacent segments around an internal junction with positive
/// sinusoidal weights.
#[derive(Debug, Clone, PartialEq)]
pub struct SinusoidalTransform {
    /// Number of wire segments.
    pub n_segments: usize,
    /// Number of basis unknowns.
    pub n_basis: usize,
    /// Row-major dense transform data.
    pub t: Vec<Vec<f64>>,
}

impl ContinuityTransform {
    /// Build the tip-constrained difference transform for a single chain.
    ///
    /// For `n_segments < 2`, the basis dimension is zero.
    pub fn for_single_chain(n_segments: usize) -> Self {
        let n_basis = n_segments.saturating_sub(1);
        let mut t = vec![vec![0.0; n_basis]; n_segments];

        // Segment n uses node values a_n and a_{n+1} (with virtual a_0=a_N=0):
        // I_n = a_{n+1} - a_n
        for n in 0..n_segments {
            // +a_{n+1}
            if n + 1 < n_segments {
                let col = n; // node index (n+1) maps to basis col n
                t[n][col] += 1.0;
            }
            // -a_n
            if n > 0 {
                let col = n - 1; // node index n maps to basis col (n-1)
                t[n][col] -= 1.0;
            }
        }

        Self {
            n_segments,
            n_basis,
            t,
        }
    }

    /// Apply transform to basis coefficients.
    pub fn segment_currents(&self, a_basis: &[Complex64]) -> Vec<Complex64> {
        let mut out = vec![Complex64::new(0.0, 0.0); self.n_segments];
        if a_basis.len() != self.n_basis {
            return out;
        }
        for (r, row) in self.t.iter().enumerate() {
            let mut sum = Complex64::new(0.0, 0.0);
            for (c, coeff) in row.iter().enumerate() {
                if *coeff != 0.0 {
                    sum += a_basis[c] * *coeff;
                }
            }
            out[r] = sum;
        }
        out
    }
}

impl SinusoidalTransform {
    /// Build overlapping sinusoidal shape functions for a single chain.
    ///
    /// For internal junction coefficient a_j, contributions are placed on the
    /// two neighboring segments j and j+1 with positive sine-envelope weights.
    pub fn for_single_chain(n_segments: usize) -> Self {
        let n_basis = n_segments.saturating_sub(1);
        let mut t = vec![vec![0.0; n_basis]; n_segments];

        if n_segments > 0 {
            let denom = (n_segments + 1) as f64;
            for basis_idx in 0..n_basis {
                let left_seg = basis_idx;
                let right_seg = basis_idx + 1;
                t[left_seg][basis_idx] =
                    (std::f64::consts::PI * ((left_seg + 1) as f64) / denom).sin();
                t[right_seg][basis_idx] =
                    (std::f64::consts::PI * ((right_seg + 1) as f64) / denom).sin();
            }
        }

        Self {
            n_segments,
            n_basis,
            t,
        }
    }

    /// Apply transform to basis coefficients.
    pub fn segment_currents(&self, a_basis: &[Complex64]) -> Vec<Complex64> {
        let mut out = vec![Complex64::new(0.0, 0.0); self.n_segments];
        if a_basis.len() != self.n_basis {
            return out;
        }
        for (r, row) in self.t.iter().enumerate() {
            let mut sum = Complex64::new(0.0, 0.0);
            for (c, coeff) in row.iter().enumerate() {
                if *coeff != 0.0 {
                    sum += a_basis[c] * *coeff;
                }
            }
            out[r] = sum;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_shape_for_single_chain() {
        let tr = ContinuityTransform::for_single_chain(11);
        assert_eq!(tr.n_segments, 11);
        assert_eq!(tr.n_basis, 10);
        assert_eq!(tr.t.len(), 11);
        assert_eq!(tr.t[0].len(), 10);
    }

    #[test]
    fn transform_has_expected_stencil() {
        let tr = ContinuityTransform::for_single_chain(4);
        // I0 = +a1
        assert_eq!(tr.t[0], vec![1.0, 0.0, 0.0]);
        // I1 = -a1 + a2
        assert_eq!(tr.t[1], vec![-1.0, 1.0, 0.0]);
        // I2 = -a2 + a3
        assert_eq!(tr.t[2], vec![0.0, -1.0, 1.0]);
        // I3 = -a3
        assert_eq!(tr.t[3], vec![0.0, 0.0, -1.0]);
    }

    #[test]
    fn mapping_produces_segment_currents() {
        let tr = ContinuityTransform::for_single_chain(4);
        let a = vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(2.0, 0.0),
            Complex64::new(3.0, 0.0),
        ];
        let i = tr.segment_currents(&a);
        // [a1, -a1+a2, -a2+a3, -a3]
        assert_eq!(i[0], Complex64::new(1.0, 0.0));
        assert_eq!(i[1], Complex64::new(1.0, 0.0));
        assert_eq!(i[2], Complex64::new(1.0, 0.0));
        assert_eq!(i[3], Complex64::new(-3.0, 0.0));
    }

    #[test]
    fn sinusoidal_transform_has_expected_shape() {
        let tr = SinusoidalTransform::for_single_chain(11);
        assert_eq!(tr.n_segments, 11);
        assert_eq!(tr.n_basis, 10);
        assert_eq!(tr.t.len(), 11);
        assert_eq!(tr.t[0].len(), 10);
    }

    #[test]
    fn sinusoidal_taper_reduces_edge_rows() {
        let tr = SinusoidalTransform::for_single_chain(4);
        let edge = tr.t[0][0].abs();
        let mid = tr.t[1][1].abs();
        assert!(mid > edge);
    }

    #[test]
    fn sinusoidal_transform_overlaps_adjacent_segments() {
        let tr = SinusoidalTransform::for_single_chain(4);
        assert!(tr.t[0][0] > 0.0);
        assert!(tr.t[1][0] > 0.0);
        assert!(tr.t[1][1] > 0.0);
        assert!(tr.t[2][1] > 0.0);
        assert_eq!(tr.t[0][1], 0.0);
        assert_eq!(tr.t[3][0], 0.0);
    }
}
