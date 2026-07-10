// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Pure line-mesh construction for the 3-D viewport (GUI-CHK-002).
//!
//! All functions here are display-free and unit-tested headlessly: the solved
//! geometry becomes a flat list of colored line vertices (wires + coordinate
//! axes + an optional ground grid), which the wgpu viewport uploads as a single
//! `LineList` buffer. Kept in the library crate so the CI gates can cover it.

use bytemuck::{Pod, Zeroable};

/// One vertex of a colored line segment (GPU vertex-buffer layout).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

impl LineVertex {
    fn new(pos: [f32; 3], color: [f32; 4]) -> Self {
        Self { pos, color }
    }
}

/// The solved wire geometry handed from the solver thread to the viewport: wire
/// segment endpoints (metres) plus the axis-aligned bounding box and whether a
/// ground plane is present. Cheap to clone; `Send` for `Task::perform`.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneGeometry {
    /// Each wire segment as `(start, end)` in metres.
    pub wires: Vec<([f32; 3], [f32; 3])>,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    /// True when the deck defines a ground plane (draw the z=0 grid).
    pub has_ground: bool,
}

impl SceneGeometry {
    /// Build from solver `(start, end)` segment endpoints, computing the bbox.
    pub fn from_segments(wires: Vec<([f32; 3], [f32; 3])>, has_ground: bool) -> Self {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for (a, b) in &wires {
            for p in [a, b] {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        if wires.is_empty() {
            lo = [-1.0; 3];
            hi = [1.0; 3];
        }
        Self {
            wires,
            bbox_min: lo,
            bbox_max: hi,
            has_ground,
        }
    }

    /// Bounding-box centre and half-diagonal ("radius"), used for camera fit.
    pub fn bounds(&self) -> ([f32; 3], f32) {
        let c = [
            0.5 * (self.bbox_min[0] + self.bbox_max[0]),
            0.5 * (self.bbox_min[1] + self.bbox_max[1]),
            0.5 * (self.bbox_min[2] + self.bbox_max[2]),
        ];
        let d = [
            self.bbox_max[0] - self.bbox_min[0],
            self.bbox_max[1] - self.bbox_min[1],
            self.bbox_max[2] - self.bbox_min[2],
        ];
        let r = 0.5 * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        (c, r.max(0.1))
    }
}

/// A built list of line vertices (2 per segment), ready for a `LineList` draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<LineVertex>,
}

impl MeshData {
    pub fn segment_count(&self) -> usize {
        self.vertices.len() / 2
    }
}

const WIRE_COLOR: [f32; 4] = [0.90, 0.82, 0.24, 1.0];
const GRID_COLOR: [f32; 4] = [0.30, 0.32, 0.36, 1.0];
const AXIS_X: [f32; 4] = [0.85, 0.25, 0.25, 1.0];
const AXIS_Y: [f32; 4] = [0.30, 0.75, 0.35, 1.0];
const AXIS_Z: [f32; 4] = [0.35, 0.55, 0.95, 1.0];

/// Solved geometry plus per-segment current magnitudes (mA), aligned to
/// `geometry.wires` — the payload for the current-coloring path (GUI-CHK-004).
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryCurrents {
    pub geometry: SceneGeometry,
    pub currents_ma: Vec<f32>,
}

/// Assemble the full scene mesh with uniform wire color.
pub fn build_scene(geo: &SceneGeometry) -> MeshData {
    build_scene_colored(geo, None)
}

/// Assemble the scene mesh, optionally coloring each wire segment by its current
/// magnitude (per-segment `currents_ma`, aligned to `geo.wires`). Magnitudes are
/// normalized by the maximum, so the feedpoint (peak `|I|`) is hot and the tips
/// (near-zero) are cold. Grid/axes precede the wires, whose vertices stay at the
/// stable base (see [`wire_vertex_base`]).
pub fn build_scene_colored(geo: &SceneGeometry, currents_ma: Option<&[f32]>) -> MeshData {
    let (center, radius) = geo.bounds();
    let mut v = Vec::new();

    if geo.has_ground {
        push_ground_grid(&mut v, center, radius);
    }
    push_axes(&mut v, radius);

    let peak = currents_ma
        .map(|m| m.iter().copied().fold(0.0_f32, f32::max))
        .filter(|&p| p > 0.0);
    for (i, (a, b)) in geo.wires.iter().enumerate() {
        let color = match (currents_ma, peak) {
            (Some(mags), Some(pk)) => colormap(mags.get(i).copied().unwrap_or(0.0) / pk),
            _ => WIRE_COLOR,
        };
        v.push(LineVertex::new(*a, color));
        v.push(LineVertex::new(*b, color));
    }
    MeshData { vertices: v }
}

/// A perceptual-ish "cool → hot" colormap for a normalized value `t ∈ [0,1]`
/// (dark blue → cyan → green → yellow → red). Pure and unit-tested.
pub fn colormap(t: f32) -> [f32; 4] {
    const STOPS: [[f32; 3]; 5] = [
        [0.10, 0.15, 0.45], // 0.00  dark blue
        [0.10, 0.55, 0.75], // 0.25  cyan
        [0.20, 0.70, 0.30], // 0.50  green
        [0.90, 0.75, 0.15], // 0.75  yellow
        [0.85, 0.20, 0.15], // 1.00  red
    ];
    let t = t.clamp(0.0, 1.0) * 4.0;
    let i = (t.floor() as usize).min(3);
    let f = t - i as f32;
    let (lo, hi) = (STOPS[i], STOPS[i + 1]);
    [
        lo[0] + (hi[0] - lo[0]) * f,
        lo[1] + (hi[1] - lo[1]) * f,
        lo[2] + (hi[2] - lo[2]) * f,
        1.0,
    ]
}

/// Index of the first wire vertex in a mesh built by [`build_scene`], so callers
/// can recolor wires in place without rebuilding the grid/axes.
pub fn wire_vertex_base(geo: &SceneGeometry) -> usize {
    let (_, radius) = geo.bounds();
    let grid = if geo.has_ground {
        ground_grid_vertex_count(center_extent(radius))
    } else {
        0
    };
    grid + AXIS_VERTS
}

const AXIS_VERTS: usize = 6; // 3 axes × 2 vertices

fn push_axes(v: &mut Vec<LineVertex>, radius: f32) {
    let l = radius * 0.6;
    v.push(LineVertex::new([0.0, 0.0, 0.0], AXIS_X));
    v.push(LineVertex::new([l, 0.0, 0.0], AXIS_X));
    v.push(LineVertex::new([0.0, 0.0, 0.0], AXIS_Y));
    v.push(LineVertex::new([0.0, l, 0.0], AXIS_Y));
    v.push(LineVertex::new([0.0, 0.0, 0.0], AXIS_Z));
    v.push(LineVertex::new([0.0, 0.0, l], AXIS_Z));
}

/// Grid extent (half-width) and step, chosen to comfortably frame the geometry.
fn center_extent(radius: f32) -> (f32, f32) {
    let extent = (radius * 1.5).max(1.0);
    // ~10 divisions per side, rounded to a "nice" step.
    let raw = extent / 10.0;
    let mag = 10f32.powf(raw.log10().floor());
    let step = (raw / mag).ceil() * mag;
    (extent, step)
}

fn ground_grid_vertex_count((extent, step): (f32, f32)) -> usize {
    let n = (extent / step).floor() as i32;
    // lines from -n..=n in each of two directions, 2 vertices each.
    (((2 * n + 1) * 2) as usize) * 2
}

fn push_ground_grid(v: &mut Vec<LineVertex>, center: [f32; 3], radius: f32) {
    let (extent, step) = center_extent(radius);
    let (cx, cy) = (center[0], center[1]);
    let n = (extent / step).floor() as i32;
    for i in -n..=n {
        let off = i as f32 * step;
        // Lines parallel to x (vary y).
        v.push(LineVertex::new([cx - extent, cy + off, 0.0], GRID_COLOR));
        v.push(LineVertex::new([cx + extent, cy + off, 0.0], GRID_COLOR));
        // Lines parallel to y (vary x).
        v.push(LineVertex::new([cx + off, cy - extent, 0.0], GRID_COLOR));
        v.push(LineVertex::new([cx + off, cy + extent, 0.0], GRID_COLOR));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri_deck_geo() -> SceneGeometry {
        SceneGeometry::from_segments(
            vec![
                ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
                ([1.0, 0.0, 0.0], [1.0, 2.0, 0.0]),
                ([1.0, 2.0, 0.0], [0.0, 0.0, 3.0]),
            ],
            false,
        )
    }

    #[test]
    fn bbox_spans_all_endpoints() {
        let g = tri_deck_geo();
        assert_eq!(g.bbox_min, [0.0, 0.0, 0.0]);
        assert_eq!(g.bbox_max, [1.0, 2.0, 3.0]);
        let (c, r) = g.bounds();
        assert_eq!(c, [0.5, 1.0, 1.5]);
        assert!((r - 0.5 * (1.0f32 + 4.0 + 9.0).sqrt()).abs() < 1e-5);
    }

    #[test]
    fn scene_has_two_vertices_per_wire_plus_axes() {
        let g = tri_deck_geo();
        let m = build_scene(&g);
        // 3 wires × 2 + 3 axes × 2, no grid (free space).
        assert_eq!(m.vertices.len(), 3 * 2 + AXIS_VERTS);
        // Wire vertices are last and match the endpoints.
        let base = wire_vertex_base(&g);
        assert_eq!(base, AXIS_VERTS);
        assert_eq!(m.vertices[base].pos, [0.0, 0.0, 0.0]);
        assert_eq!(m.vertices[base + 1].pos, [1.0, 0.0, 0.0]);
        assert_eq!(m.vertices[base + 5].pos, [0.0, 0.0, 3.0]);
    }

    #[test]
    fn ground_adds_grid_before_wires() {
        let g = SceneGeometry::from_segments(vec![([0.0, 0.0, 1.0], [0.0, 0.0, 3.0])], true);
        let m = build_scene(&g);
        let base = wire_vertex_base(&g);
        // Grid present → base is past the axes.
        assert!(base > AXIS_VERTS, "grid should precede axes+wires");
        // Grid vertex count is even (line pairs) and matches the helper.
        assert_eq!(m.vertices.len(), base + 2);
        // The last two vertices are the single wire.
        assert_eq!(m.vertices[base].pos, [0.0, 0.0, 1.0]);
        assert_eq!(m.vertices[base + 1].pos, [0.0, 0.0, 3.0]);
    }

    #[test]
    fn empty_geometry_has_unit_bbox() {
        let g = SceneGeometry::from_segments(vec![], false);
        assert_eq!(g.bbox_min, [-1.0, -1.0, -1.0]);
        assert_eq!(g.bbox_max, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn colormap_hits_its_stops_and_clamps() {
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(close(colormap(0.0), [0.10, 0.15, 0.45, 1.0]));
        assert!(close(colormap(0.5), [0.20, 0.70, 0.30, 1.0]));
        assert!(close(colormap(1.0), [0.85, 0.20, 0.15, 1.0]));
        // Out-of-range clamps to the endpoints.
        assert_eq!(colormap(-1.0), colormap(0.0));
        assert_eq!(colormap(2.0), colormap(1.0));
        // Midpoint of the first segment interpolates.
        let q = colormap(0.125);
        assert!((q[1] - 0.35).abs() < 1e-5, "cyan channel lerps");
    }

    #[test]
    fn current_coloring_paints_peak_hot_and_zero_cold() {
        // Two collinear segments, currents 0 and 10 mA → tip cold, peak hot.
        let g = SceneGeometry::from_segments(
            vec![
                ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
                ([0.0, 0.0, 1.0], [0.0, 0.0, 2.0]),
            ],
            false,
        );
        let m = build_scene_colored(&g, Some(&[0.0, 10.0]));
        let base = wire_vertex_base(&g);
        // First segment (mag 0) → colormap(0); second (mag 10 = peak) → colormap(1).
        assert_eq!(m.vertices[base].color, colormap(0.0));
        assert_eq!(m.vertices[base + 1].color, colormap(0.0));
        assert_eq!(m.vertices[base + 2].color, colormap(1.0));
        assert_eq!(m.vertices[base + 3].color, colormap(1.0));
    }

    #[test]
    fn no_currents_uses_uniform_wire_color() {
        let g = SceneGeometry::from_segments(vec![([0.0; 3], [1.0, 0.0, 0.0])], false);
        let m = build_scene_colored(&g, None);
        let base = wire_vertex_base(&g);
        assert_eq!(m.vertices[base].color, WIRE_COLOR);
        // All-zero currents also fall back to uniform (no valid peak).
        let m0 = build_scene_colored(&g, Some(&[0.0]));
        assert_eq!(m0.vertices[base].color, WIRE_COLOR);
    }
}
