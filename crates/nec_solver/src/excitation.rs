// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Excitation vector builder.
//!
//! Converts `EX` cards from the parsed deck into a complex right-hand-side
//! vector V, where V[i] is the impressed voltage on segment i (0 elsewhere).
//!
//! Only excitation type 0 (series voltage source) is implemented in Phase 1.

use num_complex::Complex64;
use std::collections::BTreeMap;

use nec_model::card::{Card, ExCard};
use nec_model::deck::NecDeck;

use crate::geometry::Segment;

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
/// For the **driven wire** (the one containing the first type-0 EX source):
/// - `s` is measured along the wire's axis with s=0 at the feed segment midpoint.
/// - `b_m = -j * (2π/η₀) * V_source * sin(k * |s_m|)`
/// - `cos_vec[m] = cos(k * s_m)`
///
/// For **non-driven wires** (no EX source on the wire):
/// - They are coupled only through the Z-matrix; no incident driving field.
/// - `b_m = 0`
/// - `cos_vec[m] = cos(k * s_local_m)` where `s_local` is measured along
///   that wire's own axis with s=0 at the wire's midpoint.
///
/// This formulation supports non-collinear and junctioned multi-wire geometries.
/// The cos_vec column in the Hallén augmented system is shared across all
/// segments; per-wire C constants are handled in `solve_hallen` via the
/// `wire_endpoints` argument (one C column per wire).
pub fn build_hallen_rhs(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
) -> Result<HallenRhs, ExcitationError> {
    let mut first_ex: Option<&ExCard> = None;
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if ex.excitation_type != 0 {
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
        });
    };

    let feed_idx = segs
        .iter()
        .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        .ok_or(ExcitationError::SegmentNotFound {
            tag: ex.tag,
            segment: ex.segment,
        })?;

    let v_source = Complex64::new(ex.voltage_real, ex.voltage_imag);
    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let scale = 2.0 * std::f64::consts::PI / ETA0;
    let driven_tag = segs[feed_idx].tag;

    // Build a map from wire tag → (feed_seg_idx, v_source) for every type-0 EX card.
    // This handles multi-source decks where multiple wires each have an excitation.
    let mut source_by_tag: BTreeMap<u32, (usize, Complex64)> =
        BTreeMap::from([(driven_tag, (feed_idx, v_source))]);
    for card in &deck.cards {
        let Card::Ex(ex2) = card else { continue };
        if ex2.excitation_type != 0 || ex2.tag == driven_tag {
            continue;
        }
        if let Some(idx) = segs
            .iter()
            .position(|s| s.tag == ex2.tag && s.tag_index == ex2.segment)
        {
            source_by_tag.insert(
                ex2.tag,
                (idx, Complex64::new(ex2.voltage_real, ex2.voltage_imag)),
            );
        }
    }

    // Group segment indices by tag to compute per-wire geometry.
    let mut wire_first_by_tag: BTreeMap<u32, usize> = BTreeMap::new();
    let mut wire_last_by_tag: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, seg) in segs.iter().enumerate() {
        wire_first_by_tag.entry(seg.tag).or_insert(i);
        wire_last_by_tag.insert(seg.tag, i);
    }

    let mut rhs = vec![Complex64::new(0.0, 0.0); segs.len()];
    let mut cos_vec = vec![0.0; segs.len()];

    for (m, seg) in segs.iter().enumerate() {
        let first = wire_first_by_tag[&seg.tag];
        let last = wire_last_by_tag[&seg.tag];
        let wire_dir = segs[first].direction;
        // Geometric midpoint of the wire (average of first and last segment midpoints).
        let wire_mid = [
            (segs[first].midpoint[0] + segs[last].midpoint[0]) / 2.0,
            (segs[first].midpoint[1] + segs[last].midpoint[1]) / 2.0,
            (segs[first].midpoint[2] + segs[last].midpoint[2]) / 2.0,
        ];
        // s_local: coordinate along the wire's own axis, s=0 at wire midpoint.
        let dl = [
            seg.midpoint[0] - wire_mid[0],
            seg.midpoint[1] - wire_mid[1],
            seg.midpoint[2] - wire_mid[2],
        ];
        let s_local = dl[0] * wire_dir[0] + dl[1] * wire_dir[1] + dl[2] * wire_dir[2];
        cos_vec[m] = (k * s_local).cos();

        if let Some(&(fi, vsrc)) = source_by_tag.get(&seg.tag) {
            // Driven wire: rhs uses s measured from this wire's source segment midpoint.
            let src_mid = segs[fi].midpoint;
            let src_dir = segs[fi].direction;
            let d = [
                seg.midpoint[0] - src_mid[0],
                seg.midpoint[1] - src_mid[1],
                seg.midpoint[2] - src_mid[2],
            ];
            let s = d[0] * src_dir[0] + d[1] * src_dir[1] + d[2] * src_dir[2];
            rhs[m] = Complex64::new(0.0, -scale * (k * s.abs()).sin()) * vsrc;
        }
        // else: non-driven wire — rhs[m] stays 0.0.
    }

    Ok(HallenRhs { rhs, cos_vec })
}

fn apply_ex(ex: &ExCard, segs: &[Segment], v: &mut [Complex64]) -> Result<(), ExcitationError> {
    if ex.excitation_type != 0 {
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
    fn hallen_rhs_accepts_non_collinear_topology() {
        // Non-collinear multi-wire geometries are now supported; build_hallen_rhs
        // should succeed and return per-wire local cos_vec values.
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
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();
        assert_eq!(h.rhs.len(), segs.len());
        assert_eq!(h.cos_vec.len(), segs.len());
        // Non-driven segments should have zero RHS.
        for (idx, seg) in segs.iter().enumerate() {
            if seg.tag != 1 {
                assert_eq!(h.rhs[idx].re, 0.0);
                assert_eq!(h.rhs[idx].im, 0.0);
            }
        }
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
    }
}
