// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `TL` cards as two-port admittance networks (FND-111, FND-123).
//!
//! A transmission line is a two-port connected ACROSS the gaps of its two end
//! segments, in parallel with whatever else is there — the model NEC-2 uses, and
//! the one nec2c's answer confirms: combining nec2c's own structure 2-port with
//! the admittance below reproduces its `TL` result to 0.005 Ω. The network solve
//! itself is [`crate::network`]; this file turns a card into its `[Y]`.
//!
//! For a line of characteristic impedance `Z0` and propagation `γℓ`,
//! `[Z] = Z0·[[coth γℓ, csch γℓ], [csch γℓ, coth γℓ]]`, so
//!
//! ```text
//! Y11 = Y22 = coth(γℓ) / Z0        Y12 = Y21 = −csch(γℓ) / Z0
//! ```
//!
//! which for a lossless line (`γℓ = jθ`) is `−j·cot θ / Z0` and `+j·csc θ / Z0`.
//! The sign of `Y12` is not a convention to pick: `−j·csc θ` gives 0.70 + j10.5 Ω
//! on the validation deck where nec2c gives 84.83 + j31.13.
//!
//! `γℓ = αℓ + j·kℓ/vf`, with `αℓ` from the fnec F8 loss in dB and `vf` from F7.
//! A crossed line (negative `Z0`) negates `Y12`. The shunt admittances F3–F6 add
//! to `Y11` and `Y22`.

use num_complex::Complex64;

use nec_model::card::TlCard;

use crate::geometry::Segment;
use crate::network::TwoPort;

const C0: f64 = 299_792_458.0; // m/s
const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

/// The admittance parameters `(Y11, Y12)` of a uniform line, `Y22 = Y11`.
///
/// Errors when `sinh(γℓ)` vanishes: a lossless line an exact multiple of half a
/// wavelength long has no admittance matrix.
pub fn line_admittance(z0: f64, gamma_l: Complex64) -> Result<(Complex64, Complex64), String> {
    let sinh = gamma_l.sinh();
    if sinh.norm() < 1e-9 {
        return Err(format!(
            "its electrical length is a multiple of half a wavelength (γℓ = {gamma_l:.6}), \
             where a line has no admittance matrix"
        ));
    }
    Ok((
        gamma_l.cosh() / sinh / z0,
        -Complex64::new(1.0, 0.0) / sinh / z0,
    ))
}

