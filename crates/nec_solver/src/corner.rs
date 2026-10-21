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
//! terms cancel identically, so parallel pairs are skipped. The integral's part
//! along the source is that bracket times cos²α, so it is taken in closed form
//! and only the perpendicular part is integrated numerically ([`f_n`], FND-281).

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
    // The integral's part along the source, `cos α · (ŝ·d)`, is exact:
    // `∫ G′(R)(ŝ·d)/R dl′ = G(R_start) − G(R_end)`, since `dR/dl′ = −(ŝ·d)/R`.
    // Taken out, what is left is `(1 − cos²α)` times the edge term and the
    // perpendicular part `t⊥ = tan − cos α · ŝ`, both of order sin α: the term
    // vanishes with the bend. Integrated whole, by Gauss–Legendre, the parallel
    // part's error near the source's end did not (143 against an exact 9e-6 two
    // radii from a 2e-4 rad bend), so a vanishing bend was not the straight join
    // (FND-281).
    let ca = dot(src.direction, tan);
    let edge = green(reff(src.start), k) - green(reff(src.end), k);
    let mut val = (1.0 - ca * ca) * edge;
    if ca.abs() > 1e-12 {
        let t_perp = [
            tan[0] - ca * src.direction[0],
            tan[1] - ca * src.direction[1],
            tan[2] - ca * src.direction[2],
        ];
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
            acc += w * green_prime(re, k) * (dot(t_perp, d) / re);
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

/// [`graded`] with the interval also split at `breaks`: each piece is graded
/// toward both of its ends. The integrand peaks wherever a source segment's
/// endpoint comes close to the section, which for a source on another wire (or
/// an image) can be anywhere along it, not only at the section's own nodes. A
/// crossing 5 cm apart left its peak inside one wide panel and broke the
/// crossing's antisymmetry (FND-162 stage 4 design review). The sinh grading is
/// logarithmic from `a` upward, so it resolves any peak width at a breakpoint.
pub(crate) fn graded_breaks(
    from: f64,
    to: f64,
    a: f64,
    breaks: &[f64],
    g: impl Fn(f64) -> Complex64,
) -> Complex64 {
    let (lo, hi, sgn) = if to >= from {
        (from, to, 1.0)
    } else {
        (to, from, -1.0)
    };
    let mut cuts: Vec<f64> = breaks
        .iter()
        .copied()
        .filter(|&b| b > lo + a && b < hi - a)
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|x, y| (*x - *y).abs() <= a);
    let mut edges = Vec::with_capacity(cuts.len() + 2);
    edges.push(lo);
    edges.extend(cuts);
    edges.push(hi);
    let total: Complex64 = edges.windows(2).map(|w| graded(w[0], w[1], a, &g)).sum();
    sgn * total
}

/// Where along a section a source segment's endpoints fall, as arc lengths `s`:
/// the projection of each endpoint onto the line through `pt` (at `s_ref`) with
/// direction `tan`, kept only when the endpoint lies within `near` of that line.
/// Those are the integrand's interior peaks ([`graded_breaks`]).
pub(crate) fn source_breaks(
    s_ref: f64,
    pt: [f64; 3],
    tan: [f64; 3],
    src: &Segment,
    near: f64,
) -> [Option<f64>; 2] {
    let at = |p: [f64; 3]| {
        let d = dist(p, pt);
        let along = dot(d, tan);
        let perp2 = (dot(d, d) - along * along).max(0.0);
        (perp2 < near * near).then_some(s_ref + along)
    };
    [at(src.start), at(src.end)]
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: f64 = 2.0 * std::f64::consts::PI * 28.4e6 / 299_792_458.0;
    const A: f64 = 0.001;
    const H: f64 = 0.264;

    /// A source segment on the z axis ending at the node at the origin.
    fn source() -> Segment {
        Segment {
            tag: 1,
            tag_index: 10,
            global_index: 9,
            start: [0.0, 0.0, -H],
            end: [0.0, 0.0, 0.0],
            midpoint: [0.0, 0.0, -H / 2.0],
            direction: [0.0, 0.0, 1.0],
            length: H,
            radius: A,
            part: crate::geometry::SegmentPart::Whole,
        }
    }

    /// The tangent of a section leaving the node at `theta` from the source.
    fn tangent(theta: f64) -> [f64; 3] {
        [theta.sin(), 0.0, theta.cos()]
    }

    /// The defining formula, integrated finely and graded toward both ends.
    fn f_n_reference(r: [f64; 3], tan: [f64; 3], src: &Segment) -> Complex64 {
        let reff = |p: [f64; 3]| {
            let d = dist(r, p);
            (dot(d, d) + A * A).sqrt()
        };
        let edge = green(reff(src.start), K) - green(reff(src.end), K);
        let half = src.length / 2.0;
        let along = graded(-half, half, A, |u| {
            let rp = [
                src.midpoint[0] + u * src.direction[0],
                src.midpoint[1] + u * src.direction[1],
                src.midpoint[2] + u * src.direction[2],
            ];
            let d = dist(r, rp);
            let re = (dot(d, d) + A * A).sqrt();
            green_prime(re, K) * (dot(tan, d) / re)
        });
        edge - dot(src.direction, tan) * along
    }

    /// FND-281: at a vanishing bend the term vanishes, even two radii past the
    /// node, where the source's end peaks on the scale of the radius. Integrated
    /// whole by Gauss–Legendre it read 143 there at 2e-4 rad, and 143.1 at 0.02.
    #[test]
    fn the_corner_term_vanishes_with_the_bend() {
        let src = source();
        for theta in [2e-4, 2e-2] {
            let tan = tangent(theta);
            for dist_out in [2.0 * A, 10.0 * A, H / 2.0] {
                let r = [dist_out * tan[0], 0.0, dist_out * tan[2]];
                let got = f_n(r, tan, &src, K);
                let exact = f_n_reference(r, tan, &src);
                assert!(
                    got.norm() < 1e-2 * theta * (1.0 / A),
                    "θ = {theta}, {dist_out} m out: f_n = {got} (exact {exact})"
                );
            }
        }
    }

    /// The parallel part is taken in closed form: a segment length or more from
    /// the source, where 8 points resolve the integrand, the split equals the
    /// formula.
    #[test]
    fn the_split_corner_term_is_the_formula() {
        let src = source();
        for theta in [0.3, 1.0, std::f64::consts::FRAC_PI_2, 2.5] {
            let tan = tangent(theta);
            for dist_out in [H, 2.0 * H] {
                let r = [dist_out * tan[0], 0.0, dist_out * tan[2]];
                let got = f_n(r, tan, &src, K);
                let want = f_n_reference(r, tan, &src);
                assert!(
                    (got - want).norm() < 1e-5 * want.norm() + 1e-9,
                    "θ = {theta}, {dist_out} m out: {got} against {want}"
                );
            }
        }
    }
}
