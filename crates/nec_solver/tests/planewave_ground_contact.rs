// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-170 — an incident plane wave on wires touching perfect ground.
//!
//! Such a deck solves on the doubled structure: the wires and their mirror images
//! in FREE SPACE (the images are explicit wires), lit by the incident field over
//! PEC (the direct wave plus its reflection). For θ̂ polarization the reflection
//! is exactly a direct wave arriving from 180° − θ, so by linearity the doubled
//! structure's currents are the free-space currents at θ plus those at 180° − θ —
//! nec2c's monopole is that sum to its printed digits, and fnec's must be to
//! round-off (G1): the strongest gate, since it needs no reference.
//!
//! Against nec2c 1.3.1 (captured 2026-10-03, 14.2 MHz, `GN 1`; echo checked:
//! `FREQUENCY : 1.4200E+01`, `PERFECT GROUND`), whole current table, max
//! |ΔI| / peak, 21 → 41 segments per wire: a λ/4 monopole at θ = 45°
//! 4.64 → 2.70 %, at 60° 4.58 → 2.68 % — the doubled dipole's own free-space
//! receive level — and an inverted-L touching ground at θ = 60°, φ = 30°
//! 0.59 → 0.37 %. Loads mirror as the same load on the image (FND-197).

use nec_solver::validate::pre_solve_error;
use nec_solver::{
    assemble_z_matrix_with_ground, build_deck_stamps, build_geometry, ground_model_from_deck,
    plan_hallen_planewave, solve_hallen_planewave_planned, solve_hallen_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;
const PEC: &str = "GE 1\nGN 1\n";
const FREE: &str = "GE 0\n";

fn deck(geometry: &str, ground: &str, wave: &str) -> nec_model::deck::NecDeck {
    nec_parser::parse(&format!(
        "CE\n{geometry}{ground}{wave}FR 0 1 0 0 14.2 0\nEN\n"
    ))
    .expect("parses")
    .deck
}

/// The single-shot route every frontend takes: refusals, the matrix over the
/// deck's ground, the deck's loads, `solve_hallen_routed`.
fn receive(geometry: &str, ground: &str, wave: &str) -> Result<Vec<Complex64>, String> {
    let d = deck(geometry, ground, wave);
    let segs = build_geometry(&d).expect("geometry");
    let g = ground_model_from_deck(&d);
    if let Some(why) = pre_solve_error(&d, &segs, &g) {
        return Err(why);
    }
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &g);
    let loads = build_deck_stamps(&d, &segs, FREQ).diagonal;
    solve_hallen_routed(&d, &segs, &mut z, FREQ, &loads)
        // nec2c numbers the deck's segments (FND-227).
        .map(|r| nec_solver::deck_values(&segs, &r.currents))
        .map_err(|e| e.to_string())
}

/// The receive-sweep route (the CLI's): one plan over the deck's ground, then the
/// planned solve per direction with that direction's deck.
fn receive_planned(geometry: &str, wave: &str) -> Vec<Complex64> {
    let d = deck(geometry, PEC, wave);
    let segs = build_geometry(&d).expect("geometry");
    let g = ground_model_from_deck(&d);
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &g);
    let plan = plan_hallen_planewave(
        &segs,
        FREQ,
        &g,
        &build_deck_stamps(&d, &segs, FREQ).diagonal,
    );
    let currents =
        solve_hallen_planewave_planned(&d, &segs, &z, FREQ, &plan).expect("planned receive");
    nec_solver::deck_values(&segs, &currents)
}

fn monopole(n: u32) -> String {
    format!("GW 1 {n} 0 0 0 0 0 5.282 .001\n")
}

/// The monopole's doubled structure, drawn as a free-space deck: the wire and
/// its image, joined at z = 0.
fn monopole_doubled(n: u32) -> String {
    format!("GW 1 {n} 0 0 0 0 0 5.282 .001\nGW 2 {n} 0 0 0 0 0 -5.282 .001\n")
}

fn inverted_l(n: u32) -> String {
    format!("GW 1 {n} 0 0 0 0 0 3 .001\nGW 2 {n} 0 0 3 4.5 0 3 .001\n")
}

fn inverted_l_doubled(n: u32) -> String {
    format!(
        "GW 1 {n} 0 0 0 0 0 3 .001\nGW 2 {n} 0 0 3 4.5 0 3 .001\n\
         GW 3 {n} 0 0 0 0 0 -3 .001\nGW 4 {n} 0 0 -3 4.5 0 -3 .001\n"
    )
}

