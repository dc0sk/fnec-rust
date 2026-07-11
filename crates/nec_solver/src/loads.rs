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
//! |  2 | Series RL (lumped)        | R + jωL                                            |
//! |  3 | Series RC (lumped)        | R − j/(ωC)  (C=0 ⇒ no capacitor term)             |
//! |  4 | Series impedance Z=R+jX   | R + jX  (flat, frequency-independent)              |
//! |  5 | Wire conductivity (dist.) | Exact round-wire skin-effect Zᵢ (DC↔surface Z)   |
//!
//! Other load types are stored in `LdCard` but emit a warning via the returned
//! `Vec<LoadWarning>` and are otherwise ignored.

use num_complex::Complex64;

use nec_model::card::{Card, LdCard};
use nec_model::deck::NecDeck;

use crate::geometry::Segment;

const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7;

/// Ratio `I0(z)/I1(z)` of modified Bessel functions for complex `z`, used by the
/// round-wire internal-impedance formula. Power series for `|z| ≤ 15`, and the
/// large-argument asymptotic expansion beyond it (where the series would
/// overflow). `I0/I1 → 2/z` as `z → 0` and `→ 1` as `|z| → ∞`.
fn i0_over_i1(z: Complex64) -> Complex64 {
    if z.norm() > 15.0 {
        let inv = Complex64::new(1.0, 0.0) / z;
        // I0/I1 ~ 1 + 1/(2z) + 3/(8z²) + …
        Complex64::new(1.0, 0.0) + inv * 0.5 + inv * inv * 0.375
    } else {
        let half = z * 0.5;
        let half_sq = half * half;
        let mut i0 = Complex64::new(1.0, 0.0);
        let mut term0 = Complex64::new(1.0, 0.0);
        let mut i1 = half;
        let mut term1 = half;
        for k in 1..300 {
            let kf = f64::from(k);
            term0 *= half_sq / (kf * kf);
            i0 += term0;
            term1 *= half_sq / (kf * (kf + 1.0));
            i1 += term1;
            if term0.norm() <= 1e-17 * i0.norm() && term1.norm() <= 1e-17 * i1.norm() {
                break;
            }
        }
        i0 / i1
    }
}

