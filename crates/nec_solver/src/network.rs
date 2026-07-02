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
