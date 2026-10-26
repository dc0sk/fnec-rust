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

use crate::geometry::{GroundModel, Segment};
use crate::linear::{solve_square_in_place, SolveError};
use crate::sommerfeld::{ground_image_factor, ReflectedRemainder};
use num_complex::Complex64;

use crate::constants::{C0, EPS0, MU0};

/// 6-point Gauss–Legendre nodes/weights on [-1, 1] (matches the Python oracle).
/// Observer-side panels for a near pair (see `assemble`).
const NEAR_OUTER_PANELS: usize = 8;

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
    /// The Sommerfeld MPIE ground requires the wire to lie **entirely above** the
    /// `z = 0` ground plane (the reflected kernel needs a positive height-sum);
    /// the geometry is buried or touches the plane.
    UnsupportedGround,
    /// The geometry is empty or contains a zero-length (self-coincident) segment,
    /// which has no defined tangent — the reduced kernel cannot be built.
    DegenerateGeometry,
    /// `radii` does not give every segment one positive radius: its length is not
    /// the segment count, or a radius is zero, negative or NaN.
    InvalidRadii {
        /// Number of segments in the geometry.
        segments: usize,
        /// Number of radii given.
        radii: usize,
    },
    /// A load vector that is neither empty nor one impedance per segment, or that
    /// holds a non-finite impedance.
    InvalidLoads {
        /// Number of segments in the geometry.
        segments: usize,
        /// Number of loads given.
        loads: usize,
    },
}

impl std::fmt::Display for MpieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MpieError::Solve(e) => write!(f, "MPIE solve failed: {e}"),
            MpieError::InvalidFeed { node } => {
                write!(f, "node {node} is not a valid degree-2 feed node")
            }
            MpieError::UnsupportedGround => write!(
                f,
                "MPIE Sommerfeld ground requires the wire to lie entirely above the ground plane"
            ),
            MpieError::DegenerateGeometry => write!(
                f,
                "MPIE geometry is empty or has a zero-length segment (no defined tangent)"
            ),
            MpieError::InvalidRadii { segments, radii } => write!(
                f,
                "MPIE geometry needs one positive radius per segment: {radii} radii for \
                 {segments} segments, or a radius that is not positive"
            ),
            MpieError::InvalidLoads { segments, loads } => write!(
                f,
                "MPIE loads must be empty or one finite impedance per segment: {loads} loads \
                 for {segments} segments, or a load that is not finite"
            ),
        }
    }
}

/// Reject an empty geometry, one with a zero-length (self-coincident) segment,
/// which would otherwise divide by a zero segment length in the reduced kernel, or
/// one without a positive radius for every segment.
fn check_geometry(geom: &MpieGeometry) -> Result<(), MpieError> {
    if geom.segments.is_empty() || geom.nodes.is_empty() {
        return Err(MpieError::DegenerateGeometry);
    }
    if geom.segments.iter().any(|&[a, b]| a == b) {
        return Err(MpieError::DegenerateGeometry);
    }
    if geom.radii.len() != geom.segments.len() || geom.radii.iter().any(|&a| a.is_nan() || a <= 0.0)
    {
        return Err(MpieError::InvalidRadii {
            segments: geom.segments.len(),
            radii: geom.radii.len(),
        });
    }
    Ok(())
}

impl std::error::Error for MpieError {}

impl From<SolveError> for MpieError {
    fn from(e: SolveError) -> Self {
        MpieError::Solve(e)
    }
}

/// A wire graph for the MPIE solver: `nodes` in metres, `segments` as node-index
/// pairs, and each segment's own radius for the reduced kernel.
#[derive(Debug, Clone, PartialEq)]
pub struct MpieGeometry {
    /// Node positions (metres).
    pub nodes: Vec<[f64; 3]>,
    /// Segments as `[node_start, node_end]` index pairs.
    pub segments: Vec<[usize; 2]>,
    /// Wire radius (metres) of each segment, in the order of `segments`.
    pub radii: Vec<f64>,
}

