// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Geometry builder: converts parsed `GwCard` entries into a flat list of
//! wire [`Segment`]s with precomputed spatial properties.
//!
//! Each NEC `GW` card defines a straight wire subdivided into N equal segments.
//! The builder expands every card into N segments and computes, for each:
//!
//! - `start` / `end` — endpoints in metres
//! - `midpoint`      — centre point (used as the match point in MoM)
//! - `direction`     — unit vector from start to end
//! - `length`        — segment length in metres
//! - `radius`        — wire radius in metres
//! - `tag`           — wire tag number from the GW card
//! - `tag_index`     — 1-based segment index within the tag
//! - `global_index`  — 0-based index in the flat segment list

use nec_model::card::{Card, GeCard, GmCard, GrCard, GwCard};
use nec_model::deck::NecDeck;

/// Ground model extracted from a GN card (or absence thereof).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GroundModel {
    /// No ground — free-space simulation (default when no GN card is present).
    #[default]
    FreeSpace,
    /// Perfect electric conductor (PEC) ground at z = 0.
    ///
    /// Implemented via the image method: for each real segment a mirror image
    /// at z → −z is added to the Green's function kernel.
    PerfectConductor,
    /// Simple finite-ground model (GN type 0) using medium parameters.
    ///
    /// This Phase-1 model is a pragmatic Fresnel-coefficient approximation
    /// used to enable baseline non-PEC ground behavior in impedance solves.
    SimpleFiniteGround {
        /// Relative dielectric constant (EPSE).
        eps_r: f64,
        /// Conductivity in S/m (SIG).
        sigma: f64,
    },
    /// GN card was present, but the requested ground type is not implemented.
    ///
    /// The current Phase-1 behavior is to fall back to free-space while
    /// emitting a runtime warning in the CLI.
    Deferred {
        gn_type: i32,
        /// Relative dielectric constant (EPSE) from the GN card, if present.
        eps_r: Option<f64>,
        /// Conductivity in S/m (SIG) from the GN card, if present.
        sigma: Option<f64>,
    },
}

/// Extract the ground model from a parsed deck.
///
/// Resolution order:
/// 1. If a `GN` card is present it takes priority.
///    - `GN -1` → [`GroundModel::FreeSpace`] (null ground; equivalent to no GN card).
///    - `GN 1`  → [`GroundModel::PerfectConductor`].
///    - `GN 0`  → [`GroundModel::SimpleFiniteGround`] using parsed EPSE/SIG
///      (or conservative defaults when omitted).
///    - `GN 2`  → [`GroundModel::SimpleFiniteGround`] (Phase-2 scoped
///      runtime path; currently reuses the simple finite-ground approximation).
///    - Other GN types → [`GroundModel::Deferred`].
/// 2. If no `GN` card is present but a `GE` card has `ground_reflection_flag > 0`
///    (the standard NEC flag for "enable image-method PEC ground at z = 0"),
///    infer [`GroundModel::PerfectConductor`].
/// 3. Otherwise returns [`GroundModel::FreeSpace`].
pub fn ground_model_from_deck(deck: &NecDeck) -> GroundModel {
    // GN card takes priority.
    for card in &deck.cards {
        if let Card::Gn(gn) = card {
            return match gn.ground_type {
                // -1 = null ground: NEC spec says this is equivalent to no
                // GN card — treat as free-space without emitting a warning.
                -1 => GroundModel::FreeSpace,
                1 => GroundModel::PerfectConductor,
                0 | 2 => GroundModel::SimpleFiniteGround {
                    // Keep GN0 usable even when medium params are omitted.
                    // Values are the same "average ground" defaults commonly
                    // used in NEC workflows. GN2 shares this scoped scalar-Γ
                    // approximation by default; the exact Sommerfeld treatment is
                    // opt-in via `--ground-solver sommerfeld` / `--solver mpie`.
                    eps_r: gn.eps_r.unwrap_or(13.0),
                    sigma: gn.sigma.unwrap_or(0.005),
                },
                other => GroundModel::Deferred {
                    gn_type: other,
                    eps_r: gn.eps_r,
                    sigma: gn.sigma,
                },
            };
        }
    }
    // No GN card: check GE flag.  GE I1=1 is the conventional NEC shorthand
    // for "perfect electric conductor ground, apply image method", and GE I1=-1
    // is a half-space variant that is not yet supported.
    for card in &deck.cards {
        if let Card::Ge(GeCard {
            ground_reflection_flag,
        }) = card
        {
            if *ground_reflection_flag == 1 {
                return GroundModel::PerfectConductor;
            }
        }
    }
    GroundModel::FreeSpace
}

/// A single NEC wire segment.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Wire tag number (from GW card).
    pub tag: u32,
    /// 1-based segment index within the tag.
    pub tag_index: u32,
    /// 0-based index in the global segment list.
    pub global_index: usize,
    /// Segment start point in metres.
    pub start: [f64; 3],
    /// Segment end point in metres.
    pub end: [f64; 3],
    /// Centre (match) point in metres.
    pub midpoint: [f64; 3],
    /// Unit direction vector (start → end).
    pub direction: [f64; 3],
    /// Segment length in metres.
    pub length: f64,
    /// Wire radius in metres.
    pub radius: f64,
}

