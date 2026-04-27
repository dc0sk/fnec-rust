// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Excitation vector builder.
//!
//! Converts `EX` cards from the parsed deck into a complex right-hand-side
//! vector V, where V[i] is the impressed voltage on segment i (0 elsewhere).
//!
//! Excitation types implemented in Phase 1:
//! - type 0: series voltage source
//! - type 3: normalized voltage source (currently treated as type 0)

use num_complex::Complex64;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use nec_model::card::{Card, ExCard};
use nec_model::deck::NecDeck;

use crate::geometry::{wire_endpoints_from_segs, Segment};

const C0: f64 = 299_792_458.0; // m/s
const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7; // H/m
const ETA0: f64 = MU0 * C0; // free-space wave impedance

/// Right-hand side data for Hallén's integral equation.
#[derive(Debug)]
pub struct HallenRhs {
    /// Hallén RHS vector b.
    pub rhs: Vec<Complex64>,
    /// cos(k·s_m) samples for the homogeneous-term column.
    pub cos_vec: Vec<f64>,
    /// Per-wire endpoint indices: (first_seg_idx, last_seg_idx) for each wire.
    ///
    /// Used by the solver to enforce zero tip-current at each wire end.
    /// Derived from the geometry; empty only when there are no segments.
    pub wire_endpoints: Vec<(usize, usize)>,
}

/// Error from the excitation builder.
#[derive(Debug, Clone, PartialEq)]
pub enum ExcitationError {
    /// An EX card referenced a (tag, segment) pair not present in the geometry.
    SegmentNotFound { tag: u32, segment: u32 },
    /// An EX card uses an excitation type not yet supported.
    UnsupportedType {
        ex_type: u32,
        tag: u32,
        segment: u32,
        i4: u32,
    },
    /// Hallen RHS currently assumes all wires are collinear with the feed axis.
    UnsupportedHallenTopology {
        /// Wire tags that are not collinear with the feed segment axis.
        non_collinear_tags: Vec<u32>,
        /// Absolute cosine alignment per non-collinear tag.
        ///
        /// 1.0 means collinear, 0.0 means orthogonal.
        tag_abs_alignment_cos: Vec<(u32, f64)>,
    },
    /// Two or more EX 0 cards target the same wire tag.
    ///
    /// The Hallén path uses per-tag source data and cannot correctly represent
    /// multiple feed points on the same tag.  Use distinct tags for each source.
    DuplicateSourceTag { tag: u32 },
}

impl std::fmt::Display for ExcitationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExcitationError::SegmentNotFound { tag, segment } => {
                write!(f, "EX: no segment with tag {tag}, index {segment}")
            }
            ExcitationError::UnsupportedType {
                ex_type,
                tag,
                segment,
                i4,
            } => {
                write!(
                    f,
                    "EX: excitation type {ex_type} at tag {tag}, segment {segment}, I4={i4} is not yet supported"
                )
            }
            ExcitationError::UnsupportedHallenTopology {
                non_collinear_tags,
                tag_abs_alignment_cos,
            } => write!(
                f,
                "Hallén solver currently supports only collinear wire topologies aligned with the driven segment; non-collinear tags: {:?}; abs(cos)-alignment by tag: {:?}",
                non_collinear_tags,
                tag_abs_alignment_cos
            ),
            ExcitationError::DuplicateSourceTag { tag } => write!(
                f,
                "Hallén solver: two or more EX 0 cards target tag {tag}; use distinct tags for multiple feed points"
            ),
        }
    }
}

impl std::error::Error for ExcitationError {}

/// Build the complex excitation (RHS) vector V from `EX` cards in `deck`.
///
/// `segs` is the flat segment list produced by [`crate::geometry::build_geometry`].
/// The returned vector has length `segs.len()`.
pub fn build_excitation(
    deck: &NecDeck,
    segs: &[Segment],
) -> Result<Vec<Complex64>, ExcitationError> {
    let mut v = vec![Complex64::new(0.0, 0.0); segs.len()];

    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        apply_ex(&ex, segs, &mut v)?;
    }

    Ok(v)
}

/// Scale a wire-voltage excitation vector for NEC-2 style pulse EFIE solves.
///
/// NEC-2 applies impressed voltage sources as E = -V/(dl*lambda) in the
/// matrix RHS for wire equations. `build_excitation()` provides the V/dl part;
/// this helper applies the 1/lambda factor.
pub fn scale_excitation_for_pulse_rhs(v: &[Complex64], freq_hz: f64) -> Vec<Complex64> {
    let lambda = C0 / freq_hz;
    v.iter().map(|vi| -*vi / lambda).collect()
}

