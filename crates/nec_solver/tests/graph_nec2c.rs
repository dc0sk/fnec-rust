// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-162 stages 2+3 — degree-3 junctions and closed loops on a section graph.
//!
//! The path decomposition refuses a node where three or more wire ends meet and a
//! conductor with no free end, and those decks fell back to one homogeneous term
//! per wire with pairwise junction rows: a stem-fed Y answered −1.83 − j1673 Ω
//! against nec2c's 23.66 − j1756, and a 1 λ square loop 17 − j1163 against
//! 111.01 − j146.27. The section graph gives every straight section its own
//! (C, D) and closes each node with Kirchhoff's current law and equal potential,
//! the corner term included; a loop is a cycle of sections.
//!
//! Expectations are nec2c 1.3.1, captured 2026-09-30. Each deck is gated at two
//! meshes and its error must shrink. The kill criteria are the design review's
//! (Fable), whose prototype these numbers reproduce: the Y at 11 segments per
//! wire 20.74 − j1700, the loop at 21 per side 109.3 − j144.5.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck, solve_hallen_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn solve(body: &str) -> (Vec<nec_solver::Segment>, Vec<Complex64>) {
    let deck = nec_parser::parse(&format!("CE\n{body}FR 0 1 0 0 14.2 0\nEN\n"))
        .expect("parses")
        .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    // The deck's lumped loads, as every frontend passes them.
    let loads = nec_solver::build_deck_stamps(&deck, &segs, FREQ).diagonal;
    let routed = solve_hallen_routed(&deck, &segs, &mut z, FREQ, &loads).expect("solves");
    (segs, routed.currents)
}

fn current(segs: &[nec_solver::Segment], currents: &[Complex64], tag: u32, seg: u32) -> Complex64 {
    let idx = segs
        .iter()
        .position(|s| s.tag == tag && s.tag_index == seg)
        .expect("segment");
    currents[idx]
}

fn z_in(body: &str, feed: (u32, u32)) -> Complex64 {
    let (segs, currents) = solve(body);
    Complex64::new(1.0, 0.0) / current(&segs, &currents, feed.0, feed.1)
}

fn rel(a: Complex64, b: Complex64) -> f64 {
    (a - b).norm() / b.norm()
}

fn assert_converges(label: &str, coarse: f64, fine: f64, bound: f64) {
    println!("{label}: {:.2} % -> {:.2} %", coarse * 100.0, fine * 100.0);
    assert!(
        fine < bound,
        "{label}: {:.2} % at the finer mesh",
        fine * 100.0
    );
    assert!(
        fine < coarse,
        "{label}: must shrink, {coarse:.4} -> {fine:.4}"
    );
}

fn y(n: u32) -> String {
    format!(
        "GW 1 {n} 0 0 0 0 0 3 .001\nGW 2 {n} 0 0 3 -2 0 5 .001\nGW 3 {n} 0 0 3 2 0 5 .001\nGE 0\n"
    )
}

fn tee(n: u32) -> String {
    format!(
        "GW 1 {n} 0 0 0 0 0 4 .001\nGW 2 {n} 0 0 4 -3 0 4 .001\nGW 3 {n} 0 0 4 3 0 4 .001\nGE 0\n"
    )
}

/// A 1 λ square loop, side 5.278 m, with its corners at the wire ends.
fn square_loop(n: u32, z0: f64) -> String {
    let (a, z1) = (5.278, z0 + 5.278);
    format!(
        "GW 1 {n} 0 0 {z0} {a} 0 {z0} .001\nGW 2 {n} {a} 0 {z0} {a} 0 {z1} .001\n\
         GW 3 {n} {a} 0 {z1} 0 0 {z1} .001\nGW 4 {n} 0 0 {z1} 0 0 {z0} .001\n"
    )
}

/// Before: −1.83 − j1673 at 11 segments per wire. Kill criterion: under 1 % at
/// 41, shrinking. Measured 3.19 → 1.68 → 0.74 % at 11 / 21 / 41.
#[test]
fn a_stem_fed_y_converges_to_nec2c() {
    let e21 = rel(
        z_in(&format!("{}EX 0 1 5 0 1 0\n", y(21)), (1, 5)),
        Complex64::new(22.582, -1797.6),
    );
    let e41 = rel(
        z_in(&format!("{}EX 0 1 10 0 1 0\n", y(41)), (1, 10)),
        Complex64::new(21.434, -1639.4),
    );
    assert_converges("stem-fed Y", e21, e41, 0.01);
}

