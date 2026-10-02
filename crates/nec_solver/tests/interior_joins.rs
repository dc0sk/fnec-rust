// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-192 — a wire end or a crossing at another wire's interior JOINT is a
//! connection.
//!
//! NEC connects coincident segment ends wherever they are on their wires. A T
//! drawn as a bar with the stem's end on one of the bar's joints (the usual way,
//! in EZNEC-style decks) was solved by fnec as two unconnected wires, silently:
//! 12.31 − j1123.3 Ω where nec2c gives 23.00 − j66.87. `wire_endpoints_from_segs`
//! now breaks a wire at any interior joint where another wire's segment starts or
//! ends, and the collinear merge no longer welds the halves back over the stem, so
//! every consumer of the wire list sees the junction. A touch in the MIDDLE of a
//! segment stays unconnected, as in NEC.
//!
//! Expectations are nec2c 1.3.1, 14.2 MHz, captured 2026-10-02.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck, solve_hallen_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn parse(body: &str) -> (nec_model::deck::NecDeck, Vec<nec_solver::Segment>) {
    let deck = nec_parser::parse(&format!("CE\n{body}FR 0 1 0 0 14.2 0\nEN\n"))
        .expect("parses")
        .deck;
    let segs = build_geometry(&deck).expect("geometry");
    (deck, segs)
}

fn z_in(body: &str, feed: (u32, u32)) -> Complex64 {
    let (deck, segs) = parse(body);
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, FREQ, &[]).expect("solves");
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Complex64::new(1.0, 0.0) / routed.currents[idx]
}

fn rel(a: Complex64, b: Complex64) -> f64 {
    (a - b).norm() / b.norm()
}

/// A 10 m bar of 2n segments with a 5 m stem standing on its middle joint.
fn t_drawn_through(n: u32) -> String {
    format!(
        "GW 1 {} -5 0 0 5 0 0 .001\nGW 2 {n} 0 0 0 0 0 5 .001\nGE 0\n",
        2 * n
    )
}

/// The same T with the bar drawn as two halves.
fn t_drawn_as_halves(n: u32) -> String {
    format!(
        "GW 1 {n} -5 0 0 0 0 0 .001\nGW 3 {n} 0 0 0 5 0 0 .001\nGW 2 {n} 0 0 0 0 0 5 .001\nGE 0\n"
    )
}

fn feed(n: u32) -> String {
    format!("EX 0 2 {} 0 1 0\n", n / 2 + 1)
}

/// The T drawn either way is the same antenna, to every digit, and it converges
/// on nec2c: 3.7 → 1.7 % at 21 → 41 (34.94 − j108.28 and 35.07 − j105.63 against
/// 35.40 − j104.24 and 35.31 − j103.76). Before: 12.3 − j1123 drawn through.
#[test]
fn a_t_drawn_through_a_bar_joint_is_the_t() {
    for n in [21, 41] {
        let through = z_in(
            &format!("{}{}", t_drawn_through(n), feed(n)),
            (2, n / 2 + 1),
        );
        let halves = z_in(
            &format!("{}{}", t_drawn_as_halves(n), feed(n)),
            (2, n / 2 + 1),
        );
        assert!(
            rel(through, halves) < 1e-9,
            "n={n}: {through} against {halves}"
        );
    }
    let e21 = rel(
        z_in(&format!("{}{}", t_drawn_through(21), feed(21)), (2, 11)),
        Complex64::new(35.402, -104.24),
    );
    let e41 = rel(
        z_in(&format!("{}{}", t_drawn_through(41), feed(41)), (2, 21)),
        Complex64::new(35.312, -103.76),
    );
    assert!(
        e41 < 0.03 && e41 < e21,
        "T against nec2c: {e21:.4} -> {e41:.4}"
    );
}

/// Two wires crossing at a joint each has are four arms meeting, as nec2c
/// connects them; drawn either way, the same antenna. Against nec2c: 4.0 → 2.3 %
/// at 21 → 41.
#[test]
fn an_x_crossing_at_a_shared_joint_is_four_arms() {
    let x2 = |n: u32| {
        format!(
            "GW 1 {m} -5 0 0 5 0 0 .001\nGW 2 {m} 0 0 -5 0 0 5 .001\nGE 0\nEX 0 2 {f} 0 1 0\n",
            m = 2 * n,
            f = n / 2 + 1
        )
    };
    let x4 = |n: u32| {
        format!(
            "GW 1 {n} -5 0 0 0 0 0 .001\nGW 3 {n} 0 0 0 5 0 0 .001\nGW 2 {n} 0 0 -5 0 0 0 .001\n\
             GW 4 {n} 0 0 0 0 0 5 .001\nGE 0\nEX 0 2 {f} 0 1 0\n",
            f = n / 2 + 1
        )
    };
    let (a, b) = (z_in(&x2(21), (2, 11)), z_in(&x4(21), (2, 11)));
    assert!(rel(a, b) < 1e-9, "{a} against {b}");
    let e21 = rel(a, Complex64::new(81.651, -78.877));
    let e41 = rel(z_in(&x2(41), (2, 21)), Complex64::new(81.601, -78.622));
    assert!(
        e41 < 0.03 && e41 < e21,
        "X against nec2c: {e21:.4} -> {e41:.4}"
    );
}

/// The junction checks use the merged wire list; the merge must not weld the bar
/// back together over the stem.
#[test]
fn the_merged_wire_list_sees_the_junction() {
    let (_, segs) = parse(&format!("{}{}", t_drawn_through(21), feed(21)));
    assert!(nec_solver::validate::has_wire_junction(&segs));
}

/// A plane wave on the T drawn through the bar is the plane wave on the T drawn as
/// halves: every current, to 1e-9. It used to solve the unconnected wires; since
/// FND-162 stage 5 a junction's receive solve runs on the section graph.
#[test]
fn a_plane_wave_on_a_drawn_through_t_is_received_as_the_t() {
    let receive = |geometry: String| {
        let (deck, segs) = parse(&format!("{geometry}EX 1 1 1 0 45 0 0\n"));
        let z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
        nec_solver::solve_hallen_planewave_routed(&deck, &segs, &z, FREQ).expect("receives")
    };
    // Both decks list the bar's segments left to right, then the stem's.
    let (through, halves) = (receive(t_drawn_through(21)), receive(t_drawn_as_halves(21)));
    assert_eq!(through.len(), halves.len());
    let peak = halves.iter().map(|i| i.norm()).fold(0.0, f64::max);
    for (k, (a, b)) in through.iter().zip(&halves).enumerate() {
        assert!((a - b).norm() < 1e-9 * peak, "segment {k}: {a} against {b}");
    }
}