impl MpieGeometry {
    /// A wire graph whose every segment has the same `radius`.
    pub fn uniform(nodes: Vec<[f64; 3]>, segments: Vec<[usize; 2]>, radius: f64) -> Self {
        let radii = vec![radius; segments.len()];
        MpieGeometry {
            nodes,
            segments,
            radii,
        }
    }
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
        MpieGeometry::uniform(self.nodes.clone(), segments, self.radius)
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
    /// Wire radius (metres).
    radius: f64,
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

fn seg_geom(geom: &MpieGeometry) -> Result<Vec<SegGeom>, MpieError> {
    check_geometry(geom)?;
    Ok(geom
        .segments
        .iter()
        .zip(&geom.radii)
        .map(|(&[n0, n1], &radius)| {
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
                radius,
            }
        })
        .collect())
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

/// The static part of the reduced kernel, `1/R` with `R = √(|r − r(t)|² + a²)`,
/// integrated EXACTLY over a straight segment for an observer at `r`:
/// `(∫₀ᴸ dt/R, ∫₀ᴸ t·dt/R)`, `t` measured from `p0` (FND-157).
///
/// With `s₀` the observer's projection on the segment axis and `b² = ρ² + a²`:
/// `∫dt/R = asinh((L−s₀)/b) + asinh(s₀/b)` and
/// `∫t·dt/R = √((L−s₀)²+b²) − √(s₀²+b²) + s₀·∫dt/R`.
fn static_kernel_moments(r: [f64; 3], seg: &SegGeom, radius: f64) -> (f64, f64) {
    let d = sub(r, seg.p0);
    let s0 = dot(d, seg.tangent);
    let rho2 = (dot(d, d) - s0 * s0).max(0.0);
    let b = (rho2 + radius * radius).sqrt();
    let l = seg.len;
    let i0 = ((l - s0) / b).asinh() + (s0 / b).asinh();
    let i1 = ((l - s0) * (l - s0) + b * b).sqrt() - (s0 * s0 + b * b).sqrt() + s0 * i0;
    (i0, i1)
}

/// Whether two legs are close enough that the 6-point rule cannot resolve the
/// reduced kernel's `~a`-wide peak between them: the same segment, or segments
/// whose midpoints are within about one segment length of each other.
fn near_pair(a: &SegGeom, b: &SegGeom) -> bool {
    norm(sub(quad_point(a, 0.5), quad_point(b, 0.5))) < 1.01 * (a.len + b.len)
}

/// The radius the reduced kernel takes between two segments: their geometric
/// mean, so `Z_mn = Z_nm` holds term by term as the Galerkin form needs — the fill
/// computes one triangle and mirrors it. The source's radius alone (NEC-2's choice)
/// or the observer's breaks that symmetry; mirrored, either read a 5:1 radius step
/// as 50 − j445 Ω or 19 959 + j30 813 Ω. Among symmetric means the choice is not
/// visible in an impedance: max, min, RMS and geometric mean agree within 0.06 Ω
/// on that deck, and it reaches only pairs near enough to see the radius.
fn pair_radius(a: &SegGeom, b: &SegGeom) -> f64 {
    (a.radius * b.radius).sqrt()
}

/// Position at parameter `u ∈ [0, 1]` along a segment (`p0 → p1`).
fn quad_point(seg: &SegGeom, u: f64) -> [f64; 3] {
    [
        seg.p0[0] + u * (seg.p1[0] - seg.p0[0]),
        seg.p0[1] + u * (seg.p1[1] - seg.p0[1]),
        seg.p0[2] + u * (seg.p1[2] - seg.p0[2]),
    ]
}

/// The two integrals one pair of basis legs contributes to `Z_mn`, before the
/// tangent and charge factors: `(∬ f_m f_n G, ∬ G)` over leg `m` × leg `n`.
///
/// Near pairs: the reduced kernel's static part `1/R` is a spike of width ~a,
/// which the 6-point rule cannot resolve — it under-integrated the self and
/// adjacent terms, reading 6% low on a dipole, 35 Ω off on a Yagi and 260 Ω off
/// in X on a Y-junction (FND-157). It is subtracted and integrated exactly over
/// the source leg; the remainder `(e^{-jkR} − 1)/R` is smooth. The observer side
/// uses more points there, because the exact inner integral still varies on the
/// scale `a` near the source leg's ends.
fn leg_pair_integrals(
    sm: &SegGeom,
    lm: &Leg,
    sn: &SegGeom,
    ln: &Leg,
    k: f64,
) -> (Complex64, Complex64) {
    let radius = pair_radius(sm, sn);
    let (mut za, mut zp) = (Complex64::new(0.0, 0.0), Complex64::new(0.0, 0.0));
    let near = near_pair(sm, sn);
    let outer = if near { NEAR_OUTER_PANELS } else { 1 };
    for pa in 0..outer {
        for gi in 0..6 {
            let ua = (pa as f64 + 0.5 * (GL_NODES[gi] + 1.0)) / outer as f64;
            let wa = 0.5 * GL_WEIGHTS[gi] * sm.len / outer as f64;
            let ra = quad_point(sm, ua);
            let fm = lm.f_scalar(ua);
            let mut inner_a = Complex64::new(0.0, 0.0);
            let mut inner_p = Complex64::new(0.0, 0.0);
            for gj in 0..6 {
                let ub = 0.5 * (GL_NODES[gj] + 1.0);
                let wb = 0.5 * GL_WEIGHTS[gj] * sn.len;
                let rb = quad_point(sn, ub);
                let dist = norm(sub(ra, rb));
                let mut g = g_reduced(dist, radius, k);
                if near {
                    g -= 1.0 / (dist * dist + radius * radius).sqrt();
                }
                inner_a += wb * ln.f_scalar(ub) * g;
                inner_p += wb * g;
            }
            if near {
                let (i0, i1) = static_kernel_moments(ra, sn, radius);
                // ∫ f_n/R: the ramp is t/L toward p1, 1 − t/L toward p0.
                inner_a += if ln.v_is_p1 {
                    i1 / sn.len
                } else {
                    i0 - i1 / sn.len
                };
                inner_p += i0;
            }
            za += wa * fm * inner_a;
            zp += wa * inner_p;
        }
    }
    (za, zp)
}

/// The free-space reaction of basis `bm` on basis `bn` (legs into `segs`).
fn reaction(segs: &[SegGeom], bm: &[Leg], bn: &[Leg], freq_hz: f64) -> Complex64 {
    let w = 2.0 * std::f64::consts::PI * freq_hz;
    let k = w / C0;
    let pre_a = Complex64::new(0.0, w * MU0 / (4.0 * std::f64::consts::PI));
    let pre_p =
        Complex64::new(1.0, 0.0) / Complex64::new(0.0, w * EPS0 * 4.0 * std::f64::consts::PI);
    let mut za = Complex64::new(0.0, 0.0);
    let mut zp = Complex64::new(0.0, 0.0);
    for lm in bm {
        let sm = &segs[lm.seg];
        let tm = lm.flow_tangent(segs);
        let cm = lm.charge(segs);
        for ln in bn {
            let sn = &segs[ln.seg];
            let tt = dot(tm, ln.flow_tangent(segs));
            let cc = cm * ln.charge(segs);
            let (a, p) = leg_pair_integrals(sm, lm, sn, ln, k);
            za += tt * a;
            zp += cc * p;
        }
    }
    pre_a * za + pre_p * zp
}

/// Assemble the dense free-space MPIE impedance matrix over the given bases.
/// Galerkin-symmetric by construction (upper triangle filled and mirrored).
fn assemble(segs: &[SegGeom], bases: &[Vec<Leg>], freq_hz: f64) -> Vec<Vec<Complex64>> {
    let nb = bases.len();
    let mut z = vec![vec![Complex64::new(0.0, 0.0); nb]; nb];
    for m in 0..nb {
        for n in m..nb {
            let entry = reaction(segs, &bases[m], &bases[n], freq_hz);
            z[m][n] = entry;
            z[n][m] = entry;
        }
    }
    z
}

/// Add a perfect ground's reflected term by its exact image (FND-238): the image
/// of a current `J` at `r` is `−M·J` at `M·r`, `M = diag(1, 1, −1)`, so the image
/// of basis `f_n` is `−mirror(f_n)` — its charge `−q` — and the ground adds
/// `−Z_free(f_m, mirror(f_n))` to every entry. The mirrored legs keep their flags
/// on z-negated segments, so their flow tangent and charge come out mirrored, and
/// the free-space fill's near-pair handling (exact static moments where a segment
/// is within about a segment length of its image) carries over unchanged. It
/// replaced a reflected kernel evaluated once per segment pair at the midpoints,
/// which read a reactance ~200 Ω off, sign flipped, at a bend 0.1 m up with
/// 0.25 m segments.
///
/// `scale` is 1 over a perfect ground; over a finite one it is the image factor
/// `K` and [`add_ground_remainder`] adds the rest (FND-239).
fn add_pec_image(
    z: &mut [Vec<Complex64>],
    segs: &[SegGeom],
    bases: &[Vec<Leg>],
    freq_hz: f64,
    scale: Complex64,
) {
    let n = segs.len();
    let mut both: Vec<SegGeom> = Vec::with_capacity(2 * n);
    both.extend(segs.iter().map(|s| SegGeom {
        p0: s.p0,
        p1: s.p1,
        tangent: s.tangent,
        len: s.len,
        radius: s.radius,
    }));
    both.extend(segs.iter().map(|s| SegGeom {
        p0: [s.p0[0], s.p0[1], -s.p0[2]],
        p1: [s.p1[0], s.p1[1], -s.p1[2]],
        tangent: [s.tangent[0], s.tangent[1], -s.tangent[2]],
        len: s.len,
        radius: s.radius,
    }));
    let mirrored: Vec<Vec<Leg>> = bases
        .iter()
        .map(|b| {
            b.iter()
                .map(|l| Leg {
                    seg: l.seg + n,
                    ..*l
                })
                .collect()
        })
        .collect();
    let nb = bases.len();
    for m in 0..nb {
        for k in m..nb {
            let refl = -scale * reaction(&both, &bases[m], &mirrored[k], freq_hz);
            z[m][k] += refl;
            if k != m {
                z[k][m] += refl;
            }
        }
    }
}

/// Solve the free-space MPIE for a delta-gap voltage source at `feed_node`
/// (which must be a degree-2 node).
pub fn solve_mpie(
    geom: &MpieGeometry,
    freq_hz: f64,
    feed_node: usize,
) -> Result<MpieSolution, MpieError> {
    let segs = seg_geom(geom)?;
    let (bases, node_feed) = build_bases(geom.nodes.len(), &geom.segments);
    let feed_basis = node_feed
        .get(feed_node)
        .copied()
        .flatten()
        .ok_or(MpieError::InvalidFeed { node: feed_node })?;

    let mut z = assemble(&segs, &bases, freq_hz);
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

/// The MPIE's ground needs the wire wholly above the plane: `Some` where a node of
/// `geom` touches or crosses it. The one copy of that test — the session asks it
/// before placing feeds, so a base-fed monopole is refused for touching the ground,
/// not as a feed at a free wire end (the plane is not part of the wire graph).
pub fn ground_contact_error(
    geom: &MpieGeometry,
    freq_hz: f64,
    ground: &GroundModel,
) -> Option<MpieError> {
    if matches!(ground, GroundModel::FreeSpace) {
        return None;
    }
    let lam = C0 / freq_hz;
    geom.nodes
        .iter()
        .any(|n| n[2] <= 1e-6 * lam)
        .then_some(MpieError::UnsupportedGround)
}

/// Assemble the MPIE impedance matrix with a Sommerfeld half-space added to the
/// free-space fill (PH9-CHK-007 Phase D/E). The reflected term is added exactly
/// where the free-space `e^{-jkR}/R` sits, so the surface wave enters the currents
/// themselves (not just the feed Z).
///
/// Over a **perfect** ground every geometry takes the exact image
/// ([`add_pec_image`], FND-238). Over a **finite** ground, every geometry — straight
/// or bent, horizontal or not — takes `K ×` that image plus the remainder
/// ([`add_ground_remainder`], FND-239). A wire that reaches the `z = 0` plane
/// returns [`MpieError::UnsupportedGround`].
pub fn assemble_with_ground(
    geom: &MpieGeometry,
    freq_hz: f64,
    ground: &GroundModel,
) -> Result<Vec<Vec<Complex64>>, MpieError> {
    let segs = seg_geom(geom)?;
    let (bases, _) = build_bases(geom.nodes.len(), &geom.segments);
    let mut z = assemble(&segs, &bases, freq_hz);

    let (eps_r, sigma) = match ground {
        GroundModel::FreeSpace => return Ok(z),
        GroundModel::PerfectConductor => {
            if let Some(e) = ground_contact_error(geom, freq_hz, ground) {
                return Err(e);
            }
            add_pec_image(&mut z, &segs, &bases, freq_hz, Complex64::new(1.0, 0.0));
            return Ok(z);
        }
        GroundModel::SimpleFiniteGround { eps_r, sigma } => (*eps_r, *sigma),
    };
    if let Some(e) = ground_contact_error(geom, freq_hz, ground) {
        return Err(e);
    }
    let k = ground_image_factor(freq_hz, eps_r, sigma);
    add_pec_image(&mut z, &segs, &bases, freq_hz, k);
    add_ground_remainder(&mut z, &segs, &bases, freq_hz, eps_r, sigma);
    Ok(z)
}

/// Add the finite ground's reflected term beyond `K ×` the perfect image
/// (FND-239): the Galerkin reaction of [`ReflectedRemainder`]'s kernel, which is
/// `~1/R` at the image distance `R ≥ d = z + z'` rather than the full term's
/// `~1/R³`, and is tabulated once per solve so it can be integrated as finely as
/// the ground's nearness asks.
///
/// It replaced three paths that sampled the full reflected kernel too coarsely
/// near the ground — once per segment pair at the midpoints (bent), a 40 × 40
/// `(Δs, Σs)` grid (straight, not horizontal), a 240-point ρ-grid (horizontal) —
/// which read four radials 0.1 m over `GN 2` as 496 − j848 Ω (nec2c 38.47 +
/// j22.35) and a vertical dipole 0.05 m up 115 + j81 at 21 segments and 133 +
/// j112 at 63, moving away from nec2c's 107.70 + j56.75.
fn add_ground_remainder(
    z: &mut [Vec<Complex64>],
    segs: &[SegGeom],
    bases: &[Vec<Leg>],
    freq_hz: f64,
    eps_r: f64,
    sigma: f64,
) {
    let n = segs.len();
    let ends: Vec<[f64; 3]> = segs.iter().flat_map(|s| [s.p0, s.p1]).collect();
    let (z_lo, z_hi) = ends
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[2]), hi.max(p[2]))
        });
    let mut rho_max = 0.0_f64;
    for a in &ends {
        for b in &ends {
            rho_max = rho_max.max((a[0] - b[0]).hypot(a[1] - b[1]));
        }
    }
    let table = ReflectedRemainder::new(freq_hz, eps_r, sigma, 2.0 * z_lo, 2.0 * z_hi, rho_max);

    // moments[a·n + b][i][j] = ∬ g_i(u) g_j(v) E_rem(obs a(u), src b(v)), with
    // g_0 = 1 − u and g_1 = u; reciprocity gives the (b, a) entry as the transpose.
    let mut moments = vec![[[Complex64::new(0.0, 0.0); 2]; 2]; n * n];
    for a in 0..n {
        for b in a..n {
            let m = remainder_pair_moments(&table, &segs[a], &segs[b]);
            let mt = [[m[0][0], m[1][0]], [m[0][1], m[1][1]]];
            if a == b {
                moments[a * n + a] = [
                    [m[0][0], 0.5 * (m[0][1] + m[1][0])],
                    [0.5 * (m[0][1] + m[1][0]), m[1][1]],
                ];
            } else {
                moments[a * n + b] = m;
                moments[b * n + a] = mt;
            }
        }
    }
    let ramp = |l: &Leg| usize::from(l.v_is_p1);
    let nb = bases.len();
    for m in 0..nb {
        for k in m..nb {
            let mut acc = Complex64::new(0.0, 0.0);
            for lm in &bases[m] {
                let sig_m = dot(lm.flow_tangent(segs), segs[lm.seg].tangent);
                for ln in &bases[k] {
                    let sig = sig_m * dot(ln.flow_tangent(segs), segs[ln.seg].tangent);
                    acc += sig * moments[lm.seg * n + ln.seg][ramp(lm)][ramp(ln)];
                }
            }
            z[m][k] += acc;
            if k != m {
                z[k][m] += acc;
            }
        }
    }
}

