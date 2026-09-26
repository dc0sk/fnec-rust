// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-123 — `TL`/`NT` networks solved as networks, across the port gaps.
//!
//! These gates are chosen so that none of them re-derives the answer the code
//! computes. "The result equals Ys + Yn combined as circuits" would be vacuous:
//! the implementation does exactly that algebra on the same port solves, so a
//! wrong structure response would pass it. Instead:
//!
//! - **One-port NT ≡ LD.** A one-port of admittance 1/Z_L across a gap forces
//!   V = −Z_L·I there, which is what `LD 4 Z_L` does by a DIFFERENT code path
//!   (a matrix column, gated by the port identity and nec2c in FND-122/124). A
//!   sign error in the superposition gives Y/(Y−g) instead of Y/(Y+g).
//! - **Shunt across the feed.** Analytic: Z = 1/(1/Z_ant + Y). This is the
//!   driven-port bookkeeping — the source current must include the branch.
//! - **Both ends on the feed segment** is a one-port of Y11 + Y22 + 2·Y12, which
//!   nec2c confirms.
//! - **nec2c** on the coupled pair with a line between the feeds.
//!
//! The one-port equivalence is not exact, and the reason is worth stating: the
//! Hallén system is least-squares and slightly inconsistent, so the LS solution
//! of `(A + c·eₚᵀ)·x = b` (the load) differs from superposed LS solutions of
//! `A·x = b + V·c` (the network) by the inconsistency — 0.07% on a straight
//! wire, 0.5% on a start-to-start split, the same effect FND-118 met.

use nec_solver::{
    assemble_z_matrix_with_ground, build_deck_stamps, build_geometry, ground_model_from_deck,
    solve_hallen_routed, HallenSessionError,
};
use num_complex::Complex64;

const F: f64 = 14.2e6;
const DIPOLE: &str = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\n";
const PAIR: &str =
    "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1 0 -5.282 1 0 5.282 0.001\nGE\n";
const SPLIT_V: &str = "CE\nGW 1 21 0 0 3 -5 0 0 .001\nGW 2 21 0 0 3 5 0 0 .001\nGE\n";

