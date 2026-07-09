// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

// The impedance matrix is filled and mirrored with explicit index arithmetic
// (Z[m][n] = Z[n][m]); suppress the iterator-conversion lint.
#![allow(clippy::needless_range_loop)]

//! Mixed-potential EFIE (MPIE) solver — free-space core + junctions/loops.
//!
//! A second solver alongside the Hallén hybrid. It carries the vector potential
//! `A` and the scalar potential `Φ` **separately** (Hallén folds `Φ` into a
//! homogeneous `C·cos(ks)` term and so cannot represent it), which is what lets
//! the MPIE reach the three deferred Phase-9 frontiers — Sommerfeld ground
//! currents, degree-3 junctions, and closed loops. See `docs/mpie-solver-scope.md`.
//!
//! This module is a Rust port of the validated Python oracle
//! `studies/sommerfeld-ground/efie_mpie_ground.py` (its `zmat_free`) and its
//! junction extension `studies/.../mpie_junction.py`.
//!
//! ## Formulation (Galerkin)
//!
//! ```text
//! Z_mn = jωμ₀/4π ∬ f_m·f_n (ŝ·ŝ') G ds ds'
//!      + 1/(jωε₀4π) ∬ (∇·f_m)(∇·f_n) G ds ds'
//! ```
//!
//! with the reduced (thin-wire) kernel `G = e^{-jkR}/R`, `R = √(|r−r'|² + a²)`.
//! A delta-gap source at node `j` sets `V_j = 1`; solving `Z·I = V` gives the
//! nodal (peak) basis currents, and the feedpoint impedance is `Z_in = 1/I_j`.
//!
//! ## Basis (leg-based triangles)
//!
//! The geometry is a graph of nodes and segments. Every basis function is a
//! "dipole" spanning two segment **legs** that share a node `V`: the current
//! rises 0→1 toward `V` on the first leg (flowing *toward* `V`) and falls 1→0
//! away from `V` on the second (flowing *away*), so its charge (`∇·f`) is `+1/ℓ`
//! on the first and `−1/ℓ` on the second. Then, per node:
//!
//! - **degree 1 (free end):** no basis — `I = 0` by construction.
//! - **degree 2 (interior / bend):** one basis pairing its two segments.
//! - **degree N ≥ 3 (junction):** `N−1` bases, each pairing a reference arm with
//!   one other arm. Kirchhoff's current law at the node is then satisfied *by
//!   construction* (each dipole carries current in on one arm and out on
//!   another), with no explicit KCL row — the property the entire-domain Hallén
//!   junction prototype could not achieve.
//!
//! A closed loop is a cyclic chain: every node is degree 2, there is no free
//! end, and the same basis handles it with no endpoint condition.

use crate::geometry::Segment;
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

/// Error from the MPIE solver.
#[derive(Debug, Clone, PartialEq)]
pub enum MpieError {
    /// The underlying dense linear solve failed.
    Solve(SolveError),
    /// The requested feed node cannot host a delta-gap source (it is a free end,
    /// a junction, or out of range) — only degree-2 nodes are feedable.
    InvalidFeed { node: usize },
}

impl std::fmt::Display for MpieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MpieError::Solve(e) => write!(f, "MPIE solve failed: {e}"),
            MpieError::InvalidFeed { node } => {
                write!(f, "node {node} is not a valid degree-2 feed node")
            }
        }
    }
}

impl std::error::Error for MpieError {}

impl From<SolveError> for MpieError {
    fn from(e: SolveError) -> Self {
        MpieError::Solve(e)
    }
}

/// A wire graph for the MPIE solver: `nodes` in metres, `segments` as node-index
/// pairs, and a uniform `radius` for the reduced kernel.
#[derive(Debug, Clone, PartialEq)]
pub struct MpieGeometry {
    /// Node positions (metres).
    pub nodes: Vec<[f64; 3]>,
    /// Segments as `[node_start, node_end]` index pairs.
    pub segments: Vec<[usize; 2]>,
    /// Wire radius (metres) for the reduced thin-wire kernel.
    pub radius: f64,
}

