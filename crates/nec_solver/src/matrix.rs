// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Hallén A-matrix assembly.
//!
//! Implements Hallén's integral equation with pulse basis functions and
//! point matching.
//!
//! Matrix element A[m,n] = cos(α) · ∫_{seg_n} G(R_eff) dl
//!
//! where G(R) = exp(−jkR)/R is the free-space scalar Green's function,
//! R_eff = sqrt(|r_obs − r_src|² + a²) is the reduced kernel distance,
//! and cos(α) is the dot product of the observation and source unit
//! direction vectors.
//!
//! For the self element (m == n) singularity subtraction is applied:
//!   G(R_eff) = [G(R_eff) − 1/R_eff]  +  1/R_eff
//! The smooth part is integrated with 4-point GL; the singular 1/R_eff
//! part is handled analytically via 2·ln((L/2 + R_end)/a).
//!
//! Off-diagonal elements use 8-point GL with the reduced kernel.

use num_complex::Complex64;

use crate::geometry::{GroundModel, Segment};

// ---------------------------------------------------------------------------
// Physical constants (SI)
// ---------------------------------------------------------------------------

const C0: f64 = 299_792_458.0; // m/s
const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7; // H/m

// ---------------------------------------------------------------------------
// 4-point Gauss-Legendre quadrature nodes and weights on [-1, 1]
// ---------------------------------------------------------------------------

const GL_N4: [f64; 4] = [
    -0.861_136_311_594_953,
    -0.339_981_043_584_856,
    0.339_981_043_584_856,
    0.861_136_311_594_953,
];
const GL_W4: [f64; 4] = [
    0.347_854_845_137_454,
    0.652_145_154_862_626,
    0.652_145_154_862_626,
    0.347_854_845_137_454,
];