/// The four ramp moments of the remainder between observation segment `sa` and
/// source segment `sb`. Both integrals are 6-point Gauss on panels graded
/// geometrically away from the point nearest the image — the outer around the
/// closest approach of `sa` to the mirrored `sb`, the inner around each outer
/// point's nearest point on the mirrored `sb` — starting at that distance, so a
/// `~1/R` kernel is resolved however close the ground is and however long the
/// segment.
fn remainder_pair_moments(
    table: &ReflectedRemainder,
    sa: &SegGeom,
    sb: &SegGeom,
) -> [[Complex64; 2]; 2] {
    let mirror = |p: [f64; 3]| [p[0], p[1], -p[2]];
    let (mb0, mb1) = (mirror(sb.p0), mirror(sb.p1));
    let (ua, _, r_min) = closest_between_segments(sa.p0, sa.p1, mb0, mb1);
    let mut m = [[Complex64::new(0.0, 0.0); 2]; 2];
    for (lo, hi) in graded_panels(ua * sa.len, sa.len, r_min) {
        for gi in 0..6 {
            let ta = lo + 0.5 * (GL_NODES[gi] + 1.0) * (hi - lo);
            let wa = 0.5 * GL_WEIGHTS[gi] * (hi - lo);
            let u = ta / sa.len;
            let ra = quad_point(sa, u);
            let (vb, r_near) = closest_on_segment(mirror(ra), sb.p0, sb.p1);
            let mut inner = [Complex64::new(0.0, 0.0); 2];
            for (lo_b, hi_b) in graded_panels(vb * sb.len, sb.len, r_near) {
                for gj in 0..6 {
                    let tb = lo_b + 0.5 * (GL_NODES[gj] + 1.0) * (hi_b - lo_b);
                    let wb = 0.5 * GL_WEIGHTS[gj] * (hi_b - lo_b);
                    let v = tb / sb.len;
                    let rb = quad_point(sb, v);
                    let e = table.kernel(
                        sb.tangent,
                        sa.tangent,
                        ra[0] - rb[0],
                        ra[1] - rb[1],
                        ra[2] + rb[2],
                    );
                    inner[0] += wb * (1.0 - v) * e;
                    inner[1] += wb * v * e;
                }
            }
            for j in 0..2 {
                m[0][j] += wa * (1.0 - u) * inner[j];
                m[1][j] += wa * u * inner[j];
            }
        }
    }
    m
}

