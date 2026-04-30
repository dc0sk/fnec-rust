// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_model::card::Card;
use nec_solver::GroundModel;

pub(super) fn sinusoidal_a4_topology_supported(
    segs: &[nec_solver::Segment],
    wire_endpoints: &[(usize, usize)],
) -> bool {
    if segs.is_empty() {
        return false;
    }

    let ref_dir = segs[0].direction;
    for seg in segs {
        let dot = seg.direction[0] * ref_dir[0]
            + seg.direction[1] * ref_dir[1]
            + seg.direction[2] * ref_dir[2];
        if dot.abs() < 1.0 - 1e-9 {
            return false;
        }
    }

    // A4 phase-2: wire-chain detection is orientation/order agnostic.
    // Build an undirected graph from wire endpoints and require a single
    // connected path (degrees <=2, exactly two degree-1 nodes unless there is
    // only one wire). This still rejects disconnected and branched topologies.
    const TOUCH_EPS: f64 = 1e-9;
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut degree: Vec<usize> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();

    for (first, last) in wire_endpoints.iter().copied() {
        if first > last || last >= segs.len() {
            return false;
        }
        let a = segs[first].start;
        let b = segs[last].end;
        let ia = find_or_insert_node(&mut nodes, &mut degree, a, TOUCH_EPS);
        let ib = find_or_insert_node(&mut nodes, &mut degree, b, TOUCH_EPS);
        if ia == ib {
            return false;
        }
        degree[ia] += 1;
        degree[ib] += 1;
        edges.push((ia, ib));
    }

    if degree.iter().any(|d| *d > 2) {
        return false;
    }
    let degree_one = degree.iter().filter(|d| **d == 1).count();
    if wire_endpoints.len() == 1 {
        if degree_one != 2 {
            return false;
        }
    } else if degree_one != 2 {
        return false;
    }

    if nodes.is_empty() {
        return false;
    }

    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (u, v) in edges {
        adjacency[u].push(v);
        adjacency[v].push(u);
    }

    let start = match degree.iter().position(|d| *d > 0) {
        Some(idx) => idx,
        None => return false,
    };
    let mut seen = vec![false; nodes.len()];
    let mut stack = vec![start];
    while let Some(u) = stack.pop() {
        if seen[u] {
            continue;
        }
        seen[u] = true;
        for &v in &adjacency[u] {
            if !seen[v] {
                stack.push(v);
            }
        }
    }
    if degree
        .iter()
        .enumerate()
        .any(|(idx, d)| *d > 0 && !seen[idx])
    {
        return false;
    }

    true
}

pub(super) fn points_close(a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt() <= eps
}

pub(super) fn segment_intersection_error(segs: &[nec_solver::Segment]) -> Option<String> {
    const TOUCH_EPS: f64 = 1.0e-9;
    const CROSS_EPS: f64 = 1.0e-7;
    const INTERIOR_EPS: f64 = 1.0e-6;

    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            let a = &segs[i];
            let b = &segs[j];

            // Ignore same-wire neighboring segments and endpoint junctions.
            if a.tag == b.tag {
                continue;
            }
            if segments_share_endpoint(a, b, TOUCH_EPS) {
                continue;
            }

            let (dist, s, t) = segment_closest_distance_and_params(a.start, a.end, b.start, b.end);
            let a_interior = s > INTERIOR_EPS && s < 1.0 - INTERIOR_EPS;
            let b_interior = t > INTERIOR_EPS && t < 1.0 - INTERIOR_EPS;
            if dist <= CROSS_EPS && a_interior && b_interior {
                return Some(format!(
                    "unsupported intersecting-wire geometry between tag {} seg {} and tag {} seg {}; only endpoint junctions are currently supported",
                    a.tag, a.tag_index, b.tag, b.tag_index
                ));
            }
        }
    }

    None
}

pub(super) fn source_risk_geometry_error(
    cards: &[Card],
    segs: &[nec_solver::Segment],
) -> Option<String> {
    const MIN_SOURCE_LENGTH_TO_RADIUS_RATIO: f64 = 2.0;

    for card in cards {
        if let Card::Ex(ex) = card {
            let Some(seg) = segs
                .iter()
                .find(|s| s.tag == ex.tag && s.tag_index == ex.segment)
            else {
                continue;
            };

            if seg.radius <= 0.0 {
                continue;
            }

            let length_to_radius = seg.length / seg.radius;
            if length_to_radius < MIN_SOURCE_LENGTH_TO_RADIUS_RATIO {
                return Some(format!(
                    "unsupported source-risk geometry: EX on tiny segment tag {} seg {} (length={:.6e} m, radius={:.6e} m, L/r={:.3}). Increase segment length or reduce wire radius; tiny-loop/source-risk classes are deferred",
                    ex.tag,
                    ex.segment,
                    seg.length,
                    seg.radius,
                    length_to_radius,
                ));
            }
        }
    }

    None
}