/// Resolve one `TL` card to a two-port, or say why it cannot be used.
///
/// Returns notes (segment-0 interpretation) alongside, which are informational.
pub(crate) fn tl_two_port(
    tl: &TlCard,
    segs: &[Segment],
    freq_hz: f64,
) -> Result<(TwoPort, Vec<String>), String> {
    let name = format!("TL {} {} {} {}", tl.tag1, tl.segment1, tl.tag2, tl.segment2);
    let mut notes = Vec::new();
    let mut end = |tag: u32, seg: u32| -> Result<usize, String> {
        let (idx, _, note) = find_segment_index(segs, tag, seg)
            .ok_or_else(|| format!("{name}: end ({tag}, {seg}) is not in the geometry"))?;
        notes.extend(note);
        Ok(idx)
    };
    let a = end(tl.tag1, tl.segment1)?;
    let b = end(tl.tag2, tl.segment2)?;
    if tl.z0 == 0.0 || !tl.z0.is_finite() {
        return Err(format!(
            "{name}: characteristic impedance must be nonzero, got {}",
            tl.z0
        ));
    }
    if !(tl.velocity_factor > 0.0 && tl.velocity_factor.is_finite()) {
        return Err(format!(
            "{name}: velocity factor (F7) must be positive, got {}",
            tl.velocity_factor
        ));
    }
    if !(tl.loss_db >= 0.0 && tl.loss_db.is_finite()) {
        return Err(format!(
            "{name}: loss (F8) must be ≥ 0 dB, got {}",
            tl.loss_db
        ));
    }
    // NEC-2: a length of zero or less means the straight-line distance between
    // the two segment centres.
    let length = if tl.length > 0.0 {
        tl.length
    } else {
        let (p, q) = (segs[a].midpoint, segs[b].midpoint);
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    if length <= 0.0 {
        return Err(format!(
            "{name}: both ends are on one segment and no length is given, so the line has none"
        ));
    }
    let k = TWO_PI * freq_hz / C0;
    let gamma_l = Complex64::new(
        tl.loss_db * std::f64::consts::LN_10 / 20.0,
        k * length / tl.velocity_factor,
    );
    let (y11, mut y12) =
        line_admittance(tl.z0.abs(), gamma_l).map_err(|e| format!("{name}: {e}"))?;
    if tl.z0 < 0.0 {
        y12 = -y12; // crossed line
    }
    Ok((
        TwoPort {
            seg_a: a,
            seg_b: b,
            y11: y11 + Complex64::new(tl.shunt1.0, tl.shunt1.1),
            y12,
            y22: y11 + Complex64::new(tl.shunt2.0, tl.shunt2.1),
        },
        notes,
    ))
}

pub(crate) fn find_segment_index(
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
    use crate::geometry::build_geometry;
    use nec_model::card::{Card, GwCard};
    use nec_model::deck::NecDeck;

    /// Two parallel 21-segment wires 1 m apart.
    fn pair() -> Vec<Segment> {
        let mut deck = NecDeck::new();
        for (tag, x) in [(1u32, 0.0), (2u32, 1.0)] {
            deck.cards.push(Card::Gw(GwCard {
                tag,
                segments: 21,
                start: [x, 0.0, -5.0],
                end: [x, 0.0, 5.0],
                radius: 0.001,
            }));
        }
        build_geometry(&deck).unwrap()
    }

    fn tl(z0: f64, length: f64) -> TlCard {
        TlCard {
            tag1: 1,
            segment1: 11,
            tag2: 2,
            segment2: 11,
            z0,
            length,
            shunt1: (0.0, 0.0),
            shunt2: (0.0, 0.0),
            velocity_factor: 1.0,
            loss_db: 0.0,
        }
    }

    /// The lossless line in closed form, independently of the γℓ route.
    #[test]
    fn a_lossless_line_has_the_textbook_admittance() {
        let f = 14.2e6;
        let (tp, _) = tl_two_port(&tl(50.0, 0.1), &pair(), f).unwrap();
        let theta = TWO_PI * f / C0 * 0.1;
        let y11 = Complex64::new(0.0, -1.0 / theta.tan() / 50.0);
        let y12 = Complex64::new(0.0, 1.0 / theta.sin() / 50.0);
        assert!((tp.y11 - y11).norm() < 1e-12 && (tp.y22 - y11).norm() < 1e-12);
        assert!((tp.y12 - y12).norm() < 1e-12, "{} vs {y12}", tp.y12);
    }

    /// Loss → 0 recovers the lossless line; the lossy Y12 sign is the one that
    /// is easy to get wrong by analogy (−csch, not +csch).
    #[test]
    fn a_vanishing_loss_recovers_the_lossless_line() {
        let segs = pair();
        let (lossless, _) = tl_two_port(&tl(50.0, 0.1), &segs, 14.2e6).unwrap();
        let mut card = tl(50.0, 0.1);
        card.loss_db = 1e-12;
        let (lossy, _) = tl_two_port(&card, &segs, 14.2e6).unwrap();
        assert!((lossy.y11 - lossless.y11).norm() < 1e-9);
        assert!((lossy.y12 - lossless.y12).norm() < 1e-9);
    }

    #[test]
    fn a_crossed_line_negates_only_the_transfer_admittance() {
        let segs = pair();
        let (straight, _) = tl_two_port(&tl(50.0, 0.1), &segs, 14.2e6).unwrap();
        let (crossed, _) = tl_two_port(&tl(-50.0, 0.1), &segs, 14.2e6).unwrap();
        assert_eq!(crossed.y11, straight.y11);
        assert_eq!(crossed.y12, -straight.y12);
    }

    /// NEC-2: length 0 is the distance between the segment centres (1 m here).
    #[test]
    fn a_zero_length_means_the_centre_distance() {
        let segs = pair();
        let (zero, _) = tl_two_port(&tl(50.0, 0.0), &segs, 14.2e6).unwrap();
        let (one, _) = tl_two_port(&tl(50.0, 1.0), &segs, 14.2e6).unwrap();
        assert!((zero.y12 - one.y12).norm() < 1e-12);
    }

    #[test]
    fn shunt_admittances_add_to_their_own_end() {
        let mut card = tl(50.0, 0.1);
        card.shunt1 = (0.01, 0.0);
        card.shunt2 = (0.0, 0.02);
        let segs = pair();
        let (bare, _) = tl_two_port(&tl(50.0, 0.1), &segs, 14.2e6).unwrap();
        let (tp, _) = tl_two_port(&card, &segs, 14.2e6).unwrap();
        assert!((tp.y11 - bare.y11 - Complex64::new(0.01, 0.0)).norm() < 1e-15);
        assert!((tp.y22 - bare.y22 - Complex64::new(0.0, 0.02)).norm() < 1e-15);
        assert_eq!(tp.y12, bare.y12);
    }

    #[test]
    fn unusable_cards_are_errors_not_skips() {
        let segs = pair();
        let mut missing = tl(50.0, 0.1);
        missing.tag2 = 9;
        for (card, needle) in [
            (missing, "not in the geometry"),
            (tl(0.0, 0.1), "nonzero"),
            (
                TlCard {
                    velocity_factor: 0.0,
                    ..tl(50.0, 0.1)
                },
                "velocity factor",
            ),
            (
                TlCard {
                    loss_db: -1.0,
                    ..tl(50.0, 0.1)
                },
                "loss",
            ),
            // λ/2 at 14.2 MHz: no admittance matrix.
            (tl(50.0, C0 / 14.2e6 / 2.0), "half a wavelength"),
        ] {
            let err = tl_two_port(&card, &segs, 14.2e6).unwrap_err();
            assert!(err.contains(needle), "{err}");
        }
    }

    #[test]
    fn segment_zero_maps_to_the_tag_centre_with_a_note() {
        let segs = pair();
        let card = TlCard {
            segment1: 0,
            ..tl(50.0, 0.1)
        };
        let (tp, notes) = tl_two_port(&card, &segs, 14.2e6).unwrap();
        assert_eq!(segs[tp.seg_a].tag_index, 11);
        assert!(
            notes.iter().any(|n| n.contains("center segment 11")),
            "{notes:?}"
        );
    }
}