/// Panels covering `[0, len]`, graded geometrically away from `at`: widths `h`,
/// `2h`, `4h`, … on each side, `h` the distance to the kernel's nearest
/// approach. One panel where that distance is at least the length.
fn graded_panels(at: f64, len: f64, h: f64) -> Vec<(f64, f64)> {
    let h = h.max(1e-9 * len);
    if h >= len {
        return vec![(0.0, len)];
    }
    let at = at.clamp(0.0, len);
    let mut panels = Vec::new();
    let (mut a, mut w) = (at, h);
    while a > 0.0 {
        let b = (a - w).max(0.0);
        panels.push((b, a));
        a = b;
        w *= 2.0;
    }
    let (mut a, mut w) = (at, h);
    while a < len {
        let b = (a + w).min(len);
        panels.push((a, b));
        a = b;
        w *= 2.0;
    }
    panels
}

/// The parameter `t ∈ [0, 1]` of the point on segment `p0 → p1` nearest `q`, and
/// the distance to it.
fn closest_on_segment(q: [f64; 3], p0: [f64; 3], p1: [f64; 3]) -> (f64, f64) {
    let d = sub(p1, p0);
    let t = (dot(sub(q, p0), d) / dot(d, d)).clamp(0.0, 1.0);
    let c = [p0[0] + t * d[0], p0[1] + t * d[1], p0[2] + t * d[2]];
    (t, norm(sub(q, c)))
}

