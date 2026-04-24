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

use nec_model::card::{Card, GnCard, GwCard};
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
}

/// Extract the ground model from a parsed deck.
///
/// Uses the first `GN` card found.  Returns [`GroundModel::FreeSpace`] if no
/// GN card is present.  GN types other than 1 (PEC) are also treated as
/// free-space in Phase 1 (Sommerfeld ground is deferred).
pub fn ground_model_from_deck(deck: &NecDeck) -> GroundModel {
    for card in &deck.cards {
        if let Card::Gn(GnCard { ground_type }) = card {
            return match ground_type {
                1 => GroundModel::PerfectConductor,
                _ => GroundModel::FreeSpace,
            };
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

/// Build the flat segment list from all `GW` cards in `deck`.
///
/// Segments are appended in deck order: all segments of the first GW card
/// come first, then the second, and so on.
pub fn build_geometry(deck: &NecDeck) -> Result<Vec<Segment>, GeometryError> {
    let mut segments: Vec<Segment> = Vec::new();

    for card in &deck.cards {
        let Card::Gw(gw) = card else { continue };
        expand_wire(gw, &mut segments)?;
    }

    if segments.is_empty() {
        return Err(GeometryError::NoWires);
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, GnCard, GwCard};
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
}