/// A single open wire chain (convenience wrapper over [`MpieGeometry`]).
///
/// `nodes` are the `nseg + 1` node positions in metres; consecutive nodes bound
/// one segment.
#[derive(Debug, Clone, PartialEq)]
pub struct MpieWire {
    /// Node positions (metres), length `nseg + 1`.
    pub nodes: Vec<[f64; 3]>,
    /// Wire radius (metres) for the reduced thin-wire kernel.
    pub radius: f64,
}

impl MpieWire {
    /// Build the chain geometry (consecutive nodes → segments).
    pub fn geometry(&self) -> MpieGeometry {
        let segments = (0..self.nodes.len().saturating_sub(1))
            .map(|i| [i, i + 1])
            .collect();
        MpieGeometry {
            nodes: self.nodes.clone(),
            segments,
            radius: self.radius,
        }
    }
}

/// Result of an MPIE solve.
#[derive(Debug, Clone)]
pub struct MpieSolution {
    /// Feedpoint impedance `Z_in = V_feed / I_feed`.
    pub z_in: Complex64,
    /// Solved (peak) current at each basis function.
    pub basis_currents: Vec<Complex64>,
    /// The basis index that was fed.
    pub feed_basis: usize,
}

/// Per-segment geometry precomputed from an [`MpieGeometry`].
struct SegGeom {
    p0: [f64; 3],
    p1: [f64; 3],
    /// Unit tangent along the segment (`p0 → p1`).
    tangent: [f64; 3],
    /// Segment length (metres).
    len: f64,
}

/// One leg of a triangle basis: a segment, whether the shared node `V` is at the
/// segment's `p1` end, and whether the current flows *toward* `V`.
#[derive(Clone, Copy)]
struct Leg {
    seg: usize,
    v_is_p1: bool,
    toward: bool,
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

fn seg_geom(geom: &MpieGeometry) -> Vec<SegGeom> {
    geom.segments
        .iter()
        .map(|&[n0, n1]| {
            let p0 = geom.nodes[n0];
            let p1 = geom.nodes[n1];
            let d = sub(p1, p0);
            let len = norm(d);
            let tangent = if len > 0.0 {
                [d[0] / len, d[1] / len, d[2] / len]
            } else {
                [0.0, 0.0, 0.0]
            };
            SegGeom {
                p0,
                p1,
                tangent,
                len,
            }
        })
        .collect()
}

impl Leg {
    /// Unit tangent in the current-flow direction for this leg.
    fn flow_tangent(&self, segs: &[SegGeom]) -> [f64; 3] {
        let t = segs[self.seg].tangent;
        let sign = if self.v_is_p1 { 1.0 } else { -1.0 } * if self.toward { 1.0 } else { -1.0 };
        [sign * t[0], sign * t[1], sign * t[2]]
    }

    /// Charge density `∇·f` on this leg: `+1/ℓ` flowing toward `V`, `−1/ℓ` away.
    fn charge(&self, segs: &[SegGeom]) -> f64 {
        let s = if self.toward { 1.0 } else { -1.0 };
        s / segs[self.seg].len
    }

