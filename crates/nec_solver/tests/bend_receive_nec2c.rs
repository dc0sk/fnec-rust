// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-162 — the plane-wave RECEIVE solve on bent conductors, gated against nec2c.
//!
//! The receive solve used to keep one homogeneous term per path, on the belief that
//! its forcing "jumps at a bend". The forcing is a sum of delta-gap feeds (see
//! `planewave.rs`'s superposition test), smooth at every node, so the driven solve's
//! section layout, bend rows and corner term hold for it unchanged. Before, a 90° L
//! was 106–122 % off nec2c at the corner and got WORSE with refinement.
//!
//! Every expectation is nec2c 1.3.1, captured 2026-09-28: the currents at the
//! middle of the first run, both segments at the first bend, and the middle of the
//! second run, and the peak |I| they are normalised to. Each deck is gated at two
//! meshes and the error must shrink, since one mesh can agree by cancellation.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck,
    hallen_leaves_a_bend_unmodelled, plan_hallen_planewave, solve_hallen_planewave_planned,
    solve_hallen_planewave_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn deck(geometry: &str, wave: &str) -> nec_model::deck::NecDeck {
    nec_parser::parse(&format!(
        "CE\n{geometry}GE 0\n{wave}FR 0 1 0 0 14.2 0\nEN\n"
    ))
    .expect("parses")
    .deck
}

/// The receive currents through the production seam.
fn receive(geometry: &str, wave: &str) -> Vec<Complex64> {
    let d = deck(geometry, wave);
    let segs = build_geometry(&d).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&d));
    solve_hallen_planewave_routed(&d, &segs, &z, FREQ).expect("receive solve")
}

/// Max over the gated segments (1-based, nec2c numbering) of |ΔI| / peak |I_nec2c|.
fn err(got: &[Complex64], peak: f64, refs: &[(usize, f64, f64)]) -> f64 {
    refs.iter()
        .map(|&(seg, re, im)| (got[seg - 1] - Complex64::new(re, im)).norm() / peak)
        .fold(0.0, f64::max)
}

fn assert_converges(label: &str, coarse: f64, fine: f64, bound: f64) {
    println!("{label}: {:.2} % -> {:.2} %", coarse * 100.0, fine * 100.0);
    assert!(
        fine < bound,
        "{label}: {:.2} % from nec2c at the finer mesh",
        fine * 100.0
    );
    assert!(
        fine < coarse,
        "{label}: the error must shrink with refinement: {:.2} % -> {:.2} %",
        coarse * 100.0,
        fine * 100.0
    );
}

fn l(n: u32) -> String {
    format!("GW 1 {n} 0 0 0 0 0 5 .001\nGW 2 {n} 0 0 5 5 0 5 .001\n")
}

/// A 90° L lit at θ = 45°, so both arms see the field. Per-path: 106 % / 122 %.
#[test]
fn an_l_receives_like_nec2c() {
    let wave = "EX 1 1 1 0 45 0 0\n";
    let e21 = err(
        &receive(&l(21), wave),
        2.203910e-02,
        &[
            (11, -1.253000e-02, -9.138500e-03),
            (21, -1.879000e-02, -1.114400e-02),
            (22, -1.905800e-02, -1.098600e-02),
            (32, -1.535000e-02, -7.472600e-03),
        ],
    );
    let e41 = err(
        &receive(&l(41), wave),
        2.208442e-02,
        &[
            (21, -1.257500e-02, -9.130900e-03),
            (41, -1.892100e-02, -1.109700e-02),
            (42, -1.906100e-02, -1.101500e-02),
            (62, -1.539500e-02, -7.464500e-03),
        ],
    );
    assert_converges("L, θ=45°", e21, e41, 0.05);
}

