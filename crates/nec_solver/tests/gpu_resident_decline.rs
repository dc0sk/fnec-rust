// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-297 a / FND-293: `gpu_resident_decline` is the one answer to "does the
//! GPU-resident solve take this deck" — for the CLI's automatic pick, its matrix
//! fill, its resident attempt and the worker. It asks the stamps by value, so a
//! load that stamps zero is no load, while any load that stamps something, and any
//! network, keeps the deck off the device.
//!
//! Each `None` case is paired with `Some` cases, so a predicate that never declines
//! (or always does) fails here.

use nec_model::deck::NecDeck;
use nec_solver::{build_geometry, gpu_resident_decline, ground_model_from_deck, LaplaceLoad};

const F_HZ: f64 = 14.2e6;

/// 801 segments, above this host class's crossover; `extra` goes after `GE 0`.
fn dipole(extra: &str) -> NecDeck {
    let text = format!(
        "CE\nGW 1 801 0 0 -5.282 0 0 5.282 .001\nGE 0\n{extra}EX 0 1 401 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n"
    );
    nec_parser::parse(&text).expect("deck parses").deck
}

fn decline(deck: &NecDeck, freqs_hz: &[f64], laplace: &[LaplaceLoad]) -> Option<&'static str> {
    let segs = build_geometry(deck).expect("geometry");
    gpu_resident_decline(
        deck,
        &segs,
        &ground_model_from_deck(deck),
        freqs_hz,
        laplace,
    )
}

/// A zero-valued `LD 4` stamps nothing: the deck and its LD-free twin get the same
/// answer, the device's. The CLI's automatic pick used to decline the first on
/// its card type alone.
#[test]
fn a_load_that_stamps_zero_is_no_load() {
    assert_eq!(decline(&dipole(""), &[F_HZ], &[]), None);
    assert_eq!(decline(&dipole("LD 4 1 200 200 0 0\n"), &[F_HZ], &[]), None);
}

/// A load that stamps a reactance is declined, by name.
#[test]
fn a_load_that_stamps_a_value_is_declined() {
    let why = decline(&dipole("LD 4 1 200 200 0 1000\n"), &[F_HZ], &[])
        .expect("a 1000 Ω reactance stamps the matrix");
    assert!(why.contains("stamps loads or networks"), "{why}");
}

/// An `NT` is declined even with zero admittances: a network inserts itself into
/// the port gaps whatever its values (FND-123). The card-type list the solve once
/// used omitted `NT` (FND-023).
#[test]
fn a_network_is_declined_whatever_its_values() {
    for nt in [
        "NT 1 200 1 600 0 0.01 0 0 0 0.01\n",
        "NT 1 200 1 600 0 0 0 0 0 0\n",
    ] {
        let why = decline(&dipole(nt), &[F_HZ], &[]).unwrap_or_else(|| panic!("{nt}"));
        assert!(why.contains("stamps loads or networks"), "{nt}: {why}");
    }
}

/// Over a sweep: a zero-valued load stays `None` at every point, a series
/// inductor (ωL, non-zero everywhere) is declined, and with no frequency given
/// only the class is asked.
#[test]
fn a_sweep_is_asked_at_its_points() {
    let sweep = [7.0e6, 14.2e6, 21.0e6];
    assert_eq!(decline(&dipole("LD 4 1 200 200 0 0\n"), &sweep, &[]), None);
    // A series-L load: its reactance is ωL, non-zero at every point.
    assert!(decline(&dipole("LD 0 1 200 200 0 1e-6 0\n"), &sweep, &[]).is_some());
    // No frequency asked: only the class decides.
    assert_eq!(
        decline(&dipole("LD 0 1 200 200 0 1e-6 0\n"), &[], &[]),
        None
    );
}

/// A Laplace load (`--loads-config`) is declined: the device would not see it.
#[test]
fn a_laplace_load_is_declined() {
    let load = LaplaceLoad {
        tag: 1,
        seg_first: 200,
        seg_last: 200,
        numerator: vec![50.0],
        denominator: vec![1.0],
    };
    let why = decline(&dipole(""), &[F_HZ], &[load]).expect("a Laplace load stamps");
    assert!(why.contains("Laplace"), "{why}");
}

/// Outside the class — a bend, or a ground — the class's own reason is given.
#[test]
fn a_deck_outside_the_class_is_declined_with_the_class_reason() {
    let bent = nec_parser::parse(
        "CE\nGW 1 400 0 0 0 5 0 0 .001\nGW 2 400 5 0 0 5 0 5 .001\nGE 0\nEX 0 1 200 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    )
    .expect("deck parses")
    .deck;
    let why = decline(&bent, &[F_HZ], &[]).expect("a bend takes the path basis");
    assert!(
        why.contains("Hallén route the device does not implement"),
        "{why}"
    );

    let over_ground = nec_parser::parse(
        "CE\nGW 1 801 0 0 1 0 0 11.564 .001\nGE 0\nGN 1\nEX 0 1 401 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    )
    .expect("deck parses")
    .deck;
    let why = decline(&over_ground, &[F_HZ], &[]).expect("PEC ground");
    assert!(why.contains("free space only"), "{why}");
}

/// Every point of the run is asked, not only the first: a series LC of 1 H and
/// 1 F stamps exactly zero at ω = 1 rad/s (f = 1/2π Hz, where `2π·f` rounds to
/// 1.0 and ωL − 1/(ωC) is 0.0) and a reactance everywhere else. So the device
/// takes the resonant point alone, and declines any run that also holds another
/// frequency — wherever in the list it sits.
#[test]
fn every_point_of_the_run_is_asked() {
    let resonant_hz = 1.0 / (2.0 * std::f64::consts::PI);
    let lc = dipole("LD 0 1 200 200 0 1 1\n");
    assert_eq!(decline(&lc, &[resonant_hz], &[]), None);
    assert!(decline(&lc, &[resonant_hz, F_HZ], &[]).is_some());
    assert!(decline(&lc, &[F_HZ, resonant_hz], &[]).is_some());
}