/// Before: about 0 − j1300. Kill criterion: under 2 % at 41, shrinking.
/// Measured 6.45 → 2.23 → 1.33 %.
#[test]
fn a_stem_fed_t_converges_to_nec2c() {
    let e21 = rel(
        z_in(&format!("{}EX 0 1 4 0 1 0\n", tee(21)), (1, 4)),
        Complex64::new(28.028, -1621.2),
    );
    let e41 = rel(
        z_in(&format!("{}EX 0 1 7 0 1 0\n", tee(41)), (1, 7)),
        Complex64::new(27.277, -1664.3),
    );
    assert_converges("stem-fed T", e21, e41, 0.02);
}

/// A dipole fed off-centre on one arm with a 2 m stub hanging from its centre:
/// the feed and the junction are on different sections, and the stub carries the
/// current the junction rows must split. Kill criterion: under 4 % at 41,
/// shrinking. Measured 9.26 → 5.11 → 2.94 %.
///
/// A regression gate, not evidence for the graph: the per-wire fallback passes
/// it too (4.29 → 2.87 % with the route switched off), because a feed mid-arm
/// with the stub at the current maximum leaves the junction rows little to get
/// wrong. The other decks here fail with the route off.
#[test]
fn a_dipole_with_a_centre_stub_converges_to_nec2c() {
    let deck = |n: u32, seg: u32| {
        format!(
            "GW 1 {n} -5.25 0 0 0 0 0 .001\nGW 2 {n} 0 0 0 5.25 0 0 .001\n\
             GW 3 {n} 0 0 0 0 0 -2 .001\nGE 0\nEX 0 2 {seg} 0 1 0\n"
        )
    };
    let e21 = rel(z_in(&deck(21, 5), (2, 5)), Complex64::new(89.832, 38.39));
    let e41 = rel(z_in(&deck(41, 8), (2, 8)), Complex64::new(87.144, 37.64));
    assert_converges("T-dipole", e21, e41, 0.04);
}

/// A T fed on the segment touching its node. The junction-feed caveat called
/// this an unreliable V/I; on the section graph it converges like any other
/// feed. Kill criterion: under 5 % at 49 per wire, shrinking. Measured
/// 12.4 → 7.0 → 4.1 % at 13 / 25 / 49.
#[test]
fn a_t_fed_at_its_node_converges_to_nec2c() {
    let deck = |n: u32| {
        format!(
            "GW 1 {n} 0 0 0 5.282 0 0 .001\nGW 2 {n} 0 0 0 -5.282 0 0 .001\n\
             GW 3 {n} 0 0 0 0 0 5.282 .001\nGE 0\nEX 0 1 1 0 1 0\n"
        )
    };
    let e25 = rel(z_in(&deck(25), (1, 1)), Complex64::new(45.43, 13.91));
    let e49 = rel(z_in(&deck(49), (1, 1)), Complex64::new(45.49, 14.06));
    assert_converges("node-fed T", e25, e49, 0.05);
}

/// Before: 17 − j1163 at 11 per side. Kill criterion: under 1 % at 41,
/// shrinking. Measured 1.84 → 1.05 → 0.57 %.
#[test]
fn a_square_loop_converges_to_nec2c() {
    let e21 = rel(
        z_in(
            &format!("{}GE 0\nEX 0 1 11 0 1 0\n", square_loop(21, 0.0)),
            (1, 11),
        ),
        Complex64::new(110.19, -146.23),
    );
    let e41 = rel(
        z_in(
            &format!("{}GE 0\nEX 0 1 21 0 1 0\n", square_loop(41, 0.0)),
            (1, 21),
        ),
        Complex64::new(109.71, -146.21),
    );
    assert_converges("square loop", e21, e41, 0.01);
}

