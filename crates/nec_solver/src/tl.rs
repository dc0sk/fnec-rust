// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! TL network builder: converts supported TL cards into impedance-matrix stamps.
//!
//! Supported subset (initial executable semantics):
//! - `tl_type = 0` (lossless)
//! - `num_segments >= 0` (`0` is treated as a single-section shorthand)
//! - endpoint segments (`segment=0` is accepted and mapped to the tag center;
//!   for even segment counts, the lower of the two center segments is used)
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
        // NSEG>1 cards are accepted using the same uniform-line stamp semantics
        // as a single-section card; NSEG=0 remains a single-section shorthand.
        let _effective_num_segments = if tl.num_segments == 0 {
            1
        } else {
            tl.num_segments
        };

        let Some((i1, resolved_seg1, center_warn1)) =
            find_segment_index(segs, tl.tag1, tl.segment1)
        else {
            warnings.push(TlWarning {
                message: format!(
                    "TL endpoint ({}, {}) not found in geometry; TL card ignored",
                    tl.tag1, tl.segment1
                ),
            });
            continue;
        };
        let Some((i2, resolved_seg2, center_warn2)) =
            find_segment_index(segs, tl.tag2, tl.segment2)
        else {
            warnings.push(TlWarning {
                message: format!(
                    "TL endpoint ({}, {}) not found in geometry; TL card ignored",
                    tl.tag2, tl.segment2
                ),
            });
            continue;
        };
        if let Some(warn) = center_warn1 {
            warnings.push(TlWarning { message: warn });
        }
        if let Some(warn) = center_warn2 {
            warnings.push(TlWarning { message: warn });
        }
        if i1 == i2 {
            warnings.push(TlWarning {
                message: format!(
                    "TL endpoints resolve to the same segment (({}, {}) and ({}, {})); TL card ignored",
                    tl.tag1, resolved_seg1, tl.tag2, resolved_seg2
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

fn find_segment_index(
    segs: &[Segment],
    tag: u32,
    segment: u32,
) -> Option<(usize, u32, Option<String>)> {
    if segment == 0 {
        return find_center_segment_index(segs, tag);
    }
    let idx = segs
        .iter()
        .position(|s| s.tag == tag && s.tag_index == segment)?;
    Some((idx, segment, None))
}

fn find_center_segment_index(segs: &[Segment], tag: u32) -> Option<(usize, u32, Option<String>)> {
    let tagged: Vec<(usize, u32)> = segs
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            if s.tag == tag {
                Some((i, s.tag_index))
            } else {
                None
            }
        })
        .collect();
    if tagged.is_empty() {
        return None;
    }

    let n = tagged.len() as u32;
    let (pick_offset, message) = if tagged.len() % 2 == 1 {
        let offset = tagged.len() / 2;
        let (_, resolved_seg) = tagged[offset];
        (
            offset,
            Some(format!(
                "TL endpoint ({}, 0): interpreting segment 0 as center segment {} for tag {}",
                tag, resolved_seg, tag
            )),
        )
    } else {
        let lower_center_seg = n / 2;
        let offset = tagged
            .iter()
            .position(|(_, seg_idx)| *seg_idx == lower_center_seg)
            .unwrap_or((tagged.len() / 2).saturating_sub(1));
        (
            offset,
            Some(format!(
                "TL endpoint ({}, 0): tag has even segment count {}; using lower center segment {}",
                tag, n, lower_center_seg
            )),
        )
    };

    let (idx, resolved_seg) = tagged[pick_offset];
    Some((idx, resolved_seg, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, GwCard, TlCard};

    fn warn_contains(warns: &[TlWarning], needle: &str) -> bool {
        warns.iter().any(|w| w.message.contains(needle))
    }

    fn segs_two_wire_geometry_with_segments(per_wire_segments: u32) -> Vec<Segment> {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: per_wire_segments,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: per_wire_segments,
            start: [1.0, 0.0, -1.0],
            end: [1.0, 0.0, 1.0],
            radius: 0.001,
        }));
        crate::geometry::build_geometry(&deck).expect("geometry should build")
    }

    fn segs_two_wire_geometry() -> Vec<Segment> {
        segs_two_wire_geometry_with_segments(3)
    }

    #[test]
    fn segment_zero_even_segment_count_uses_lower_center() {
        let segs = segs_two_wire_geometry_with_segments(4);
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Tl(TlCard {
            tag1: 1,
            segment1: 0,
            tag2: 2,
            segment2: 0,
            num_segments: 1,
            tl_type: 0,
            z0: 50.0,
            length: 1.0,
            f3: 1.0,
        }));

        let (stamps, warns) = build_tl_stamps(&deck, &segs, 14.2e6);
        assert_eq!(stamps.len(), 4);
        assert_eq!(warns.len(), 2);
        assert!(warn_contains(
            &warns,
            "tag has even segment count 4; using lower center segment 2"
        ));
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
    fn nseg_zero_is_accepted_like_single_section() {
        let segs = segs_two_wire_geometry();
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Tl(TlCard {
            tag1: 1,
            segment1: 2,
            tag2: 2,
            segment2: 2,
            num_segments: 0,
            tl_type: 0,
            z0: 50.0,
            length: 1.0,
            f3: 1.0,
        }));

        let (stamps, warns) = build_tl_stamps(&deck, &segs, 14.2e6);
        assert!(warns.is_empty());
        assert_eq!(stamps.len(), 4);
    }

    #[test]
    fn nseg_gt_one_is_accepted_like_single_section() {
        let segs = segs_two_wire_geometry();

        let mut deck_nseg1 = NecDeck::new();
        deck_nseg1.cards.push(Card::Tl(TlCard {
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

        let mut deck_nseg3 = NecDeck::new();
        deck_nseg3.cards.push(Card::Tl(TlCard {
            tag1: 1,
            segment1: 2,
            tag2: 2,
            segment2: 2,
            num_segments: 3,
            tl_type: 0,
            z0: 50.0,
            length: 1.0,
            f3: 1.0,
        }));

        let (stamps_nseg1, warns_nseg1) = build_tl_stamps(&deck_nseg1, &segs, 14.2e6);
        let (stamps_nseg3, warns_nseg3) = build_tl_stamps(&deck_nseg3, &segs, 14.2e6);

        assert!(warns_nseg1.is_empty());
        assert!(warns_nseg3.is_empty());
        assert_eq!(stamps_nseg1, stamps_nseg3);
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

    #[test]
    fn segment_zero_maps_to_tag_center() {
        let segs = segs_two_wire_geometry();
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Tl(TlCard {
            tag1: 1,
            segment1: 0,
            tag2: 2,
            segment2: 0,
            num_segments: 1,
            tl_type: 0,
            z0: 50.0,
            length: 1.0,
            f3: 1.0,
        }));

        let (stamps, warns) = build_tl_stamps(&deck, &segs, 14.2e6);
        assert_eq!(stamps.len(), 4);
        assert_eq!(warns.len(), 2);
        assert!(warn_contains(
            &warns,
            "interpreting segment 0 as center segment"
        ));
    }
}
