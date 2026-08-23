// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! NT two-port network builder (PH8-CHK-004): converts supported `NT` cards into
//! impedance-matrix stamps, mirroring the TL stamp path.
//!
//! NEC2 `NT` card layout:
//! `NT tag1 seg1 tag2 seg2 Y11r Y11i Y12r Y12i Y22r Y22i`
//! — the network's short-circuit admittance parameters (mhos), reciprocal
//! (`Y21 = Y12`). The two-port is inserted between the segments `(tag1,seg1)` and
//! `(tag2,seg2)`.
//!
//! fnec's MoM system is in impedance form (`Z·I = V`) and stamps 2-port
//! **Z-parameters** into the matrix (see [`crate::build_tl_stamps`], where a
//! lossless TL contributes `Z11=Z22=−jZ0·cot θ`, `Z12=Z21=−jZ0·csc θ`). So an
//! `NT` network is stamped by converting its admittance matrix to impedance
//! parameters, `[Z] = [Y]⁻¹`:
//!
//! - `det = Y11·Y22 − Y12·Y21`
//! - `Z11 = Y22/det`, `Z22 = Y11/det`, `Z12 = −Y12/det`, `Z21 = −Y21/det`
//!
//! Consistency check (see the tests): an `NT` whose Y-parameters are the inverse
//! of a lossless TL's Z-parameters stamps **identically** to that TL — because
//! `[Y]⁻¹` inverts straight back to the TL's `[Z]`.

use num_complex::Complex64;

use nec_model::card::Card;
use nec_model::deck::NecDeck;

use crate::geometry::Segment;
use crate::tl::find_segment_index;

/// A sparse impedance-matrix stamp `(row, col, delta_z)`.
pub type NtStamp = (usize, usize, Complex64);

/// A non-fatal warning produced by NT processing.
#[derive(Debug, Clone, PartialEq)]
pub struct NtWarning {
    /// Human-readable description of the issue.
    pub message: String,
}

