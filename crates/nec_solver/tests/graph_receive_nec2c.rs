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
//!
//! FND-265: the T and the loaded T are held at 21 and 63 per wire to one nec2c
//! reference on a 27× mesh, read from a checked-in series (`corpus/series/`), with
//! the junction node's pins gated apart (`assert_receive_converges`).

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
///
/// The references are nec2c on each deck's own mesh, and the band stands
/// (FND-296 asked): nec2c's own series at the 21-deck's points
/// (`scripts/nec2c-receive-series.py`, `EK 0`, 1 / 3 / 9 / 27 ×, captured
/// 2026-10-23) moves 0.030, 0.006 and 0.004 % of the peak a tripling over the
/// whole table, so nec2c's mesh error is a 1/30 of the band at 21 already. The
/// 21 → 41 change in the pinned values is the pins' positions — segment 1 of 41
/// is not segment 1 of 21 — not nec2c's drift.
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

/// A checked-in series deck (`corpus/series/`), as text.
fn series_deck(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/series")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// FND-265: one antenna solved at 21 and 63 per wire, held to ONE nec2c
/// reference at the same points (a 21-mesh pin k is the 63-mesh segment 3k − 1).
///
/// Away from the junction node the error, plus three of nec2c's last steps there
/// (`step_rest`, what nec2c may still move), must stay under 1 % of the peak and
/// shrink. The pins touching the node (`node`) are not a convergence gate: there
/// fnec and nec2c settle apart, nec2c drifting toward fnec at a near-constant
/// ratio (the H's class, FND-191 / FND-264), so fnec's error against nec2c's last
/// point GROWS as fnec refines. Those pins hold a band (under 1 %, a tripwire on
/// that known gap — a legitimate change to fnec's junction rows may trip it) and
/// fnec's own change between the meshes, under 0.3 % (a wrong junction condition
/// diverges there). The 1 % is of the peak: a whole-table level, not a per-point
/// one — the end pins carry a few % of the peak.
fn assert_receive_converges(
    label: &str,
    (c21, c63): (&[Complex64], &[Complex64]),
    peak: f64,
    pins: &[(usize, f64, f64)],
    node: &[usize],
    step_rest: f64,
) {
    let at63 = |k: usize| 3 * k - 1;
    let err = |c: &[Complex64], map: &dyn Fn(usize) -> usize, on_node: bool| {
        pins.iter()
            .filter(|p| node.contains(&p.0) == on_node)
            .map(|&(k, re, im)| (c[map(k) - 1] - Complex64::new(re, im)).norm() / peak)
            .fold(0.0, f64::max)
    };
    let (r21, r63) = (err(c21, &|k| k, false), err(c63, &at63, false));
    println!(
        "{label}: away from the node {:.3} % -> {:.3} % (+ 3 x {:.3} % nec2c's step)",
        r21 * 100.0,
        r63 * 100.0,
        step_rest * 100.0
    );
    assert!(
        r63 + 3.0 * step_rest < 0.01,
        "{label}: {:.3} % at 63 + 3 x nec2c's {:.3} % step reaches 1 %",
        r63 * 100.0,
        step_rest * 100.0
    );
    assert!(r63 < r21, "{label}: must shrink, {r21:.5} -> {r63:.5}");
    if node.is_empty() {
        return;
    }
    let (n21, n63) = (err(c21, &|k| k, true), err(c63, &at63, true));
    let own = node
        .iter()
        .map(|&k| (c63[at63(k) - 1] - c21[k - 1]).norm() / peak)
        .fold(0.0, f64::max);
    println!(
        "{label}: at the node {:.3} % / {:.3} %, fnec's own 21 -> 63 change {:.3} %",
        n21 * 100.0,
        n63 * 100.0,
        own * 100.0
    );
    assert!(
        n21 < 0.01 && n63 < 0.01,
        "{label}: node pins {n21:.5} / {n63:.5}"
    );
    assert!(
        own < 0.003,
        "{label}: fnec's node moved {own:.5} between meshes"
    );
}

/// A T (4 m stem, 3 m bar halves) lit at θ = 45°: every arm sees the field, and
/// the node splits the received current three ways. nec2c's reference is
/// `corpus/series/t45-21.json` at 27× (567 per wire, the 63 deck's 9×, identical
/// to the digit), extended kernel. Its corner segments are 5.3 radii there, under
/// the 10 the series script flags: the 0.2 mm control (`t45-21-r02`, 26 radii)
/// has the same shape (steps 0.171, 0.115, 0.098 % against 0.252, 0.180, 0.135 %),
/// where the inverted-L's 27× breaks (`planewave_ground_contact.rs`). Measured
/// 0.415 → 0.237 % away from the node, 0.361 / 0.497 % at it, fnec's own node
/// change 0.250 %. Was: 21 / 41 against nec2c on the same mesh (FND-265).
#[test]
fn a_t_receives_like_nec2c() {
    // t45-21.json at 27x; nec2c's last step (9x -> 27x): rest 0.00088, node 0.00135.
    const PEAK: f64 = 5.583037e-03;
    const PINS: [(usize, f64, f64); 6] = [
        (1, 1.02030e-04, -3.12190e-04),
        (11, 1.45570e-03, -3.79640e-03),
        (21, 1.73780e-03, -4.50770e-03),
        (22, 3.23860e-03, -4.50970e-03),
        (43, -1.55920e-03, 7.44000e-05),
        (63, -1.28590e-04, 2.63740e-05),
    ];
    assert_receive_converges(
        "T at 45°",
        (
            &routed_deck_currents(&series_deck("t45-21.nec")),
            &routed_deck_currents(&series_deck("t45-63.nec")),
        ),
        PEAK,
        &PINS,
        &[21, 22, 43],
        0.00088,
    );
}

/// FND-197: a loaded junction deck lit by a plane wave. A 300 Ω load on the T's
/// bar, broadside. It was refused as an `LD` card and — as a CLI `--loads-config`
/// load — solved over the plain basis's stamps, 25 % off. At 21 and 63 per wire,
/// the load on segment 5 and 14 (the same point, 0.214 of the bar), against
/// nec2c's `corpus/series/loaded-t-21.json` at 27× (the 63 deck's 9×, identical),
/// extended kernel; its 5.3-radii corners as the T's above. Measured 0.480 →
/// 0.334 % away from the node, 0.503 / 0.706 % at it (nec2c's node at 1 / 3 / 9 /
/// 27 ×: Im −0.95536, −0.95809, −0.96019, −0.96184 of the peak; fnec's, settling,
/// −0.96682 … −0.97044), fnec's own node change 0.219 %. Was: 21 / 41 with the
/// load at 5 and 10 — 0.214 and 0.232 of the bar, two antennas (FND-258's class) —
/// against nec2c's thin kernel below its thin-wire limit (FND-265).
#[test]
fn a_loaded_t_receives_like_nec2c() {
    // loaded-t-21.json at 27x; nec2c's last step (9x -> 27x): rest 0.00110, node 0.00165.
    const PEAK: f64 = 6.375283e-03;
    const PINS: [(usize, f64, f64); 7] = [
        (1, -4.44350e-05, -4.38030e-04),
        (11, -6.29860e-04, -5.32630e-03),
        (21, -9.35250e-04, -6.13200e-03),
        (22, -1.19800e-03, -2.50790e-03),
        (26, -1.24360e-03, -1.99930e-03),
        (43, 2.49770e-04, -3.47320e-03),
        (63, 9.42300e-06, -1.34150e-04),
    ];
    assert_receive_converges(
        "loaded T",
        (
            &routed_deck_currents(&series_deck("loaded-t-21.nec")),
            &routed_deck_currents(&series_deck("loaded-t-63.nec")),
        ),
        PEAK,
        &PINS,
        &[21, 22, 43],
        0.00110,
    );
}

/// A 1 λ loop with a 100 + j50 Ω load mid-side, lit broadside. Kill criterion:
/// under 1 % at 41, shrinking. Measured 0.87 → 0.51 % over the whole table.
/// The band stands as the unloaded loop's does: nec2c's own series on the
/// 21-deck (the load at the same point, `scripts/nec2c-receive-series.py`,
/// `EK 0`, captured 2026-10-23) moves 0.154, 0.078 and 0.046 % of the peak a
/// tripling, a tail of about 0.07 % (FND-296).
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