fn wave(theta: f64, phi: f64) -> String {
    format!("EX 1 1 1 0 {theta} {phi} 0\n")
}

/// G1: on PEC, θ̂ polarization, the contact deck's currents equal the doubled
/// structure's free-space currents at θ plus at 180° − θ, on the original wires,
/// to round-off — for a straight wire and a bent one.
#[test]
fn a_contact_deck_is_its_free_space_double_lit_from_both_sides() {
    for (contact, doubled, n_orig, theta, phi) in [
        (monopole(21), monopole_doubled(21), 21, 45.0, 0.0),
        (monopole(21), monopole_doubled(21), 21, 60.0, 0.0),
        (inverted_l(21), inverted_l_doubled(21), 42, 60.0, 30.0),
    ] {
        let on_ground = receive(&contact, PEC, &wave(theta, phi)).expect("solves on ground");
        let direct = receive(&doubled, FREE, &wave(theta, phi)).expect("free, θ");
        let mirror = receive(&doubled, FREE, &wave(180.0 - theta, phi)).expect("free, 180-θ");
        assert_eq!(on_ground.len(), n_orig, "only the original wires come back");
        let peak = on_ground.iter().map(|c| c.norm()).fold(0.0, f64::max);
        let worst = (0..n_orig)
            .map(|i| (on_ground[i] - (direct[i] + mirror[i])).norm() / peak)
            .fold(0.0, f64::max);
        assert!(
            worst < 1e-9,
            "θ = {theta}°: the contact solve differs from its double by {worst:.3e} of the peak"
        );
    }
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

/// A λ/4 monopole on PEC lit at θ = 45°, against nec2c at two meshes. Kill
/// criterion: under 4 % at 41 (the doubled dipole's free-space receive level),
/// shrinking.
#[test]
fn a_monopole_on_perfect_ground_receives_like_nec2c() {
    let w = wave(45.0, 0.0);
    let e21 = err(
        &receive(&monopole(21), PEC, &w).unwrap(),
        9.542_895e-2,
        &[
            (1, -8.0685e-02, 5.0957e-02),
            (6, -7.4341e-02, 4.7012e-02),
            (11, -5.8178e-02, 3.6925e-02),
            (16, -3.4077e-02, 2.1763e-02),
            (21, -3.8111e-03, 2.4567e-03),
        ],
    );
    let e41 = err(
        &receive(&monopole(41), PEC, &w).unwrap(),
        9.531_862e-2,
        &[
            (1, -8.0474e-02, 5.1084e-02),
            (11, -7.4388e-02, 4.7280e-02),
            (21, -5.8004e-02, 3.7003e-02),
            (31, -3.3320e-02, 2.1392e-02),
            (41, -2.1075e-03, 1.3662e-03),
        ],
    );
    assert_converges("monopole on GN 1, θ = 45°", e21, e41, 0.04);
}

/// fnec's receive currents on a checked-in series deck (`corpus/series/`), at the
/// deck's segments, by the route `receive` takes.
fn receive_series_deck(name: &str) -> Vec<Complex64> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/series")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let d = nec_parser::parse(&text).expect("parses").deck;
    let segs = build_geometry(&d).expect("geometry");
    let g = ground_model_from_deck(&d);
    assert_eq!(pre_solve_error(&d, &segs, &g), None);
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &g);
    let loads = build_deck_stamps(&d, &segs, FREQ).diagonal;
    let r = solve_hallen_routed(&d, &segs, &mut z, FREQ, &loads).expect("solves");
    nec_solver::deck_values(&segs, &r.currents)
}

