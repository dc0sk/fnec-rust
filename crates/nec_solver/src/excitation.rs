// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Excitation vector builder.
//!
//! Converts `EX` cards from the parsed deck into a complex right-hand-side
//! vector V, where V[i] is the impressed voltage on segment i (0 elsewhere).
//!
//! Only excitation type 0 (series voltage source) is implemented in Phase 1.

use num_complex::Complex64;

use nec_model::card::{Card, ExCard};
use nec_model::deck::NecDeck;

use crate::geometry::Segment;

const C0: f64 = 299_792_458.0; // m/s
const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7; // H/m
const ETA0: f64 = MU0 * C0; // free-space wave impedance

/// Right-hand side data for Hallén's integral equation.
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
    UnsupportedType { ex_type: u32 },
}

impl std::fmt::Display for ExcitationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExcitationError::SegmentNotFound { tag, segment } => {
                write!(f, "EX: no segment with tag {tag}, index {segment}")
            }
            ExcitationError::UnsupportedType { ex_type } => {
                write!(f, "EX: excitation type {ex_type} is not yet supported")
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
        apply_ex(ex, segs, &mut v)?;
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
    v.iter().map(|vi| *vi / lambda).collect()
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
    let mut first_ex: Option<&ExCard> = None;
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if ex.excitation_type != 0 {
            return Err(ExcitationError::UnsupportedType {
                ex_type: ex.excitation_type,
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
    let feed_dir = segs[feed_idx].direction;
    let feed_mid = segs[feed_idx].midpoint;
    let scale = 1.0 / ETA0;

    let mut rhs = vec![Complex64::new(0.0, 0.0); segs.len()];
    let mut cos_vec = vec![0.0; segs.len()];
    for (m, seg) in segs.iter().enumerate() {
        let d = [
            seg.midpoint[0] - feed_mid[0],
            seg.midpoint[1] - feed_mid[1],
            seg.midpoint[2] - feed_mid[2],
        ];
        let s = d[0] * feed_dir[0] + d[1] * feed_dir[1] + d[2] * feed_dir[2];
        rhs[m] = Complex64::new(0.0, -scale * (k * s.abs()).sin()) * v_source;
        cos_vec[m] = (k * s).cos();
    }

    Ok(HallenRhs { rhs, cos_vec })
}

fn apply_ex(ex: &ExCard, segs: &[Segment], v: &mut [Complex64]) -> Result<(), ExcitationError> {
    if ex.excitation_type != 0 {
        return Err(ExcitationError::UnsupportedType {
            ex_type: ex.excitation_type,
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
            voltage_real: 1.0,
            voltage_imag: 0.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert!(matches!(
            build_excitation(&deck, &segs),
            Err(ExcitationError::UnsupportedType { ex_type: 5 })
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
}
