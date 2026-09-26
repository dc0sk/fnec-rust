// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH8-CHK-004: `NT` networks, read as the admittance parameters they are (FND-123).
//
// These check the card reading only. Whether a network then does the right thing
// to the antenna is gated in `network_solve.rs`, against identities and nec2c.

use nec_model::card::{Card, GwCard, NtCard, TlCard};
use nec_model::deck::NecDeck;
use nec_solver::{build_geometry, build_networks};
use num_complex::Complex64;

const FREQ_HZ: f64 = 14.2e6;

fn wire() -> NecDeck {
    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 51,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    deck
}

fn nt(fields: &[&str]) -> Card {
    Card::Nt(NtCard {
        raw_fields: fields.iter().map(|s| (*s).to_string()).collect(),
    })
}

/// An NT given a line's own admittance parameters is that line.
#[test]
fn an_nt_with_a_lines_admittance_is_that_line() {
    let mut tl_deck = wire();
    tl_deck.cards.push(Card::Tl(TlCard {
        tag1: 1,
        segment1: 10,
        tag2: 1,
        segment2: 40,
        z0: 50.0,
        length: 5.0,
        shunt1: (0.0, 0.0),
        shunt2: (0.0, 0.0),
        velocity_factor: 1.0,
        loss_db: 0.0,
    }));
    let segs = build_geometry(&tl_deck).unwrap();
    let (tl_nets, _) = build_networks(&tl_deck, &segs, FREQ_HZ).unwrap();
    let t = &tl_nets.two_ports[0];
    let f = |z: Complex64| [format!("{}", z.re), format!("{}", z.im)];
    let (a, b, c) = (f(t.y11), f(t.y12), f(t.y22));
    let mut nt_deck = wire();
    nt_deck.cards.push(nt(&[
        "1", "10", "1", "40", &a[0], &a[1], &b[0], &b[1], &c[0], &c[1],
    ]));
    let (nt_nets, _) = build_networks(&nt_deck, &segs, FREQ_HZ).unwrap();
    assert_eq!(nt_nets, tl_nets);
}

/// Every unusable NT is an error naming the card — never a skip, which would
/// solve a different antenna from the one the deck describes.
#[test]
fn an_unusable_nt_is_an_error_not_a_skip() {
    let segs = build_geometry(&wire()).unwrap();
    for (fields, needle) in [
        (
            &["1", "1", "26", "1", "1", "26", "50.0", "0.0"][..],
            "needs 10",
        ),
        (
            &["1", "x", "1", "40", "0", "0", "0", "0", "0", "0"][..],
            "segment identifier",
        ),
        (
            &["1", "10", "1", "40", "0", "q", "0", "0", "0", "0"][..],
            "not a number",
        ),
        (
            &["1", "10", "7", "40", "0", "0", "0", "0", "0", "0"][..],
            "not in the geometry",
        ),
    ] {
        let mut deck = wire();
        deck.cards.push(nt(fields));
        let err = build_networks(&deck, &segs, FREQ_HZ).unwrap_err();
        assert!(err.contains(needle) && err.starts_with("NT "), "{err}");
    }
}