/// Error returned by [`build_geometry`].
#[derive(Debug, Clone, PartialEq)]
pub enum GeometryError {
    /// A GW card specified zero segments.
    ZeroSegments { tag: u32 },
    /// A GW card has zero-length wire (start == end).
    ZeroLengthWire { tag: u32 },
    /// No GW cards were found in the deck.
    NoWires,
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeometryError::ZeroSegments { tag } => {
                write!(f, "GW tag {tag}: segment count must be ≥ 1")
            }
            GeometryError::ZeroLengthWire { tag } => {
                write!(f, "GW tag {tag}: wire has zero length (start == end)")
            }
            GeometryError::NoWires => write!(f, "deck contains no GW (wire) cards"),
        }
    }
}

impl std::error::Error for GeometryError {}

/// Compute per-wire endpoint indices from a flat segment list.
///
/// Wires are identified by contiguous runs of segments sharing the same tag.
/// Returns a `Vec` of `(first, last)` inclusive global-index pairs, one per
/// wire, in deck order.  An empty segment list returns an empty `Vec`.
pub fn wire_endpoints_from_segs(segs: &[Segment]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut current_tag = u32::MAX;
    let mut first = 0usize;
    for (i, seg) in segs.iter().enumerate() {
        if seg.tag != current_tag {
            if current_tag != u32::MAX {
                out.push((first, i - 1));
            }
            current_tag = seg.tag;
            first = i;
        }
    }
    if current_tag != u32::MAX {
        out.push((first, segs.len() - 1));
    }
    out
}

/// Merge tag-based wire chains that are **collinear continuations** of one another
/// into single logical wires, for the Hallén homogeneous solution (PH9-CHK-002).
///
/// Two consecutive wire blocks are merged when the first block's last segment
/// connects **end-to-start** to the next block's first segment, the two segments
/// share direction (collinear) and radius, and they are contiguous in the segment
/// array. The result is the natural single-wire grouping for a straight conductor
/// that was split across several `GW` cards — so `cos(k·s)` and the homogeneous
/// constant are shared across the whole conductor rather than reset at each split.
///
/// For any geometry without such collinear splits (single wires, parallel arrays,
/// bends, T/Y junctions) this returns exactly [`wire_endpoints_from_segs`] — the
/// merge is a strict no-op there.
pub fn merge_collinear_wire_endpoints(segs: &[Segment]) -> Vec<(usize, usize)> {
    let base = wire_endpoints_from_segs(segs);
    if base.len() < 2 {
        return base;
    }
    const POS_TOL: f64 = 1e-6; // metres
    const DIR_TOL: f64 = 1e-6;
    let mut merged: Vec<(usize, usize)> = Vec::new();
    let mut cur = base[0];
    for &next in &base[1..] {
        let a = &segs[cur.1]; // last segment of the current (possibly merged) block
        let b = &segs[next.0]; // first segment of the candidate block
        let contiguous = cur.1 + 1 == next.0;
        let connects = dist2(a.end, b.start) < POS_TOL * POS_TOL;
        let collinear = dir_aligned(a.direction, b.direction, DIR_TOL);
        let same_radius = (a.radius - b.radius).abs() < POS_TOL;
        if contiguous && connects && collinear && same_radius {
            cur = (cur.0, next.1); // extend the merged chain
        } else {
            merged.push(cur);
            cur = next;
        }
    }
    merged.push(cur);
    merged
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

fn dir_aligned(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    // Same direction (not merely parallel): the cross product ~0 and dot > 0.
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    dist2(cross, [0.0, 0.0, 0.0]) < tol * tol && dot > 0.0
}

/// A detected physical junction between two wire endpoints.
///
/// Encodes the current-continuity constraint that must be applied in
/// the Hallén augmented system instead of an I = 0 endpoint constraint.
///
/// The constraint is: `I[seg_a] + sign * I[seg_b] = 0`
///
/// - `sign = -1.0` for an end-to-start connection (current flows continuously
///   from wire A into wire B): `I[A_last] - I[B_first] = 0`.
/// - `sign = +1.0` for an end-to-end or start-to-start connection (current
///   reverses at the junction): `I[A_last] + I[B_last] = 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct WireJunction {
    /// Segment index of the endpoint on wire A (first or last segment of A).
    pub seg_a: usize,
    /// Segment index of the endpoint on wire B (first or last segment of B).
    pub seg_b: usize,
    /// Sign for the continuity constraint. See struct-level docs.
    pub sign: f64,
}