/// A loop with a stub rising from the middle of its top side: two degree-3 nodes
/// on one cycle and a free end. Kill criterion: under 1 % at 41, shrinking.
/// Measured 1.65 → 1.03 → 0.57 %.
#[test]
fn a_loop_with_a_stub_converges_to_nec2c() {
    let deck = |n: u32, stub: u32| {
        let (a, h) = (5.278, 2.639);
        let half = n / 2;
        format!(
            "GW 1 {n} 0 0 0 {a} 0 0 .001\nGW 2 {n} {a} 0 0 {a} 0 {a} .001\n\
             GW 3 {half} {a} 0 {a} {h} 0 {a} .001\nGW 4 {half} {h} 0 {a} 0 0 {a} .001\n\
             GW 5 {n} 0 0 {a} 0 0 0 .001\nGW 6 {stub} {h} 0 {a} {h} 0 7.278 .001\n\
             GE 0\nEX 0 1 {} 0 1 0\n",
            half + 1
        )
    };
    let e21 = rel(z_in(&deck(21, 3), (1, 11)), Complex64::new(110.19, -146.23));
    let e41 = rel(z_in(&deck(41, 7), (1, 21)), Complex64::new(109.71, -146.2));
    assert_converges("loop with stub", e21, e41, 0.01);
}

/// The same loop 3 m over perfect ground: the corner term reaches the images
/// (FND-174) on the graph route too. Kill criterion: under 1 % at 49, shrinking.
/// Measured 1.42 → 0.81 → 0.44 % at 13 / 25 / 49.
#[test]
fn a_square_loop_over_ground_converges_to_nec2c() {
    let e25 = rel(
        z_in(
            &format!("{}GE 1\nGN 1\nEX 0 1 13 0 1 0\n", square_loop(25, 3.0)),
            (1, 13),
        ),
        Complex64::new(122.47, -110.17),
    );
    let e49 = rel(
        z_in(
            &format!("{}GE 1\nGN 1\nEX 0 1 25 0 1 0\n", square_loop(49, 3.0)),
            (1, 25),
        ),
        Complex64::new(122.14, -110.31),
    );
    assert_converges("square loop over GN 1", e25, e49, 0.01);
}

/// Kirchhoff at the Y's node, read off the solved currents rather than the rows
/// that impose it: each wire's current extrapolated linearly from its last two
/// segments to the node. The stem runs into the node and both arms run out, so
/// the stem's current is the arms' sum.
#[test]
fn the_currents_meeting_at_a_y_node_sum_to_zero() {
    let n = 41;
    let (segs, i) = solve(&format!("{}EX 0 1 10 0 1 0\n", y(n)));
    let at = |tag: u32, end: u32, inner: u32| {
        1.5 * current(&segs, &i, tag, end) - 0.5 * current(&segs, &i, tag, inner)
    };
    let stem = at(1, n, n - 1);
    let arms = at(2, 1, 2) + at(3, 1, 2);
    let mismatch = (stem - arms).norm() / stem.norm();
    println!("Y node: stem {stem:.4e}, arms {arms:.4e}, mismatch {mismatch:.2e}");
    assert!(mismatch < 0.02, "KCL at the node: {mismatch:.3e}");
}

// ---- FND-162 stage 5: lumped loads on the section graph ---------------------
//
// A load is a gap its own current drives, `V = −Z_L·I_p`: a column of the graph
// system, the collocation rows of its section and that section's terms in the
// equal-potential rows. Loaded junction and loop decks used to fall back to the
// per-wire basis with pairwise junction rows. Expectations are nec2c 1.3.1,
// captured 2026-10-01; each deck converges at the unloaded decks' rate.

/// A Y with a 10 Ω + 1 µH series load mid-arm. Kill criterion: under 1 % at 41,
/// shrinking. Measured 1.67 → 0.73 %. A regression gate only: the load moves Z
/// by under 2 %, so a missing load passes it — the corner-loaded loop and the
/// feed identity below are the tests that see one.
#[test]
fn a_loaded_y_converges_to_nec2c() {
    let feed = |n: u32| (0.68 / 3.0 * f64::from(n)) as u32 + 1;
    let deck = |n: u32| {
        format!(
            "{}LD 0 2 {m} {m} 10 1e-6 0\nEX 0 1 {} 0 1 0\n",
            y(n),
            feed(n),
            m = n / 2 + 1
        )
    };
    let e21 = rel(
        z_in(&deck(21), (1, feed(21))),
        Complex64::new(23.89, -1791.0),
    );
    let e41 = rel(
        z_in(&deck(41), (1, feed(41))),
        Complex64::new(22.69, -1633.0),
    );
    assert_converges("loaded Y", e21, e41, 0.01);
}

