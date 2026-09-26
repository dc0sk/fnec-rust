// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH8-CHK-005: lossy transmission line — fnec's F8 extension (matched-line loss
// in dB) on the NEC-2 TL layout (FND-111). Checked on the line's admittance
// parameters, which is what the network solve consumes (FND-123).

use nec_model::card::{Card, GwCard, TlCard};
use nec_model::deck::NecDeck;
use nec_solver::{build_geometry, build_networks, TwoPort};
use num_complex::Complex64;

const FREQ_HZ: f64 = 14.2e6;

fn two_wire() -> NecDeck {
    let mut deck = NecDeck::new();
    for (tag, x) in [(1u32, 0.0), (2u32, 1.0)] {
        deck.cards.push(Card::Gw(GwCard {
            tag,
            segments: 51,
            start: [x, 0.0, -5.282],
            end: [x, 0.0, 5.282],
            radius: 0.001,
        }));
    }
    deck
}

fn line(z0: f64, length: f64, loss_db: f64) -> TwoPort {
    let mut deck = two_wire();
    deck.cards.push(Card::Tl(TlCard {
        tag1: 1,
        segment1: 26,
        tag2: 2,
        segment2: 26,
        z0,
        length,
        shunt1: (0.0, 0.0),
        shunt2: (0.0, 0.0),
        velocity_factor: 1.0,
        loss_db,
    }));
    let segs = build_geometry(&deck).expect("geometry");
    let (nets, _) = build_networks(&deck, &segs, FREQ_HZ).expect("a usable line");
    nets.two_ports[0].clone()
}

#[test]
fn a_lossy_line_at_zero_loss_is_the_lossless_line() {
    let k = 2.0 * std::f64::consts::PI * FREQ_HZ / 299_792_458.0;
    let theta = k * 3.0;
    let tp = line(50.0, 3.0, 0.0);
    assert!((tp.y11 - Complex64::new(0.0, -1.0 / theta.tan() / 50.0)).norm() < 1e-12);
    assert!((tp.y12 - Complex64::new(0.0, 1.0 / theta.sin() / 50.0)).norm() < 1e-12);
}

#[test]
fn loss_attenuates_the_transfer_admittance() {
    let y12 = |loss_db: f64| line(50.0, 3.0, loss_db).y12.norm();
    let (a, b, c) = (y12(0.0), y12(3.0), y12(10.0));
    assert!(
        b < a && c < b,
        "|Y12| should fall with loss: {a:.4} {b:.4} {c:.4}"
    );
}

/// A very lossy line hides its far end: its input admittance tends to 1/Z0.
#[test]
fn a_very_lossy_line_looks_matched() {
    let tp = line(50.0, 3.0, 60.0);
    assert!(
        (tp.y11 - Complex64::new(1.0 / 50.0, 0.0)).norm() < 1e-4,
        "high-loss Y11 {} should ≈ 1/Z0",
        tp.y11
    );
    assert!(
        tp.y12.norm() < 1e-4,
        "and couple almost nothing: {}",
        tp.y12
    );
}