/// Detect physical wire junctions from segment geometry and wire endpoints.
///
/// Two wire endpoints are considered a junction when the physical point
/// represented by each endpoint is within `tol` metres of the other.
/// For segment index `first`, the physical point is `segs[first].start`.
/// For segment index `last`, the physical point is `segs[last].end`.
///
/// Returns one `WireJunction` per detected junction pair. Each (seg_a, seg_b)
/// pair appears at most once. Self-junctions (wire A connecting to itself
/// for a degenerate single-segment wire) are not emitted.
pub fn detect_wire_junctions(
    segs: &[Segment],
    wire_endpoints: &[(usize, usize)],
    tol: f64,
) -> Vec<WireJunction> {
    // Build the list of (segment_index, point, is_start) for every wire endpoint.
    // is_start = true  → this is the first segment of a wire; its physical node
    //                     is segs[seg].start.
    // is_start = false → this is the last segment of a wire; its physical node
    //                     is segs[seg].end.
    struct EndpointNode {
        seg: usize,
        point: [f64; 3],
        is_start: bool,
    }

    let mut nodes: Vec<EndpointNode> = Vec::with_capacity(wire_endpoints.len() * 2);
    for &(first, last) in wire_endpoints {
        nodes.push(EndpointNode {
            seg: first,
            point: segs[first].start,
            is_start: true,
        });
        nodes.push(EndpointNode {
            seg: last,
            point: segs[last].end,
            is_start: false,
        });
    }

    let pt_dist = |a: &[f64; 3], b: &[f64; 3]| {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    };

    let mut junctions: Vec<WireJunction> = Vec::new();

    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            // Skip two endpoints that belong to the same segment (degenerate wire).
            if nodes[i].seg == nodes[j].seg {
                continue;
            }
            if pt_dist(&nodes[i].point, &nodes[j].point) > tol {
                continue;
            }
            // Determine sign from the endpoint types.
            //   end-to-start or start-to-end → sign = -1.0 (continuous flow)
            //   end-to-end or start-to-start → sign = +1.0 (anti-parallel)
            let sign = if nodes[i].is_start == nodes[j].is_start {
                1.0_f64
            } else {
                -1.0_f64
            };
            junctions.push(WireJunction {
                seg_a: nodes[i].seg,
                seg_b: nodes[j].seg,
                sign,
            });
        }
    }

    junctions
}

/// A logical conductor: a maximal chain of physical wires connected end-to-end
/// through **degree-2** junctions (bends, start-to-start / end-to-end splits, or
/// collinear continuations), forming a single continuous current path.
///
/// This is the general-junction generalization of [`merge_collinear_wire_endpoints`]
/// (PH9-CHK-002). Where the collinear merge only joins straight end-to-start `GW`
/// chains, a `ConductorPath` walks *any* degree-2 chain — including reversed
/// (start-to-start) and bent connections — and carries the per-segment traversal
/// **sign** and signed **arc-length** needed to make the Hallén homogeneous basis
/// (`cos(k·s)`) continuous across the junction.
///
/// The current on segment `m` in its own (NEC) direction is `sign[m] · I_path(s)`,
/// where `I_path` is the continuous path current and `s = s_mid[m]` is the signed
/// arc-length of the segment midpoint measured from the path's arc-length centre.
#[derive(Debug, Clone, PartialEq)]
pub struct ConductorPath {
    /// Global segment indices along the path, in traversal order.
    pub segs: Vec<usize>,
    /// Per-listed-segment sign: `+1.0` if the segment's own direction aligns with
    /// path traversal, `-1.0` if it is traversed in reverse. Parallel to `segs`.
    pub signs: Vec<f64>,
    /// Signed arc-length at each segment midpoint, measured from the path's
    /// arc-length centre (`s = 0` at the middle of the total path length).
    /// Parallel to `segs`.
    pub s_mid: Vec<f64>,
    /// The two terminal (free-end) segment indices where the `I = 0` boundary
    /// condition applies (the outermost segments of the open chain).
    pub free_ends: (usize, usize),
}

impl ConductorPath {
    /// Whether this path is a plain single wire or a collinear end-to-start chain
    /// — i.e. all segments traversed forward (`sign = +1`) in contiguous ascending
    /// index order. Such a path is handled identically by the pre-existing
    /// collinear-merge Hallén code, so the general path solver is only needed when
    /// at least one path is **not** trivial (a reversed or bent junction).
    pub fn is_trivial(&self) -> bool {
        if self.signs.iter().any(|&s| s != 1.0) {
            return false;
        }
        self.segs.windows(2).all(|w| w[1] == w[0] + 1)
    }
}

