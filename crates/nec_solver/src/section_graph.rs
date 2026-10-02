// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Hallén on a graph of sections: junctions of degree ≥ 3 and closed loops
//! (FND-162 stages 2 and 3).
//!
//! A conductor path (the degree-2 solve of stages 1b and 4) is a chain; a T, a Y
//! or a loop is not. Here the unit is the **section** — a maximal straight run of
//! segments of equal radius — and sections meet at **nodes**. Every section
//! carries its own homogeneous pair `(C, D)`; every node of degree `d` carries
//! `d` rows:
//!
//! - `d = 1` (a free end): the section current, extrapolated to the end, is zero;
//! - `d ≥ 2`: one Kirchhoff row (the extrapolated currents flowing into the node
//!   sum to zero) and `d − 1` equal-potential rows (`Φ = ∇·A / k` is the same in
//!   every section at the node — a scalar, so no frame signs).
//!
//! Unknowns `N + 2S`, rows `N + Σ d = N + 2S` for any graph, loops included.
//!
//! The delta-gap forcing is written on the fed section only: on a loop "the rest
//! of the path" has no meaning. Its derivative enters the potential rows at that
//! section's two nodes as a constant. Across the design review's prototype (a
//! Rust program against this crate) the formulation reproduced the degree-2
//! bend solve to the printed digit and converged on nec2c for a Y, a T and a 1λ
//! square loop.

use num_complex::Complex64;

use crate::geometry::{wire_endpoints_from_segs, Segment};
use crate::hallen_session::{CornerSource, JUNCTION_TOL_M};
use crate::linear::SolveError;
use crate::matrix::ZMatrix;

/// Free-space wave impedance, as the other Hallén builders define it.
const ETA0: f64 = 4.0e-7 * std::f64::consts::PI * 299_792_458.0;

/// A maximal straight run of segments.
#[derive(Debug, Clone)]
pub struct Section {
    /// Segment indices in order along the section (start node to end node).
    pub segs: Vec<usize>,
    /// `+1` where the segment points along the section, `−1` where against it.
    pub signs: Vec<f64>,
    /// Arc length of each segment's midpoint from the start node.
    pub s_mid: Vec<f64>,
    /// Section length.
    pub length: f64,
    /// Unit tangent, start node to end node.
    pub tangent: [f64; 3],
    /// Node at `s = 0` and at `s = length`.
    pub start_node: usize,
    pub end_node: usize,
}

/// A point where section ends meet (or a free end).
#[derive(Debug, Clone)]
pub struct Node {
    pub point: [f64; 3],
    /// `(section, at_end)`: `at_end` is true where the section's END lies here.
    pub ends: Vec<(usize, bool)>,
}

#[derive(Debug, Clone)]
pub struct SectionGraph {
    pub sections: Vec<Section>,
    pub nodes: Vec<Node>,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    let d = sub(a, b);
    dot(d, d) < JUNCTION_TOL_M * JUNCTION_TOL_M
}

impl SectionGraph {
    /// Whether this geometry needs the graph at all: a node of degree ≥ 3, or a
    /// cycle. Anything else is a set of chains, which the path solve handles.
    pub fn has_junction_or_loop(&self) -> bool {
        if self.nodes.iter().any(|n| n.ends.len() >= 3) {
            return true;
        }
        // A cycle: sections − (nodes − components) > 0. With every node of
        // degree ≤ 2 a component is a chain or a ring; a ring has no free end.
        let mut parent: Vec<usize> = (0..self.nodes.len()).collect();
        fn find(p: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while p[r] != r {
                r = p[r];
            }
            p[x] = r;
            r
        }
        for s in &self.sections {
            let (a, b) = (
                find(&mut parent, s.start_node),
                find(&mut parent, s.end_node),
            );
            if a == b {
                return true;
            }
            parent[a] = b;
        }
        false
    }
}

/// Why the section-graph builder refused a geometry, naming the place, so a
/// caveat can say what to change rather than only that the deck is unreliable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphRefusal {
    /// No wires at all.
    Empty,
    /// A straight run one segment long, at this tag and segment: its single row
    /// cannot fix a section's two constants. Carrying its current unchanged to
    /// both ends was measured and rejected: a folded dipole with one-segment end
    /// jumpers stayed 18.6 → 13.4 → 10.5 % off nec2c at 21/41/81 segments on the
    /// long wires, where two-segment jumpers give 12.1 → 4.0 → 0.9 %.
    OneSegmentRun { tag: u32, seg: u32 },
}

