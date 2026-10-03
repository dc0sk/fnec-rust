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
//! 0.59 → 0.37 %.

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
        .map(|r| r.currents)
        .map_err(|e| e.to_string())
}

/// The receive-sweep route (the CLI's): one plan over the deck's ground, then the
/// planned solve per direction with that direction's deck.
fn receive_planned(geometry: &str, wave: &str) -> Vec<Complex64> {
    let d = deck(geometry, PEC, wave);
    let segs = build_geometry(&d).expect("geometry");
    let g = ground_model_from_deck(&d);
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &g);
    let plan = plan_hallen_planewave(&segs, FREQ, &g);
    solve_hallen_planewave_planned(&d, &segs, &z, FREQ, &plan).expect("planned receive")
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

/// An inverted-L touching ground (the bent-path route on the doubled structure).
/// Kill criterion: under 1 % at 41, shrinking.
#[test]
fn an_inverted_l_touching_perfect_ground_receives_like_nec2c() {
    let w = wave(60.0, 30.0);
    let e21 = err(
        &receive(&inverted_l(21), PEC, &w).unwrap(),
        1.716_240e-2,
        &[
            (1, 1.0702e-03, 1.7129e-02),
            (11, 8.4545e-04, 1.6692e-02),
            (21, 2.4684e-04, 1.5545e-02),
            (22, 1.5117e-04, 1.5362e-02),
            (32, -4.3384e-04, 1.0070e-02),
            (42, -4.7306e-05, 6.3420e-04),
        ],
    );
    let e41 = err(
        &receive(&inverted_l(41), PEC, &w).unwrap(),
        1.716_554e-2,
        &[
            (1, 1.0726e-03, 1.7132e-02),
            (21, 8.4730e-04, 1.6694e-02),
            (41, 2.2936e-04, 1.5512e-02),
            (42, 1.7984e-04, 1.5421e-02),
            (62, -4.3312e-04, 1.0073e-02),
            (82, -2.6501e-05, 3.5290e-04),
        ],
    );
    assert_converges("inverted-L on GN 1", e21, e41, 0.01);
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

/// Still refused, by name: loads on a contact deck lit by a plane wave (the
/// doubled matrix would need the image loads), on both routes.
#[test]
fn a_loaded_contact_deck_is_refused_on_both_routes() {
    let geometry = format!("{}LD 0 1 5 5 50 0 0\n", monopole(21));
    let e = receive(&geometry, PEC, &wave(45.0, 0.0)).unwrap_err();
    assert!(e.contains("LD loads") && e.contains("touching"), "{e}");
    let d = deck(&geometry, PEC, &wave(45.0, 0.0));
    let segs = build_geometry(&d).unwrap();
    let g = ground_model_from_deck(&d);
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &g);
    let plan = plan_hallen_planewave(&segs, FREQ, &g);
    let e = solve_hallen_planewave_planned(&d, &segs, &z, FREQ, &plan)
        .unwrap_err()
        .to_string();
    assert!(e.contains("LD loads"), "{e}");
}
