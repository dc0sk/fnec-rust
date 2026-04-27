// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Load builder: converts LD cards into per-segment complex impedance loads.
//!
//! Load types implemented:
//!
//! | I1 | Description               | Z formula                                          |
//! |----|---------------------------|----------------------------------------------------|
//! |  0 | Series RLC (lumped)       | R + j(ωL − 1/(ωC))  (C=0 ⇒ no capacitor term)     |
//! |  1 | Parallel RLC (lumped)     | 1 / (1/R + 1/(jωL) + jωC)                         |
//! |  4 | Series impedance Z=R+jX   | R + jX  (flat, frequency-independent)              |
//! |  5 | Wire conductivity (dist.) | Σ per segment: dl/(2π·a·σ)                        |
//!
//! Other load types are stored in `LdCard` but emit a warning via the returned
//! `Vec<LoadWarning>` and are otherwise ignored.

use num_complex::Complex64;

use nec_model::card::{Card, LdCard};
use nec_model::deck::NecDeck;

use crate::geometry::Segment;

const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

/// A non-fatal warning produced by load processing.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadWarning {
    /// Human-readable description of the issue.
    pub message: String,
}

impl std::fmt::Display for LoadWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Compute a flat vector of per-segment load impedances (Ω) for `freq_hz`.
///
/// The returned `Vec` has the same length as `segs`.  Element `[i]` is the
/// total series load impedance to be added to the diagonal of the MoM matrix
/// for segment `i`.
///
/// Non-fatal issues (unsupported load types, zero-conductivity) are reported
/// via the returned `Vec<LoadWarning>`.
pub fn build_loads(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
) -> (Vec<Complex64>, Vec<LoadWarning>) {
    let n = segs.len();
    let mut loads = vec![Complex64::new(0.0, 0.0); n];
    let mut warnings = Vec::new();

    let omega = TWO_PI * freq_hz;

    for card in &deck.cards {
        let Card::Ld(ld) = card else { continue };

        for (i, seg) in segs.iter().enumerate() {
            if !segment_matches(ld, seg) {
                continue;
            }

            let z = match ld.load_type {
                0 => {
                    // Series RLC: Z = R + j(ωL − 1/(ωC))
                    let r = ld.f1;
                    let l = ld.f2;
                    let c = ld.f3;
                    let x_l = omega * l;
                    let x_c = if c > 0.0 { 1.0 / (omega * c) } else { 0.0 };
                    Complex64::new(r, x_l - x_c)
                }
                1 => {
                    // Parallel RLC: Y = 1/R + 1/(jωL) + jωC  →  Z = 1/Y
                    // Missing branches (zero L → infinite susceptance; zero R → infinite
                    // conductance) are treated as degenerate loads contributing zero impedance.
                    let r = ld.f1;
                    let l = ld.f2;
                    let c = ld.f3;
                    let mut y = Complex64::new(0.0, 0.0);
                    if r > 0.0 {
                        y += Complex64::new(1.0 / r, 0.0);
                    }
                    if l > 0.0 {
                        // 1/(jωL) = -j/(ωL)
                        y += Complex64::new(0.0, -1.0 / (omega * l));
                    }
                    if c > 0.0 {
                        y += Complex64::new(0.0, omega * c);
                    }
                    if y.norm() < 1e-30 {
                        // All branches open — contribute nothing
                        Complex64::new(0.0, 0.0)
                    } else {
                        Complex64::new(1.0, 0.0) / y
                    }
                }
                4 => {
                    // Series impedance: Z = R + jX (frequency-independent)
                    Complex64::new(ld.f1, ld.f2)
                }
                5 => {
                    // Distributed wire conductivity: Z = dl / (2π·a·σ)
                    let sigma = ld.f1;
                    if sigma <= 0.0 {
                        warnings.push(LoadWarning {
                            message: format!(
                                "LD type 5 on tag {} seg {}–{}: conductivity σ={} ≤ 0, load ignored",
                                ld.tag, ld.seg_first, ld.seg_last, sigma
                            ),
                        });
                        continue;
                    }
                    let denom = TWO_PI * seg.radius * sigma;
                    Complex64::new(seg.length / denom, 0.0)
                }
                other => {
                    warnings.push(LoadWarning {
                        message: format!(
                            "LD type {other} on tag {} is not yet supported; load ignored",
                            ld.tag
                        ),
                    });
                    continue;
                }
            };

            loads[i] += z;
        }
    }

    (loads, warnings)
}