/// Decompose the geometry into [`ConductorPath`]s for the general-junction Hallén
/// solve (PH9-CHK-002).
///
/// Returns `Some(paths)` **only** when every junction node in the deck is a simple
/// **degree-2** connection and every conductor is an **open chain** (exactly two
/// free ends). This is the supported class for the continuous-basis fix: single
/// wires, collinear splits, bent dipoles / inverted-V, and end-to-end or
/// start-to-start splits.
///
/// Returns `None` when the topology is out of scope for this slice — any node
/// where **three or more** wire ends meet (T/Y junctions), or a **closed loop**
/// (a conductor with no free end). Those cases fall back to the guarded per-wire
/// path and still require the general junction-basis work.
///
/// A single straight wire maps to one path with all `signs = +1` and `s_mid`
/// identical to the straight-axis coordinate used by [`build_hallen_rhs`], so the
/// decomposition is a numeric no-op for the non-junction case.
///
/// [`build_hallen_rhs`]: crate::build_hallen_rhs
pub fn build_conductor_paths(segs: &[Segment]) -> Option<Vec<ConductorPath>> {
    let wires = wire_endpoints_from_segs(segs);
    let w = wires.len();
    if w == 0 {
        return Some(Vec::new());
    }

    const TOL: f64 = 1e-6; // metres

    // Endpoint identity: 2 per wire. `end = 0` is the wire start (segs[first].start),
    // `end = 1` is the wire end (segs[last].end).
    let ep_point = |wi: usize, end: usize| -> [f64; 3] {
        let (first, last) = wires[wi];
        if end == 0 {
            segs[first].start
        } else {
            segs[last].end
        }
    };

    // Cluster the 2·W endpoints into coincident nodes. `node_of[wi*2 + end]` is the
    // node id; `node_members[node]` lists the (wi, end) endpoints at that node.
    let mut node_of = vec![usize::MAX; 2 * w];
    let mut node_members: Vec<Vec<(usize, usize)>> = Vec::new();
    for wi in 0..w {
        for end in 0..2 {
            let id = wi * 2 + end;
            if node_of[id] != usize::MAX {
                continue;
            }
            let p = ep_point(wi, end);
            let node = node_members.len();
            let mut members = vec![(wi, end)];
            node_of[id] = node;
            // Find all other unassigned endpoints coincident with p.
            for wj in 0..w {
                for endj in 0..2 {
                    let idj = wj * 2 + endj;
                    if node_of[idj] != usize::MAX {
                        continue;
                    }
                    if dist2(p, ep_point(wj, endj)) < TOL * TOL {
                        node_of[idj] = node;
                        members.push((wj, endj));
                    }
                }
            }
            node_members.push(members);
        }
    }

    // Any node where 3+ ends meet is out of scope for this slice (T/Y junction).
    if node_members.iter().any(|m| m.len() >= 3) {
        return None;
    }

    // Group wires into connected chains via degree-2 nodes (union-find).
    let mut parent: Vec<usize> = (0..w).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        let mut c = x;
        while parent[c] != c {
            let next = parent[c];
            parent[c] = r;
            c = next;
        }
        r
    }
    for members in &node_members {
        if members.len() == 2 {
            let (a, _) = members[0];
            let (b, _) = members[1];
            // A degree-2 node joining a wire to itself is a closed single-wire loop.
            if a == b {
                return None;
            }
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            parent[ra] = rb;
        }
    }

    // For each component, order the wires into a linear chain and emit a path.
    let mut comp_wires: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for wi in 0..w {
        let r = find(&mut parent, wi);
        comp_wires.entry(r).or_default().push(wi);
    }

    let mut paths: Vec<ConductorPath> = Vec::new();
    for (_, group) in comp_wires {
        // Count free ends (degree-1 nodes) in this component. A valid open chain
        // has exactly two; anything else (cycle, isolated closed loop) is skipped.
        let mut free_ends: Vec<(usize, usize)> = Vec::new(); // (wi, end)
        for &wi in &group {
            for end in 0..2 {
                let node = node_of[wi * 2 + end];
                if node_members[node].len() == 1 {
                    free_ends.push((wi, end));
                }
            }
        }
        if free_ends.len() != 2 {
            return None;
        }

        // Walk from one free end through the chain, appending segments with signs
        // and cumulative arc-length. Track visited wires to avoid revisiting.
        let (start_wi, start_end) = free_ends[0];
        let mut ordered_segs: Vec<usize> = Vec::new();
        let mut signs: Vec<f64> = Vec::new();
        let mut u_mid: Vec<f64> = Vec::new(); // arc-length at midpoint (from path start)
        let mut arc = 0.0f64;

        let mut cur_wi = start_wi;
        // Enter from `start_end`; we traverse *away* from the entry node.
        // entry end 0 (wire start) → forward (start→end), sign +1.
        // entry end 1 (wire end)   → reverse (end→start), sign -1.
        let mut enter_end = start_end;
        let mut visited = vec![false; w];
        loop {
            visited[cur_wi] = true;
            let (first, last) = wires[cur_wi];
            let forward = enter_end == 0;
            let sign = if forward { 1.0 } else { -1.0 };
            // Segment order along traversal.
            let seg_iter: Vec<usize> = if forward {
                (first..=last).collect()
            } else {
                (first..=last).rev().collect()
            };
            for m in seg_iter {
                let half = segs[m].length * 0.5;
                u_mid.push(arc + half);
                arc += segs[m].length;
                ordered_segs.push(m);
                signs.push(sign);
            }
            // Exit node is the opposite end of this wire.
            let exit_end = 1 - enter_end;
            let exit_node = node_of[cur_wi * 2 + exit_end];
            // Find the next wire at the exit node (degree-2) that we have not visited.
            let members = &node_members[exit_node];
            if members.len() == 1 {
                break; // reached the other free end
            }
            // degree-2: pick the partner wire.
            let mut next: Option<(usize, usize)> = None;
            for &(mwi, mend) in members {
                if mwi != cur_wi && !visited[mwi] {
                    next = Some((mwi, mend));
                }
            }
            match next {
                Some((nwi, nend)) => {
                    cur_wi = nwi;
                    // We arrive at the partner's `nend`; traverse away from it.
                    enter_end = nend;
                }
                None => break, // no unvisited partner (defensive; should be a free end)
            }
        }

        // Re-centre arc-length so s = 0 is the path midpoint.
        let s_mid: Vec<f64> = u_mid.iter().map(|u| u - arc * 0.5).collect();
        let free_a = *ordered_segs.first().unwrap();
        let free_b = *ordered_segs.last().unwrap();
        paths.push(ConductorPath {
            segs: ordered_segs,
            signs,
            s_mid,
            free_ends: (free_a, free_b),
        });
    }

    Some(paths)
}

