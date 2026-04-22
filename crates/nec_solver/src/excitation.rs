// SPDX-License-Identifier: GPL-2.0-only
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

    v[idx] += Complex64::new(ex.voltage_real, ex.voltage_imag);
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
        for (i, vi) in v.iter().enumerate() {
            if i == 5 {
                assert_eq!(
                    *vi,
                    Complex64::new(1.0, 0.0),
                    "segment 6 should have V=1+0j"
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
        assert_eq!(v[1], Complex64::new(0.5, -0.5));
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
}