/// Internal (skin-effect) series impedance of a solid round wire of radius `a`
/// (m), conductivity `sigma` (S/m) and length `dl` (m) at angular frequency
/// `omega` (rad/s):
///
/// ```text
///   Z = dl · (γ / (2π a σ)) · I0(γa)/I1(γa),   γ = (1+j)/δ,   δ = √(2/(ω μ0 σ))
/// ```
///
/// This is the exact result for a homogeneous round conductor. It reduces to the
/// DC resistance `dl/(σ π a²)` as `ω → 0` and to the surface impedance
/// `dl·(1+j)/(2π a σ δ)` once the skin depth `δ` falls below the radius —
/// matching NEC-2's wire-conductivity (`LD 5`) load. At `ω ≤ 0` the DC resistance
/// is returned.
fn wire_internal_impedance(sigma: f64, a: f64, dl: f64, omega: f64) -> Complex64 {
    if omega <= 0.0 {
        return Complex64::new(dl / (sigma * std::f64::consts::PI * a * a), 0.0);
    }
    let delta = (2.0 / (omega * MU0 * sigma)).sqrt();
    let gamma = Complex64::new(1.0 / delta, 1.0 / delta); // (1+j)/δ
    gamma / (TWO_PI * a * sigma) * i0_over_i1(gamma * a) * dl
}

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
                2 => {
                    // Series RL: Z = R + jωL
                    Complex64::new(ld.f1, omega * ld.f2)
                }
                3 => {
                    // Series RC: Z = R - j/(ωC)
                    let x_c = if ld.f3 > 0.0 {
                        1.0 / (omega * ld.f3)
                    } else {
                        0.0
                    };
                    Complex64::new(ld.f1, -x_c)
                }
                4 => {
                    // Series impedance: Z = R + jX (frequency-independent)
                    Complex64::new(ld.f1, ld.f2)
                }
                5 => {
                    // Distributed wire conductivity: exact round-wire internal
                    // (skin-effect) impedance — DC resistance at low frequency,
                    // surface impedance once the skin depth drops below the radius.
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
                    wire_internal_impedance(sigma, seg.radius, seg.length, omega)
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
    fn type5_conductivity_matches_dc_and_hf_limits() {
        let sigma = 5.8e7_f64; // copper σ
        let a = 0.001_f64;
        let len = 0.1_f64;
        let segs = vec![seg(1, 1, len, a)];
        let deck = deck_with_ld(LdCard {
            load_type: 5,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: sigma,
            f2: 0.0,
            f3: 0.0,
        });

        // Low frequency (a ≪ δ): DC resistance dl/(σπa²), negligible reactance.
        let (lo, warns) = build_loads(&deck, &segs, 1e3);
        assert!(warns.is_empty());
        let r_dc = len / (sigma * std::f64::consts::PI * a * a);
        assert!(
            (lo[0].re - r_dc).abs() < 0.05 * r_dc,
            "DC Re {} vs {r_dc}",
            lo[0].re
        );
        assert!(
            lo[0].im.abs() < 0.15 * lo[0].re,
            "reactance small near DC, got {}",
            lo[0].im
        );

        // High frequency (a ≫ δ): surface impedance dl(1+j)/(2πaσδ), Re ≈ Im.
        let f = 1e9_f64;
        let omega = TWO_PI * f;
        let delta = (2.0 / (omega * MU0 * sigma)).sqrt();
        let r_hf = len / (TWO_PI * a * sigma * delta);
        let (hi, _) = build_loads(&deck, &segs, f);
        assert!(
            (hi[0].re - r_hf).abs() < 0.02 * r_hf,
            "HF Re {} vs surface {r_hf}",
            hi[0].re
        );
        assert!(
            (hi[0].im - hi[0].re).abs() < 0.05 * hi[0].re,
            "HF Re≈Im (skin effect), got {} + j{}",
            hi[0].re,
            hi[0].im
        );
        // Skin effect drives the resistance far above the DC value.
        assert!(hi[0].re > 100.0 * r_dc, "HF Re should ≫ DC");
    }

    #[test]
    fn type2_series_rl_produces_correct_impedance() {
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let freq = 14.2e6_f64;
        let omega = TWO_PI * freq;
        let deck = deck_with_ld(LdCard {
            load_type: 2,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 10.0,
            f2: 1e-6,
            f3: 0.0,
        });
        let (loads, warns) = build_loads(&deck, &segs, freq);
        assert!(warns.is_empty());
        assert!((loads[0].re - 10.0).abs() < 1e-6);
        assert!((loads[0].im - omega * 1e-6).abs() < 1e-6);
    }

    #[test]
    fn type3_series_rc_produces_correct_impedance() {
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let freq = 14.2e6_f64;
        let c = 1e-12_f64;
        let deck = deck_with_ld(LdCard {
            load_type: 3,
            tag: 1,
            seg_first: 1,
            seg_last: 1,
            f1: 10.0,
            f2: 0.0,
            f3: c,
        });
        let (loads, warns) = build_loads(&deck, &segs, freq);
        assert!(warns.is_empty());
        assert!((loads[0].re - 10.0).abs() < 1e-6);
        assert!((loads[0].im + 1.0 / (TWO_PI * freq * c)).abs() < 1e-3);
    }

    #[test]
    fn unsupported_type_produces_warning_and_zero_load() {
        let segs = vec![seg(1, 1, 0.1, 0.001)];
        let deck = deck_with_ld(LdCard {
            load_type: 9,
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

    // ── Proptest sweeps (BL-IMPR-005) ───────────────────────────────────

    use proptest::prelude::*;

    /// Fixed geometry used by all proptest cases.
    fn proptest_seg() -> Segment {
        seg(1, 1, 0.1, 0.001)
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(512))]

        /// LD type 4 (series impedance): Z = R + jX, frequency-independent.
        /// Re and Im must equal f1 and f2 exactly for any finite values.
        #[test]
        fn proptest_ld_type4_is_frequency_independent(
            r in -1e9_f64..=1e9_f64,
            x in -1e9_f64..=1e9_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let segs = vec![proptest_seg()];
            let deck = deck_with_ld(LdCard {
                load_type: 4,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: r, f2: x, f3: 0.0,
            });
            let (loads, warns) = build_loads(&deck, &segs, freq);
            prop_assert!(warns.is_empty());
            prop_assert!((loads[0].re - r).abs() < 1e-9,
                "Re expected {r}, got {}", loads[0].re);
            prop_assert!((loads[0].im - x).abs() < 1e-9,
                "Im expected {x}, got {}", loads[0].im);
        }

        /// LD type 0 (series RLC): Re(Z) must equal R for any R≥0, L≥0, C≥0.
        #[test]
        fn proptest_ld_type0_re_equals_resistance(
            r in 0.0_f64..1e9_f64,
            l in 0.0_f64..1e-3_f64,
            c in 0.0_f64..1e-6_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let segs = vec![proptest_seg()];
            let deck = deck_with_ld(LdCard {
                load_type: 0,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: r, f2: l, f3: c,
            });
            let (loads, warns) = build_loads(&deck, &segs, freq);
            prop_assert!(warns.is_empty());
            prop_assert!((loads[0].re - r).abs() < 1e-6,
                "Re expected {r}, got {}", loads[0].re);
        }

        /// LD type 0 (series RLC): Im(Z) = ωL − 1/(ωC) for C > 0.
        #[test]
        fn proptest_ld_type0_im_formula(
            r in 0.0_f64..1e6_f64,
            l in 1e-9_f64..1e-3_f64,
            c in 1e-15_f64..1e-6_f64,
            freq in 1e6_f64..1e9_f64,
        ) {
            let segs = vec![proptest_seg()];
            let deck = deck_with_ld(LdCard {
                load_type: 0,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: r, f2: l, f3: c,
            });
            let (loads, warns) = build_loads(&deck, &segs, freq);
            prop_assert!(warns.is_empty());
            let omega = TWO_PI * freq;
            let expected_x = omega * l - 1.0 / (omega * c);
            // Tolerance scales with magnitude due to floating-point cancellation.
            let tol = (expected_x.abs() * 1e-9).max(1e-3);
            prop_assert!((loads[0].im - expected_x).abs() < tol,
                "Im expected {expected_x}, got {}, tol={tol}", loads[0].im);
        }

        /// LD type 2 (series RL): Z = R + jωL.
        #[test]
        fn proptest_ld_type2_re_and_im_formula(
            r in 0.0_f64..1e9_f64,
            l in 0.0_f64..1e-3_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let segs = vec![proptest_seg()];
            let deck = deck_with_ld(LdCard {
                load_type: 2,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: r, f2: l, f3: 0.0,
            });
            let (loads, warns) = build_loads(&deck, &segs, freq);
            prop_assert!(warns.is_empty());
            let expected_im = TWO_PI * freq * l;
            prop_assert!((loads[0].re - r).abs() < 1e-9,
                "Re expected {r}, got {}", loads[0].re);
            prop_assert!((loads[0].im - expected_im).abs() < expected_im.abs() * 1e-9 + 1e-9,
                "Im expected {expected_im}, got {}", loads[0].im);
        }

        /// LD type 3 (series RC): Re(Z) = R, Im(Z) ≤ 0 for C > 0.
        #[test]
        fn proptest_ld_type3_re_equals_resistance_and_im_is_nonpositive(
            r in 0.0_f64..1e9_f64,
            c in 1e-15_f64..1e-6_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let segs = vec![proptest_seg()];
            let deck = deck_with_ld(LdCard {
                load_type: 3,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: r, f2: 0.0, f3: c,
            });
            let (loads, warns) = build_loads(&deck, &segs, freq);
            prop_assert!(warns.is_empty());
            prop_assert!((loads[0].re - r).abs() < 1e-9,
                "Re expected {r}, got {}", loads[0].re);
            prop_assert!(loads[0].im <= 0.0,
                "RC reactance should be non-positive, got {}", loads[0].im);
        }

        /// LD type 5 (wire conductivity): a lossy conductor is passive and
        /// inductive — Re(Z) > 0 and Im(Z) ≥ 0 (skin reactance) for σ > 0.
        #[test]
        fn proptest_ld_type5_positive_sigma_gives_passive_inductive_impedance(
            sigma in 1.0_f64..1e10_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let segs = vec![proptest_seg()];
            let deck = deck_with_ld(LdCard {
                load_type: 5,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: sigma, f2: 0.0, f3: 0.0,
            });
            let (loads, warns) = build_loads(&deck, &segs, freq);
            prop_assert!(warns.is_empty());
            prop_assert!(loads[0].re > 0.0,
                "Re should be positive for σ={sigma}, got {}", loads[0].re);
            prop_assert!(loads[0].im >= 0.0,
                "skin reactance should be non-negative, got {}", loads[0].im);
        }

        /// LD type 5: the internal resistance never drops below the DC value
        /// dl/(σπa²) — skin effect only raises it with frequency.
        #[test]
        fn proptest_ld_type5_never_below_dc_resistance(
            sigma in 1.0_f64..1e10_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let segs = vec![proptest_seg()];
            let seg_len = 0.1_f64;
            let seg_radius = 0.001_f64;
            let deck = deck_with_ld(LdCard {
                load_type: 5,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: sigma, f2: 0.0, f3: 0.0,
            });
            let (loads, warns) = build_loads(&deck, &segs, freq);
            prop_assert!(warns.is_empty());
            let r_dc = seg_len / (sigma * std::f64::consts::PI * seg_radius * seg_radius);
            prop_assert!(loads[0].re >= r_dc * (1.0 - 1e-9),
                "Re {} below DC floor {r_dc}", loads[0].re);
        }

        /// LD type 1 (parallel RLC): Re(Z) ≥ 0 for passive element values.
        #[test]
        fn proptest_ld_type1_parallel_rlc_is_passive(
            r in 0.0_f64..1e9_f64,
            l in 0.0_f64..1e-3_f64,
            c in 0.0_f64..1e-6_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let segs = vec![proptest_seg()];
            let deck = deck_with_ld(LdCard {
                load_type: 1,
                tag: 1, seg_first: 1, seg_last: 1,
                f1: r, f2: l, f3: c,
            });
            let (loads, _warns) = build_loads(&deck, &segs, freq);
            // A passive parallel network must have non-negative Re(Z).
            prop_assert!(loads[0].re >= -1e-12,
                "Re(Z) of passive parallel RLC should be ≥ 0, got {}", loads[0].re);
        }
    }
}
