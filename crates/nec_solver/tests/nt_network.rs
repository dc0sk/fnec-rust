// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH8-CHK-004: validate the NT two-port network stamp against the (already
// corpus-validated) TL stamp.
//
// An NT card whose admittance parameters are the inverse of a lossless TL's
// impedance parameters must stamp the Z matrix identically to that TL — because
// [Y]^-1 inverts straight back to the TL's [Z]. This validates the Y→Z
// conversion and the stamping topology against a known-good reference, with no
// external dependency.

use nec_model::card::{Card, GwCard, NtCard, TlCard};
use nec_model::deck::NecDeck;
use nec_solver::{build_geometry, build_nt_stamps, build_tl_stamps};

const FREQ_HZ: f64 = 14.2e6;
const C0: f64 = 299_792_458.0;

fn base_geometry() -> NecDeck {
    let mut deck = NecDeck::new();
    // A single wire long enough to host two distinct network ports.
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 51,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    deck
}

// Lossless-TL two-port admittance parameters equivalent to Z0 / length:
//   Z11=Z22=-jZ0 cot θ, Z12=Z21=-jZ0 csc θ  ⇒  [Y]=[Z]^-1:
//   Y11=Y22=-j cot θ / Z0, Y12=Y21=+j csc θ / Z0.
fn tl_equivalent_admittance(z0: f64, length: f64) -> (f64, f64) {
    let k = 2.0 * std::f64::consts::PI * FREQ_HZ / C0;
    let theta = k * length; // vf = 1
    let cot = theta.cos() / theta.sin();
    let csc = 1.0 / theta.sin();
    let y_self_imag = -cot / z0; // Y11 = Y22 = -j cot/Z0
    let y_mut_imag = csc / z0; //  Y12 = Y21 = +j csc/Z0
    (y_self_imag, y_mut_imag)
}

#[test]
fn nt_with_tl_equivalent_admittance_stamps_identically_to_tl() {
    let z0 = 50.0;
    let length = 5.0; // ~0.24 λ; away from any csc/cot singularity

    // TL reference stamps.
    let mut tl_deck = base_geometry();
    tl_deck.cards.push(Card::Tl(TlCard {
        tag1: 1,
        segment1: 10,
        tag2: 1,
        segment2: 40,
        num_segments: 1,
        tl_type: 0,
        z0,
        length,
        f3: 1.0, // velocity factor
    }));
    let segs = build_geometry(&tl_deck).expect("geometry");
    let (tl_stamps, tl_warn) = build_tl_stamps(&tl_deck, &segs, FREQ_HZ);
    assert!(tl_warn.is_empty(), "unexpected TL warnings: {tl_warn:?}");
    assert_eq!(
        tl_stamps.len(),
        4,
        "TL should produce a 2x2 block of stamps"
    );

    // Equivalent NT card (Y = Z_tl^-1). Y-params are pure imaginary here.
    let (y_self_imag, y_mut_imag) = tl_equivalent_admittance(z0, length);
    let nt_fields: Vec<String> = [
        "1".to_string(),          // tag1
        "10".to_string(),         // seg1
        "1".to_string(),          // tag2
        "40".to_string(),         // seg2
        "0.0".to_string(),        // Y11 real
        format!("{y_self_imag}"), // Y11 imag
        "0.0".to_string(),        // Y12 real
        format!("{y_mut_imag}"),  // Y12 imag
        "0.0".to_string(),        // Y22 real
        format!("{y_self_imag}"), // Y22 imag
    ]
    .to_vec();
    let mut nt_deck = base_geometry();
    nt_deck.cards.push(Card::Nt(NtCard {
        raw_fields: nt_fields,
    }));
    let (nt_stamps, nt_warn) = build_nt_stamps(&nt_deck, &segs);
    assert!(nt_warn.is_empty(), "unexpected NT warnings: {nt_warn:?}");
    assert_eq!(nt_stamps.len(), 4);

    // The NT stamps must equal the TL stamps entry for entry.
    for (tl, nt) in tl_stamps.iter().zip(nt_stamps.iter()) {
        assert_eq!(tl.0, nt.0, "row index mismatch");
        assert_eq!(tl.1, nt.1, "col index mismatch");
        let d = (tl.2 - nt.2).norm();
        assert!(
            d < 1e-9,
            "stamp Z mismatch at ({},{}): TL={} NT={} (|Δ|={d:.2e})",
            tl.0,
            tl.1,
            tl.2,
            nt.2
        );
    }
}

#[test]
fn nt_singular_admittance_is_skipped_with_warning() {
    // A rank-deficient Y (Y11=Y22=Y12) has det=0 and cannot be inverted to Z.
    let segs = build_geometry(&base_geometry()).expect("geometry");
    let mut deck = base_geometry();
    deck.cards.push(Card::Nt(NtCard {
        raw_fields: vec![
            "1", "10", "1", "40", "1.0", "0.0", "1.0", "0.0", "1.0", "0.0",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    }));
    let (stamps, warn) = build_nt_stamps(&deck, &segs);
    assert!(stamps.is_empty(), "singular NT must not stamp");
    assert_eq!(warn.len(), 1);
    assert!(
        warn[0].message.contains("singular admittance"),
        "expected singular-admittance warning, got: {}",
        warn[0].message
    );
}

#[test]
fn nt_missing_endpoint_is_skipped_with_warning() {
    let segs = build_geometry(&base_geometry()).expect("geometry");
    let mut deck = base_geometry();
    // tag 9 does not exist in the geometry.
    deck.cards.push(Card::Nt(NtCard {
        raw_fields: vec![
            "9", "1", "1", "40", "0.0", "-0.02", "0.0", "0.02", "0.0", "-0.02",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    }));
    let (stamps, warn) = build_nt_stamps(&deck, &segs);
    assert!(stamps.is_empty());
    assert_eq!(warn.len(), 1);
    assert!(warn[0].message.contains("not found in geometry"));
}
