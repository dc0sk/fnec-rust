// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! TL network builder: converts supported TL cards into impedance-matrix stamps.
//!
//! Supported subset (initial executable semantics):
//! - `tl_type = 0` (lossless)
//! - `num_segments = 1`
//! - explicit endpoint segments (`segment1 > 0`, `segment2 > 0`)
//! - positive characteristic impedance `z0 > 0`
//!
//! For the supported subset we stamp a symmetric 2-port Z-parameter model into
//! the MoM matrix for the connected segment pair:
//!
//! - $$Z_{11} = Z_{22} = -j Z_0 \cot(\theta)$$
//! - $$Z_{12} = Z_{21} = -j Z_0 \csc(\theta)$$
//! - $$\theta = k \cdot \ell / v_f$$

use num_complex::Complex64;

use nec_model::card::Card;
use nec_model::deck::NecDeck;

use crate::geometry::Segment;

const C0: f64 = 299_792_458.0; // m/s
const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

/// A sparse matrix stamp `(row, col, delta_z)` for impedance matrix updates.
pub type TlStamp = (usize, usize, Complex64);

/// A non-fatal warning produced by TL processing.
#[derive(Debug, Clone, PartialEq)]
pub struct TlWarning {
    /// Human-readable description of the issue.
    pub message: String,
}

impl std::fmt::Display for TlWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Build sparse impedance stamps from supported `TL` cards.
pub fn build_tl_stamps(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
) -> (Vec<TlStamp>, Vec<TlWarning>) {
    let mut stamps: Vec<TlStamp> = Vec::new();
    let mut warnings: Vec<TlWarning> = Vec::new();

    let k = TWO_PI * freq_hz / C0;

    for card in &deck.cards {
        let Card::Tl(tl) = card else { continue };

        if tl.tl_type != 0 {
            warnings.push(TlWarning {
                message: format!(
                    "TL type {} between ({}, {}) and ({}, {}) is not yet supported; TL card ignored",
                    tl.tl_type, tl.tag1, tl.segment1, tl.tag2, tl.segment2
                ),
            });
            continue;
        }
        if tl.num_segments != 1 {
            warnings.push(TlWarning {
                message: format!(
                    "TL with NSEG={} between ({}, {}) and ({}, {}) is not yet supported; TL card ignored",
                    tl.num_segments, tl.tag1, tl.segment1, tl.tag2, tl.segment2
                ),
            });
            continue;
        }

        let Some(i1) = find_segment_index(segs, tl.tag1, tl.segment1) else {
            warnings.push(TlWarning {
                message: format!(
                    "TL endpoint ({}, {}) not found in geometry; TL card ignored",
                    tl.tag1, tl.segment1
                ),
            });
            continue;
        };
        let Some(i2) = find_segment_index(segs, tl.tag2, tl.segment2) else {
            warnings.push(TlWarning {
                message: format!(
                    "TL endpoint ({}, {}) not found in geometry; TL card ignored",
                    tl.tag2, tl.segment2
                ),
            });
            continue;
        };
        if i1 == i2 {
            warnings.push(TlWarning {
                message: format!(
                    "TL endpoints resolve to the same segment ({}, {}); TL card ignored",
                    tl.tag1, tl.segment1
                ),
            });
            continue;
        }
        if tl.z0 <= 0.0 {
            warnings.push(TlWarning {
                message: format!(
                    "TL between ({}, {}) and ({}, {}): characteristic impedance z0={} must be > 0; TL card ignored",
                    tl.tag1, tl.segment1, tl.tag2, tl.segment2, tl.z0
                ),
            });
            continue;
        }
        if tl.length <= 0.0 {
            warnings.push(TlWarning {
                message: format!(
                    "TL between ({}, {}) and ({}, {}): length={} must be > 0; TL card ignored",
                    tl.tag1, tl.segment1, tl.tag2, tl.segment2, tl.length
                ),
            });
            continue;
        }

        let vf = if (0.0..=1.0).contains(&tl.f3) && tl.f3 > 0.0 {
            tl.f3
        } else {
            1.0
        };
        let theta = k * tl.length / vf;
        let sin_theta = theta.sin();
        if sin_theta.abs() < 1e-9 {
            warnings.push(TlWarning {
                message: format!(
                    "TL between ({}, {}) and ({}, {}): electrical length is near a singular csc/cot point (theta={:.6e}); TL card ignored",
                    tl.tag1, tl.segment1, tl.tag2, tl.segment2, theta
                ),
            });
            continue;
        }

        let cot = theta.cos() / sin_theta;
        let csc = 1.0 / sin_theta;
        let z_diag = Complex64::new(0.0, -tl.z0 * cot);
        let z_off = Complex64::new(0.0, -tl.z0 * csc);

        stamps.push((i1, i1, z_diag));
        stamps.push((i2, i2, z_diag));
        stamps.push((i1, i2, z_off));
        stamps.push((i2, i1, z_off));
    }

    (stamps, warnings)
}

fn find_segment_index(segs: &[Segment], tag: u32, segment: u32) -> Option<usize> {
    if segment == 0 {
        return None;
    }
    segs.iter()
        .position(|s| s.tag == tag && s.tag_index == segment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, GwCard, TlCard};

    fn segs_two_wire_geometry() -> Vec<Segment> {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 3,
            start: [1.0, 0.0, -1.0],
            end: [1.0, 0.0, 1.0],
            radius: 0.001,
        }));
        crate::geometry::build_geometry(&deck).expect("geometry should build")
    }

    #[test]
    fn supported_lossless_tl_produces_four_stamps() {
        let segs = segs_two_wire_geometry();
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Tl(TlCard {
            tag1: 1,
            segment1: 2,
            tag2: 2,
            segment2: 2,
            num_segments: 1,
            tl_type: 0,
            z0: 50.0,
            length: 1.0,
            f3: 1.0,
        }));

        let (stamps, warns) = build_tl_stamps(&deck, &segs, 14.2e6);
        assert!(warns.is_empty());
        assert_eq!(stamps.len(), 4);
        assert!(stamps.iter().any(|(r, c, _)| r == c));
        assert!(stamps.iter().any(|(r, c, _)| r != c));
    }

    #[test]
    fn unsupported_tl_type_emits_warning() {
        let segs = segs_two_wire_geometry();
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Tl(TlCard {
            tag1: 1,
            segment1: 2,
            tag2: 2,
            segment2: 2,
            num_segments: 1,
            tl_type: 1,
            z0: 50.0,
            length: 1.0,
            f3: 1.0,
        }));

        let (stamps, warns) = build_tl_stamps(&deck, &segs, 14.2e6);
        assert!(stamps.is_empty());
        assert_eq!(warns.len(), 1);
        assert!(warns[0].message.contains("not yet supported"));
    }
}