/// Closest approach of segments `a0 → a1` and `b0 → b1`: the parameters on each
/// and the distance (Ericson, *Real-Time Collision Detection*, §5.1.9).
fn closest_between_segments(
    a0: [f64; 3],
    a1: [f64; 3],
    b0: [f64; 3],
    b1: [f64; 3],
) -> (f64, f64, f64) {
    let (d1, d2, r) = (sub(a1, a0), sub(b1, b0), sub(a0, b0));
    let (aa, ee, ff) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let c = dot(d1, r);
    let bb = dot(d1, d2);
    let denom = aa * ee - bb * bb;
    let mut s = if denom > 1e-12 * aa * ee {
        ((bb * ff - c * ee) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (bb * s + ff) / ee;
    if t < 0.0 {
        t = 0.0;
        s = (-c / aa).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((bb - c) / aa).clamp(0.0, 1.0);
    }
    let pa = [a0[0] + s * d1[0], a0[1] + s * d1[1], a0[2] + s * d1[2]];
    let pb = [b0[0] + t * d2[0], b0[1] + t * d2[1], b0[2] + t * d2[2]];
    (s, t, norm(sub(pa, pb)))
}

/// Solve the MPIE with a Sommerfeld half-space (any straight wire above ground)
/// for a delta-gap at degree-2 `feed_node`.
pub fn solve_mpie_ground(
    geom: &MpieGeometry,
    freq_hz: f64,
    feed_node: usize,
    ground: &GroundModel,
) -> Result<MpieSolution, MpieError> {
    check_geometry(geom)?;
    let (_, node_feed) = build_bases(geom.nodes.len(), &geom.segments);
    let feed_basis = node_feed
        .get(feed_node)
        .copied()
        .flatten()
        .ok_or(MpieError::InvalidFeed { node: feed_node })?;

    let mut z = assemble_with_ground(geom, freq_hz, ground)?;
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

/// Solve with several delta-gap sources at once: `feeds` pairs an interior node
/// with the voltage applied there, along the basis's reference direction. One
/// matrix, one right-hand side carrying every source — MoM is linear, so this is
/// the superposition of the single-source solves. `ground` `None` (or free
/// space) is the free-space assembly, as in [`solve_mpie`]; anything else is
/// [`solve_mpie_ground`]'s.
///
/// Returns the basis currents. With a single 1 V feed it is exactly the
/// `basis_currents` of [`solve_mpie`] / [`solve_mpie_ground`].
pub fn solve_mpie_sources(
    geom: &MpieGeometry,
    freq_hz: f64,
    ground: &GroundModel,
    feeds: &[(usize, Complex64)],
    loads: &[Complex64],
) -> Result<Vec<Complex64>, MpieError> {
    check_geometry(geom)?;
    if !(loads.is_empty() || loads.len() == geom.segments.len())
        || loads
            .iter()
            .any(|z| !(z.re.is_finite() && z.im.is_finite()))
    {
        return Err(MpieError::InvalidLoads {
            segments: geom.segments.len(),
            loads: loads.len(),
        });
    }
    let (bases, node_feed) = build_bases(geom.nodes.len(), &geom.segments);
    let has_ground = !matches!(ground, GroundModel::FreeSpace);
    let mut z = if has_ground {
        assemble_with_ground(geom, freq_hz, ground)?
    } else {
        assemble(&seg_geom(geom)?, &bases, freq_hz)
    };
    // Once, after the ground: the image and the remainder fold into the same
    // basis matrix, with no image segments a load could be stamped on again.
    stamp_loads(&mut z, &readout_rows(&bases, geom.segments.len()), loads);
    let mut v = vec![Complex64::new(0.0, 0.0); z.len()];
    for &(node, volts) in feeds {
        let basis = node_feed
            .get(node)
            .copied()
            .flatten()
            .ok_or(MpieError::InvalidFeed { node })?;
        v[basis] += volts;
    }
    Ok(solve_square_in_place(&mut z, &mut v)?)
}

/// Assemble the dense free-space MPIE impedance matrix for a straight/bent chain.
pub fn assemble_free_space_z(
    wire: &MpieWire,
    freq_hz: f64,
) -> Result<Vec<Vec<Complex64>>, MpieError> {
    let geom = wire.geometry();
    let segs = seg_geom(&geom)?;
    let (bases, _) = build_bases(geom.nodes.len(), &geom.segments);
    Ok(assemble(&segs, &bases, freq_hz))
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

/// Build an [`MpieGeometry`] from fnec [`Segment`]s, deduplicating shared
/// endpoints (within [`crate::hallen_session::JUNCTION_TOL_M`]) into graph nodes. `geom.segments` is in the **same
/// order** as `segs`, so a current vector from [`segment_currents`] aligns 1:1
/// with the input segments — the bridge from a parsed NEC deck to the MPIE.
pub fn geometry_from_segments(segs: &[Segment]) -> MpieGeometry {
    const TOL: f64 = crate::hallen_session::JUNCTION_TOL_M;
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let find_or_add = |p: [f64; 3], nodes: &mut Vec<[f64; 3]>| -> usize {
        for (i, q) in nodes.iter().enumerate() {
            if norm(sub(p, *q)) < TOL {
                return i;
            }
        }
        nodes.push(p);
        nodes.len() - 1
    };
    let mut segments = Vec::with_capacity(segs.len());
    for s in segs {
        let a = find_or_add(s.start, &mut nodes);
        let b = find_or_add(s.end, &mut nodes);
        segments.push([a, b]);
    }
    let radii = segs.iter().map(|s| s.radius).collect();
    MpieGeometry {
        nodes,
        segments,
        radii,
    }
}

/// The interior (degree-2) node to feed for a delta-gap on segment `seg_idx`:
/// its start node if feedable, else its end node, else `None` (a junction feed,
/// which the nodal MPIE does not support — matching the Hallén guard).
pub fn feed_node_for_segment(geom: &MpieGeometry, seg_idx: usize) -> Option<usize> {
    let (_, node_feed) = build_bases(geom.nodes.len(), &geom.segments);
    let [a, b] = geom.segments.get(seg_idx).copied()?;
    if node_feed.get(a).copied().flatten().is_some() {
        Some(a)
    } else if node_feed.get(b).copied().flatten().is_some() {
        Some(b)
    } else {
        None
    }
}

/// The sign relating the fed basis's **reference current direction** at
/// `feed_node` to segment `seg_idx`'s own `p0 → p1` tangent (`+1.0` when they
/// agree, `-1.0` when they oppose); `None` if `feed_node` is not feedable or
/// `seg_idx` is not one of its two arms.
///
/// The nodal triangle basis picks its reference direction from the incidence
/// order of the two arms, which for a *start-to-start* junction (both wires
/// written outward from the shared node, e.g. an apex-fed inverted-V entered as
/// two `GW` cards both starting at the apex) is opposite to the driven
/// segment's own tangent. A caller that re-references the solve to a source
/// applied along that segment — as `EX` does — must scale the solved currents by
/// this sign, or the whole current vector (and hence `V/I`) comes out negated.
pub fn feed_reference_sign(geom: &MpieGeometry, feed_node: usize, seg_idx: usize) -> Option<f64> {
    let (bases, node_feed) = build_bases(geom.nodes.len(), &geom.segments);
    let feed_basis = node_feed.get(feed_node).copied().flatten()?;
    bases[feed_basis]
        .iter()
        .find(|l| l.seg == seg_idx)
        .map(|l| {
            let v = if l.v_is_p1 { 1.0 } else { -1.0 };
            let t = if l.toward { 1.0 } else { -1.0 };
            v * t
        })
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
    readout_rows(&bases, geom.segments.len())
        .iter()
        .map(|row| {
            row.iter()
                .map(|&(b, t)| basis_currents.get(b).copied().unwrap_or_default() * t)
                .sum()
        })
        .collect()
}

/// Each segment's midpoint current as a row over the bases, `I_mid = Σ t_b·I_b`:
/// `t_b = ½·s_b` for every basis with a leg on the segment, `s_b` that leg's
/// flow along the segment's `p0 → p1`. One basis at a free end; two along a
/// wire; beside a junction the reference arm has a leg in every junction basis.
///
/// The one copy of that projection: the readout ([`segment_currents`]) and the
/// load stamp ([`stamp_loads`]) both take it, so a load is the transpose of the
/// readout, as the `EX` feed is (FND-224).
fn readout_rows(bases: &[Vec<Leg>], n_segments: usize) -> Vec<Vec<(usize, f64)>> {
    let mut rows = vec![Vec::new(); n_segments];
    for (bi, basis) in bases.iter().enumerate() {
        for leg in basis {
            let sign = if leg.v_is_p1 { 1.0 } else { -1.0 } * if leg.toward { 1.0 } else { -1.0 };
            rows[leg.seg].push((bi, 0.5 * sign));
        }
    }
    rows
}

/// Stamp a series load `z_i` on each segment: `Z_mn += z_i·t_m·t_n`, `t` the
/// segment's readout row. The load's drop `z_i·I_mid` is a source on the segment,
/// projected on the bases as the `EX` gap is, so a load on the fed segment raises
/// the input impedance by exactly `z_i` — NEC's own model, the centre current
/// times the load, for lumped and distributed loads alike.
fn stamp_loads(z: &mut [Vec<Complex64>], rows: &[Vec<(usize, f64)>], loads: &[Complex64]) {
    for (row, &zl) in rows.iter().zip(loads) {
        if zl == Complex64::new(0.0, 0.0) {
            continue;
        }
        for &(m, tm) in row {
            for &(n, tn) in row {
                z[m][n] += zl * (tm * tn);
            }
        }
    }
}

/// Build far-field-ready [`Segment`]s from the wire graph. Pair with
/// [`segment_currents`] and feed both into `compute_radiation_pattern`.
pub fn segments_for_farfield(geom: &MpieGeometry) -> Result<Vec<Segment>, MpieError> {
    Ok(seg_geom(geom)?
        .iter()
        .enumerate()
        .map(|(i, s)| Segment {
            tag: 1,
            tag_index: (i + 1) as u32,
            abs_index: (i + 1) as u32,
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
            radius: s.radius,
            part: crate::geometry::SegmentPart::Whole,
        })
        .collect())
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

    /// FND-157: every leg-pair integral equals brute force, each term separately.
    ///
    /// Compared term by term because a whole-element comparison cannot see an
    /// error in the vector-potential term: the charge term dominates a short
    /// segment by ~(kΔ)⁻², so swapping the rising and falling ramp moments of the
    /// extraction passed all three nec2c gates AND an element-wise check at 2e-3.
    /// Brute force here is a midpoint rule on 1024 panels per leg, finer than the
    /// 1 mm kernel peak on a 0.2 m leg. The bound is 2e-4, not 1e-4: with a pair
    /// radius near 1.5 mm the adjacent pair reads 1.2–1.4e-4 whether the three
    /// segments share it or not, and 4096 panels move that only to 1.16e-4 — the
    /// quadrature's own floor, under 0.01 % of an impedance, not the brute force's.
    #[test]
    fn leg_pair_integrals_match_brute_force_term_by_term() {
        // Three segments: self, adjacent and distant pairs, at a cost a unit test can
        // afford — each its own radius, so the pairs across a radius step are checked.
        let mut geom = straight_wire([0.0, 0.0, 0.0], [0.0, 0.0, 0.6], 3, 0.001).geometry();
        geom.radii = vec![0.001, 0.003, 0.002];
        let segs = seg_geom(&geom).expect("valid geometry");
        let k = 2.0 * std::f64::consts::PI * FREQ / C0;
        const P: usize = 1024;
        let legs: Vec<Leg> = (0..segs.len())
            .flat_map(|seg| {
                [true, false].map(|v_is_p1| Leg {
                    seg,
                    v_is_p1,
                    toward: true,
                })
            })
            .collect();
        for lm in &legs {
            for ln in &legs {
                let (sm, sn) = (&segs[lm.seg], &segs[ln.seg]);
                let (a, pot) = leg_pair_integrals(sm, lm, sn, ln, k);
                let (mut ba, mut bp) = (Complex64::new(0.0, 0.0), Complex64::new(0.0, 0.0));
                let w = sm.len * sn.len / (P * P) as f64;
                for i in 0..P {
                    let ua = (i as f64 + 0.5) / P as f64;
                    let ra = quad_point(sm, ua);
                    for j in 0..P {
                        let ub = (j as f64 + 0.5) / P as f64;
                        let g =
                            g_reduced(norm(sub(ra, quad_point(sn, ub))), pair_radius(sm, sn), k)
                                * w;
                        ba += lm.f_scalar(ua) * ln.f_scalar(ub) * g;
                        bp += g;
                    }
                }
                let (ra_, rp) = ((a - ba).norm() / ba.norm(), (pot - bp).norm() / bp.norm());
                assert!(
                    ra_ < 2e-4 && rp < 2e-4,
                    "legs {}/{} → {}/{}: A-term {ra_:.2e}, charge term {rp:.2e}",
                    lm.seg,
                    lm.v_is_p1,
                    ln.seg,
                    ln.v_is_p1
                );
            }
        }
    }

    /// Degenerate geometry (empty, or a zero-length self-coincident segment) is
    /// rejected with a clear error instead of panicking / producing NaNs.
    #[test]
    fn degenerate_geometry_is_rejected() {
        let empty = MpieGeometry {
            nodes: vec![],
            segments: vec![],
            radii: vec![],
        };
        assert!(matches!(
            solve_mpie(&empty, FREQ, 0),
            Err(MpieError::DegenerateGeometry)
        ));

        let self_loop = MpieGeometry {
            nodes: vec![[0.0, 0.0, 1.0]],
            segments: vec![[0, 0]],
            radii: vec![0.001],
        };
        assert!(matches!(
            solve_mpie(&self_loop, FREQ, 0),
            Err(MpieError::DegenerateGeometry)
        ));
        assert!(matches!(
            solve_mpie_ground(&self_loop, FREQ, 0, &GroundModel::FreeSpace),
            Err(MpieError::DegenerateGeometry)
        ));
    }

    /// A geometry without one positive radius per segment is refused by name on
    /// every entry, including `assemble_with_ground`, which has no feed to check —
    /// before the radius was checked where the segments are built, a short `radii`
    /// built fewer segments than the bases index and panicked out of bounds.
    #[test]
    fn a_geometry_without_one_positive_radius_per_segment_is_refused() {
        let wire = straight_wire([0.0, 0.0, 1.0], [0.0, 0.0, 6.0], 4, 0.001).geometry();
        let short = MpieGeometry {
            radii: vec![0.001],
            ..wire.clone()
        };
        let zero = MpieGeometry {
            radii: vec![0.001, 0.0, 0.001, 0.001],
            ..wire.clone()
        };
        let nan = MpieGeometry {
            radii: vec![0.001, f64::NAN, 0.001, 0.001],
            ..wire.clone()
        };
        for (geom, radii) in [(&short, 1), (&zero, 4), (&nan, 4)] {
            let refused = Err(MpieError::InvalidRadii { segments: 4, radii });
            assert_eq!(solve_mpie(geom, FREQ, 2).map(|_| ()), refused);
            assert_eq!(
                solve_mpie_sources(
                    geom,
                    FREQ,
                    &GroundModel::FreeSpace,
                    &[(2, Complex64::new(1.0, 0.0))],
                    &[]
                )
                .map(|_| ()),
                refused
            );
            for ground in [GroundModel::PerfectConductor, GroundModel::FreeSpace] {
                assert_eq!(
                    assemble_with_ground(geom, FREQ, &ground).map(|_| ()),
                    refused
                );
            }
            assert_eq!(segments_for_farfield(geom).map(|_| ()), refused);
        }
        assert!(solve_mpie(&wire, FREQ, 2).is_ok());
        assert!(MpieError::InvalidRadii {
            segments: 4,
            radii: 1
        }
        .to_string()
        .contains("one positive radius per segment"));
    }

    /// A load vector must be empty or give every segment one finite impedance: a
    /// short one would stamp the wrong segments, a NaN one poison the solve.
    #[test]
    fn a_load_vector_without_one_finite_load_per_segment_is_refused() {
        let wire = straight_wire([0.0, 0.0, 1.0], [0.0, 0.0, 6.0], 4, 0.001).geometry();
        let one = Complex64::new(1.0, 0.0);
        let solve = |loads: &[Complex64]| {
            solve_mpie_sources(&wire, FREQ, &GroundModel::FreeSpace, &[(2, one)], loads).map(|_| ())
        };
        let z = Complex64::new(50.0, 0.0);
        assert_eq!(
            solve(&[z]),
            Err(MpieError::InvalidLoads {
                segments: 4,
                loads: 1
            })
        );
        assert_eq!(
            solve(&[z, Complex64::new(f64::NAN, 0.0), z, z]),
            Err(MpieError::InvalidLoads {
                segments: 4,
                loads: 4
            })
        );
        assert_eq!(solve(&[]), Ok(()));
        assert_eq!(solve(&[z; 4]), Ok(()));
        assert!(MpieError::InvalidLoads {
            segments: 4,
            loads: 1
        }
        .to_string()
        .contains("one finite impedance per segment"));
    }

    /// With mixed radii the reaction is symmetric term by term, `reaction(m, n) ==
    /// reaction(n, m)`, which the fill assumes when it mirrors one triangle. A test
    /// on the assembled matrix could not see this — the fill makes it symmetric
    /// whatever the pair radius — so it asks `reaction` both ways round: the
    /// source's radius alone, or the observer's, fails here and nowhere else.
    #[test]
    fn the_reaction_is_symmetric_across_a_radius_step() {
        let mut geom = straight_wire([0.0, 0.0, 0.0], [0.0, 0.0, 1.2], 6, 0.001).geometry();
        geom.radii = vec![0.005, 0.005, 0.005, 0.001, 0.001, 0.001];
        let segs = seg_geom(&geom).expect("valid geometry");
        let (bases, _) = build_bases(geom.nodes.len(), &geom.segments);
        for m in 0..bases.len() {
            for n in 0..bases.len() {
                let mn = reaction(&segs, &bases[m], &bases[n], FREQ);
                let nm = reaction(&segs, &bases[n], &bases[m], FREQ);
                assert!(
                    (mn - nm).norm() <= 1e-9 * mn.norm(),
                    "bases {m}, {n}: {mn} vs {nm}"
                );
            }
        }
    }

    /// Gate A1: the Galerkin impedance matrix is symmetric (Z_mn = Z_nm).
    #[test]
    fn z_matrix_is_symmetric() {
        let z = assemble_free_space_z(&half_wave_dipole(20), FREQ).expect("valid geometry");
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

    /// A half-wave dipole at N=40 lands on the converged physical value.
    ///
    /// This test reproduced a "validated Python oracle" (74.36 + j41.36) until
    /// FND-157, and the oracle shared this file's defect: both integrated the
    /// reduced kernel's `~a`-wide self-term peak with a 6-point rule, and read 6%
    /// low. Two codes with the same quadrature agreeing proved nothing about the
    /// quadrature. The reference now is nec2c's converged dipole, this one
    /// (half-length c/4f = 5.2780 m, 1 mm) centre-fed at 21 segments meshed 27×
    /// finer: 79.474 + j45.517 (1 / 3 / 9 / 27 ×: 78.852 + j44.708, 79.210 +
    /// j45.136, 79.385 + j45.373, 79.474 + j45.517; ~79.6 + j46.6 was the 5.282 m
    /// corpus dipole's, FND-259);
    /// a node feed has no exact nec2c twin, so this is a band, and the exact pins
    /// are the CLI decks in `mpie_solver_cli.rs`, which do have one.
    #[test]
    fn a_half_wave_dipole_lands_on_the_converged_value() {
        let wire = half_wave_dipole(40);
        let sol = solve_mpie_free_space(&wire, FREQ, 40 / 2 - 1).unwrap();
        assert!(
            (78.5..80.0).contains(&sol.z_in.re),
            "R={} (nec2c converges to 79.47; the old quadrature gave 74.36)",
            sol.z_in.re
        );
        assert!(
            (43.0..47.5).contains(&sol.z_in.im),
            "X={} (nec2c converges to 45.52)",
            sol.z_in.im
        );
    }

    /// `geometry_from_segments` round-trips a dipole: solving the segment-derived
    /// graph equals the chain solve (the deck → MPIE bridge preserves the physics).
    #[test]
    fn geometry_from_segments_round_trips_a_dipole() {
        let wire = half_wave_dipole(20);
        let segs = segments_for_farfield(&wire.geometry()).expect("valid geometry");
        let rebuilt = geometry_from_segments(&segs);
        assert_eq!(rebuilt.segments.len(), 20);
        let feed = feed_node_for_segment(&rebuilt, 10).unwrap();
        let z_rebuilt = solve_mpie(&rebuilt, FREQ, feed).unwrap().z_in;
        let z_chain = solve_mpie_free_space(&wire, FREQ, 9).unwrap().z_in;
        assert!(
            (z_rebuilt - z_chain).norm() < 1e-6,
            "rebuilt {z_rebuilt} vs chain {z_chain}"
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

/// FND-239: the remainder's pair integrals resolve the ground however close it is.
#[cfg(test)]
mod remainder_fill_tests {
    use super::*;

    const FREQ: f64 = 14.2e6;

    fn seg(p0: [f64; 3], p1: [f64; 3]) -> SegGeom {
        let d = sub(p1, p0);
        let len = norm(d);
        SegGeom {
            p0,
            p1,
            tangent: [d[0] / len, d[1] / len, d[2] / len],
            len,
            radius: 0.001,
        }
    }

    /// The same four moments on `panels` uniform panels per segment.
    fn uniform_moments(
        table: &ReflectedRemainder,
        sa: &SegGeom,
        sb: &SegGeom,
        panels: usize,
    ) -> [[Complex64; 2]; 2] {
        let points = |s: &SegGeom| -> Vec<(f64, f64)> {
            (0..panels)
                .flat_map(|p| {
                    (0..6).map(move |g| {
                        let h = 1.0 / panels as f64;
                        (
                            (p as f64 + 0.5 * (GL_NODES[g] + 1.0)) * h,
                            0.5 * GL_WEIGHTS[g] * h * s.len,
                        )
                    })
                })
                .collect()
        };
        let (pa, pb) = (points(sa), points(sb));
        let mut m = [[Complex64::new(0.0, 0.0); 2]; 2];
        for &(u, wa) in &pa {
            let ra = quad_point(sa, u);
            for &(v, wb) in &pb {
                let rb = quad_point(sb, v);
                let e = table.kernel(
                    sb.tangent,
                    sa.tangent,
                    ra[0] - rb[0],
                    ra[1] - rb[1],
                    ra[2] + rb[2],
                );
                let (gu, gv) = ([1.0 - u, u], [1.0 - v, v]);
                for i in 0..2 {
                    for j in 0..2 {
                        m[i][j] += wa * wb * gu[i] * gv[j] * e;
                    }
                }
            }
        }
        m
    }

    fn worst(a: &[[Complex64; 2]; 2], b: &[[Complex64; 2]; 2]) -> (f64, f64) {
        let mut err = 0.0_f64;
        let mut mag = 0.0_f64;
        for i in 0..2 {
            for j in 0..2 {
                err = err.max((a[i][j] - b[i][j]).norm());
                mag = mag.max(b[i][j].norm());
            }
        }
        (err, mag)
    }

    /// 0.25 m segments 2.5 mm above the ground — a hundredth of their length — the
    /// graded panels match 150 uniform panels (each a sixth of the height sum) on a
    /// horizontal pair, a vertical pair, a bend and two horizontal segments side by
    /// side; and swapping observer and source gives the transpose (reciprocity),
    /// which the fill relies on and an asymmetric panel rule would break.
    #[test]
    fn graded_panels_resolve_a_segment_a_hundredth_of_its_length_up() {
        let h = 0.0025;
        let table = ReflectedRemainder::new(FREQ, 13.0, 0.005, 2.0 * h, 2.0 * (h + 0.25), 0.36);
        let horizontal = seg([0.0, 0.0, h], [0.25, 0.0, h]);
        let next = seg([0.25, 0.0, h], [0.5, 0.0, h]);
        let vertical = seg([0.0, 0.0, h], [0.0, 0.0, h + 0.25]);
        let beside = seg([0.0, 0.1, h], [0.25, 0.1, h]);
        for (name, a, b) in [
            ("horizontal self", &horizontal, &horizontal),
            ("horizontal neighbours", &horizontal, &next),
            ("vertical self", &vertical, &vertical),
            ("bend", &vertical, &horizontal),
            ("side by side", &horizontal, &beside),
        ] {
            let graded = remainder_pair_moments(&table, a, b);
            let (err, mag) = worst(&graded, &uniform_moments(&table, a, b, 150));
            assert!(
                err < 1e-4 * mag,
                "{name}: graded off by {err:.3e} of {mag:.3e}"
            );
            let swapped = remainder_pair_moments(&table, b, a);
            let t = [
                [swapped[0][0], swapped[1][0]],
                [swapped[0][1], swapped[1][1]],
            ];
            let (err, mag) = worst(&t, &graded);
            assert!(
                err < 1e-4 * mag,
                "{name}: (b, a) is not the transpose: {err:.3e} of {mag:.3e}"
            );
        }
    }

    /// Over a ground conducting like a perfect one, the finite-ground fill is the
    /// perfect ground's: `K → 1` and the remainder vanishes. An L 0.05 m up.
    #[test]
    fn a_nearly_perfect_finite_ground_is_the_perfect_one() {
        let mut nodes = vec![];
        for i in 0..=8 {
            nodes.push([0.0, 0.0, 0.05 + 5.28 * (8 - i) as f64 / 8.0]);
        }
        for i in 1..=8 {
            nodes.push([5.3 * i as f64 / 8.0, 0.0, 0.05]);
        }
        let geom = MpieGeometry::uniform(nodes, (0..16).map(|i| [i, i + 1]).collect(), 0.001);
        let pec = assemble_with_ground(&geom, FREQ, &GroundModel::PerfectConductor).unwrap();
        let near = assemble_with_ground(
            &geom,
            FREQ,
            &GroundModel::SimpleFiniteGround {
                eps_r: 1.0,
                sigma: 1e7,
            },
        )
        .unwrap();
        let scale = pec.iter().flatten().map(|v| v.norm()).fold(0.0, f64::max);
        for (rp, rn) in pec.iter().zip(&near) {
            for (p, n) in rp.iter().zip(rn) {
                assert!((p - n).norm() < 1e-3 * scale, "{p} vs {n} (scale {scale})");
            }
        }
    }
}
