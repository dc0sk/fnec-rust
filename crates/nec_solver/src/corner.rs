// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The corner term of the Hallén bend condition (FND-162, stage 1b).
//!
//! Hallén's rows hold the TANGENTIAL vector potential `A_s` of each segment, and
//! integrate `(∂²_s + k²) A_s = −jωμε V δ` along a straight run. At a bend that
//! equation is incomplete: `∇·A = ∂A_s/∂s + F`, where `F` is the transverse
//! divergence contributed by every source segment NOT parallel to the observation
//! section (Mei 1965's curved-wire term). On each section the missing part of
//! `A_s` is `Q(s) = −∫_{s₀}^{s} F(s″) cos k(s − s″) ds″`, referenced at a node
//! `s₀` of that section, and it enters the matrix: row `m` gains
//! `sign_m · ∫_{s₀}^{s_m} f_n(s″) cos k(s_m − s″) ds″` in column `n`.
//!
//! Formulation by the design review (Fable, 2026-09-27); prototyped in numpy and
//! gated before this was written: with the term, a 90° L goes from 45 % off
//! nec2c to 5.2 % (21 segments per arm) and 3.1 % (41), converging; an
//! inverted-V fed away from its apex from 38 % to 9.2 % and 5.0 %.
//!
//! Per unit current on source segment `n` (code units, ×4π/μ₀):
//!
//! ```text
//! f_n(r) = [G(R(r, start_n)) − G(R(r, end_n))]
//!          − cos α · ∫_n G′(R) · (ŝ·(r − r′)) / R dl′
//! G(R) = e^{−jkR}/R,  G′(R) = −(1 + jkR) e^{−jkR}/R²,  R = √(|r − r′|² + a²)
//! ```
//!
//! The first bracket is the pulse-edge charge term; for a parallel source the two
//! terms cancel identically, so parallel pairs are skipped.

use num_complex::Complex64;

use crate::geometry::Segment;

/// 8-point Gauss–Legendre on [−1, 1].
const GL8: [(f64, f64); 8] = [
    (-0.960_289_856_497_536_3, 0.101_228_536_290_376_26),
    (-0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
    (-0.525_532_409_916_329, 0.313_706_645_877_887_3),
    (-0.183_434_642_495_649_8, 0.362_683_783_378_362),
    (0.183_434_642_495_649_8, 0.362_683_783_378_362),
    (0.525_532_409_916_329, 0.313_706_645_877_887_3),
    (0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
    (0.960_289_856_497_536_3, 0.101_228_536_290_376_26),
];

fn green(r: f64, k: f64) -> Complex64 {
    Complex64::new(0.0, -k * r).exp() / r
}

fn green_prime(r: f64, k: f64) -> Complex64 {
    -(Complex64::new(1.0, k * r)) * Complex64::new(0.0, -k * r).exp() / (r * r)
}

fn dist(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Whether a source segment is parallel (or antiparallel) to a section tangent,
/// in which case its corner contribution is identically zero.
pub(crate) fn parallel(tan: [f64; 3], src: &Segment) -> bool {
    dot(tan, src.direction).abs() > 1.0 - 1e-9
}

/// `f_n` at observation point `r` on a section with tangent `tan`.
pub(crate) fn f_n(r: [f64; 3], tan: [f64; 3], src: &Segment, k: f64) -> Complex64 {
    let a = src.radius;
    let reff = |p: [f64; 3]| {
        let d = dist(r, p);
        (dot(d, d) + a * a).sqrt()
    };
    let mut val = green(reff(src.start), k) - green(reff(src.end), k);
    let ca = dot(src.direction, tan);
    if ca.abs() > 1e-12 {
        let half = src.length / 2.0;
        let mut acc = Complex64::new(0.0, 0.0);
        for &(u, w) in &GL8 {
            let rp = [
                src.midpoint[0] + u * half * src.direction[0],
                src.midpoint[1] + u * half * src.direction[1],
                src.midpoint[2] + u * half * src.direction[2],
            ];
            let d = dist(r, rp);
            let re = (dot(d, d) + a * a).sqrt();
            acc += w * green_prime(re, k) * (dot(tan, d) / re);
        }
        val -= ca * half * acc;
    }
    val
}

/// `∫_{from}^{to} g(s) ds` along a section, with the integrand's width-`a` peaks
/// at either end resolved: each half of the interval is mapped by
/// `u = a·sinh t` toward its end, then integrated with composite 8-point
/// Gauss–Legendre. `to < from` gives the signed (negated) integral.
pub(crate) fn graded(from: f64, to: f64, a: f64, g: impl Fn(f64) -> Complex64) -> Complex64 {
    const PANELS: usize = 12;
    let (lo, hi, sgn) = if to >= from {
        (from, to, 1.0)
    } else {
        (to, from, -1.0)
    };
    let len = hi - lo;
    if len <= 0.0 {
        return Complex64::new(0.0, 0.0);
    }
    let half_len = len / 2.0;
    let tmax = (half_len / a).asinh();
    let mut total = Complex64::new(0.0, 0.0);
    // Two halves, each graded toward its own end: u measured inward from that end.
    for (end, dir) in [(lo, 1.0), (hi, -1.0)] {
        let dt = tmax / PANELS as f64;
        for p in 0..PANELS {
            let (t0, t1) = (p as f64 * dt, (p + 1) as f64 * dt);
            let (mid, hw) = ((t0 + t1) / 2.0, (t1 - t0) / 2.0);
            for &(x, w) in &GL8 {
                let t = mid + hw * x;
                let u = a * t.sinh();
                total += w * hw * a * t.cosh() * g(end + dir * u);
            }
        }
    }
    sgn * total
}