/// Build Hallén RHS data (b and cos(k·s)) for the current geometry.
///
/// This uses the first type-0 EX source as the feed reference. The coordinate
/// s is measured along the driven segment direction with s=0 at the feed
/// segment midpoint.
///
/// b_m = -j * (2π/η0) * V_source * sin(k * |s_m|)
/// cos_vec[m] = cos(k * s_m)
pub fn build_hallen_rhs(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
) -> Result<HallenRhs, ExcitationError> {
    build_hallen_rhs_with_options(deck, segs, freq_hz, false)
}

/// Build Hallén RHS data with optional non-collinear topology allowance.
///
/// When `allow_non_collinear` is `false` (default behavior), non-collinear
/// segment directions are rejected with [`ExcitationError::UnsupportedHallenTopology`].
/// When it is `true`, the RHS is still built using feed-axis projection and
/// should be treated as experimental for non-collinear decks.
pub fn build_hallen_rhs_with_options(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    allow_non_collinear: bool,
) -> Result<HallenRhs, ExcitationError> {
    let mut first_ex: Option<&ExCard> = None;
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if ex.excitation_type != 0 && ex.excitation_type != 3 {
            return Err(ExcitationError::UnsupportedType {
                ex_type: ex.excitation_type,
                tag: ex.tag,
                segment: ex.segment,
                i4: ex.i4,
            });
        }
        if first_ex.is_none() {
            first_ex = Some(ex);
        }
    }

    let Some(ex) = first_ex else {
        return Ok(HallenRhs {
            rhs: vec![Complex64::new(0.0, 0.0); segs.len()],
            cos_vec: vec![0.0; segs.len()],
            wire_endpoints: wire_endpoints_from_segs(segs),
        });
    };

    let feed_idx = segs
        .iter()
        .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        .ok_or(ExcitationError::SegmentNotFound {
            tag: ex.tag,
            segment: ex.segment,
        })?;

    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let feed_dir = segs[feed_idx].direction;
    let scale = 2.0 * std::f64::consts::PI / ETA0;

    let mut non_collinear_tags: BTreeSet<u32> = BTreeSet::new();
    let mut tag_abs_alignment_cos: BTreeMap<u32, f64> = BTreeMap::new();
    for seg in segs {
        let dot = seg.direction[0] * feed_dir[0]
            + seg.direction[1] * feed_dir[1]
            + seg.direction[2] * feed_dir[2];
        let abs_dot = dot.abs();
        if abs_dot < 1.0 - 1e-9 {
            non_collinear_tags.insert(seg.tag);
            // Keep the best alignment observed for the tag.
            tag_abs_alignment_cos
                .entry(seg.tag)
                .and_modify(|v| *v = v.max(abs_dot))
                .or_insert(abs_dot);
        }
    }
    if !allow_non_collinear && !non_collinear_tags.is_empty() {
        return Err(ExcitationError::UnsupportedHallenTopology {
            non_collinear_tags: non_collinear_tags.into_iter().collect(),
            tag_abs_alignment_cos: tag_abs_alignment_cos.into_iter().collect(),
        });
    }

    // Reject decks with two or more EX 0 cards on the same tag.
    // The Hallén path stores a single per-tag source; silently ignoring a
    // second feed on the same wire would produce an incorrect drive vector.
    {
        let mut seen_tags: BTreeSet<u32> = BTreeSet::new();
        for card in &deck.cards {
            let Card::Ex(ex) = card else { continue };
            if !seen_tags.insert(ex.tag) {
                return Err(ExcitationError::DuplicateSourceTag { tag: ex.tag });
            }
        }
    }

    // Collect per-tag source data for every driven wire (all type-0 EX cards).
    //
    // Map: tag → (feed_midpoint, feed_direction, source_voltage)
    let mut tag_sources: BTreeMap<u32, ([f64; 3], [f64; 3], Complex64)> = BTreeMap::new();
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if tag_sources.contains_key(&ex.tag) {
            continue; // unreachable after the check above, kept for safety
        }
        let seg_idx = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
            .ok_or(ExcitationError::SegmentNotFound {
                tag: ex.tag,
                segment: ex.segment,
            })?;
        tag_sources.insert(
            ex.tag,
            (
                segs[seg_idx].midpoint,
                segs[seg_idx].direction,
                Complex64::new(ex.voltage_real, ex.voltage_imag),
            ),
        );
    }

    // Build per-wire local coordinates for cos(k*s):
    // - driven wires are referenced to their own source segment midpoint
    // - passive wires are referenced to the wire centre midpoint
    let mut tag_axis: BTreeMap<u32, ([f64; 3], [f64; 3])> = BTreeMap::new();
    let mut i = 0usize;
    while i < segs.len() {
        let tag = segs[i].tag;
        let first = i;
        while i + 1 < segs.len() && segs[i + 1].tag == tag {
            i += 1;
        }
        let last = i;

        if let Some(&(src_mid, src_dir, _)) = tag_sources.get(&tag) {
            tag_axis.insert(tag, (src_mid, src_dir));
        } else {
            let a = segs[first].midpoint;
            let b = segs[last].midpoint;
            let center = [
                0.5 * (a[0] + b[0]),
                0.5 * (a[1] + b[1]),
                0.5 * (a[2] + b[2]),
            ];
            tag_axis.insert(tag, (center, segs[first].direction));
        }
        i += 1;
    }

    let mut rhs = vec![Complex64::new(0.0, 0.0); segs.len()];
    let mut cos_vec = vec![0.0; segs.len()];
    for (m, seg) in segs.iter().enumerate() {
        let (axis_origin, axis_dir) = tag_axis
            .get(&seg.tag)
            .copied()
            .unwrap_or((seg.midpoint, seg.direction));
        let ds_axis = [
            seg.midpoint[0] - axis_origin[0],
            seg.midpoint[1] - axis_origin[1],
            seg.midpoint[2] - axis_origin[2],
        ];
        let s_axis = ds_axis[0] * axis_dir[0] + ds_axis[1] * axis_dir[1] + ds_axis[2] * axis_dir[2];
        cos_vec[m] = (k * s_axis).cos();

        if let Some(&(src_mid, src_dir, src_v)) = tag_sources.get(&seg.tag) {
            // Driven wire: RHS uses arc-length from this wire's own feed point.
            let ds = [
                seg.midpoint[0] - src_mid[0],
                seg.midpoint[1] - src_mid[1],
                seg.midpoint[2] - src_mid[2],
            ];
            let s = ds[0] * src_dir[0] + ds[1] * src_dir[1] + ds[2] * src_dir[2];
            rhs[m] = Complex64::new(0.0, -scale * (k * s.abs()).sin()) * src_v;
        }
        // Passive wire: rhs[m] stays zero.
    }

    Ok(HallenRhs {
        rhs,
        cos_vec,
        wire_endpoints: wire_endpoints_from_segs(segs),
    })
}