/// A T with a 1 µH coil on the arm segment that touches the node, where the
/// load's term in the equal-potential rows is largest. Kill criterion: under
/// 2 % at 41, shrinking. Measured 2.21 → 1.35 %. It sees a load with the wrong
/// sign, not a missing one (the coil moves Z by about 2 %).
#[test]
fn a_t_with_a_coil_at_its_node_converges_to_nec2c() {
    let deck = |n: u32| {
        format!(
            "{}LD 0 2 1 1 0 1e-6 0\nEX 0 1 {} 0 1 0\n",
            tee(n),
            feed_t(n)
        )
    };
    let e21 = rel(
        z_in(&deck(21), (1, feed_t(21))),
        Complex64::new(29.70, -1592.6),
    );
    let e41 = rel(
        z_in(&deck(41), (1, feed_t(41))),
        Complex64::new(28.98, -1636.3),
    );
    assert_converges("T, coil at the node", e21, e41, 0.02);
}

fn feed_t(n: u32) -> u32 {
    (0.68 / 4.0 * f64::from(n)) as u32 + 1
}

/// A 1 λ square loop with a 100 Ω + 2 µH load on the segment beside a corner.
/// The load moves Z by about 85 Ω (unloaded 110 − j146), so this deck sees a
/// load that is wrong or missing. Kill criterion: under 2 % at 41, shrinking.
/// Measured 2.05 → 1.43 %.
#[test]
fn a_loop_loaded_at_a_corner_converges_to_nec2c() {
    let deck = |n: u32| {
        format!(
            "{}GE 0\nLD 0 2 1 1 100 2e-6 0\nEX 0 1 {} 0 1 0\n",
            square_loop(n, 0.0),
            n / 2 + 1
        )
    };
    let e21 = rel(z_in(&deck(21), (1, 11)), Complex64::new(191.06, -140.79));
    let e41 = rel(z_in(&deck(41), (1, 21)), Complex64::new(194.48, -138.97));
    assert_converges("loop loaded at a corner", e21, e41, 0.02);
}

/// The port identity, which needs no reference: a load on the feed segment
/// raises the input impedance by exactly Z_L, whatever the rest of the system.
/// Measured to 4e-5 Ω on a T-dipole.
#[test]
fn a_load_at_the_feed_adds_exactly_its_impedance() {
    let n = 21;
    let feed = (1.0 / 5.25 * f64::from(n)) as u32 + 1;
    let wires = format!(
        "GW 1 {n} -5.25 0 0 0 0 0 .001\nGW 2 {n} 0 0 0 5.25 0 0 .001\nGW 3 {n} 0 0 0 0 0 -2 .001\nGE 0\n"
    );
    let unloaded = z_in(&format!("{wires}EX 0 2 {feed} 0 1 0\n"), (2, feed));
    let loaded = z_in(
        &format!("{wires}LD 0 2 {feed} {feed} 50 5e-6 0\nEX 0 2 {feed} 0 1 0\n"),
        (2, feed),
    );
    let z_l = Complex64::new(50.0, 2.0 * std::f64::consts::PI * FREQ * 5e-6);
    assert!(
        ((loaded - unloaded) - z_l).norm() < 1e-3,
        "loaded − unloaded = {} against Z_L = {z_l}",
        loaded - unloaded
    );
}

// ---- FND-162 stage 5: junctions and loops touching perfect ground ------------
//
// A deck touching PEC ground is solved as its doubled image problem (FND-082).
// Doubled, a top-hat monopole is an H and a folded monopole a closed loop; both
// were refused ("cannot represent"), and both are section graphs.

fn top_hat_on_ground(n: u32, m: u32) -> String {
    format!(
        "GW 1 {n} 0 0 0 0 0 5 .001\nGW 2 {m} 0 0 5 -3 0 5 .001\nGW 3 {m} 0 0 5 3 0 5 .001\n\
         GE 1\nGN 1\nEX 0 1 1 0 1 0\n"
    )
}

/// The top-hat with its image written out: a 10 m stem with a hat at each end,
/// fed across the joint by the gap and its image (1 V each).
fn h_in_free_space(n: u32, m: u32) -> String {
    format!(
        "GW 1 {} 0 0 -5 0 0 5 .001\nGW 2 {m} 0 0 5 -3 0 5 .001\nGW 3 {m} 0 0 5 3 0 5 .001\n\
         GW 4 {m} 0 0 -5 -3 0 -5 .001\nGW 5 {m} 0 0 -5 3 0 -5 .001\nGE 0\n\
         EX 0 1 {n} 0 1 0\nEX 0 1 {} 0 1 0\n",
        2 * n,
        n + 1
    )
}