fn z_in(geometry: &str, cards: &str, feed: (u32, u32)) -> Result<Complex64, HallenSessionError> {
    let text = format!(
        "{geometry}{cards}EX 0 {} {} 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
        feed.0, feed.1
    );
    let deck = nec_parser::parse(&text).expect("deck parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, F, &ground_model_from_deck(&deck));
    let loads = build_deck_stamps(&deck, &segs, F).diagonal;
    let routed = solve_hallen_routed(&deck, &segs, &mut z, F, &loads)?;
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed segment");
    Ok(Complex64::new(1.0, 0.0) / routed.source_current(idx))
}

fn ok(geometry: &str, cards: &str, feed: (u32, u32)) -> Complex64 {
    z_in(geometry, cards, feed).expect("solves")
}

#[test]
fn a_one_port_nt_is_the_series_load_it_describes() {
    let load = ok(DIPOLE, "LD 4 1 13 13 100 0 0\n", (1, 26));
    let net = ok(DIPOLE, "NT 1 13 1 13 0.01 0 0 0 0 0\n", (1, 26));
    assert!(
        (load - net).norm() < 0.25,
        "a 1/100 S one-port at segment 13 must act as LD 100 Ω there: LD {load:.3}, NT {net:.3}"
    );
    // Well away from the no-network answer, so the agreement is not trivial.
    assert!((net - ok(DIPOLE, "", (1, 26))).norm() > 20.0);
}

/// The conductor-path route, on BOTH arms of a start-to-start split — one of
/// which is walked in reverse, so a path-sign slip in the unit gap shows here.
#[test]
fn a_one_port_nt_is_the_series_load_on_a_conductor_path_too() {
    for (tag, seg) in [(1, 3), (2, 5)] {
        let load = ok(
            SPLIT_V,
            &format!("LD 4 {tag} {seg} {seg} 100 0 0\n"),
            (1, 11),
        );
        let net = ok(
            SPLIT_V,
            &format!("NT {tag} {seg} {tag} {seg} 0.01 0 0 0 0 0\n"),
            (1, 11),
        );
        assert!(
            (load - net).norm() / load.norm() < 0.015,
            "arm {tag}: LD {load:.3} vs one-port NT {net:.3}"
        );
    }
}

#[test]
fn a_shunt_across_the_feed_is_in_parallel_with_the_antenna() {
    let bare = ok(DIPOLE, "", (1, 26));
    let y = Complex64::new(0.01, -0.004);
    let net = ok(
        DIPOLE,
        &format!("NT 1 26 1 26 {} {} 0 0 0 0\n", y.re, y.im),
        (1, 26),
    );
    let want = Complex64::new(1.0, 0.0) / (Complex64::new(1.0, 0.0) / bare + y);
    assert!((net - want).norm() < 1e-6, "{net} vs analytic {want}");
}

/// Both ends of a two-port on the feed segment: a one-port of Y11 + Y22 + 2·Y12.
#[test]
fn both_ports_on_the_feed_segment_sum_to_a_one_port() {
    let bare = ok(DIPOLE, "", (1, 26));
    let net = ok(DIPOLE, "NT 1 26 1 26 0 -0.002 0 0.003 0 -0.001\n", (1, 26));
    let y = Complex64::new(0.0, -0.002 - 0.001 + 2.0 * 0.003);
    let want = Complex64::new(1.0, 0.0) / (Complex64::new(1.0, 0.0) / bare + y);
    assert!((net - want).norm() < 1e-6, "{net} vs {want}");
}

/// nec2c 1.3.1, `TL 1 26 2 26 50.0 0.1` across two λ/2 dipoles 1 m apart, fed on
/// wire 1: 84.826 + j31.131 (captured 2026-09-26). The unloaded pair differs from
/// nec2c by 0.9 / 5.1 Ω (FND-156's residual), which bounds how close this can be.
/// The old series-Z stamp gave 3.67 − j14.49 here.
#[test]
fn a_line_between_two_feeds_tracks_nec2c() {
    let z = ok(PAIR, "TL 1 26 2 26 50.0 0.1\n", (1, 26));
    assert!(
        (z.re - 84.826).abs() < 2.0 && (z.im - 31.131).abs() < 5.0,
        "pair + TL: {z:.3}, nec2c 84.826 + j31.131"
    );
}

/// Loads are stamped once, however many unit-gap solves the networks need: a
/// feed load still shifts Z_in by exactly itself with a line elsewhere.
#[test]
fn a_feed_load_still_shifts_z_by_itself_with_a_network_present() {
    let line = "TL 1 10 2 10 75.0 0.3\n";
    let bare = ok(PAIR, line, (1, 26));
    let loaded = ok(PAIR, &format!("{line}LD 4 1 26 26 50 25 0\n"), (1, 26));
    let d = loaded - bare;
    assert!(
        (d - Complex64::new(50.0, 25.0)).norm() < 0.05,
        "a 50 + j25 feed load shifted Z by {d:.4} with a TL present"
    );
}

#[test]
fn networks_with_other_drives_are_refused_not_approximated() {
    for drive in ["EX 1 1 1 0 90 0 0\n", "EX 4 1 26 0 1.0 0.0\n"] {
        let text = format!("{PAIR}TL 1 26 2 26 50.0 0.1\n{drive}FR 0 1 0 0 14.2 0\nEN\n");
        let deck = nec_parser::parse(&text).expect("deck parses").deck;
        let segs = build_geometry(&deck).expect("geometry");
        let mut z = assemble_z_matrix_with_ground(&segs, F, &ground_model_from_deck(&deck));
        let err = solve_hallen_routed(&deck, &segs, &mut z, F, &[]).expect_err("refused");
        assert!(err.to_string().contains("delta-gap"), "{err}");
    }
}