/// Why a geometry is out of scope for the [`ConductorPath`] (degree-2) Hallén
/// solve — used for accurate user-facing diagnostics (PH9-CHK-002 guardrail).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedTopology {
    /// A node where **three or more** wire ends meet (a T/Y junction). The current
    /// splits among the arms with a Kirchhoff constraint that the continuous
    /// single-path basis cannot represent.
    HighDegreeJunction,
    /// A **closed loop** — a conductor with no free end. The Hallén homogeneous
    /// solution needs a periodic (rather than `I = 0`) closure that is not yet
    /// modelled.
    ClosedLoop,
}

/// Classify why [`build_conductor_paths`] rejects a geometry, so callers can warn
/// with a class-specific message instead of silently returning an unreliable solve.
///
/// Returns `None` exactly when the geometry **is** supported (single wires,
/// collinear chains, or any degree-2 junctioned conductor path). When
/// [`build_conductor_paths`] returns `None`, the only causes are a node where 3+
/// wire ends meet ([`UnsupportedTopology::HighDegreeJunction`]) or a conductor with
/// no free end ([`UnsupportedTopology::ClosedLoop`]); the high-degree case is
/// reported in preference when both are present.
pub fn classify_unsupported_topology(segs: &[Segment]) -> Option<UnsupportedTopology> {
    if build_conductor_paths(segs).is_some() {
        return None;
    }
    if has_high_degree_node(segs) {
        Some(UnsupportedTopology::HighDegreeJunction)
    } else {
        Some(UnsupportedTopology::ClosedLoop)
    }
}

/// Whether any coincident-endpoint node joins **three or more** wire ends (a T/Y
/// junction), using the same endpoint clustering as [`build_conductor_paths`].
fn has_high_degree_node(segs: &[Segment]) -> bool {
    let wires = wire_endpoints_from_segs(segs);
    let w = wires.len();
    if w == 0 {
        return false;
    }
    const TOL: f64 = 1e-6; // metres
    let ep_point = |wi: usize, end: usize| -> [f64; 3] {
        let (first, last) = wires[wi];
        if end == 0 {
            segs[first].start
        } else {
            segs[last].end
        }
    };
    let mut assigned = vec![false; 2 * w];
    for wi in 0..w {
        for end in 0..2 {
            let id = wi * 2 + end;
            if assigned[id] {
                continue;
            }
            let p = ep_point(wi, end);
            let mut count = 1;
            assigned[id] = true;
            for wj in 0..w {
                for endj in 0..2 {
                    let idj = wj * 2 + endj;
                    if assigned[idj] {
                        continue;
                    }
                    if dist2(p, ep_point(wj, endj)) < TOL * TOL {
                        assigned[idj] = true;
                        count += 1;
                    }
                }
            }
            if count >= 3 {
                return true;
            }
        }
    }
    false
}