/// The same L lit edge-on (θ = 90°, E along z): the horizontal arm sees NO field,
/// so every ampere on it arrives through the bend rows and the corner term.
#[test]
fn an_l_with_one_arm_dark_is_fed_through_the_bend() {
    let wave = "EX 1 1 1 0 90 0 0\n";
    let e21 = err(
        &receive(&l(21), wave),
        4.267111e-02,
        &[
            (11, -1.536400e-02, -2.836200e-02),
            (21, -2.103000e-02, -3.712900e-02),
            (22, -2.102700e-02, -3.688400e-02),
            (32, -1.532200e-02, -2.561200e-02),
        ],
    );
    let e41 = err(
        &receive(&l(41), wave),
        4.281918e-02,
        &[
            (21, -1.546200e-02, -2.842000e-02),
            (41, -2.117100e-02, -3.716100e-02),
            (42, -2.116900e-02, -3.703500e-02),
            (62, -1.542000e-02, -2.566900e-02),
        ],
    );
    assert_converges("L, dark arm", e21, e41, 0.05);
}

/// Two bends: the middle section is referenced at one node, so its corner
/// integral enters the other node's potential row. Per-path: 41 % / 51 %.
#[test]
fn a_two_bend_u_receives_like_nec2c() {
    let u = |n: u32| {
        format!("GW 1 {n} 0 0 0 0 0 3 .001\nGW 2 {n} 0 0 3 3 0 3 .001\nGW 3 {n} 3 0 3 3 0 0 .001\n")
    };
    let wave = "EX 1 1 1 0 40 20 0\n";
    let e11 = err(
        &receive(&u(11), wave),
        9.647509e-03,
        &[
            (6, -3.732800e-03, 1.273500e-03),
            (11, -6.529900e-03, 2.942000e-03),
            (12, -7.022600e-03, 3.304100e-03),
            (17, -8.486500e-03, 4.382900e-03),
        ],
    );
    let e21 = err(
        &receive(&u(21), wave),
        9.680624e-03,
        &[
            (11, -3.743700e-03, 1.279200e-03),
            (21, -6.666500e-03, 3.038300e-03),
            (22, -6.926500e-03, 3.230300e-03),
            (32, -8.506600e-03, 4.393200e-03),
        ],
    );
    assert_converges("U", e11, e21, 0.025);
}

/// The receive-pattern sweep builds the direction-independent plan once; per
/// direction it must give exactly what the one-shot seam gives.
#[test]
fn a_planned_receive_solve_equals_the_one_shot_solve() {
    let geo = l(11);
    let plan_deck = deck(&geo, "EX 1 1 1 0 45 0 0\n");
    let segs = build_geometry(&plan_deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&plan_deck));
    let plan = plan_hallen_planewave(&segs, FREQ, &ground_model_from_deck(&plan_deck));
    for wave in ["EX 1 1 1 0 30 0 0\n", "EX 1 1 1 0 75 40 90\n"] {
        let d = deck(&geo, wave);
        let planned = solve_hallen_planewave_planned(&d, &segs, &z, FREQ, &plan).unwrap();
        let one_shot = solve_hallen_planewave_routed(&d, &segs, &z, FREQ).unwrap();
        assert_eq!(planned, one_shot, "{wave:?}");
    }
}

/// The fallback predicate on a free-space deck.
fn unmodelled(segs: &[nec_solver::Segment]) -> bool {
    hallen_leaves_a_bend_unmodelled(segs, &nec_solver::GroundModel::FreeSpace)
}

/// A straight run one segment long cannot carry its own two constants, so the
/// deck falls back and the bend goes unmodelled; the caveat keys on this.
#[test]
fn only_a_one_segment_run_leaves_a_bend_unmodelled() {
    let segs_of = |g: &str| build_geometry(&deck(g, "EX 1 1 1 0 45 0 0\n")).unwrap();
    assert!(!unmodelled(&segs_of(&l(21))), "an L is modelled");
    assert!(
        unmodelled(&segs_of(
            "GW 1 21 0 0 0 0 0 5 .001\nGW 2 1 0 0 5 .3 0 5 .001\nGW 3 21 .3 0 5 .3 0 0 .001\n"
        )),
        "a U whose middle run is one segment falls back"
    );
    assert!(
        !unmodelled(&segs_of("GW 1 21 0 0 -5 0 0 5 .001\n")),
        "a straight wire has no bend"
    );
}
