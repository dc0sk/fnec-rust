// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-276: a remedy may name the MPIE only for a deck the MPIE solves. The
// predicate was deck-only and recommended the MPIE for an end-fed arc, a feed
// between two junctions and a wire on the ground, each of which the MPIE then
// refused. It now asks the solve's own refusals (`mpie_plan`), on the MPIE's own
// geometry. This pins the agreement deck by deck, and which refusal each row
// meets — so a refusal added to the solve and not to the plan fails here.
//
// It does not guard the refusals themselves: drop one from the plan and both
// sides agree again. `end_segment_source.rs` and `mpie_nec2c.rs` pin those.

use nec_solver::validate::mpie_compatible_deck;
use nec_solver::{
    build_deck_geometry, frequencies_hz, ground_model_from_deck, solve_mpie_session,
    MpieSessionError,
};

/// The refusal a deck must meet, or `None` where the MPIE solves it.
type Refusal = Option<fn(&MpieSessionError) -> bool>;

const ARC: &str = "GA 1 10 0.5 0 180 .001";

fn deck(geometry: &str, ground: &str, ex: &str) -> String {
    format!("CE\n{geometry}\n{ground}\n{ex}\nFR 0 1 0 0 30 0\nEN\n")
}

#[test]
fn the_remedy_check_agrees_with_the_mpie_solve() {
    let dipole = "GW 1 11 0 0 -2.5 0 0 2.5 .001";
    let h_bar = "GW 1 10 0 0 0 0 0 1 .001\nGW 2 10 1 0 0 1 0 1 .001\nGW 3 1 0 0 .5 1 0 .5 .001";
    let cases: Vec<(&str, String, Refusal)> = vec![
        (
            "dipole, centre",
            deck(dipole, "GE 0", "EX 0 1 6 0 1 0"),
            None,
        ),
        ("arc, one in", deck(ARC, "GE 0", "EX 0 1 2 0 1 0"), None),
        (
            "dipole, end",
            deck(dipole, "GE 0", "EX 0 1 1 0 1 0"),
            Some(|e| matches!(e, MpieSessionError::EndSegmentFeed { .. })),
        ),
        (
            "arc, end",
            deck(ARC, "GE 0", "EX 0 1 1 0 1 0"),
            Some(|e| matches!(e, MpieSessionError::EndSegmentFeed { .. })),
        ),
        (
            "bar between junctions",
            deck(h_bar, "GE 0", "EX 0 3 1 0 1 0"),
            Some(|e| matches!(e, MpieSessionError::NoInteriorNode)),
        ),
        (
            "arc on perfect ground",
            deck(ARC, "GE 1\nGN 1", "EX 0 1 5 0 1 0"),
            Some(|e| matches!(e, MpieSessionError::Solve(_))),
        ),
        (
            "no such segment",
            deck(dipole, "GE 0", "EX 0 1 99 0 1 0"),
            Some(|e| matches!(e, MpieSessionError::DrivenSegmentNotFound { .. })),
        ),
        (
            "plane wave",
            deck(dipole, "GE 0", "EX 1 1 1 0 0 0"),
            Some(|e| matches!(e, MpieSessionError::Unsupported(_))),
        ),
        (
            "current source",
            deck(dipole, "GE 0", "EX 4 1 6 0 1 0"),
            Some(|e| matches!(e, MpieSessionError::Unsupported(_))),
        ),
        (
            "transmission line",
            deck(dipole, "GE 0", "TL 1 3 1 9 50 0\nEX 0 1 6 0 1 0"),
            Some(|e| matches!(e, MpieSessionError::Unsupported(_))),
        ),
        (
            "no source",
            deck(dipole, "GE 0", ""),
            Some(|e| matches!(e, MpieSessionError::NoVoltageSource)),
        ),
    ];
    for (name, text, refused) in cases {
        let d = nec_parser::parse(&text).expect("parses").deck;
        let segs = build_deck_geometry(&d).expect("geometry");
        let f = frequencies_hz(&d)[0];
        let solve = solve_mpie_session(&d, &segs, &ground_model_from_deck(&d), f, &[]);
        assert_eq!(
            mpie_compatible_deck(&d),
            solve.is_ok(),
            "{name}: remedy check against the solve's {:?}",
            solve.as_ref().err()
        );
        match (refused, &solve) {
            (None, Ok(_)) => {}
            (Some(is), Err(e)) => assert!(is(e), "{name}: refused for {e:?}"),
            (want, got) => panic!("{name}: expected refusal {}, got {got:?}", want.is_some()),
        }
    }
}
