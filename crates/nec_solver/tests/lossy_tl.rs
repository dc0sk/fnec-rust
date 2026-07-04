// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH8-CHK-005: lossy transmission line (tl_type != 0).

use nec_model::card::{Card, GwCard, TlCard};
use nec_model::deck::NecDeck;
use nec_solver::{build_geometry, build_tl_stamps};
use num_complex::Complex64;

const FREQ_HZ: f64 = 14.2e6;

fn two_wire() -> NecDeck {
    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 51,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    deck.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 51,
        start: [1.0, 0.0, -5.282],
        end: [1.0, 0.0, 5.282],
        radius: 0.001,
    }));
    deck
}

fn tl(tl_type: u32, z0: f64, length: f64, f3: f64) -> TlCard {
    TlCard {
        tag1: 1,
        segment1: 26,
        tag2: 2,
        segment2: 26,
        num_segments: 1,
        tl_type,
        z0,
        length,
        f3,
    }
}

fn stamps_for(card: TlCard) -> Vec<(usize, usize, Complex64)> {
    let mut deck = two_wire();
    deck.cards.push(Card::Tl(card));
    let segs = build_geometry(&deck).expect("geometry");
    let (stamps, warn) = build_tl_stamps(&deck, &segs, FREQ_HZ);
    assert!(warn.is_empty(), "unexpected TL warnings: {warn:?}");
    stamps
}

#[test]
fn lossy_tl_zero_loss_matches_lossless() {
    // A lossy line with 0 dB loss (α=0) must stamp identically to the lossless
    // line (velocity factor 1) — the coth/csch form reduces to −jZ0·cot/csc.
    let lossless = stamps_for(tl(0, 50.0, 3.0, 1.0)); // f3 = vf = 1.0
    let lossy0 = stamps_for(tl(1, 50.0, 3.0, 0.0)); //   f3 = 0 dB loss
    assert_eq!(lossless.len(), 4);
    for (a, b) in lossless.iter().zip(lossy0.iter()) {
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
        assert!(
            (a.2 - b.2).norm() < 1e-9,
            "zero-loss lossy stamp {} != lossless {}",
            b.2,
            a.2
        );
    }
}

#[test]
fn lossy_tl_reduces_through_coupling() {
    // Loss attenuates the far-end coupling, so |Z12| (the off-diagonal stamp)
    // decreases monotonically with loss.
    let z12 = |loss_db: f64| {
        let s = stamps_for(tl(1, 50.0, 3.0, loss_db));
        s.iter().find(|&&(r, c, _)| r != c).unwrap().2.norm()
    };
    let (a, b, c) = (z12(0.0), z12(3.0), z12(10.0));
    assert!(
        b < a && c < b,
        "|Z12| should fall with loss: {a:.3} {b:.3} {c:.3}"
    );
}

#[test]
fn lossy_tl_high_loss_input_tends_to_z0() {
    // A very lossy line hides its far end: the input impedance Z11 = Z0·coth(γℓ)
    // → Z0 as the loss grows (coth of a large real part → 1).
    let s = stamps_for(tl(1, 50.0, 3.0, 60.0)); // 60 dB — effectively matched
    let z11 = s.iter().find(|&&(r, c, _)| r == c).unwrap().2;
    assert!(
        (z11 - Complex64::new(50.0, 0.0)).norm() < 0.5,
        "high-loss Z11 {z11} should ≈ Z0=50"
    );
}
