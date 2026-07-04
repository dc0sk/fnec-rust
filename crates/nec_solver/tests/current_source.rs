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
    assemble_z_matrix_with_ground, build_current_source_shape, build_geometry, build_hallen_rhs,
    solve_hallen, solve_hallen_current_source, wire_endpoints_from_segs, GroundModel,
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
    let sol = solve_hallen(&z, &h.rhs, &h.cos_vec, &endpoints, &[]).expect("solve");
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
    let (shape, cos_vec, src_seg) =
        build_current_source_shape(&d, &segs, FREQ_HZ, feed_tag, feed_seg).expect("shape");
    let endpoints = wire_endpoints_from_segs(&segs);
    let sol =
        solve_hallen_current_source(&z, &shape, &cos_vec, src_seg, i0, &endpoints).expect("solve");
    let z_in = sol.port_voltage / i0; // Z = V_port / I₀
    (z_in, sol.currents[src_seg])
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