///
/// Cards are processed in deck order:
/// - `GW` appends new segments.
/// - `GM` transforms (rotate + translate) a range of existing wires, optionally
///   creating numbered copies when `tag_increment > 0`.
/// - `GR` creates additional copies of all existing wires, each rotated about
///   the z-axis by successive multiples of `angle_deg`.
///
/// Segments are assigned consecutive `global_index` values in output order.
pub fn build_geometry(deck: &NecDeck) -> Result<Vec<Segment>, GeometryError> {
    let mut segments: Vec<Segment> = Vec::new();

    for card in &deck.cards {
        match card {
            Card::Gw(gw) => {
                expand_wire(gw, &mut segments)?;
            }
            Card::Gm(gm) => {
                apply_gm(gm, &mut segments)?;
            }
            Card::Gr(gr) => {
                apply_gr(gr, &mut segments)?;
            }
            _ => {}
        }
    }

    if segments.is_empty() {
        return Err(GeometryError::NoWires);
    }

    // Re-number global_index in final order.
    for (i, seg) in segments.iter_mut().enumerate() {
        seg.global_index = i;
    }

    Ok(segments)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn expand_wire(gw: &GwCard, out: &mut Vec<Segment>) -> Result<(), GeometryError> {
    if gw.segments == 0 {
        return Err(GeometryError::ZeroSegments { tag: gw.tag });
    }

    let [x1, y1, z1] = gw.start;
    let [x2, y2, z2] = gw.end;

    let wire_dx = x2 - x1;
    let wire_dy = y2 - y1;
    let wire_dz = z2 - z1;
    let wire_len = (wire_dx * wire_dx + wire_dy * wire_dy + wire_dz * wire_dz).sqrt();

    if wire_len == 0.0 {
        return Err(GeometryError::ZeroLengthWire { tag: gw.tag });
    }

    let direction = [wire_dx / wire_len, wire_dy / wire_len, wire_dz / wire_len];
    let n = gw.segments as f64;
    let seg_len = wire_len / n;

    for i in 0..gw.segments {
        let t0 = i as f64 / n;
        let t1 = (i as f64 + 1.0) / n;
        let tm = (t0 + t1) * 0.5;

        let start = [x1 + wire_dx * t0, y1 + wire_dy * t0, z1 + wire_dz * t0];
        let end = [x1 + wire_dx * t1, y1 + wire_dy * t1, z1 + wire_dz * t1];
        let midpoint = [x1 + wire_dx * tm, y1 + wire_dy * tm, z1 + wire_dz * tm];

        out.push(Segment {
            tag: gw.tag,
            tag_index: i + 1,
            global_index: out.len(),
            start,
            end,
            midpoint,
            direction,
            length: seg_len,
            radius: gw.radius,
        });
    }

    Ok(())
}

/// Apply a GM card: rotate then translate matching wires; optionally replicate.
///
/// When `tag_increment == 0` the existing wires are transformed in place.
/// When `tag_increment > 0` a copy with incremented tags is appended and the
/// originals are left unchanged.
///
/// Tag filtering: `first_tag..=last_tag` if both are > 0; `first_tag..` if
/// only `first_tag` > 0; `..=last_tag` if only `last_tag` > 0; all wires if
/// both are 0.
fn apply_gm(gm: &GmCard, segs: &mut Vec<Segment>) -> Result<(), GeometryError> {
    if gm.tag_increment == 0 {
        // In-place transform: mutate matching segments.
        for seg in segs.iter_mut() {
            if tag_in_range(seg.tag, gm.first_tag, gm.last_tag) {
                seg.start = transform_point(seg.start, gm);
                seg.end = transform_point(seg.end, gm);
                seg.midpoint = transform_point(seg.midpoint, gm);
                seg.direction = recompute_direction(seg.start, seg.end);
            }
        }
    } else {
        // Copy transform: append new segments with incremented tag numbers.
        let base: Vec<Segment> = segs
            .iter()
            .filter(|s| tag_in_range(s.tag, gm.first_tag, gm.last_tag))
            .cloned()
            .collect();
        for mut seg in base {
            seg.tag += gm.tag_increment;
            seg.start = transform_point(seg.start, gm);
            seg.end = transform_point(seg.end, gm);
            seg.midpoint = transform_point(seg.midpoint, gm);
            seg.direction = recompute_direction(seg.start, seg.end);
            segs.push(seg);
        }
    }
    Ok(())
}

/// Apply a GR card: repeat existing wires `count` times, rotating each copy
/// about the z-axis by successive multiples of `angle_deg`.
fn apply_gr(gr: &GrCard, segs: &mut Vec<Segment>) -> Result<(), GeometryError> {
    if gr.count == 0 {
        return Ok(());
    }
    // Snapshot original set of segments (before any copies).
    let originals: Vec<Segment> = segs.clone();
    for copy_idx in 1..=gr.count {
        let total_angle_deg = gr.angle_deg * copy_idx as f64;
        let cos_a = total_angle_deg.to_radians().cos();
        let sin_a = total_angle_deg.to_radians().sin();
        for orig in &originals {
            let mut seg = orig.clone();
            seg.tag += gr.tag_increment * copy_idx;
            seg.start = rotate_z(seg.start, cos_a, sin_a);
            seg.end = rotate_z(seg.end, cos_a, sin_a);
            seg.midpoint = rotate_z(seg.midpoint, cos_a, sin_a);
            seg.direction = recompute_direction(seg.start, seg.end);
            segs.push(seg);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// True if `tag` falls in [first_tag, last_tag] with 0 meaning "no limit".
fn tag_in_range(tag: u32, first_tag: u32, last_tag: u32) -> bool {
    let above_first = first_tag == 0 || tag >= first_tag;
    let below_last = last_tag == 0 || tag <= last_tag;
    above_first && below_last
}

/// Apply GM rotation (Rx Ry Rz) then translation to a point.
///
/// Rotation order follows NEC convention: Rx, then Ry, then Rz.
fn transform_point(p: [f64; 3], gm: &GmCard) -> [f64; 3] {
    let p = rotate_x(
        p,
        gm.rot_x_deg.to_radians().cos(),
        gm.rot_x_deg.to_radians().sin(),
    );
    let p = rotate_y(
        p,
        gm.rot_y_deg.to_radians().cos(),
        gm.rot_y_deg.to_radians().sin(),
    );
    let p = rotate_z(
        p,
        gm.rot_z_deg.to_radians().cos(),
        gm.rot_z_deg.to_radians().sin(),
    );
    [
        p[0] + gm.translate_x,
        p[1] + gm.translate_y,
        p[2] + gm.translate_z,
    ]
}

fn rotate_x(p: [f64; 3], c: f64, s: f64) -> [f64; 3] {
    [p[0], c * p[1] - s * p[2], s * p[1] + c * p[2]]
}

fn rotate_y(p: [f64; 3], c: f64, s: f64) -> [f64; 3] {
    [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
}

fn rotate_z(p: [f64; 3], c: f64, s: f64) -> [f64; 3] {
    [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]]
}

fn recompute_direction(start: [f64; 3], end: [f64; 3]) -> [f64; 3] {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let dz = end[2] - start[2];
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len == 0.0 {
        [0.0, 0.0, 1.0] // degenerate — direction undefined
    } else {
        [dx / len, dy / len, dz / len]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, GmCard, GnCard, GrCard, GwCard};
    use nec_model::deck::NecDeck;

    fn deck_with_gw(tag: u32, segs: u32, start: [f64; 3], end: [f64; 3], r: f64) -> NecDeck {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag,
            segments: segs,
            start,
            end,
            radius: r,
        }));
        deck
    }

    /// Dipole along Z: 11-segment half-wave dipole at 28 MHz.
    /// Total length = 5.354 m; each segment ≈ 0.4867 m.
    #[test]
    fn dipole_segment_count_and_length() {
        let deck = deck_with_gw(1, 11, [0.0, 0.0, -2.677], [0.0, 0.0, 2.677], 0.001);
        let segs = build_geometry(&deck).expect("should succeed");

        assert_eq!(segs.len(), 11);

        let expected_len = 5.354 / 11.0;
        for s in &segs {
            let diff = (s.length - expected_len).abs();
            assert!(diff < 1e-10, "segment length off: {}", s.length);
        }
    }

    #[test]
    fn direction_is_unit_vector() {
        let deck = deck_with_gw(1, 5, [0.0, 0.0, -1.0], [0.0, 0.0, 1.0], 0.001);
        let segs = build_geometry(&deck).unwrap();
        for s in &segs {
            let [dx, dy, dz] = s.direction;
            let mag = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!((mag - 1.0).abs() < 1e-12, "direction not unit: {mag}");
        }
    }

    #[test]
    fn midpoint_is_centre_of_segment() {
        let deck = deck_with_gw(1, 3, [0.0, 0.0, 0.0], [3.0, 0.0, 0.0], 0.001);
        let segs = build_geometry(&deck).unwrap();
        // Segments at x=[0,1], [1,2], [2,3]; midpoints at x=0.5, 1.5, 2.5
        let expected_midpoints = [0.5_f64, 1.5, 2.5];
        for (s, &ex) in segs.iter().zip(expected_midpoints.iter()) {
            assert!((s.midpoint[0] - ex).abs() < 1e-12);
        }
    }

    #[test]
    fn tag_and_indices_are_correct() {
        let deck = deck_with_gw(7, 4, [0.0, 0.0, 0.0], [4.0, 0.0, 0.0], 0.001);
        let segs = build_geometry(&deck).unwrap();
        for (i, s) in segs.iter().enumerate() {
            assert_eq!(s.tag, 7);
            assert_eq!(s.tag_index, i as u32 + 1);
            assert_eq!(s.global_index, i);
        }
    }

    #[test]
    fn two_wires_global_indices_are_contiguous() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, 0.0],
            end: [3.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 2,
            start: [0.0, 1.0, 0.0],
            end: [2.0, 1.0, 0.0],
            radius: 0.001,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 5);
        for (i, s) in segs.iter().enumerate() {
            assert_eq!(s.global_index, i);
        }
    }

    #[test]
    fn zero_segments_is_error() {
        let deck = deck_with_gw(1, 0, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.001);
        assert!(matches!(
            build_geometry(&deck),
            Err(GeometryError::ZeroSegments { tag: 1 })
        ));
    }

    #[test]
    fn zero_length_wire_is_error() {
        let deck = deck_with_gw(1, 3, [1.0, 1.0, 1.0], [1.0, 1.0, 1.0], 0.001);
        assert!(matches!(
            build_geometry(&deck),
            Err(GeometryError::ZeroLengthWire { tag: 1 })
        ));
    }

    #[test]
    fn no_wires_is_error() {
        let deck = NecDeck::new();
        assert!(matches!(build_geometry(&deck), Err(GeometryError::NoWires)));
    }

    #[test]
    fn ground_model_defaults_to_free_space_without_gn() {
        let deck = NecDeck::new();
        assert_eq!(ground_model_from_deck(&deck), GroundModel::FreeSpace);
    }

    #[test]
    fn ground_model_detects_perfect_conductor_gn1() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 1,
            eps_r: None,
            sigma: None,
        }));
        assert_eq!(ground_model_from_deck(&deck), GroundModel::PerfectConductor);
    }

    #[test]
    fn ground_model_maps_gn0_to_simple_finite_ground() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 0,
            eps_r: None,
            sigma: None,
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 13.0,
                sigma: 0.005,
            }
        );
    }

    #[test]
    fn ground_model_gn0_uses_explicit_medium_params() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 0,
            eps_r: Some(5.0),
            sigma: Some(0.001),
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 5.0,
                sigma: 0.001,
            }
        );
    }

    #[test]
    fn ground_model_maps_gn2_to_simple_finite_ground() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 2,
            eps_r: None,
            sigma: None,
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 13.0,
                sigma: 0.005,
            }
        );
    }

    #[test]
    fn ground_model_gn_negative1_returns_free_space() {
        // GN -1 is the NEC "null ground" card — it cancels a previous GN
        // statement and should produce FreeSpace with no warning.
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: -1,
            eps_r: None,
            sigma: None,
        }));
        assert_eq!(ground_model_from_deck(&deck), GroundModel::FreeSpace);
    }

    #[test]
    fn ground_model_gn2_uses_explicit_medium_params() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 2,
            eps_r: Some(13.0),
            sigma: Some(0.005),
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 13.0,
                sigma: 0.005,
            }
        );
    }

    // -------------------------------------------------------------------------
    // GM / GR tests
    // -------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn make_gm(
        tag_increment: u32,
        last_tag: u32,
        first_tag: u32,
        rx: f64,
        ry: f64,
        rz: f64,
        tx: f64,
        ty: f64,
        tz: f64,
    ) -> GmCard {
        GmCard {
            tag_increment,
            last_tag,
            first_tag,
            rot_x_deg: rx,
            rot_y_deg: ry,
            rot_z_deg: rz,
            translate_x: tx,
            translate_y: ty,
            translate_z: tz,
        }
    }

    /// GM with tag_increment=0 translates segments in place.
    #[test]
    fn gm_inplace_translate() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }));
        // Translate +2 m along z
        deck.cards
            .push(Card::Gm(make_gm(0, 0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0)));
        let segs = build_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 1);
        assert!((segs[0].start[2] - 2.0).abs() < 1e-12);
        assert!((segs[0].end[2] - 2.0).abs() < 1e-12);
    }

    /// GM with tag_increment>0 creates a copy with incremented tag.
    #[test]
    fn gm_copy_increments_tag() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }));
        // Copy with tag_increment=1, translate +1 m along y
        deck.cards
            .push(Card::Gm(make_gm(1, 0, 0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0)));
        let segs = build_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 2);
        // Original (tag=1) unchanged
        assert_eq!(segs[0].tag, 1);
        assert!((segs[0].start[1]).abs() < 1e-12);
        // Copy (tag=2) translated
        assert_eq!(segs[1].tag, 2);
        assert!((segs[1].start[1] - 1.0).abs() < 1e-12);
    }

    /// GR with count=3 and 90-degree steps produces 4 wires (original + 3 copies).
    #[test]
    fn gr_repeat_produces_correct_count() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [1.0, 0.0, 0.0],
            end: [2.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            count: 3,
            angle_deg: 90.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        // 1 original + 3 copies = 4 segments (each wire has 1 segment)
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].tag, 1);
        assert_eq!(segs[1].tag, 2);
        assert_eq!(segs[2].tag, 3);
        assert_eq!(segs[3].tag, 4);
    }

    /// GR 90-degree rotation moves (1,0,0) to (0,1,0).
    #[test]
    fn gr_rotation_is_correct() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [1.0, 0.0, 0.0],
            end: [2.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            count: 1,
            angle_deg: 90.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 2);
        // Copy should be at y=1..2, x≈0
        assert!((segs[1].start[0]).abs() < 1e-12, "x should be ~0");
        assert!((segs[1].start[1] - 1.0).abs() < 1e-12, "y should be ~1");
        assert!((segs[1].end[0]).abs() < 1e-12);
        assert!((segs[1].end[1] - 2.0).abs() < 1e-12);
    }

    /// global_index values are contiguous 0..N-1 after GR expansion.
    #[test]
    fn gr_global_indices_are_contiguous() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [1.0, 0.0, 0.0],
            end: [2.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            count: 2,
            angle_deg: 120.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 9); // 3 wires × 3 segments
        for (i, s) in segs.iter().enumerate() {
            assert_eq!(s.global_index, i);
        }
    }
}