pub(super) fn buried_wire_geometry_error(
    segs: &[nec_solver::Segment],
    ground: &GroundModel,
) -> Option<String> {
    const BURIED_Z_EPS: f64 = 1.0e-9;

    // PH2-CHK-002 guardrail: buried-wire handling is not yet supported for
    // active image/finite-ground paths. Keep deferred/free-space behavior
    // unchanged so existing deferred contracts remain stable.
    if !matches!(
        ground,
        GroundModel::PerfectConductor | GroundModel::SimpleFiniteGround { .. }
    ) {
        return None;
    }

    for seg in segs {
        if seg.start[2] < -BURIED_Z_EPS || seg.end[2] < -BURIED_Z_EPS {
            return Some(format!(
                "unsupported buried-wire geometry for active ground model on tag {} seg {} (z < 0). Use free-space or move geometry to z >= 0; buried/near-ground classes are deferred",
                seg.tag, seg.tag_index
            ));
        }
    }

    None
}

fn segments_share_endpoint(a: &nec_solver::Segment, b: &nec_solver::Segment, eps: f64) -> bool {
    points_close(a.start, b.start, eps)
        || points_close(a.start, b.end, eps)
        || points_close(a.end, b.start, eps)
        || points_close(a.end, b.end, eps)
}

fn segment_closest_distance_and_params(
    p1: [f64; 3],
    q1: [f64; 3],
    p2: [f64; 3],
    q2: [f64; 3],
) -> (f64, f64, f64) {
    const SMALL_NUM: f64 = 1.0e-12;

    let u = [q1[0] - p1[0], q1[1] - p1[1], q1[2] - p1[2]];
    let v = [q2[0] - p2[0], q2[1] - p2[1], q2[2] - p2[2]];
    let w = [p1[0] - p2[0], p1[1] - p2[1], p1[2] - p2[2]];

    let a = dot3(u, u);
    let b = dot3(u, v);
    let c = dot3(v, v);
    let d = dot3(u, w);
    let e = dot3(v, w);
    let mut s_d = a * c - b * b;
    let mut t_d = s_d;

    let mut s_n;
    let mut t_n;

    if s_d < SMALL_NUM {
        s_n = 0.0;
        s_d = 1.0;
        t_n = e;
        t_d = c;
    } else {
        s_n = b * e - c * d;
        t_n = a * e - b * d;

        if s_n < 0.0 {
            s_n = 0.0;
            t_n = e;
            t_d = c;
        } else if s_n > s_d {
            s_n = s_d;
            t_n = e + b;
            t_d = c;
        }
    }

    if t_n < 0.0 {
        t_n = 0.0;
        if -d < 0.0 {
            s_n = 0.0;
        } else if -d > a {
            s_n = s_d;
        } else {
            s_n = -d;
            s_d = a;
        }
    } else if t_n > t_d {
        t_n = t_d;
        if -d + b < 0.0 {
            s_n = 0.0;
        } else if -d + b > a {
            s_n = s_d;
        } else {
            s_n = -d + b;
            s_d = a;
        }
    }

    let s_c = if s_n.abs() < SMALL_NUM {
        0.0
    } else {
        s_n / s_d
    };
    let t_c = if t_n.abs() < SMALL_NUM {
        0.0
    } else {
        t_n / t_d
    };

    let dx = w[0] + s_c * u[0] - t_c * v[0];
    let dy = w[1] + s_c * u[1] - t_c * v[1];
    let dz = w[2] + s_c * u[2] - t_c * v[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    (dist, s_c, t_c)
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn find_or_insert_node(
    nodes: &mut Vec<[f64; 3]>,
    degree: &mut Vec<usize>,
    p: [f64; 3],
    eps: f64,
) -> usize {
    for (idx, node) in nodes.iter().enumerate() {
        if points_close(*node, p, eps) {
            return idx;
        }
    }
    nodes.push(p);
    degree.push(0);
    nodes.len() - 1
}
