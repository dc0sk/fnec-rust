// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH8-CHK-001: validate the current-source (EX type 4) Hallén solve.
//
// The port impedance Z = V/I is a property of the antenna, independent of how the
// port is driven. So a current source of I₀ at the feed must yield the same
// feedpoint impedance as a voltage source at the same feed — the internal
// consistency gate (no external reference needed).

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, solve_current_source_hallen,
    solve_hallen, wire_endpoints_from_segs, GroundModel,
};
use num_complex::Complex64;

const FREQ_HZ: f64 = 14.2e6;

fn dipole(seg: u32, half_len: f64) -> NecDeck {
    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: seg,
        start: [0.0, 0.0, -half_len],
        end: [0.0, 0.0, half_len],
        radius: 0.001,
    }));
    deck
}

fn voltage_source_impedance(deck: &NecDeck, feed_tag: u32, feed_seg: u32) -> Complex64 {
    let mut d = deck.clone();
    d.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: feed_tag,
        segment: feed_seg,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    let segs = build_geometry(&d).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ_HZ, &GroundModel::FreeSpace);
    let h = build_hallen_rhs(&d, &segs, FREQ_HZ).expect("rhs");
    let endpoints = wire_endpoints_from_segs(&segs);
    let sol = solve_hallen(&z, &h.rhs, &h.cos_vec, &h.sin_vec, &endpoints, &[]).expect("solve");
    let feed_idx = segs
        .iter()
        .position(|s| s.tag == feed_tag && s.tag_index == feed_seg)
        .unwrap();
    Complex64::new(1.0, 0.0) / sol.currents[feed_idx] // Z = V / I_feed
}

fn current_source_impedance(
    deck: &NecDeck,
    feed_tag: u32,
    feed_seg: u32,
    i0: Complex64,
) -> (Complex64, Complex64) {
    let mut d = deck.clone();
    d.cards.push(Card::Ex(ExCard {
        excitation_type: 4, // NEC2 current source
        tag: feed_tag,
        segment: feed_seg,
        i4: 0,
        voltage_real: i0.re,
        voltage_imag: i0.im,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    let segs = build_geometry(&d).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ_HZ, &GroundModel::FreeSpace);
    // Through the production entry point, not a solver called directly: the
    // bespoke current-source solvers are gone (FND-118), and the routing this
    // function performs is part of what a caller gets.
    let fp = solve_current_source_hallen(&d, &segs, &z, FREQ_HZ).expect("solve");
    let src_seg = segs
        .iter()
        .position(|s| s.tag == feed_tag && s.tag_index == feed_seg)
        .expect("feed segment");
    let z_in = fp.port_voltage / i0; // Z = V_port / I₀
    (z_in, fp.currents[src_seg])
}

#[test]
fn current_source_impedance_matches_voltage_source() {
    // Center-fed λ/2 dipole, 51 segments, feed at segment 26.
    let deck = dipole(51, 5.282);
    let z_v = voltage_source_impedance(&deck, 1, 26);
    let (z_i, i_feed) = current_source_impedance(&deck, 1, 26, Complex64::new(1.0, 0.0));

    println!("Z(voltage source) = {z_v:.4}");
    println!("Z(current source) = {z_i:.4}");
    println!("forced feed current = {i_feed:.6e} (should be ~1.0)");

    // The forced current must be honored.
    assert!(
        (i_feed - Complex64::new(1.0, 0.0)).norm() < 1e-6,
        "current source did not impose I₀=1 at the feed: got {i_feed}"
    );
    // Port impedance must be independent of drive type.
    let rel = (z_i - z_v).norm() / z_v.norm();
    assert!(
        rel < 5e-3,
        "current-source impedance {z_i} disagrees with voltage-source {z_v} (rel {rel:.2e})"
    );
}

#[test]
fn current_source_scales_currents_with_i0() {
    // Doubling the forced current doubles every segment current and the port
    // voltage (linearity), leaving the impedance unchanged.
    let deck = dipole(41, 5.282);
    let (z1, _) = current_source_impedance(&deck, 1, 21, Complex64::new(1.0, 0.0));
    let (z2, i2) = current_source_impedance(&deck, 1, 21, Complex64::new(2.0, 0.0));

    assert!(
        (i2 - Complex64::new(2.0, 0.0)).norm() < 1e-6,
        "I₀=2 not imposed: got {i2}"
    );
    let rel = (z2 - z1).norm() / z1.norm();
    assert!(
        rel < 1e-9,
        "impedance changed with drive amplitude (rel {rel:.2e})"
    );
}

#[test]
fn current_source_offset_feed_matches_voltage_source() {
    // Off-center feed: impedance consistency must still hold.
    let deck = dipole(51, 5.282);
    let z_v = voltage_source_impedance(&deck, 1, 18);
    let (z_i, _) = current_source_impedance(&deck, 1, 18, Complex64::new(1.0, 0.0));
    let rel = (z_i - z_v).norm() / z_v.norm();
    assert!(
        rel < 5e-3,
        "off-center current-source Z {z_i} != voltage-source Z {z_v} (rel {rel:.2e})"
    );
}

#[test]
fn current_source_two_wire_array_matches_voltage_source() {
    // Two parallel dipoles; current source on wire 1. The port impedance (with
    // array mutual coupling) must still be independent of drive type.
    let mut base = NecDeck::new();
    base.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 21,
        start: [0.0, 0.0, -2.5],
        end: [0.0, 0.0, 2.5],
        radius: 0.001,
    }));
    base.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 21,
        start: [1.0, 0.0, -2.5],
        end: [1.0, 0.0, 2.5],
        radius: 0.001,
    }));
    // feed at wire-1 center (seg 11).
    let z_v = voltage_source_impedance(&base, 1, 11);
    let (z_i, i_feed) = current_source_impedance(&base, 1, 11, Complex64::new(1.0, 0.0));
    assert!(
        (i_feed - Complex64::new(1.0, 0.0)).norm() < 1e-6,
        "forced current not honored: {i_feed}"
    );
    let rel = (z_i - z_v).norm() / z_v.norm();
    assert!(
        rel < 5e-3,
        "two-wire current-source Z {z_i} != voltage-source Z {z_v} (rel {rel:.2e})"
    );
}