impl std::fmt::Display for GraphRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphRefusal::Empty => write!(f, "the deck has no wires"),
            GraphRefusal::OneSegmentRun { tag, seg } => write!(
                f,
                "the straight run at tag {tag} segment {seg} is one segment long; give it \
                 at least two segments"
            ),
        }
    }
}

/// Build the section graph of `segs`, or `None` when the geometry falls outside
/// what it models ([`section_graph`] names why).
pub fn build_section_graph(segs: &[Segment]) -> Option<SectionGraph> {
    section_graph(segs).ok()
}

/// Build the section graph of `segs`, or the reason it falls outside what the
/// builder models: a straight run one segment long (its single row cannot fix
/// two constants).
pub fn section_graph(segs: &[Segment]) -> Result<SectionGraph, GraphRefusal> {
    let wires = wire_endpoints_from_segs(segs);
    if wires.is_empty() {
        return Err(GraphRefusal::Empty);
    }
    // Cluster wire ends into physical points.
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut at_point: Vec<Vec<(usize, bool)>> = Vec::new(); // (wire, is_wire_end)
    let point_of = |p: [f64; 3], points: &mut Vec<[f64; 3]>, at: &mut Vec<Vec<(usize, bool)>>| {
        if let Some(i) = points.iter().position(|&q| near(p, q)) {
            i
        } else {
            points.push(p);
            at.push(Vec::new());
            points.len() - 1
        }
    };
    let mut wire_pts = Vec::with_capacity(wires.len());
    for (w, &(first, last)) in wires.iter().enumerate() {
        let a = point_of(segs[first].start, &mut points, &mut at_point);
        let b = point_of(segs[last].end, &mut points, &mut at_point);
        at_point[a].push((w, false));
        at_point[b].push((w, true));
        wire_pts.push((a, b));
    }
    // A wire end on another wire's interior joint cannot reach here:
    // `wire_endpoints_from_segs` splits the wire there (FND-192).

    // A point with exactly two wire ends that continue straight (same direction
    // through it, same radius) is not a node: the two wires are one section.
    let continues = |pi: usize| -> Option<(usize, bool, usize, bool)> {
        let ends = &at_point[pi];
        if ends.len() != 2 {
            return None;
        }
        let (w1, e1) = ends[0];
        let (w2, e2) = ends[1];
        let seg_at = |w: usize, is_end: bool| {
            let (f, l) = wires[w];
            if is_end {
                l
            } else {
                f
            }
        };
        let (s1, s2) = (&segs[seg_at(w1, e1)], &segs[seg_at(w2, e2)]);
        // Outward directions from the point along each wire.
        let out = |s: &Segment, is_end: bool| {
            let d = s.direction;
            if is_end {
                [-d[0], -d[1], -d[2]]
            } else {
                d
            }
        };
        let (o1, o2) = (out(s1, e1), out(s2, e2));
        let straight = dot(o1, o2) < -1.0 + 1e-9 && (s1.radius - s2.radius).abs() < JUNCTION_TOL_M;
        straight.then_some((w1, e1, w2, e2))
    };
    let is_node: Vec<bool> = (0..points.len())
        .map(|pi| continues(pi).is_none())
        .collect();

    // Walk chains of wires between nodes into sections.
    let mut used = vec![false; wires.len()];
    let mut node_id = vec![usize::MAX; points.len()];
    let mut nodes: Vec<Node> = Vec::new();
    let mut node_of = |pi: usize, nodes: &mut Vec<Node>| {
        if node_id[pi] == usize::MAX {
            node_id[pi] = nodes.len();
            nodes.push(Node {
                point: points[pi],
                ends: Vec::new(),
            });
        }
        node_id[pi]
    };
    let mut sections = Vec::new();
    for w0 in 0..wires.len() {
        if used[w0] {
            continue;
        }
        // Back up from w0 through non-node points to the chain's first wire.
        let (mut w, mut forward) = (w0, true); // forward: walk the wire start→end
        let mut guard = 0;
        loop {
            let p = if forward {
                wire_pts[w].0
            } else {
                wire_pts[w].1
            };
            if is_node[p] || guard > wires.len() {
                break;
            }
            let (w1, e1, w2, e2) = continues(p).expect("a non-node point continues");
            let (nw, ne) = if w1 == w { (w2, e2) } else { (w1, e1) };
            if nw == w0 {
                break; // a straight ring: never a real antenna, stop anywhere
            }
            w = nw;
            forward = ne; // arriving at nw's END means walking it end→start backwards … forward = is_end
            guard += 1;
        }
        // Now walk forward from (w, forward) collecting segments.
        let start_pt = if forward {
            wire_pts[w].0
        } else {
            wire_pts[w].1
        };
        let mut sec_segs = Vec::new();
        let mut sec_signs = Vec::new();
        let (mut cw, mut cf) = (w, forward);
        let end_pt;
        loop {
            used[cw] = true;
            let (f, l) = wires[cw];
            if cf {
                for m in f..=l {
                    sec_segs.push(m);
                    sec_signs.push(1.0);
                }
            } else {
                for m in (f..=l).rev() {
                    sec_segs.push(m);
                    sec_signs.push(-1.0);
                }
            }
            let p = if cf { wire_pts[cw].1 } else { wire_pts[cw].0 };
            if is_node[p] {
                end_pt = p;
                break;
            }
            let (w1, e1, w2, e2) = continues(p).expect("a non-node point continues");
            let (nw, ne) = if w1 == cw { (w2, e2) } else { (w1, e1) };
            if used[nw] {
                end_pt = p;
                break;
            }
            cw = nw;
            cf = !ne; // entering nw at its end means walking it backwards
        }
        if sec_segs.len() < 2 {
            let one = &segs[sec_segs[0]];
            return Err(GraphRefusal::OneSegmentRun {
                tag: one.tag,
                seg: one.tag_index,
            });
        }
        let first = &segs[sec_segs[0]];
        let tangent = [
            first.direction[0] * sec_signs[0],
            first.direction[1] * sec_signs[0],
            first.direction[2] * sec_signs[0],
        ];
        let mut s_mid = Vec::with_capacity(sec_segs.len());
        let mut acc = 0.0;
        for &m in &sec_segs {
            s_mid.push(acc + segs[m].length / 2.0);
            acc += segs[m].length;
        }
        let sn = node_of(start_pt, &mut nodes);
        let en = node_of(end_pt, &mut nodes);
        let id = sections.len();
        nodes[sn].ends.push((id, false));
        nodes[en].ends.push((id, true));
        sections.push(Section {
            segs: sec_segs,
            signs: sec_signs,
            s_mid,
            length: acc,
            tangent,
            start_node: sn,
            end_node: en,
        });
    }
    Ok(SectionGraph { sections, nodes })
}

