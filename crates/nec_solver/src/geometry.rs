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

use nec_model::card::{Card, GeCard, GmCard, GnCard, GrCard, GwCard};
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
    /// GN card was present, but the requested ground type is not implemented.
    ///
    /// The current Phase-1 behavior is to fall back to free-space while
    /// emitting a runtime warning in the CLI.
    Deferred { gn_type: i32 },
}

/// Extract the ground model from a parsed deck.
///
/// Resolution order:
/// 1. If a `GN` card is present it takes priority.
///    - `GN 1`  → [`GroundModel::PerfectConductor`].
///    - Other GN types → [`GroundModel::Deferred`].
/// 2. If no `GN` card is present but a `GE` card has `ground_reflection_flag > 0`
///    (the standard NEC flag for "enable image-method PEC ground at z = 0"),
///    infer [`GroundModel::PerfectConductor`].
/// 3. Otherwise returns [`GroundModel::FreeSpace`].
pub fn ground_model_from_deck(deck: &NecDeck) -> GroundModel {
    // GN card takes priority.
    for card in &deck.cards {
        if let Card::Gn(GnCard { ground_type }) = card {
            return match ground_type {
                1 => GroundModel::PerfectConductor,
                other => GroundModel::Deferred { gn_type: *other },
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

/// Build the flat segment list from all `GW`, `GM`, and `GR` cards in `deck`.
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
        deck.cards.push(Card::Gn(GnCard { ground_type: 1 }));
        assert_eq!(ground_model_from_deck(&deck), GroundModel::PerfectConductor);
    }

    #[test]
    fn ground_model_marks_gn0_as_deferred() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard { ground_type: 0 }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::Deferred { gn_type: 0 }
        );
    }

    #[test]
    fn ground_model_marks_gn2_as_deferred() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard { ground_type: 2 }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::Deferred { gn_type: 2 }
        );
    }

    // -------------------------------------------------------------------------
    // GM / GR tests
    // -------------------------------------------------------------------------

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