// 8-point Gauss-Legendre quadrature (better accuracy for off-diagonal)
const GL_N8: [f64; 8] = [
    -0.960_289_856_497_536,
    -0.796_666_477_413_627,
    -0.525_532_409_916_329,
    -0.183_434_642_495_650,
    0.183_434_642_495_650,
    0.525_532_409_916_329,
    0.796_666_477_413_627,
    0.960_289_856_497_536,
];
const GL_W8: [f64; 8] = [
    0.101_228_536_290_376,
    0.222_381_034_453_374,
    0.313_706_645_877_887,
    0.362_683_783_378_362,
    0.362_683_783_378_362,
    0.313_706_645_877_887,
    0.222_381_034_453_374,
    0.101_228_536_290_376,
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Dense N×N complex matrix (row-major).
///
/// Used to store the Hallén A-matrix whose elements are Green's function
/// integrals: A[m,n] = cos(α) · ∫_{seg_n} G(R_eff) dl.
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

    pub(crate) fn set(&mut self, row: usize, col: usize, val: Complex64) {
        self.data[row * self.n + col] = val;
    }

    /// Write an element — exposed for use by unit tests in sibling modules.
    #[cfg(test)]
    pub fn set_test(&mut self, row: usize, col: usize, val: Complex64) {
        self.set(row, col, val);
    }

    /// Add `loads[i]` to the diagonal element [i, i] for each segment.
    ///
    /// Used to apply series lumped or distributed impedance loads to the
    /// assembled MoM matrix. `loads` must have the same length as the matrix
    /// dimension (`n`).
    pub fn add_to_diagonal(&mut self, loads: &[Complex64]) {
        debug_assert_eq!(
            loads.len(),
            self.n,
            "loads length must equal matrix dimension"
        );
        for (i, &z) in loads.iter().enumerate() {
            let cur = self.data[i * self.n + i];
            self.data[i * self.n + i] = cur + z;
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Assemble the N×N Hallén A-matrix for `segs` at frequency `freq_hz` in free space.
///
/// Convenience wrapper around [`assemble_z_matrix_with_ground`] using
/// [`GroundModel::FreeSpace`].
pub fn assemble_z_matrix(segs: &[Segment], freq_hz: f64) -> ZMatrix {
    assemble_z_matrix_with_ground(segs, freq_hz, &GroundModel::FreeSpace)
}

/// Assemble the N×N Hallén A-matrix for `segs` at frequency `freq_hz`,
/// optionally including a ground-plane image contribution.
///
/// For [`GroundModel::PerfectConductor`] each element A[i,j] gets an
/// additional term from the mirror image of segment j across z = 0:
///
/// ```text
/// A[i,j] = A_free[i,j] + A_free(segs[i], image(segs[j]))
/// ```
///
/// where `image(seg)` has its z-coordinates negated and its z-direction
/// component sign-flipped, which produces the correct ±1 reflection
/// coefficient for horizontal (cos = +1) and vertical (cos = −1) currents.
pub fn assemble_z_matrix_with_ground(
    segs: &[Segment],
    freq_hz: f64,
    ground: &GroundModel,
) -> ZMatrix {
    let n = segs.len();
    let mut z = ZMatrix::new(n);

    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;

    for i in 0..n {
        for j in 0..n {
            let direct = elem(&segs[i], &segs[j], k, i == j);
            let image_contrib = match ground {
                GroundModel::FreeSpace | GroundModel::Deferred { .. } => Complex64::new(0.0, 0.0),
                GroundModel::PerfectConductor => {
                    let img = image_segment(&segs[j]);
                    // Image is always at a different location than the
                    // observation segment, so never treat as self-element.
                    elem(&segs[i], &img, k, false)
                }
            };
            z.set(i, j, direct + image_contrib);
        }
    }

    z
}

/// Assemble the N×N Pocklington impedance matrix for `segs` at `freq_hz`.
///
/// Z_ij = (j*omega*mu0/4pi) * cos(alpha) * [k^2 * ∫G dl + (gzp2 - gzp1)]
///
/// where gzp terms are source-endpoint derivatives of the scalar Green's
/// function after integration-by-parts treatment of the axial second
/// derivative term.
pub fn assemble_pocklington_matrix(segs: &[Segment], freq_hz: f64) -> ZMatrix {
    let n = segs.len();
    let mut z = ZMatrix::new(n);

    let omega = 2.0 * std::f64::consts::PI * freq_hz;
    let k = omega / C0;
    let pre = Complex64::new(0.0, omega * MU0 / (4.0 * std::f64::consts::PI));

    for i in 0..n {
        for j in 0..n {
            z.set(i, j, elem_pocklington(&segs[i], &segs[j], k, pre, i == j));
        }
    }

    z
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Mirror a segment across the z = 0 ground plane.
///
/// - Endpoints and midpoint have their z-coordinate negated.
/// - The direction vector has its z-component negated.
///
/// For a perfect electric conductor at z = 0, the image current of a
/// horizontal element flows in the same direction (cos_alpha = same sign) and
/// the image of a vertical element flows in the opposite direction
/// (cos_alpha = −1), which is the correct image-theory reflection coefficient.
fn image_segment(seg: &Segment) -> Segment {
    Segment {
        tag: seg.tag,
        tag_index: seg.tag_index,
        global_index: seg.global_index,
        start: [seg.start[0], seg.start[1], -seg.start[2]],
        end: [seg.end[0], seg.end[1], -seg.end[2]],
        midpoint: [seg.midpoint[0], seg.midpoint[1], -seg.midpoint[2]],
        direction: [seg.direction[0], seg.direction[1], -seg.direction[2]],
        length: seg.length,
        radius: seg.radius,
    }
}

/// Compute the (obs, src) Hallén A-matrix element:
///   A = cos(α) · ∫_{src} G(R_eff) dl
///
/// For is_self: singularity subtraction + analytic log term.
/// For off-diagonal: 8-point GL with reduced kernel.
fn elem(obs: &Segment, src: &Segment, k: f64, is_self: bool) -> Complex64 {
    let cos_alpha = dot3(obs.direction, src.direction);
    let half = src.length * 0.5;
    let a = src.radius;

    let int_k = if is_self {
        // Singularity subtraction: smooth (GL) + analytic (log) parts.
        let mut smooth = Complex64::new(0.0, 0.0);
        for m in 0..4 {
            let l = GL_N4[m] * half;
            let r_eff = (l * l + a * a).sqrt();
            let dynamic_part = green_k(r_eff, k) - 1.0 / r_eff;
            smooth += GL_W4[m] * dynamic_part;
        }
        smooth *= half;

        let r_end = (half * half + a * a).sqrt();
        let analytic = 2.0 * ((half + r_end) / a).ln();

        smooth + analytic
    } else {
        // Off-diagonal: 8-point GL with reduced kernel.
        let mut sum = Complex64::new(0.0, 0.0);
        for m in 0..8 {
            let r_src = mad3(src.midpoint, GL_N8[m] * half, src.direction);
            let rho = sub3(obs.midpoint, r_src);
            let r_sq = rho[0] * rho[0] + rho[1] * rho[1] + rho[2] * rho[2];
            let r_eff = (r_sq + a * a).sqrt();
            sum += GL_W8[m] * green_k(r_eff, k);
        }
        sum * half
    };

    int_k * cos_alpha
}

fn elem_pocklington(
    obs: &Segment,
    src: &Segment,
    k: f64,
    pre: Complex64,
    is_self: bool,
) -> Complex64 {
    let cos_alpha = dot3(obs.direction, src.direction);
    let half = src.length * 0.5;
    let a = src.radius;

    let int_k = if is_self {
        let mut smooth = Complex64::new(0.0, 0.0);
        for m in 0..4 {
            let l = GL_N4[m] * half;
            let r_eff = (l * l + a * a).sqrt();
            let dynamic_part = green_k(r_eff, k) - 1.0 / r_eff;
            smooth += GL_W4[m] * dynamic_part;
        }
        smooth *= half;

        let r_end = (half * half + a * a).sqrt();
        let analytic = 2.0 * ((half + r_end) / a).ln();
        smooth + analytic
    } else {
        let mut sum = Complex64::new(0.0, 0.0);
        for m in 0..8 {
            let r_src = mad3(src.midpoint, GL_N8[m] * half, src.direction);
            let rho = sub3(obs.midpoint, r_src);
            let r = (rho[0] * rho[0] + rho[1] * rho[1] + rho[2] * rho[2]).sqrt();
            sum += GL_W8[m] * green_k(r, k);
        }
        sum * half
    };

    let r_plus = mad3(src.midpoint, half, src.direction);
    let r_minus = mad3(src.midpoint, -half, src.direction);
    let gzp = gzp2_minus_gzp1(obs.midpoint, r_plus, r_minus, src.direction, k);

    pre * cos_alpha * (k * k * int_k + gzp)
}

fn gzp2_minus_gzp1(
    p_obs: [f64; 3],
    r_plus: [f64; 3],
    r_minus: [f64; 3],
    t_src: [f64; 3],
    k: f64,
) -> Complex64 {
    let gzp = |r_end: [f64; 3]| -> Complex64 {
        let rho = sub3(r_end, p_obs);
        let r = (rho[0] * rho[0] + rho[1] * rho[1] + rho[2] * rho[2]).sqrt();
        if r < 1e-15 {
            return Complex64::new(0.0, 0.0);
        }
        let cos_src = dot3(rho, t_src) / r;
        let kprime =
            -(Complex64::new(0.0, k * r) + 1.0) * Complex64::new(0.0, -k * r).exp() / (r * r);
        kprime * cos_src
    };
    gzp(r_plus) - gzp(r_minus)
}

/// G(R) = exp(−jkR) / R — free-space scalar Green's function.
fn green_k(r: f64, k: f64) -> Complex64 {
    Complex64::new(0.0, -k * r).exp() / r
}

// --- 3-vector helpers -------------------------------------------------------

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn mad3(base: [f64; 3], scale: f64, dir: [f64; 3]) -> [f64; 3] {
    [
        base[0] + scale * dir[0],
        base[1] + scale * dir[1],
        base[2] + scale * dir[2],
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Segment;

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
    const SEG_LEN: f64 = 5.354 / 11.0; // ~0.487 m
    const RADIUS: f64 = 0.001;
    const DIR_Z: [f64; 3] = [0.0, 0.0, 1.0];

    /// Self element: finite, positive real part (dominated by ln(L/a) ≈ 12).
    #[test]
    fn self_impedance_is_finite_and_positive_real() {
        let s = make_seg(1, 1, 0, [0.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_z_matrix(&[s], FREQ);
        let z11 = z.get(0, 0);
        assert!(z11.re.is_finite(), "A11.re not finite");
        assert!(z11.im.is_finite(), "A11.im not finite");
        assert!(z11.re > 0.0, "A11.re should be positive, got {}", z11.re);
    }

    /// Reciprocity: A_ij = A_ji.
    #[test]
    fn reciprocity_two_collinear_segments() {
        let s0 = make_seg(1, 1, 0, [0.0, 0.0, -SEG_LEN * 0.5], DIR_Z, SEG_LEN, RADIUS);
        let s1 = make_seg(1, 2, 1, [0.0, 0.0, SEG_LEN * 0.5], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_z_matrix(&[s0, s1], FREQ);
        let z01 = z.get(0, 1);
        let z10 = z.get(1, 0);
        assert!(
            (z01.re - z10.re).abs() < 1e-6,
            "A01.re={:.8} ≠ A10.re={:.8}",
            z01.re,
            z10.re
        );
        assert!(
            (z01.im - z10.im).abs() < 1e-6,
            "A01.im={:.8} ≠ A10.im={:.8}",
            z01.im,
            z10.im
        );
    }

    /// Identical segments far apart should have the same self element.
    #[test]
    fn identical_segments_have_equal_self_impedance() {
        let s0 = make_seg(1, 1, 0, [0.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let s1 = make_seg(2, 1, 1, [100.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_z_matrix(&[s0, s1], FREQ);
        assert!(
            (z.get(0, 0).re - z.get(1, 1).re).abs() < 1e-6,
            "A00={} vs A11={}",
            z.get(0, 0),
            z.get(1, 1)
        );
    }

    /// Mutual element should decay with increasing separation.
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
            "|A01|={near_mag:.6} should be > |A02|={far_mag:.6}"
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

    #[test]
    fn pocklington_elements_are_finite() {
        let s = make_seg(1, 1, 0, [0.0, 0.0, 0.0], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_pocklington_matrix(&[s], FREQ);
        let z11 = z.get(0, 0);
        assert!(z11.re.is_finite(), "Z11.re not finite");
        assert!(z11.im.is_finite(), "Z11.im not finite");
    }

    #[test]
    fn pocklington_reciprocity_two_collinear_segments() {
        let s0 = make_seg(1, 1, 0, [0.0, 0.0, -SEG_LEN * 0.5], DIR_Z, SEG_LEN, RADIUS);
        let s1 = make_seg(1, 2, 1, [0.0, 0.0, SEG_LEN * 0.5], DIR_Z, SEG_LEN, RADIUS);
        let z = assemble_pocklington_matrix(&[s0, s1], FREQ);
        let z01 = z.get(0, 1);
        let z10 = z.get(1, 0);
        assert!(
            (z01.re - z10.re).abs() < 1e-6,
            "Z01.re={}, Z10.re={}",
            z01.re,
            z10.re
        );
        assert!(
            (z01.im - z10.im).abs() < 1e-6,
            "Z01.im={}, Z10.im={}",
            z01.im,
            z10.im
        );
    }

    #[test]
    fn pec_ground_changes_hallen_matrix_element() {
        let s = make_seg(1, 1, 0, [0.0, 0.0, 1.0], DIR_Z, SEG_LEN, RADIUS);
        let z_free = assemble_z_matrix_with_ground(&[s.clone()], FREQ, &GroundModel::FreeSpace);
        let z_pec = assemble_z_matrix_with_ground(&[s], FREQ, &GroundModel::PerfectConductor);

        let delta = z_pec.get(0, 0) - z_free.get(0, 0);
        assert!(
            delta.norm() > 1e-8,
            "PEC image term should modify A11, delta={delta}"
        );
    }

    #[test]
    fn image_segment_reflects_across_z0_plane() {
        let s = make_seg(3, 2, 7, [1.0, -2.0, 4.0], [0.0, 0.6, 0.8], SEG_LEN, RADIUS);
        let img = image_segment(&s);

        assert_eq!(img.start[2], -s.start[2]);
        assert_eq!(img.end[2], -s.end[2]);
        assert_eq!(img.midpoint[2], -s.midpoint[2]);
        assert_eq!(img.direction[0], s.direction[0]);
        assert_eq!(img.direction[1], s.direction[1]);
        assert_eq!(img.direction[2], -s.direction[2]);
    }
}