/// Section index and position within it, per global segment.
fn locate(graph: &SectionGraph, n: usize) -> Vec<(usize, usize)> {
    let mut at = vec![(usize::MAX, 0); n];
    for (j, s) in graph.sections.iter().enumerate() {
        for (i, &m) in s.segs.iter().enumerate() {
            at[m] = (j, i);
        }
    }
    at
}

/// The section current extrapolated to its node at `s = 0` (`at_end = false`) or
/// `s = length`, as `(segment, coefficient)` terms — the free-end rule of FND-156
/// (linear through the two outermost segments), in the section's frame.
fn extrapolated(sec: &Section, segs: &[Segment], at_end: bool) -> [(usize, f64); 2] {
    let k = sec.segs.len();
    let (e, i) = if at_end { (k - 1, k - 2) } else { (0, 1) };
    let (he, hi) = (segs[sec.segs[e]].length, segs[sec.segs[i]].length);
    let t = he / (he + hi);
    [
        (sec.segs[e], (1.0 + t) * sec.signs[e]),
        (sec.segs[i], -t * sec.signs[i]),
    ]
}

/// The delta-gap feed, in section coordinates.
#[derive(Debug, Clone, Copy)]
pub struct GraphFeed {
    pub seg: usize,
    pub volts: Complex64,
}

/// The section-graph Hallén system of one matrix, without its forcing.
///
/// Everything a set of gap sources does not change is built once: the
/// collocation rows with their (C, D) columns and the corner term, and the node
/// rows. The forcing is linear in the gaps — each a particular solution on its
/// own section plus that section's constant in the equal-potential rows — so one
/// system serves any number of forcings (the unit-gap solves of a network, the
/// gaps of a plane wave), and a lumped load is a column: the gap its own current
/// drives through it.
/// A node end: a section, and whether it is that section's end (not its start).
type NodeEnd = (usize, bool);

pub(crate) struct GraphSystem {
    graph: SectionGraph,
    /// Section and position along it of every segment.
    at: Vec<(usize, usize)>,
    m: Vec<Vec<Complex64>>,
    /// For each row past the collocation rows, the two node ends whose
    /// potentials it differences; `None` for a free-end or Kirchhoff row.
    phi_pairs: Vec<Option<(NodeEnd, NodeEnd)>>,
    n: usize,
    cols: usize,
    k: f64,
}

