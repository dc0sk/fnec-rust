// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-162 stage 5 — the plane-wave RECEIVE solve on junctions and loops.
//!
//! A plane wave is a delta gap on every segment, `V_p = E_t(p)·Δl_p`, so the
//! section-graph system takes it as it takes a feed; the plan builds that system
//! once and reuses it for every incidence direction. Junction and loop decks lit
//! by a plane wave were refused (`JunctionedGeometryNotSupported`).
//!
//! Loaded decks (FND-197) solve too: the loads are columns of the graph system.
//!
//! Every expectation is nec2c 1.3.1, captured 2026-10-02: the peak |I| and the
//! currents at a handful of segments (the feed corner, a mid-run, the node). The
//! whole current table agrees too (max |ΔI| / peak at 21 → 41 per wire: loop
//! broadside 0.78 → 0.48 %, at 45° 0.64 → 0.42 %; T broadside 0.94 → 0.55 %, at
//! 45° 0.82 → 0.56 %). Each deck is gated at two meshes and must converge.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck,
    solve_hallen_planewave_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn receive(geometry: &str, wave: &str) -> Vec<Complex64> {
    let deck = nec_parser::parse(&format!(
        "CE\n{geometry}GE 0\n{wave}FR 0 1 0 0 14.2 0\nEN\n"
    ))
    .expect("parses")
    .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let currents = solve_hallen_planewave_routed(
        &deck,
        &segs,
        &z,
        FREQ,
        &nec_solver::build_deck_stamps(&deck, &segs, FREQ).diagonal,
    )
    .expect("receive solve");
    // nec2c numbers the deck's segments: compare at those (FND-227).
    nec_solver::deck_values(&segs, &currents)
}