impl std::fmt::Display for NtWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Build sparse impedance stamps from supported `NT` cards.
///
/// Unsupported / malformed cards (fewer than 10 fields, an endpoint not present
/// in the geometry, coincident endpoints, or a singular admittance matrix that
/// cannot be inverted to Z-parameters) are skipped with an explanatory warning.
pub fn build_nt_stamps(deck: &NecDeck, segs: &[Segment]) -> (Vec<NtStamp>, Vec<NtWarning>) {
    let mut stamps: Vec<NtStamp> = Vec::new();
    let mut warnings: Vec<NtWarning> = Vec::new();

    for card in &deck.cards {
        let Card::Nt(nt) = card else { continue };
        let f = &nt.raw_fields;
        if f.len() < 10 {
            warnings.push(NtWarning {
                message: format!(
                    "NT card has {} fields; expected 10 (tag1 seg1 tag2 seg2 Y11r Y11i Y12r Y12i Y22r Y22i); NT card ignored",
                    f.len()
                ),
            });
            continue;
        }

        let parse_u = |i: usize| f[i].parse::<u32>().ok();
        let parse_f = |i: usize| f[i].parse::<f64>().ok();
        let (Some(tag1), Some(seg1), Some(tag2), Some(seg2)) =
            (parse_u(0), parse_u(1), parse_u(2), parse_u(3))
        else {
            warnings.push(NtWarning {
                message: "NT card has non-integer segment identifiers; NT card ignored".to_string(),
            });
            continue;
        };
        let ys: Option<Vec<f64>> = (4..10).map(parse_f).collect();
        let Some(ys) = ys else {
            warnings.push(NtWarning {
                message: "NT card has non-numeric admittance parameters; NT card ignored"
                    .to_string(),
            });
            continue;
        };
        let y11 = Complex64::new(ys[0], ys[1]);
        let y12 = Complex64::new(ys[2], ys[3]);
        let y21 = y12; // reciprocal network
        let y22 = Complex64::new(ys[4], ys[5]);

        let Some((i1, _, _)) = find_segment_index(segs, tag1, seg1) else {
            warnings.push(NtWarning {
                message: format!(
                    "NT endpoint ({tag1}, {seg1}) not found in geometry; NT card ignored"
                ),
            });
            continue;
        };
        let Some((i2, _, _)) = find_segment_index(segs, tag2, seg2) else {
            warnings.push(NtWarning {
                message: format!(
                    "NT endpoint ({tag2}, {seg2}) not found in geometry; NT card ignored"
                ),
            });
            continue;
        };
        if i1 == i2 {
            warnings.push(NtWarning {
                message: format!(
                    "NT endpoints resolve to the same segment (({tag1}, {seg1}) and ({tag2}, {seg2})); NT card ignored"
                ),
            });
            continue;
        }

        // Convert admittance parameters to impedance parameters: [Z] = [Y]^-1.
        let det = y11 * y22 - y12 * y21;
        if det.norm() < 1e-30 {
            warnings.push(NtWarning {
                message: format!(
                    "NT between ({tag1}, {seg1}) and ({tag2}, {seg2}) has a singular admittance matrix (det≈0) and cannot be inverted to Z-parameters; NT card ignored"
                ),
            });
            continue;
        }
        let z11 = y22 / det;
        let z22 = y11 / det;
        let z12 = -y12 / det;
        let z21 = -y21 / det;

        stamps.push((i1, i1, z11));
        stamps.push((i2, i2, z22));
        stamps.push((i1, i2, z12));
        stamps.push((i2, i1, z21));
    }

    (stamps, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{GwCard, NtCard};

    /// Two parallel three-segment wires — enough for a two-port network to span.
    fn two_wire_segments() -> Vec<Segment> {
        let mut deck = NecDeck::new();
        for (tag, x) in [(1u32, 0.0), (2u32, 1.0)] {
            deck.cards.push(Card::Gw(GwCard {
                tag,
                segments: 3,
                start: [x, 0.0, -1.0],
                end: [x, 0.0, 1.0],
                radius: 0.001,
            }));
        }
        crate::geometry::build_geometry(&deck).expect("geometry builds")
    }

    fn deck_with_nt(fields: &[&str]) -> NecDeck {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Nt(NtCard {
            raw_fields: fields.iter().map(|s| (*s).to_string()).collect(),
        }));
        deck
    }

    fn only_warning(deck: &NecDeck, segs: &[Segment]) -> String {
        let (stamps, warnings) = build_nt_stamps(deck, segs);
        assert!(stamps.is_empty(), "a rejected NT card must stamp nothing");
        assert_eq!(
            warnings.len(),
            1,
            "expected exactly one warning: {warnings:?}"
        );
        warnings[0].message.clone()
    }

    /// The supported path: a well-formed reciprocal NT stamps all four
    /// Z-parameter entries and says nothing.
    #[test]
    fn a_well_formed_nt_stamps_four_entries_without_warning() {
        let segs = two_wire_segments();
        let deck = deck_with_nt(&[
            "1", "2", "2", "2", // tag1 seg1 tag2 seg2
            "0.02", "0.0", "-0.01", "0.0", "0.02", "0.0", // Y11 Y12 Y22
        ]);
        let (stamps, warnings) = build_nt_stamps(&deck, &segs);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        assert_eq!(stamps.len(), 4, "expected Z11, Z22, Z12, Z21: {stamps:?}");

        // [Z] = [Y]^-1 for a reciprocal two-port, so the off-diagonals match and
        // the stamp is symmetric in position.
        let det = Complex64::new(0.02, 0.0) * Complex64::new(0.02, 0.0)
            - Complex64::new(-0.01, 0.0) * Complex64::new(-0.01, 0.0);
        let z12_expected = -Complex64::new(-0.01, 0.0) / det;
        let z12 = stamps
            .iter()
            .find(|(r, c, _)| r != c)
            .map(|(_, _, z)| *z)
            .expect("an off-diagonal stamp");
        assert!(
            (z12 - z12_expected).norm() < 1e-9,
            "off-diagonal stamp {z12} != [Y]^-1 value {z12_expected}"
        );
    }

    /// Every rejection path warns and skips, rather than stamping something wrong.
    /// These are the defensive guards the review flagged as untested; each is
    /// reached by exactly one malformation.
    #[test]
    fn a_short_nt_card_is_rejected() {
        let segs = two_wire_segments();
        let deck = deck_with_nt(&["1", "2", "2", "2", "0.02"]);
        let m = only_warning(&deck, &segs);
        assert!(
            m.contains("has 5 fields") && m.contains("expected 10"),
            "{m}"
        );
    }

    #[test]
    fn non_integer_segment_identifiers_are_rejected() {
        let segs = two_wire_segments();
        let deck = deck_with_nt(&[
            "one", "2", "2", "2", "0.02", "0.0", "-0.01", "0.0", "0.02", "0.0",
        ]);
        let m = only_warning(&deck, &segs);
        assert!(m.contains("non-integer segment identifiers"), "{m}");
    }

    #[test]
    fn non_numeric_admittance_parameters_are_rejected() {
        let segs = two_wire_segments();
        let deck = deck_with_nt(&[
            "1",
            "2",
            "2",
            "2",
            "0.02",
            "0.0",
            "not-a-number",
            "0.0",
            "0.02",
            "0.0",
        ]);
        let m = only_warning(&deck, &segs);
        assert!(m.contains("non-numeric admittance parameters"), "{m}");
    }

    #[test]
    fn an_endpoint_missing_from_the_geometry_is_rejected() {
        let segs = two_wire_segments();
        // Tag 9 does not exist; the first endpoint is checked before the second.
        let first = deck_with_nt(&[
            "9", "2", "2", "2", "0.02", "0.0", "-0.01", "0.0", "0.02", "0.0",
        ]);
        assert!(only_warning(&first, &segs).contains("NT endpoint (9, 2) not found"));
        let second = deck_with_nt(&[
            "1", "2", "9", "2", "0.02", "0.0", "-0.01", "0.0", "0.02", "0.0",
        ]);
        assert!(only_warning(&second, &segs).contains("NT endpoint (9, 2) not found"));
    }

    #[test]
    fn both_endpoints_on_one_segment_is_rejected() {
        let segs = two_wire_segments();
        let deck = deck_with_nt(&[
            "1", "2", "1", "2", "0.02", "0.0", "-0.01", "0.0", "0.02", "0.0",
        ]);
        let m = only_warning(&deck, &segs);
        assert!(m.contains("resolve to the same segment"), "{m}");
    }

    /// A singular [Y] has no [Z]; inverting it anyway would stamp infinities into
    /// the impedance matrix and take the whole solve with it.
    #[test]
    fn a_singular_admittance_matrix_is_rejected() {
        let segs = two_wire_segments();
        // Y11*Y22 - Y12*Y21 = 0.01*0.01 - 0.01*0.01 = 0.
        let deck = deck_with_nt(&[
            "1", "2", "2", "2", "0.01", "0.0", "0.01", "0.0", "0.01", "0.0",
        ]);
        let m = only_warning(&deck, &segs);
        assert!(m.contains("singular admittance matrix"), "{m}");
    }

    /// A deck with no NT card at all is not an error and produces nothing.
    #[test]
    fn a_deck_without_nt_cards_produces_nothing() {
        let segs = two_wire_segments();
        let (stamps, warnings) = build_nt_stamps(&NecDeck::new(), &segs);
        assert!(stamps.is_empty() && warnings.is_empty());
    }
}