fn z_at_7mhz(body: &str, feed: (u32, u32)) -> Complex64 {
    let deck = nec_parser::parse(&format!("CE\n{body}FR 0 1 0 0 7.1 0\nEN\n"))
        .expect("parses")
        .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let f = 7.1e6;
    let mut z = assemble_z_matrix_with_ground(&segs, f, &ground_model_from_deck(&deck));
    let routed = solve_hallen_routed(&deck, &segs, &mut z, f, &[]).expect("solves");
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Complex64::new(1.0, 0.0) / routed.currents[idx]
}

/// The image problem IS the doubled structure: a top-hat monopole on PEC ground
/// equals its H drawn in free space, to every digit.
#[test]
fn a_grounded_top_hat_equals_its_image_h() {
    let grounded = z_at_7mhz(&top_hat_on_ground(21, 13), (1, 1));
    let explicit = z_at_7mhz(&h_in_free_space(21, 13), (1, 21));
    assert!(
        rel(grounded, explicit) < 1e-9,
        "grounded {grounded} against its image H {explicit}"
    );
}

/// FND-191: the H is where fnec and nec2c take different junction conditions.
/// fnec closes a degree-3 node with equal scalar potential; NEC-2 with Wu–King's
/// equal charge density. Where a near-uniform stem current meets charged hats at
/// both ends they converge to different answers: nec2c 16.47 − j57.43 at 41/25,
/// fnec 16.69 − j50.31, 12 % apart at every mesh and every radius. fnec's MPIE —
/// separate code with no imposed junction condition — agrees with the Hallén
/// graph, so the H is gated against the MPIE (its port is the two gaps in
/// series: twice the per-gap impedance), and the nec2c gap is pinned as a band so
/// a change in either solver shows. The maintainer's decision, 2026-10-02.
#[test]
fn the_h_agrees_with_the_mpie_and_keeps_its_measured_gap_to_nec2c() {
    let hallen = z_at_7mhz(&h_in_free_space(41, 25), (1, 41));
    let mpie = Complex64::new(33.386_903, -100.004_424);
    let e_mpie = rel(hallen * 2.0, mpie);
    assert!(
        e_mpie < 0.01,
        "Hallén {hallen} against the MPIE's {mpie}/2: {e_mpie:.4}"
    );
    let gap = rel(hallen, Complex64::new(16.468, -57.426));
    assert!(
        (0.10..0.14).contains(&gap),
        "the junction-condition gap to nec2c moved: {:.2} % (was 11.9 %)",
        gap * 100.0
    );
}

/// A folded monopole, both legs grounded: doubled, a closed loop. No degree-3
/// node, so no junction-condition gap; against nec2c GN 1, 0.73 → 0.49 % at
/// 21/5 → 41/9. Kill criterion: under 1 % at 41, shrinking.
#[test]
fn a_folded_monopole_on_ground_converges_to_nec2c() {
    let deck = |n: u32, m: u32| {
        format!(
            "GW 1 {n} 0 0 0 0 0 5 .001\nGW 2 {m} 0 0 5 1 0 5 .001\nGW 3 {n} 1 0 5 1 0 0 .001\n\
             GE 1\nGN 1\nEX 0 1 1 0 1 0\n"
        )
    };
    let e21 = rel(z_in(&deck(21, 5), (1, 1)), Complex64::new(172.38, 103.84));
    let e41 = rel(z_in(&deck(41, 9), (1, 1)), Complex64::new(174.54, 102.49));
    assert_converges("folded monopole on GN 1", e21, e41, 0.01);
}

/// The route flags and the topology caveat ask about the solve that runs — the
/// image problem's graph — so a correct grounded top-hat is neither flagged
/// unsupported (the GPU gate reads that flag) nor warned about.
#[test]
fn a_grounded_top_hat_is_neither_flagged_nor_warned() {
    let deck = nec_parser::parse(&format!(
        "CE\n{}FR 0 1 0 0 7.1 0\nEN\n",
        top_hat_on_ground(21, 13)
    ))
    .expect("parses")
    .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let route = nec_solver::hallen_route(&deck, &segs);
    assert!(!route.unsupported_topology, "{route:?}");
    assert_eq!(
        nec_solver::validate::unsupported_topology_warning(&deck, &segs, "re-run with mpie"),
        None
    );
}
