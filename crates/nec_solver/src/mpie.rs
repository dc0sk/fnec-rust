// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

// The impedance matrix is filled and mirrored with explicit index arithmetic
// (Z[m][n] = Z[n][m]); suppress the iterator-conversion lint.
#![allow(clippy::needless_range_loop)]

//! Mixed-potential EFIE (MPIE) solver — free-space core (Phase A).
//!
//! A second solver alongside the Hallén hybrid. It carries the vector potential
//! `A` and the scalar potential `Φ` **separately** (Hallén folds `Φ` into a
//! homogeneous `C·cos(ks)` term and so cannot represent it), which is what lets
//! the MPIE reach the three deferred Phase-9 frontiers — Sommerfeld ground
//! currents, degree-3 junctions, and closed loops. See `docs/mpie-solver-scope.md`.
//!
//! This module is the **free-space straight-wire core**: a piecewise-linear
//! (triangle) current basis, Galerkin testing, and a direct dense solve. It is a
//! Rust port of the validated Python oracle
//! `studies/sommerfeld-ground/efie_mpie_ground.py` (its `zmat_free`).
//!
//! ## Formulation (Galerkin)
//!
//! ```text
//! Z_mn = jωμ₀/4π ∬ f_m·f_n (ŝ·ŝ') G ds ds'
//!      + 1/(jωε₀4π) ∬ (∇·f_m)(∇·f_n) G ds ds'
//! ```
//!
//! with the reduced (thin-wire) kernel `G = e^{-jkR}/R`, `R = √(|r−r'|² + a²)`.
//! A delta-gap source at interior node `j` sets `V_j = 1`; solving `Z·I = V`
//! gives the nodal (peak) basis currents, and the feedpoint impedance is
//! `Z_in = 1/I_j`.
//!
//! ## Basis
//!
//! For a chain of `nseg` segments there are `nseg+1` nodes and `nseg−1` interior
//! (triangle) basis functions. Basis `b` is centred on the shared node between
//! segments `b` and `b+1`: it rises 0→1 across segment `b` and falls 1→0 across
//! segment `b+1`, so its charge (`∇·f`) is `+1/ℓ_b` on the rising segment and
//! `−1/ℓ_{b+1}` on the falling segment. Free ends carry no basis (`I = 0` by
//! construction), matching the Hallén solver's endpoint condition.

use crate::linear::{solve_square_in_place, SolveError};
use num_complex::Complex64;

const C0: f64 = 299_792_458.0;
const MU0: f64 = 4.0e-7 * std::f64::consts::PI;
const EPS0: f64 = 8.854_187_812_8e-12;

/// 6-point Gauss–Legendre nodes/weights on [-1, 1] (matches the Python oracle).
const GL_NODES: [f64; 6] = [
    -0.932_469_514_203_152,
    -0.661_209_386_466_265,
    -0.238_619_186_083_197,
    0.238_619_186_083_197,
    0.661_209_386_466_265,
    0.932_469_514_203_152,
];
const GL_WEIGHTS: [f64; 6] = [
    0.171_324_492_379_170,
    0.360_761_573_048_139,
    0.467_913_934_572_691,
    0.467_913_934_572_691,
    0.360_761_573_048_139,
    0.171_324_492_379_170,
];

/// A single open wire chain for the MPIE solver.
///
/// `nodes` are the `nseg + 1` node positions in metres; consecutive nodes bound
/// one segment. `radius` is the (uniform) wire radius used by the reduced kernel.
#[derive(Debug, Clone, PartialEq)]
pub struct MpieWire {
    /// Node positions (metres), length `nseg + 1`.
    pub nodes: Vec<[f64; 3]>,
    /// Wire radius (metres) for the reduced thin-wire kernel.
    pub radius: f64,
}

/// Result of a free-space MPIE solve.
#[derive(Debug, Clone)]
pub struct MpieSolution {
    /// Feedpoint impedance `Z_in = V_feed / I_feed`.
    pub z_in: Complex64,
    /// Nodal (peak) current at each interior triangle basis, length `nseg − 1`.
    pub basis_currents: Vec<Complex64>,
    /// The interior-node index that was fed.
    pub feed_basis: usize,
}