impl GraphSystem {
    /// Build the system: `z` with ground images, `sources` the corner sources.
    pub(crate) fn build(
        z: &ZMatrix,
        segs: &[Segment],
        graph: &SectionGraph,
        sources: &[CornerSource],
        k: f64,
    ) -> Result<Self, SolveError> {
        let n = z.n;
        let ns = graph.sections.len();
        let cols = n + 2 * ns;
        let c_col = |j: usize| n + j;
        let d_col = |j: usize| n + ns + j;
        let at = locate(graph, n);
        if at.iter().any(|&(j, _)| j == usize::MAX) {
            return Err(SolveError::HallenDimensionMismatch {
                z_n: n,
                rhs_len: 0,
                cos_len: 0,
            });
        }

        // The corner reference of each section: its start node unless that is a free
        // end (then its end node) — the rule that reproduces the path solve.
        let ref_at_end: Vec<bool> = graph
            .sections
            .iter()
            .map(|s| graph.nodes[s.start_node].ends.len() < 2)
            .collect();
        let ref_s = |j: usize| {
            if ref_at_end[j] {
                graph.sections[j].length
            } else {
                0.0
            }
        };
        let ref_pt = |j: usize| {
            let s = &graph.sections[j];
            graph.nodes[if ref_at_end[j] {
                s.end_node
            } else {
                s.start_node
            }]
            .point
        };

        let mut m = vec![vec![Complex64::new(0.0, 0.0); cols]; n];
        for r in 0..n {
            for (c, cell) in m[r].iter_mut().enumerate().take(n) {
                *cell = z.get(r, c);
            }
            let (j, i) = at[r];
            let sec = &graph.sections[j];
            let (sg, s) = (sec.signs[i], sec.s_mid[i]);
            m[r][c_col(j)] = Complex64::new(-sg * (k * s).cos(), 0.0);
            m[r][d_col(j)] = Complex64::new(-sg * (k * s).sin(), 0.0);
        }
        // The corner term on every section, from every non-parallel source.
        for (j, sec) in graph.sections.iter().enumerate() {
            let (s0, p0, tan) = (ref_s(j), ref_pt(j), sec.tangent);
            let at_s = |s: f64| {
                [
                    p0[0] + (s - s0) * tan[0],
                    p0[1] + (s - s0) * tan[1],
                    p0[2] + (s - s0) * tan[2],
                ]
            };
            for src in sources {
                if crate::corner::parallel(tan, &src.seg) {
                    continue;
                }
                let breaks: Vec<f64> =
                    crate::corner::source_breaks(s0, p0, tan, &src.seg, 0.1 * sec.length)
                        .into_iter()
                        .flatten()
                        .collect();
                for (i, &r) in sec.segs.iter().enumerate() {
                    let sm = sec.s_mid[i];
                    let v = crate::corner::graded_breaks(s0, sm, src.seg.radius, &breaks, |s| {
                        crate::corner::f_n(at_s(s), tan, &src.seg, k) * (k * (sm - s)).cos()
                    });
                    m[r][src.col] += v * src.weight * sec.signs[i];
                }
            }
        }

        // Node rows.
        let mut extra: Vec<Vec<Complex64>> = Vec::new();
        let mut phi_pairs = Vec::new();
        // Φ_j at the node, as (row coefficients, constant) in section j's frame.
        let phi = |j: usize, at_end: bool| -> Vec<Complex64> {
            let sec = &graph.sections[j];
            let sigma = if at_end { sec.length } else { 0.0 };
            let mut row = vec![Complex64::new(0.0, 0.0); cols];
            row[c_col(j)] = Complex64::new(-(k * sigma).sin(), 0.0);
            row[d_col(j)] = Complex64::new((k * sigma).cos(), 0.0);
            // The corner integral of a section referenced at its OTHER node.
            if at_end != ref_at_end[j] {
                let (s0, p0, tan) = (ref_s(j), ref_pt(j), sec.tangent);
                let at_s = |s: f64| {
                    [
                        p0[0] + (s - s0) * tan[0],
                        p0[1] + (s - s0) * tan[1],
                        p0[2] + (s - s0) * tan[2],
                    ]
                };
                for src in sources {
                    if crate::corner::parallel(tan, &src.seg) {
                        continue;
                    }
                    let breaks: Vec<f64> =
                        crate::corner::source_breaks(s0, p0, tan, &src.seg, 0.1 * sec.length)
                            .into_iter()
                            .flatten()
                            .collect();
                    let v = crate::corner::graded_breaks(s0, sigma, src.seg.radius, &breaks, |s| {
                        crate::corner::f_n(at_s(s), tan, &src.seg, k) * (k * (sigma - s)).sin()
                    });
                    row[src.col] += v * src.weight;
                }
            }
            row
        };
        for node in &graph.nodes {
            if node.ends.len() == 1 {
                let (j, at_end) = node.ends[0];
                let mut row = vec![Complex64::new(0.0, 0.0); cols];
                for (seg, c) in extrapolated(&graph.sections[j], segs, at_end) {
                    row[seg] += Complex64::new(c, 0.0);
                }
                extra.push(row);
                phi_pairs.push(None);
                continue;
            }
            // Kirchhoff: current flowing into the node sums to zero.
            let mut kcl = vec![Complex64::new(0.0, 0.0); cols];
            for &(j, at_end) in &node.ends {
                let inflow = if at_end { 1.0 } else { -1.0 };
                for (seg, c) in extrapolated(&graph.sections[j], segs, at_end) {
                    kcl[seg] += Complex64::new(inflow * c, 0.0);
                }
            }
            extra.push(kcl);
            phi_pairs.push(None);
            // Equal potential against the first section at the node.
            let row0 = phi(node.ends[0].0, node.ends[0].1);
            for &(j, at_end) in &node.ends[1..] {
                let rj = phi(j, at_end);
                extra.push(row0.iter().zip(&rj).map(|(a, b)| a - b).collect());
                phi_pairs.push(Some((node.ends[0], (j, at_end))));
            }
        }
        m.extend(extra);
        Ok(GraphSystem {
            graph: graph.clone(),
            at,
            m,
            phi_pairs,
            n,
            cols,
            k,
        })
    }