/// Returns true if this segment should receive the load described by `ld`.
fn segment_matches(ld: &LdCard, seg: &Segment) -> bool {
    // Tag 0 = apply to all tags.
    if ld.tag != 0 && seg.tag != ld.tag {
        return false;
    }
    // seg_first 0 = all segments of the tag.
    if ld.seg_first == 0 {
        return true;
    }
    // seg_last 0 means same as seg_first (single segment).
    let last = if ld.seg_last == 0 {
        ld.seg_first
    } else {
        ld.seg_last
    };
    seg.tag_index >= ld.seg_first && seg.tag_index <= last
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, LdCard};
    use nec_model::deck::NecDeck;

    fn seg(tag: u32, tag_index: u32, length: f64, radius: f64) -> Segment {
        let half = length / 2.0;
        Segment {
            tag,
            tag_index,
            global_index: (tag_index - 1) as usize,
            start: [0.0, 0.0, -half],
            end: [0.0, 0.0, half],
            midpoint: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
            length,
            radius,
        }
    }

    fn deck_with_ld(ld: LdCard) -> NecDeck {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Ld(ld));
        deck
    }

    #[test]
    fn type4_series_impedance_applies_to_matching_segment() {
        let segs = vec![seg(1, 1, 0.1, 0.001), seg(1, 2, 0.1, 0.001)];
        let deck = deck_with_ld(LdCard {
            load_type: 4,
            tag: 1,
            seg_first: 2,
            seg_last: 2,
            f1: 100.0,
            f2: 50.0,
            f3: 0.0,
        });
        let (loads, warns) = build_loads(&deck, &segs, 14.2e6);
        assert!(warns.is_empty());
        assert_eq!(loads[0], Complex64::new(0.0, 0.0)); // seg 1 untouched
        assert!((loads[1].re - 100.0).abs() < 1e-9); // R=100
        assert!((loads[1].im - 50.0).abs() < 1e-9); // X=50
    }

    #[test]
    fn type0_series_rlc_produces_correct_impedance() {
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let freq = 14.2e6_f64;
        let omega = TWO_PI * freq;
        let l = 1e-6;
        let c = 1e-12;
        let deck = deck_with_ld(LdCard {
            load_type: 0,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 10.0,
            f2: l,
            f3: c,
        });
        let (loads, warns) = build_loads(&deck, &segs, freq);
        assert!(warns.is_empty());
        let expected_x = omega * l - 1.0 / (omega * c);
        assert!((loads[0].re - 10.0).abs() < 1e-6);
        assert!((loads[0].im - expected_x).abs() < 1e-3);
    }

    #[test]
    fn type5_conductivity_applies_distributed_resistance() {
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let deck = deck_with_ld(LdCard {
            load_type: 5,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 5.8e7, // copper σ
            f2: 0.0,
            f3: 0.0,
        });
        let (loads, warns) = build_loads(&deck, &segs, 14.2e6);
        assert!(warns.is_empty());
        let expected_r = 0.1 / (TWO_PI * 0.001 * 5.8e7);
        assert!((loads[0].re - expected_r).abs() < 1e-15);
        assert_eq!(loads[0].im, 0.0);
    }

    #[test]
    fn unsupported_type_produces_warning_and_zero_load() {
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let deck = deck_with_ld(LdCard {
            load_type: 3,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 0.0,
            f2: 0.0,
            f3: 0.0,
        });
        let (loads, warns) = build_loads(&deck, &segs, 14.2e6);
        assert_eq!(loads[0], Complex64::new(0.0, 0.0));
        assert_eq!(warns.len(), 1);
        assert!(warns[0].message.contains("not yet supported"));
    }

    #[test]
    fn type1_parallel_r_only_is_inverse_resistance() {
        // Parallel RLC with L=0 C=0: Y = 1/R  →  Z = R
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let deck = deck_with_ld(LdCard {
            load_type: 1,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 200.0, // R = 200 Ω
            f2: 0.0,
            f3: 0.0,
        });
        let (loads, warns) = build_loads(&deck, &segs, 14.2e6);
        assert!(warns.is_empty());
        assert!((loads[0].re - 200.0).abs() < 1e-9);
        assert!(loads[0].im.abs() < 1e-9);
    }

    #[test]
    fn type1_parallel_rlc_admittance_formula() {
        // Parallel RLC: Z = 1 / (1/R + 1/(jωL) + jωC)
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let freq = 14.2e6_f64;
        let omega = TWO_PI * freq;
        let r = 500.0_f64;
        let l = 1e-6_f64;
        let c = 1e-12_f64;
        let deck = deck_with_ld(LdCard {
            load_type: 1,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: r,
            f2: l,
            f3: c,
        });
        let (loads, warns) = build_loads(&deck, &segs, freq);
        assert!(warns.is_empty());
        let g = 1.0 / r;
        let b_l = -1.0 / (omega * l); // susceptance of inductor: -1/(ωL)
        let b_c = omega * c; // susceptance of capacitor: ωC
        let y = Complex64::new(g, b_l + b_c);
        let expected_z = Complex64::new(1.0, 0.0) / y;
        assert!(
            (loads[0].re - expected_z.re).abs() < 1e-6,
            "Re: {} vs {}",
            loads[0].re,
            expected_z.re
        );
        assert!(
            (loads[0].im - expected_z.im).abs() < 1e-6,
            "Im: {} vs {}",
            loads[0].im,
            expected_z.im
        );
    }

    #[test]
    fn type1_all_open_branches_produces_zero_load() {
        // All parameters zero → open circuit → no contribution
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let deck = deck_with_ld(LdCard {
            load_type: 1,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 0.0,
            f2: 0.0,
            f3: 0.0,
        });
        let (loads, warns) = build_loads(&deck, &segs, 14.2e6);
        assert!(warns.is_empty());
        assert_eq!(loads[0], Complex64::new(0.0, 0.0));
    }

    #[test]
    fn tag0_applies_to_all_segments() {
        let segs = vec![seg(1, 1, 0.1, 0.001), seg(2, 1, 0.1, 0.001)];
        let deck = deck_with_ld(LdCard {
            load_type: 4,
            tag: 0, // all tags
            seg_first: 0,
            seg_last: 0,
            f1: 50.0,
            f2: 0.0,
            f3: 0.0,
        });
        let (loads, warns) = build_loads(&deck, &segs, 14.2e6);
        assert!(warns.is_empty());
        assert!((loads[0].re - 50.0).abs() < 1e-9);
        assert!((loads[1].re - 50.0).abs() < 1e-9);
    }
}