/// Per-segment geometry precomputed from an [`MpieWire`].
struct SegGeom {
    /// Segment start/end positions.
    p0: [f64; 3],
    p1: [f64; 3],
    /// Unit tangent along the segment.
    tangent: [f64; 3],
    /// Segment length (metres).
    len: f64,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

impl MpieWire {
    fn segments(&self) -> Vec<SegGeom> {
        self.nodes
            .windows(2)
            .map(|w| {
                let d = sub(w[1], w[0]);
                let len = norm(d);
                let tangent = if len > 0.0 {
                    [d[0] / len, d[1] / len, d[2] / len]
                } else {
                    [0.0, 0.0, 0.0]
                };
                SegGeom {
                    p0: w[0],
                    p1: w[1],
                    tangent,
                    len,
                }
            })
            .collect()
    }
}

/// Reduced-kernel free-space Green's function `e^{-jkR}/R`, `R = √(d² + a²)`.
fn g_reduced(dist: f64, radius: f64, k: f64) -> Complex64 {
    let r = (dist * dist + radius * radius).sqrt();
    Complex64::from_polar(1.0 / r, -k * r)
}

/// One support segment of a triangle basis: which segment, whether the scalar
/// ramp *rises* (0→1) or *falls* (1→0) across it, and the charge sign `∇·f`.
struct BasisLeg {
    seg: usize,
    rising: bool,
    /// Charge density `∇·f` on this segment: `+1/ℓ` rising, `−1/ℓ` falling.
    charge: f64,
}

/// The two support segments of interior basis `b` (centred on node `b+1`).
fn basis_legs(b: usize, segs: &[SegGeom]) -> [BasisLeg; 2] {
    [
        BasisLeg {
            seg: b,
            rising: true,
            charge: 1.0 / segs[b].len,
        },
        BasisLeg {
            seg: b + 1,
            rising: false,
            charge: -1.0 / segs[b + 1].len,
        },
    ]
}

/// Assemble the dense free-space MPIE impedance matrix (`NB × NB`, `NB =
/// nseg − 1`). Galerkin-symmetric by construction.
pub fn assemble_free_space_z(wire: &MpieWire, freq_hz: f64) -> Vec<Vec<Complex64>> {
    let segs = wire.segments();
    let nseg = segs.len();
    let nb = nseg.saturating_sub(1);
    let mut z = vec![vec![Complex64::new(0.0, 0.0); nb]; nb];
    if nb == 0 {
        return z;
    }

    let w = 2.0 * std::f64::consts::PI * freq_hz;
    let k = w / C0;
    let pre_a = Complex64::new(0.0, w * MU0 / (4.0 * std::f64::consts::PI));
    let pre_p =
        Complex64::new(1.0, 0.0) / Complex64::new(0.0, w * EPS0 * 4.0 * std::f64::consts::PI);

    // Assemble the upper triangle and mirror: the Galerkin operator is
    // symmetric, so this both halves the fill cost and makes symmetry EXACT
    // (independent quadrature of Z_mn and Z_nm would differ by roundoff).
    for m in 0..nb {
        for n in m..nb {
            let mut za = Complex64::new(0.0, 0.0);
            let mut zp = Complex64::new(0.0, 0.0);
            for lm in basis_legs(m, &segs) {
                let sm = &segs[lm.seg];
                for ln in basis_legs(n, &segs) {
                    let sn = &segs[ln.seg];
                    let tt = dot(sm.tangent, sn.tangent);
                    for gi in 0..6 {
                        let ua = 0.5 * (GL_NODES[gi] + 1.0);
                        let wa = 0.5 * GL_WEIGHTS[gi] * sm.len;
                        let ra = quad_point(sm, ua);
                        let fm = if lm.rising { ua } else { 1.0 - ua };
                        for gj in 0..6 {
                            let ub = 0.5 * (GL_NODES[gj] + 1.0);
                            let wb = 0.5 * GL_WEIGHTS[gj] * sn.len;
                            let rb = quad_point(sn, ub);
                            let fn_ = if ln.rising { ub } else { 1.0 - ub };
                            let g = g_reduced(norm(sub(ra, rb)), wire.radius, k);
                            za += wa * wb * fm * fn_ * tt * g;
                            zp += wa * wb * lm.charge * ln.charge * g;
                        }
                    }
                }
            }
            let entry = pre_a * za + pre_p * zp;
            z[m][n] = entry;
            z[n][m] = entry;
        }
    }
    z
}

/// Position at parameter `u ∈ [0, 1]` along a segment.
fn quad_point(seg: &SegGeom, u: f64) -> [f64; 3] {
    [
        seg.p0[0] + u * (seg.p1[0] - seg.p0[0]),
        seg.p0[1] + u * (seg.p1[1] - seg.p0[1]),
        seg.p0[2] + u * (seg.p1[2] - seg.p0[2]),
    ]
}

/// Solve the free-space MPIE for a delta-gap voltage source at interior node
/// `feed_basis` (`0 ≤ feed_basis < nseg − 1`).
pub fn solve_mpie_free_space(
    wire: &MpieWire,
    freq_hz: f64,
    feed_basis: usize,
) -> Result<MpieSolution, SolveError> {
    let mut z = assemble_free_space_z(wire, freq_hz);
    let nb = z.len();
    if nb == 0 || feed_basis >= nb {
        return Err(SolveError::DimensionMismatch {
            z_n: nb,
            v_len: feed_basis + 1,
        });
    }
    let mut v = vec![Complex64::new(0.0, 0.0); nb];
    v[feed_basis] = Complex64::new(1.0, 0.0);
    let currents = solve_square_in_place(&mut z, &mut v)?;
    let z_in = Complex64::new(1.0, 0.0) / currents[feed_basis];
    Ok(MpieSolution {
        z_in,
        basis_currents: currents,
        feed_basis,
    })
}

/// Build a straight wire of `nseg` equal segments from `start` to `end`.
///
/// Convenience constructor for the common straight-dipole case.
pub fn straight_wire(start: [f64; 3], end: [f64; 3], nseg: usize, radius: f64) -> MpieWire {
    let nodes = (0..=nseg)
        .map(|i| {
            let t = i as f64 / nseg as f64;
            [
                start[0] + t * (end[0] - start[0]),
                start[1] + t * (end[1] - start[1]),
                start[2] + t * (end[2] - start[2]),
            ]
        })
        .collect();
    MpieWire { nodes, radius }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREQ: f64 = 14.2e6;

    fn half_wave_dipole(nseg: usize) -> MpieWire {
        let lam = C0 / FREQ;
        let half = lam / 4.0;
        straight_wire([0.0, 0.0, -half], [0.0, 0.0, half], nseg, 0.001)
    }

    /// Gate A1: the Galerkin impedance matrix is symmetric (Z_mn = Z_nm).
    #[test]
    fn z_matrix_is_symmetric() {
        let z = assemble_free_space_z(&half_wave_dipole(20), FREQ);
        let nb = z.len();
        assert!(nb > 0);
        for m in 0..nb {
            for n in 0..nb {
                let d = (z[m][n] - z[n][m]).norm();
                assert!(d < 1e-12, "Z[{m}][{n}] vs Z[{n}][{m}] differ by {d:e}");
            }
        }
    }

    /// Gate A1 (sanity): a short dipole is capacitive (negative reactance).
    #[test]
    fn short_dipole_is_capacitive() {
        let lam = C0 / FREQ;
        let half = lam / 40.0; // 0.05λ total — electrically short
        let wire = straight_wire([0.0, 0.0, -half], [0.0, 0.0, half], 20, 0.001);
        let sol = solve_mpie_free_space(&wire, FREQ, wire.nodes.len() / 2 - 1).unwrap();
        assert!(
            sol.z_in.im < 0.0,
            "short dipole should be capacitive, got X={}",
            sol.z_in.im
        );
        assert!(
            sol.z_in.re < 5.0,
            "short dipole radiation resistance should be small, got R={}",
            sol.z_in.re
        );
    }

    /// Reproduce the validated Python oracle at N=40 (74.36 + j41.36).
    #[test]
    fn matches_python_oracle_n40() {
        let wire = half_wave_dipole(40);
        let sol = solve_mpie_free_space(&wire, FREQ, 40 / 2 - 1).unwrap();
        assert!(
            (sol.z_in.re - 74.36).abs() < 0.1,
            "R={} (oracle 74.36)",
            sol.z_in.re
        );
        assert!(
            (sol.z_in.im - 41.36).abs() < 0.1,
            "X={} (oracle 41.36)",
            sol.z_in.im
        );
    }
}