    /// The right-hand side of a set of gap sources: each one's particular
    /// solution on its own section, and that section's term in the
    /// equal-potential rows (P′(σ)/k at the node end).
    pub(crate) fn rhs(&self, gaps: &[GraphFeed]) -> Vec<Complex64> {
        let (n, k) = (self.n, self.k);
        let scale = 2.0 * std::f64::consts::PI / ETA0;
        let mut y = vec![Complex64::new(0.0, 0.0); self.m.len()];
        for f in gaps {
            let (jf, fi) = self.at[f.seg];
            let sec = &self.graph.sections[jf];
            let (sf, sgf) = (sec.s_mid[fi], sec.signs[fi]);
            for (i, &r) in sec.segs.iter().enumerate() {
                let p = Complex64::new(0.0, -scale)
                    * f.volts
                    * sgf
                    * (k * (sec.s_mid[i] - sf).abs()).sin();
                y[r] += p * sec.signs[i];
            }
        }
        // Φ_j's constant at a node end: the fed sections' P′(σ)/k.
        let constant = |j: usize, at_end: bool| {
            let sec = &self.graph.sections[j];
            let sigma = if at_end { sec.length } else { 0.0 };
            let mut c = Complex64::new(0.0, 0.0);
            for f in gaps {
                let (jf, fi) = self.at[f.seg];
                if jf != j {
                    continue;
                }
                let (sf, sgf) = (sec.s_mid[fi], sec.signs[fi]);
                let sgn = if sigma > sf { 1.0 } else { -1.0 };
                c += Complex64::new(0.0, -scale) * f.volts * sgf * (k * (sigma - sf)).cos() * sgn;
            }
            c
        };
        for (row, pair) in self.phi_pairs.iter().enumerate() {
            if let Some(((j0, e0), (jr, er))) = *pair {
                y[n + row] = constant(jr, er) - constant(j0, e0);
            }
        }
        y
    }

    /// Stamp a lumped series load `z_l` (ohms) on segment `p`. A load is a gap
    /// its own current drives, `V = −z_l·I_p`; the forcing is linear in the gap
    /// voltage, so moving it to the left adds `z_l` times a unit gap's
    /// right-hand side to column `p` — the collocation rows of `p`'s section and
    /// that section's terms in the equal-potential rows alike.
    pub(crate) fn add_load(&mut self, p: usize, z_l: Complex64) {
        let unit = self.rhs(&[GraphFeed {
            seg: p,
            volts: Complex64::new(1.0, 0.0),
        }]);
        for (row, u) in self.m.iter_mut().zip(&unit) {
            row[p] += z_l * u;
        }
    }

    /// Solve for a set of gap sources; returns the segment currents.
    pub(crate) fn solve(&self, gaps: &[GraphFeed]) -> Result<Vec<Complex64>, SolveError> {
        let x = crate::linear::solve_normal_equations(&self.m, &self.rhs(gaps), self.cols)?;
        Ok(x[..self.n].to_vec())
    }
}