/// An inverted-L touching ground (the bent-path route on the doubled structure),
/// at 21 and 63 per wire against one nec2c reference at the same points (a 21-mesh
/// pin k is the 63-mesh segment 3k − 1): `corpus/series/inverted-l-21.json` at 9×,
/// 189 per wire (the 63 deck's 3×, identical to the digit), extended kernel.
///
/// Not at 27× (FND-265, FND-267): there the corner segments are 5.3 radii, and
/// nec2c's whole stem current shifts by 0.12 % of the peak (steps 0.114, 0.022,
/// 0.125 %), with `EK` as without — the same in free space on the doubled
/// structure. The 0.2 mm control (`inverted-l-21-r02`) stays smooth to 27× (26
/// radii; 0.074, 0.012, 0.007 %) and jumps at 81× (8.8 radii). At 9× the corners
/// are 15.9 radii. Gate: the error plus three of nec2c's last steps (0.022 %)
/// under 1 % of the peak, and shrinking: measured 0.244 → 0.123 %. Was: 21 / 41
/// against nec2c on the same mesh.
#[test]
fn an_inverted_l_touching_perfect_ground_receives_like_nec2c() {
    // inverted-l-21.json at 9x; nec2c's last step (3x -> 9x): 0.00022.
    const PEAK: f64 = 1.716362e-02;
    const STEP: f64 = 0.00022;
    const PINS: [(usize, f64, f64); 6] = [
        (1, 1.07320e-03, 1.71290e-02),
        (11, 8.48140e-04, 1.66920e-02),
        (21, 2.48750e-04, 1.55450e-02),
        (22, 1.53000e-04, 1.53620e-02),
        (32, -4.32890e-04, 1.00750e-02),
        (42, -4.89810e-05, 6.57520e-04),
    ];
    let e = |c: &[Complex64], at: &dyn Fn(usize) -> usize| {
        PINS.iter()
            .map(|&(k, re, im)| (c[at(k) - 1] - Complex64::new(re, im)).norm() / PEAK)
            .fold(0.0, f64::max)
    };
    let e21 = e(&receive_series_deck("inverted-l-21.nec"), &|k| k);
    let e63 = e(&receive_series_deck("inverted-l-63.nec"), &|k| 3 * k - 1);
    println!(
        "inverted-L on GN 1: {:.3} % -> {:.3} %",
        e21 * 100.0,
        e63 * 100.0
    );
    assert!(
        e63 + 3.0 * STEP < 0.01,
        "inverted-L: {:.3} % at 63 + 3 x nec2c's step reaches 1 %",
        e63 * 100.0
    );
    assert!(e63 < e21, "inverted-L: must shrink, {e21:.5} -> {e63:.5}");
}

/// The receive-sweep route (one plan, a deck per direction) gives exactly the
/// single-shot answer — it must double the structure itself, not solve the
/// original wires over the deck's own matrix — and takes each call's direction.
#[test]
fn the_sweep_route_doubles_the_structure_too() {
    for theta in [30.0, 75.0] {
        let single = receive(&monopole(21), PEC, &wave(theta, 0.0)).unwrap();
        let planned = receive_planned(&monopole(21), &wave(theta, 0.0));
        for (a, b) in single.iter().zip(&planned) {
            assert!(
                (a - b).norm() <= 1e-12 * b.norm().max(1e-12),
                "θ = {theta}°: {a} vs {b}"
            );
        }
    }
}

/// Loads on a contact deck lit by a plane wave (FND-197): the image of a series
/// load is the same load, so the loaded contact deck equals its loaded
/// free-space double lit from θ and from 180° − θ, to round-off — on the
/// single-shot route and on the sweep route.
#[test]
fn a_loaded_contact_deck_is_its_loaded_double() {
    let load = "LD 4 1 5 5 300 50\n";
    let contact = format!("{}{load}", monopole(21));
    let doubled = format!("{}{load}LD 4 2 5 5 300 50\n", monopole_doubled(21));
    for theta in [45.0, 60.0] {
        let on_ground = receive(&contact, PEC, &wave(theta, 0.0)).expect("solves on ground");
        let direct = receive(&doubled, FREE, &wave(theta, 0.0)).expect("free, θ");
        let mirror = receive(&doubled, FREE, &wave(180.0 - theta, 0.0)).expect("free, 180-θ");
        let planned = receive_planned(&contact, &wave(theta, 0.0));
        let peak = on_ground.iter().map(|c| c.norm()).fold(0.0, f64::max);
        for i in 0..on_ground.len() {
            let want = direct[i] + mirror[i];
            assert!(
                (on_ground[i] - want).norm() <= 1e-9 * peak,
                "θ = {theta}°, seg {i}: {} vs {want}",
                on_ground[i]
            );
            assert!(
                (planned[i] - on_ground[i]).norm() <= 1e-12 * peak,
                "θ = {theta}°, seg {i}: sweep {} vs single {}",
                planned[i],
                on_ground[i]
            );
        }
    }
}