/// Max over the pinned segments (1-based) of |ΔI| / peak |I_nec2c|.
fn err(got: &[Complex64], peak: f64, refs: &[(usize, f64, f64)]) -> f64 {
    refs.iter()
        .map(|&(seg, re, im)| (got[seg - 1] - Complex64::new(re, im)).norm() / peak)
        .fold(0.0, f64::max)
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

fn square_loop(n: u32) -> String {
    let a = 5.278;
    format!(
        "GW 1 {n} 0 0 0 {a} 0 0 .001\nGW 2 {n} {a} 0 0 {a} 0 {a} .001\n\
         GW 3 {n} {a} 0 {a} 0 0 {a} .001\nGW 4 {n} 0 0 {a} 0 0 0 .001\n"
    )
}

/// A 1 λ square loop in the x–z plane, lit broadside (θ = 90°, φ = 90°).
/// Kill criterion: under 1 % at 41, shrinking. Measured 0.55 → 0.34 % on these
/// segments.
#[test]
fn a_square_loop_receives_like_nec2c() {
    let wave = "EX 1 1 1 0 90 90 0\n";
    let e21 = err(
        &receive(&square_loop(21), wave),
        5.031_719e-2,
        &[
            (1, 2.096_900e-2, 2.683_200e-2),
            (11, 3.757_000e-11, 7.630_300e-12),
            (21, -2.096_900e-2, -2.683_200e-2),
            (22, -2.263_100e-2, -2.895_000e-2),
            (53, -3.753_000e-11, -7.629_100e-12),
            (84, 2.263_100e-2, 2.895_000e-2),
        ],
    );
    let e41 = err(
        &receive(&square_loop(41), wave),
        5.031_120e-2,
        &[
            (1, 2.137_700e-2, 2.736_200e-2),
            (21, 2.145_400e-11, 4.335_800e-12),
            (41, -2.137_700e-2, -2.736_200e-2),
            (42, -2.222_800e-2, -2.844_800e-2),
            (103, -2.151_600e-11, -4.476_700e-12),
            (164, 2.222_800e-2, 2.844_800e-2),
        ],
    );
    assert_converges("loop, broadside", e21, e41, 0.01);
}

/// A T (4 m stem, 3 m bar halves) lit at θ = 45°: every arm sees the field, and
/// the node splits the received current three ways. Kill criterion: under 1 % at
/// 41, shrinking. Measured 0.82 → 0.56 %.
#[test]
fn a_t_receives_like_nec2c() {
    let tee = |n: u32| {
        format!(
            "GW 1 {n} 0 0 0 0 0 4 .001\nGW 2 {n} 0 0 4 -3 0 4 .001\nGW 3 {n} 0 0 4 3 0 4 .001\n"
        )
    };
    let wave = "EX 1 1 1 0 45 0 0\n";
    let e21 = err(
        &receive(&tee(21), wave),
        5.530_196e-3,
        &[
            (1, 9.733_200e-5, -2.987_300e-4),
            (11, 1.443_900e-3, -3.777_200e-3),
            (21, 1.719_900e-3, -4.481_800e-3),
            (22, 3.225_400e-3, -4.492_200e-3),
            (43, -1.564_700e-3, 8.379_500e-5),
            (63, -1.232_500e-4, 2.556_000e-5),
        ],
    );
    let e41 = err(
        &receive(&tee(41), wave),
        5.554_170e-3,
        &[
            (1, 5.442_700e-5, -1.677_800e-4),
            (21, 1.447_800e-3, -3.783_100e-3),
            (41, 1.714_900e-3, -4.476_000e-3),
            (42, 3.240_600e-3, -4.510_800e-3),
            (83, -1.554_400e-3, 7.018_500e-5),
            (123, -6.977_600e-5, 1.438_800e-5),
        ],
    );
    assert_converges("T at 45°", e21, e41, 0.01);
}

/// The single-shot route a frontend takes, with the deck's loads (or `loads`
/// when given — what the CLI's `--loads-config` adds) passed in full.
fn routed(text: &str, loads: Option<Vec<Complex64>>) -> nec_solver::HallenRouted {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let loads = loads.unwrap_or_else(|| nec_solver::build_deck_stamps(&deck, &segs, FREQ).diagonal);
    nec_solver::solve_hallen_routed(&deck, &segs, &mut z, FREQ, &loads).expect("routed solve")
}

/// The geometry `routed` solves `text` on.
fn segs_of(text: &str) -> Vec<nec_solver::Segment> {
    build_geometry(&nec_parser::parse(text).expect("parses").deck).expect("geometry")
}

/// `routed`'s currents at the deck's segments.
fn routed_deck_currents(text: &str) -> Vec<Complex64> {
    // nec2c numbers the deck's segments: compare at those (FND-227).
    nec_solver::deck_values(&segs_of(text), &routed(text, None).currents)
}

fn tee(n: u32) -> String {
    format!("GW 1 {n} 0 0 0 0 0 4 .001\nGW 2 {n} 0 0 4 -3 0 4 .001\nGW 3 {n} 0 0 4 3 0 4 .001\n")
}

fn deck_text(geometry: &str, cards: &str) -> String {
    format!("CE\n{geometry}GE 0\n{cards}FR 0 1 0 0 14.2 0\nEN\n")
}

/// FND-197: a loaded junction deck lit by a plane wave. A 300 Ω load on the T's
/// bar, against nec2c at two meshes (the load at the same fraction of the bar).
/// It was refused as an `LD` card and — as a CLI `--loads-config` load — solved
/// over the plain basis's stamps, 25 % off. Kill criterion: under 1 % at 41,
/// shrinking. Measured 0.96 → 0.68 % over the whole table against nec2c on the
/// same mesh; since FND-227 refined the free ends, fnec outruns nec2c there, so
/// the table is nec2c on a 27× mesh (segment j → 27j − 13, the same point),
/// captured 2026-10-10: 0.49 → 0.32 % without the node segment, which is held to
/// the bound on its own (below).
#[test]
fn a_loaded_t_receives_like_nec2c() {
    let wave = "EX 1 1 1 0 90 0 0\n";
    let e = |n: u32, seg: u32, peak: f64, refs: &[(usize, f64, f64)]| {
        let text = deck_text(&tee(n), &format!("LD 4 2 {seg} {seg} 300 0\n{wave}"));
        err(&routed_deck_currents(&text), peak, refs)
    };
    let e21 = e(
        21,
        5,
        6.375_693e-3,
        &[
            (1, -4.4471e-05, -4.3836e-04),
            (11, -6.2995e-04, -5.3267e-03),
            (22, -1.1981e-03, -2.5080e-03),
            (26, -1.2438e-03, -1.9993e-03),
            (43, 2.4978e-04, -3.4735e-03),
            (63, 9.4342e-06, -1.3432e-04),
        ],
    );
    let e41 = e(
        41,
        10,
        6.399_255e-3,
        &[
            (1, -2.4693e-05, -2.4883e-04),
            (21, -6.1894e-04, -5.3444e-03),
            (42, -1.1673e-03, -2.5669e-03),
            (51, -1.2159e-03, -1.9830e-03),
            (83, 2.4246e-04, -3.4944e-03),
            (123, 5.1742e-06, -7.6236e-05),
        ],
    );
    assert_converges("loaded T", e21, e41, 0.01);
    // The stem's segment at the T node, apart: its error is the junction
    // condition's (fnec closes a node with equal potential, NEC-2 with Wu–King's
    // equal charge density, FND-191) and does not shrink — 0.497 % at 21, 0.503 %
    // at 41 — so in the table it set a floor the rest of it is below.
    let node21 = e(21, 5, 6.375_693e-3, &[(21, -9.3535e-04, -6.1324e-03)]);
    let node41 = e(41, 10, 6.399_255e-3, &[(41, -9.1906e-04, -6.1337e-03)]);
    println!(
        "loaded T, node segment: {:.2} % / {:.2} %",
        node21 * 100.0,
        node41 * 100.0
    );
    assert!(
        node21 < 0.01 && node41 < 0.01,
        "node segment {node21:.4} / {node41:.4}"
    );
}

/// A 1 λ loop with a 100 + j50 Ω load mid-side, lit broadside. Kill criterion:
/// under 1 % at 41, shrinking. Measured 0.87 → 0.51 % over the whole table.
#[test]
fn a_loaded_loop_receives_like_nec2c() {
    let wave = "EX 1 1 1 0 90 90 0\n";
    let e = |n: u32, peak: f64, refs: &[(usize, f64, f64)]| {
        let mid = n.div_ceil(2);
        let text = deck_text(
            &square_loop(n),
            &format!("LD 4 2 {mid} {mid} 100 50\n{wave}"),
        );
        err(&routed_deck_currents(&text), peak, refs)
    };
    let e21 = e(
        21,
        3.985_428e-2,
        &[
            (1, 2.5442e-02, 1.0300e-02),
            (11, 1.8021e-03, -1.4500e-03),
            (21, -2.2770e-02, -1.2408e-02),
            (22, -2.4767e-02, -1.3225e-02),
            (32, -3.6540e-02, -1.5912e-02),
            (53, 1.8021e-03, -1.4500e-03),
            (84, 2.7257e-02, 1.1272e-02),
        ],
    );
    let e41 = e(
        41,
        3.985_878e-2,
        &[
            (1, 2.5871e-02, 1.0550e-02),
            (21, 1.8010e-03, -1.4523e-03),
            (41, -2.3244e-02, -1.2625e-02),
            (42, -2.4266e-02, -1.3044e-02),
            (62, -3.6561e-02, -1.5875e-02),
            (103, 1.8010e-03, -1.4523e-03),
            (164, 2.6800e-02, 1.1049e-02),
        ],
    );
    assert_converges("loaded loop", e21, e41, 0.01);
}

/// The compensation theorem, an exact identity through a different code path:
/// a series load Z_L at p is the unloaded receive plus the field of a source
/// −Z_L·I_L[p] at p, so I_L = I_0 − Z_L·I_L[p]·g with g the DRIVEN graph's
/// response to 1 V at p, and I_L[p] = I_0[p] / (1 + Z_L·g[p]). Measured
/// residual 1.1e-9 of the peak (the regularised solves' floor); the solve built
/// over the plain basis's stamps was 25 % off.
#[test]
fn a_loaded_receive_is_the_unloaded_one_compensated() {
    let wave = "EX 1 1 1 0 90 0 0\n";
    let (seg, z_l) = (5u32, Complex64::new(300.0, 0.0));
    let loaded = routed(
        &deck_text(&tee(21), &format!("LD 4 2 {seg} {seg} 300 0\n{wave}")),
        None,
    )
    .currents;
    let unloaded = routed(&deck_text(&tee(21), wave), None).currents;
    let g = routed(&deck_text(&tee(21), &format!("EX 0 2 {seg} 0 1 0\n")), None).currents;
    let p = nec_solver::find_deck_segment(&segs_of(&deck_text(&tee(21), wave)), 2, seg)
        .expect("tag 2 segment");
    let i_p = unloaded[p] / (Complex64::new(1.0, 0.0) + z_l * g[p]);
    let peak = loaded.iter().map(|c| c.norm()).fold(0.0, f64::max);
    let worst = (0..loaded.len())
        .map(|i| (loaded[i] - (unloaded[i] - z_l * i_p * g[i])).norm() / peak)
        .fold(0.0, f64::max);
    println!("compensation residual: {worst:.3e} of the peak");
    assert!(worst < 1e-6, "{worst:.3e}");
}

/// FND-198: a current source on a loaded junction deck prices exactly as the
/// voltage source — the loads were applied twice (153.7 − j1530 Ω against
/// 85.5 − j1536). Through the routed entry point, with the loads as an `LD` card
/// and as a bare diagonal (what `--loads-config` adds).
#[test]
fn a_loaded_current_source_on_the_graph_prices_as_the_voltage_source() {
    let load = "LD 0 2 1 1 100 2e-6 0\n";
    let z_of = |r: nec_solver::HallenRouted, idx: usize| match r.port_voltage {
        Some(v) => v,
        None => Complex64::new(1.0, 0.0) / r.source_current(idx),
    };
    let feed = nec_solver::find_deck_segment(&segs_of(&deck_text(&tee(21), "")), 1, 4)
        .expect("tag 1 segment 4");
    let v = z_of(
        routed(
            &deck_text(&tee(21), &format!("{load}EX 0 1 4 0 1 0\n")),
            None,
        ),
        feed,
    );
    let i = z_of(
        routed(
            &deck_text(&tee(21), &format!("{load}EX 4 1 4 0 1 0\n")),
            None,
        ),
        feed,
    );
    assert!((i - v).norm() <= 1e-9 * v.norm(), "EX 4 {i} vs EX 0 {v}");
    // The same load as a diagonal, the deck carrying no LD card.
    let deck = nec_parser::parse(&deck_text(&tee(21), &format!("{load}EX 0 1 4 0 1 0\n")))
        .unwrap()
        .deck;
    let segs = build_geometry(&deck).unwrap();
    let diagonal = nec_solver::build_deck_stamps(&deck, &segs, FREQ).diagonal;
    let bare = z_of(
        routed(&deck_text(&tee(21), "EX 4 1 4 0 1 0\n"), Some(diagonal)),
        feed,
    );
    assert!(
        (bare - v).norm() <= 1e-9 * v.norm(),
        "diagonal {bare} vs LD {v}"
    );
}

/// A receive solve given no loads for a deck with `LD` cards refuses, rather than
/// solving it unloaded.
#[test]
fn a_loaded_receive_given_no_loads_refuses() {
    let deck = nec_parser::parse(&deck_text(
        &tee(21),
        "LD 4 2 5 5 300 0\nEX 1 1 1 0 90 0 0\n",
    ))
    .unwrap()
    .deck;
    let segs = build_geometry(&deck).unwrap();
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let e = solve_hallen_planewave_routed(&deck, &segs, &z, FREQ, &[])
        .unwrap_err()
        .to_string();
    assert!(e.contains("LD loads") && e.contains("none"), "{e}");
}