    /// Scalar ramp value at parameter `u ∈ [0, 1]` (`p0 → p1`): 1 at `V`, 0 far.
    fn f_scalar(&self, u: f64) -> f64 {
        if self.v_is_p1 {
            u
        } else {
            1.0 - u
        }
    }
}

/// Build the leg-based triangle bases from the wire graph, plus a per-node map to
/// the degree-2 basis that a delta-gap at that node would feed.
fn build_bases(n_nodes: usize, segments: &[[usize; 2]]) -> (Vec<Vec<Leg>>, Vec<Option<usize>>) {
    // Incidence: node → list of (segment, node-is-at-p1).
    let mut inc: Vec<Vec<(usize, bool)>> = vec![Vec::new(); n_nodes];
    for (si, &[n0, n1]) in segments.iter().enumerate() {
        inc[n0].push((si, false));
        inc[n1].push((si, true));
    }

    let mut bases: Vec<Vec<Leg>> = Vec::new();
    let mut node_feed: Vec<Option<usize>> = vec![None; n_nodes];
    for (node, legs_at) in inc.iter().enumerate() {
        let deg = legs_at.len();
        if deg < 2 {
            continue; // free end — no basis
        }
        // Reference arm = the first incident segment; pair it with each other arm.
        let (s0, v0) = legs_at[0];
        for k in 1..deg {
            let (sk, vk) = legs_at[k];
            let basis = vec![
                Leg {
                    seg: s0,
                    v_is_p1: v0,
                    toward: true,
                },
                Leg {
                    seg: sk,
                    v_is_p1: vk,
                    toward: false,
                },
            ];
            if deg == 2 {
                node_feed[node] = Some(bases.len());
            }
            bases.push(basis);
        }
    }
    (bases, node_feed)
}

/// Reduced-kernel free-space Green's function `e^{-jkR}/R`, `R = √(d² + a²)`.
fn g_reduced(dist: f64, radius: f64, k: f64) -> Complex64 {
    let r = (dist * dist + radius * radius).sqrt();
    Complex64::from_polar(1.0 / r, -k * r)
}

/// Position at parameter `u ∈ [0, 1]` along a segment (`p0 → p1`).
fn quad_point(seg: &SegGeom, u: f64) -> [f64; 3] {
    [
        seg.p0[0] + u * (seg.p1[0] - seg.p0[0]),
        seg.p0[1] + u * (seg.p1[1] - seg.p0[1]),
        seg.p0[2] + u * (seg.p1[2] - seg.p0[2]),
    ]
}

/// Assemble the dense free-space MPIE impedance matrix over the given bases.
/// Galerkin-symmetric by construction (upper triangle filled and mirrored).
fn assemble(
    segs: &[SegGeom],
    bases: &[Vec<Leg>],
    radius: f64,
    freq_hz: f64,
) -> Vec<Vec<Complex64>> {
    let nb = bases.len();
    let mut z = vec![vec![Complex64::new(0.0, 0.0); nb]; nb];
    if nb == 0 {
        return z;
    }

    let w = 2.0 * std::f64::consts::PI * freq_hz;
    let k = w / C0;
    let pre_a = Complex64::new(0.0, w * MU0 / (4.0 * std::f64::consts::PI));
    let pre_p =
        Complex64::new(1.0, 0.0) / Complex64::new(0.0, w * EPS0 * 4.0 * std::f64::consts::PI);

    for m in 0..nb {
        for n in m..nb {
            let mut za = Complex64::new(0.0, 0.0);
            let mut zp = Complex64::new(0.0, 0.0);
            for lm in &bases[m] {
                let sm = &segs[lm.seg];
                let tm = lm.flow_tangent(segs);
                let cm = lm.charge(segs);
                for ln in &bases[n] {
                    let sn = &segs[ln.seg];
                    let tt = dot(tm, ln.flow_tangent(segs));
                    let cc = cm * ln.charge(segs);
                    for gi in 0..6 {
                        let ua = 0.5 * (GL_NODES[gi] + 1.0);
                        let wa = 0.5 * GL_WEIGHTS[gi] * sm.len;
                        let ra = quad_point(sm, ua);
                        let fm = lm.f_scalar(ua);
                        for gj in 0..6 {
                            let ub = 0.5 * (GL_NODES[gj] + 1.0);
                            let wb = 0.5 * GL_WEIGHTS[gj] * sn.len;
                            let rb = quad_point(sn, ub);
                            let fn_ = ln.f_scalar(ub);
                            let g = g_reduced(norm(sub(ra, rb)), radius, k);
                            za += wa * wb * fm * fn_ * tt * g;
                            zp += wa * wb * cc * g;
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

/// Solve the free-space MPIE for a delta-gap voltage source at `feed_node`
/// (which must be a degree-2 node).
pub fn solve_mpie(
    geom: &MpieGeometry,
    freq_hz: f64,
    feed_node: usize,
) -> Result<MpieSolution, MpieError> {
    let segs = seg_geom(geom);
    let (bases, node_feed) = build_bases(geom.nodes.len(), &geom.segments);
    let feed_basis = node_feed
        .get(feed_node)
        .copied()
        .flatten()
        .ok_or(MpieError::InvalidFeed { node: feed_node })?;

    let mut z = assemble(&segs, &bases, geom.radius, freq_hz);
    let nb = z.len();
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

/// Assemble the dense free-space MPIE impedance matrix for a straight/bent chain.
pub fn assemble_free_space_z(wire: &MpieWire, freq_hz: f64) -> Vec<Vec<Complex64>> {
    let geom = wire.geometry();
    let segs = seg_geom(&geom);
    let (bases, _) = build_bases(geom.nodes.len(), &geom.segments);
    assemble(&segs, &bases, geom.radius, freq_hz)
}

/// Solve the free-space MPIE for a chain with a delta-gap at interior node
/// `feed_basis + 1` (i.e. the `feed_basis`-th interior node).
pub fn solve_mpie_free_space(
    wire: &MpieWire,
    freq_hz: f64,
    feed_basis: usize,
) -> Result<MpieSolution, MpieError> {
    // In a chain, interior node j hosts basis j−1, so feed_basis b ↔ node b+1.
    solve_mpie(&wire.geometry(), freq_hz, feed_basis + 1)
}

/// Recover the per-segment midpoint currents (signed along each segment's
/// `p0 → p1` tangent) from a solved basis-current vector, for the far-field sum.
///
/// A segment's current is a sum of triangle-basis ramps; at the midpoint each
/// leg contributes `I_basis · f(½) · (flow·tangent)`, and `f(½) = ½` for every
/// triangle leg — so the midpoint current is the average of the touching nodal
/// currents (the mean value of the piecewise-linear current over the segment,
/// which is exactly the radiation moment per unit length).
pub fn segment_currents(geom: &MpieGeometry, basis_currents: &[Complex64]) -> Vec<Complex64> {
    let (bases, _) = build_bases(geom.nodes.len(), &geom.segments);
    let mut i = vec![Complex64::new(0.0, 0.0); geom.segments.len()];
    for (bi, basis) in bases.iter().enumerate() {
        let c = basis_currents.get(bi).copied().unwrap_or_default();
        for leg in basis {
            let sign = if leg.v_is_p1 { 1.0 } else { -1.0 } * if leg.toward { 1.0 } else { -1.0 };
            i[leg.seg] += c * (0.5 * sign);
        }
    }
    i
}

/// Build far-field-ready [`Segment`]s from the wire graph. Pair with
/// [`segment_currents`] and feed both into `compute_radiation_pattern`.
pub fn segments_for_farfield(geom: &MpieGeometry) -> Vec<Segment> {
    seg_geom(geom)
        .iter()
        .enumerate()
        .map(|(i, s)| Segment {
            tag: 1,
            tag_index: (i + 1) as u32,
            global_index: i,
            start: s.p0,
            end: s.p1,
            midpoint: [
                0.5 * (s.p0[0] + s.p1[0]),
                0.5 * (s.p0[1] + s.p1[1]),
                0.5 * (s.p0[2] + s.p1[2]),
            ],
            direction: s.tangent,
            length: s.len,
            radius: geom.radius,
        })
        .collect()
}

/// Build a straight wire of `nseg` equal segments from `start` to `end`.
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

    /// Feeding a free end or a junction node is rejected.
    #[test]
    fn invalid_feed_node_is_rejected() {
        let geom = half_wave_dipole(10).geometry();
        // Node 0 is a free end.
        assert!(matches!(
            solve_mpie(&geom, FREQ, 0),
            Err(MpieError::InvalidFeed { node: 0 })
        ));
    }
}