/// The dipole of two halves at 28.4 MHz, the second half bent by `t` rad at the
/// node, fed `EX 4` on segment `feed` of the first.
fn two_halves(t: f64, feed: u32, extra: &str) -> NecDeck {
    let (x, zt) = (2.64 * t.sin(), 2.64 * t.cos());
    let text = format!(
        "CE\nGW 1 10 0 0 -2.64 0 0 0 .001\nGW 2 10 0 0 0 {x:.12} 0 {zt:.12} .001\nGE 0\n\
         {extra}EX 4 1 {feed} 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n"
    );
    nec_parser::parse(&text).expect("parses").deck
}

/// The port voltage of `deck` through the routed solve, with `loads` the run's
/// load diagonal — which the deck's cards need not carry.
fn routed_port_voltage(deck: &NecDeck, loads: &[Complex64]) -> Complex64 {
    let segs = build_geometry(deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, 28.4e6, &GroundModel::FreeSpace);
    nec_solver::solve_hallen_routed(deck, &segs, &mut z, 28.4e6, loads)
        .expect("routed")
        .port_voltage
        .expect("port voltage")
}

/// FND-310: the library entry solves straight and collinear wires and refuses a
/// bent or reversed path by name. The path solve is the routed solve's alone; its
/// second copy here rebuilt the deck's `LD` cards for its gaps and marked its own
/// source only, so a run's other loads and ports were not gaps there.
fn library_refuses_the_bent_path(deck: &NecDeck) {
    let segs = build_geometry(deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, 28.4e6, &GroundModel::FreeSpace);
    match solve_current_source_hallen(deck, &segs, &z, 28.4e6) {
        Err(e @ nec_solver::CurrentSourceError::NeedsRoutedSolve) => {
            assert!(e.to_string().contains("solve_hallen_routed"), "{e}");
        }
        other => panic!("a bent path must be refused by name, got {other:?}"),
    }
}

/// FND-270: the routed current-source solve marks the source segment as the gap
/// its node rows must not extrapolate across. The dipole of two halves, fed
/// beside their node: straight it is one section; bent by 2e-4 rad the node row
/// read the port 1 % low, against 0.11 % with the gap marked. (Through the
/// library entry until FND-310, which agreed with the routed solve exactly on
/// main a6b7ffb, bent and straight.)
#[test]
fn a_current_source_beside_a_vanishing_bend_is_the_straight_join() {
    let (straight, bent) = (
        routed_port_voltage(&two_halves(0.0, 10, ""), &[]),
        routed_port_voltage(&two_halves(2e-4, 10, ""), &[]),
    );
    let step = (bent.re - straight.re).abs() / straight.re;
    assert!(step < 0.002, "{bent} vs straight {straight}: {step:.4}");
    library_refuses_the_bent_path(&two_halves(2e-4, 10, ""));
}

/// FND-270 and FND-310: a load step is a gap too, and the run's load, not the
/// deck's card. A j1000 Ω load on the segment beyond a 2e-4 rad bend, the source
/// far from it, given **only as the load diagonal** — the deck has no `LD` card,
/// as a `--loads-config` load does not. The straight deck is one section with no
/// node row, so its answer does not depend on the gaps at all; the bent one
/// matches it only if the load is marked: 0.72 % with it, 10.8 % with the load
/// left out of the gaps. Two segments from the node the load steps 0.02 % either
/// way (the control).
#[test]
fn a_load_only_the_run_carries_beside_the_bend_is_the_straight_join() {
    let load_at = |at: u32| {
        // The diagonal an `LD 4` card would stamp, solved on a deck without it.
        let carded = two_halves(0.0, 5, &format!("LD 4 2 {at} {at} 0 1000\n"));
        let segs = build_geometry(&carded).expect("geometry");
        nec_solver::build_loads(&carded, &segs, 28.4e6).0
    };
    let rel = |a: Complex64, b: Complex64| (a - b).norm() / b.norm();
    let near = load_at(1);
    let (straight, bent) = (
        routed_port_voltage(&two_halves(0.0, 5, ""), &near),
        routed_port_voltage(&two_halves(2e-4, 5, ""), &near),
    );
    assert!(rel(bent, straight) < 1e-2, "{bent} vs straight {straight}");
    let far = load_at(3);
    let (s3, b3) = (
        routed_port_voltage(&two_halves(0.0, 5, ""), &far),
        routed_port_voltage(&two_halves(2e-4, 5, ""), &far),
    );
    assert!(rel(b3, s3) < 5e-4, "{b3} vs straight {s3}");
    library_refuses_the_bent_path(&two_halves(2e-4, 5, ""));
}