fn apply_ex(ex: &ExCard, segs: &[Segment], v: &mut [Complex64]) -> Result<(), ExcitationError> {
    if ex.excitation_type != 0 && ex.excitation_type != 3 {
        return Err(ExcitationError::UnsupportedType {
            ex_type: ex.excitation_type,
            tag: ex.tag,
            segment: ex.segment,
            i4: ex.i4,
        });
    }

    // Find the segment by tag + tag_index.
    let idx = segs
        .iter()
        .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        .ok_or(ExcitationError::SegmentNotFound {
            tag: ex.tag,
            segment: ex.segment,
        })?;

    // The EFIE RHS has units of V/m (electric field).  A series voltage
    // source of voltage V over a segment of length Δl impresses a tangential
    // field E = V / Δl at the midpoint of that segment.
    let delta_l = segs[idx].length;
    v[idx] += Complex64::new(ex.voltage_real, ex.voltage_imag) / delta_l;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, ExCard, GwCard};
    use nec_model::deck::NecDeck;

    use crate::geometry::build_geometry;

    const TEST_FREQ_HZ: f64 = 14.2e6;

    fn dipole_deck() -> NecDeck {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 6, // centre segment (1-based)
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }));
        deck
    }

    #[test]
    fn voltage_placed_at_correct_segment() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let v = build_excitation(&deck, &segs).unwrap();

        assert_eq!(v.len(), 11);
        // Only segment index 5 (0-based) should be excited (tag_index 6).
        // The stored value is V/Δl (V/m), not raw voltage.
        let seg_len = segs[5].length;
        for (i, vi) in v.iter().enumerate() {
            if i == 5 {
                let expected = Complex64::new(1.0, 0.0) / seg_len;
                assert!(
                    (vi - expected).norm() < 1e-12,
                    "segment 6 should have V/Δl={expected}, got {vi}"
                );
            } else {
                assert_eq!(*vi, Complex64::new(0.0, 0.0), "segment {i} should be zero");
            }
        }
    }

    #[test]
    fn complex_voltage_is_stored() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 2,
            i4: 0,
            voltage_real: 0.5,
            voltage_imag: -0.5,
        }));
        let segs = build_geometry(&deck).unwrap();
        let v = build_excitation(&deck, &segs).unwrap();
        // Stored value is V/Δl.  Segment 1 spans from z=-1/3 to z=+1/3 → length 2/3.
        let seg_len = segs[1].length;
        let expected = Complex64::new(0.5, -0.5) / seg_len;
        assert!(
            (v[1] - expected).norm() < 1e-12,
            "expected {expected}, got {}",
            v[1]
        );
    }

    #[test]
    fn unknown_ex_type_is_error() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 5, // not supported
            tag: 1,
            segment: 2,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert!(matches!(
            build_excitation(&deck, &segs),
            Err(ExcitationError::UnsupportedType {
                ex_type: 5,
                tag: 1,
                segment: 2,
                i4: 0,
            })
        ));
    }

    #[test]
    fn ex_type3_is_currently_accepted_like_type0() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 3,
            tag: 1,
            segment: 2,
            i4: 0,
            voltage_real: 1.5,
            voltage_imag: -0.25,
        }));

        let segs = build_geometry(&deck).unwrap();
        let v = build_excitation(&deck, &segs).expect("EX type 3 should be accepted");
        let expected = Complex64::new(1.5, -0.25) / segs[1].length;
        assert!((v[1] - expected).norm() < 1e-12);
    }

    #[test]
    fn ex_type3_matches_ex_type0_vector() {
        let mut deck_ex0 = NecDeck::new();
        deck_ex0.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck_ex0.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 2,
            i4: 0,
            voltage_real: 0.8,
            voltage_imag: -0.3,
        }));

        let mut deck_ex3 = NecDeck::new();
        deck_ex3.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck_ex3.cards.push(Card::Ex(ExCard {
            excitation_type: 3,
            tag: 1,
            segment: 2,
            i4: 0,
            voltage_real: 0.8,
            voltage_imag: -0.3,
        }));

        let segs_ex0 = build_geometry(&deck_ex0).unwrap();
        let segs_ex3 = build_geometry(&deck_ex3).unwrap();
        let v_ex0 = build_excitation(&deck_ex0, &segs_ex0).expect("EX type 0 should be accepted");
        let v_ex3 = build_excitation(&deck_ex3, &segs_ex3).expect("EX type 3 should be accepted");

        assert_eq!(v_ex0.len(), v_ex3.len());
        for (i, (a, b)) in v_ex0.iter().zip(v_ex3.iter()).enumerate() {
            assert!(
                (*a - *b).norm() < 1e-12,
                "segment {i} mismatch: ex0={a}, ex3={b}"
            );
        }
    }

    #[test]
    fn segment_not_found_is_error() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 99, // no such tag
            segment: 1,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert!(matches!(
            build_excitation(&deck, &segs),
            Err(ExcitationError::SegmentNotFound { tag: 99, .. })
        ));
    }

    #[test]
    fn hallen_rhs_has_expected_shapes() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();
        assert_eq!(h.rhs.len(), segs.len());
        assert_eq!(h.cos_vec.len(), segs.len());
    }

    #[test]
    fn hallen_rhs_feedpoint_cos_is_one_and_rhs_is_zero() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();

        // EX is on segment 6 (1-based) => index 5
        let feed_idx = 5usize;
        assert!(
            (h.cos_vec[feed_idx] - 1.0).abs() < 1e-12,
            "cos(feed) expected 1, got {}",
            h.cos_vec[feed_idx]
        );
        assert!(
            h.rhs[feed_idx].norm() < 1e-12,
            "rhs(feed) expected ~0, got {}",
            h.rhs[feed_idx]
        );
    }

    #[test]
    fn hallen_rhs_uses_two_pi_over_eta0_scale() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();

        let sample_idx = 0usize;
        let scale = 2.0 * std::f64::consts::PI / ETA0;
        let k = 2.0 * std::f64::consts::PI * TEST_FREQ_HZ / C0;
        let feed_mid = segs[5].midpoint;
        let sample_mid = segs[sample_idx].midpoint;
        let s = sample_mid[2] - feed_mid[2];
        let expected = Complex64::new(0.0, -scale * (k * s.abs()).sin());

        assert!(
            (h.rhs[sample_idx] - expected).norm() < 1e-12,
            "expected {expected}, got {}",
            h.rhs[sample_idx]
        );
    }

    #[test]
    fn hallen_rhs_is_symmetric_for_symmetric_dipole() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();

        let n = segs.len();
        for i in 0..n {
            let j = n - 1 - i;
            assert!(
                (h.cos_vec[i] - h.cos_vec[j]).abs() < 1e-12,
                "cos symmetry mismatch at {i}/{j}: {} vs {}",
                h.cos_vec[i],
                h.cos_vec[j]
            );
            assert!(
                (h.rhs[i] - h.rhs[j]).norm() < 1e-12,
                "rhs symmetry mismatch at {i}/{j}: {} vs {}",
                h.rhs[i],
                h.rhs[j]
            );
        }
    }

    #[test]
    fn hallen_rhs_rejects_non_collinear_topology() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 9,
            start: [-0.25, 0.0, 2.677],
            end: [0.25, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 4,
            segments: 9,
            start: [0.25, 0.0, 2.677],
            end: [0.25, 0.0, 3.177],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 6,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }));

        let segs = build_geometry(&deck).unwrap();
        let err = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap_err();
        assert_eq!(
            err,
            ExcitationError::UnsupportedHallenTopology {
                non_collinear_tags: vec![2],
                tag_abs_alignment_cos: vec![(2, 0.0)],
            }
        );
    }

    #[test]
    fn hallen_rhs_allows_parallel_multi_wire_topology() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 11,
            start: [1.0, 0.0, -2.677],
            end: [1.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 6,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }));

        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();
        assert_eq!(h.rhs.len(), segs.len());
        assert_eq!(h.cos_vec.len(), segs.len());

        // Driven wire (tag 1) segments must have nonzero RHS; passive wire (tag 2) must be zero.
        let driven_segs: Vec<usize> = segs
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tag == 1)
            .map(|(i, _)| i)
            .collect();
        let passive_segs: Vec<usize> = segs
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tag == 2)
            .map(|(i, _)| i)
            .collect();

        // Non-feed driven segments must have nonzero RHS.
        let feed_idx_in_segs = segs
            .iter()
            .position(|s| s.tag == 1 && s.tag_index == 6)
            .unwrap();
        for &i in &driven_segs {
            if i != feed_idx_in_segs {
                assert!(
                    h.rhs[i].norm() > 1e-12,
                    "driven seg {i} expected nonzero rhs, got {}",
                    h.rhs[i]
                );
            }
        }
        // All passive wire segments must have zero RHS.
        for &i in &passive_segs {
            assert!(
                h.rhs[i].norm() < 1e-30,
                "passive seg {i} expected zero rhs, got {}",
                h.rhs[i]
            );
        }
    }

    #[test]
    fn hallen_rhs_rejects_bent_topology() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 9,
            start: [-0.25, 0.0, 2.677],
            end: [0.25, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 6,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }));

        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs_with_options(&deck, &segs, TEST_FREQ_HZ, true).unwrap();
        assert_eq!(h.rhs.len(), segs.len());
        assert_eq!(h.cos_vec.len(), segs.len());
    }

    /// Two EX 0 cards on the same tag must be rejected with DuplicateSourceTag.
    #[test]
    fn hallen_rhs_rejects_duplicate_source_tag() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        // Two EX cards referencing the same tag — should be rejected.
        for seg in [3u32, 9] {
            deck.cards.push(Card::Ex(ExCard {
                excitation_type: 0,
                tag: 1,
                segment: seg,
                i4: 0,
                voltage_real: 1.0,
                voltage_imag: 0.0,
            }));
        }
        let segs = build_geometry(&deck).unwrap();
        let result = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ);
        assert!(
            matches!(result, Err(ExcitationError::DuplicateSourceTag { tag: 1 })),
            "expected DuplicateSourceTag error, got: {result:?}"
        );
    }
}
