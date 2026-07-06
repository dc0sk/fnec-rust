// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-002 (current-source junction solve): a junctioned antenna driven by an
// EX-type-4 current source must solve on continuous conductor paths, the
// symmetric-source cousin of the plane-wave receive path.
//
// The port impedance Z = V/I is a property of the antenna, independent of how it is
// driven. So on a degree-2 junctioned geometry the current-source solve
// (Z = V_port/i0, via solve_hallen_current_source_paths) must match the
// voltage-source solve (Z = 1/I_feed, via solve_hallen_paths) at the same feed —
// the internal consistency gate, no external reference needed.

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::{
    assemble_z_matrix_with_ground, build_conductor_paths, build_current_source_shape_paths,
    build_geometry, build_hallen_rhs_paths, solve_hallen_current_source_paths, solve_hallen_paths,
    ConductorPath, GroundModel, Segment,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn ex(excitation_type: u32, tag: u32, seg: u32, v_re: f64, v_im: f64) -> ExCard {
    ExCard {
        excitation_type,
        tag,
        segment: seg,
        i4: 0,
        voltage_real: v_re,
        voltage_imag: v_im,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }
}

fn paths_index_vectors(paths: &[ConductorPath], n: usize) -> (Vec<usize>, Vec<usize>) {
    let mut path_of = vec![0usize; n];
    let mut free_ends = Vec::with_capacity(paths.len() * 2);
    for (pi, p) in paths.iter().enumerate() {
        for &m in &p.segs {
            path_of[m] = pi;
        }
        free_ends.push(p.free_ends.0);
        free_ends.push(p.free_ends.1);
    }
    (path_of, free_ends)
}

/// Voltage-source feedpoint impedance through the conductor-path delta-gap solver.
fn voltage_source_z(deck: &NecDeck, segs: &[Segment], feed_tag: u32, feed_seg: u32) -> Complex64 {
    let z = assemble_z_matrix_with_ground(segs, FREQ, &GroundModel::FreeSpace);
    let paths = build_conductor_paths(segs).expect("supported degree-2 topology");
    let h = build_hallen_rhs_paths(deck, segs, FREQ, &paths).unwrap();
    let (path_of, free_ends) = paths_index_vectors(&paths, segs.len());
    let sol = solve_hallen_paths(&z, &h.rhs, &h.cos_vec, &path_of, &free_ends).unwrap();
    let idx = segs
        .iter()
        .position(|s| s.tag == feed_tag && s.tag_index == feed_seg)
        .unwrap();
    Complex64::new(1.0, 0.0) / sol.currents[idx]
}

/// Current-source feedpoint impedance (Z = V_port/i0) + the forced feed current,
/// through the conductor-path current-source solver.
fn current_source_z(
    deck: &NecDeck,
    segs: &[Segment],
    feed_tag: u32,
    feed_seg: u32,
    i0: Complex64,
) -> (Complex64, Complex64) {
    let z = assemble_z_matrix_with_ground(segs, FREQ, &GroundModel::FreeSpace);
    let paths = build_conductor_paths(segs).expect("supported degree-2 topology");
    let (shape, cos_vec, src_seg) =
        build_current_source_shape_paths(deck, segs, FREQ, feed_tag, feed_seg, &paths).unwrap();
    let (path_of, free_ends) = paths_index_vectors(&paths, segs.len());
    let sol =
        solve_hallen_current_source_paths(&z, &shape, &cos_vec, src_seg, i0, &path_of, &free_ends)
            .unwrap();
    (sol.port_voltage / i0, sol.currents[src_seg])
}

/// Split λ/2 dipole: two 26-seg arms that BOTH start at the origin (start-to-start,
/// one arm reversed) — a genuine degree-2 junction, fed at the join (wire 1 seg 1).
fn split_dipole() -> NecDeck {
    let mut d = NecDeck::new();
    d.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 26,
        start: [0.0, 0.0, 0.0],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    d.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 26,
        start: [0.0, 0.0, 0.0],
        end: [0.0, 0.0, -5.282],
        radius: 0.001,
    }));
    d
}

/// Bent inverted-V, arms starting at the apex (~5.15 m each, 30° half-angle).
fn inverted_v() -> NecDeck {
    let mut d = NecDeck::new();
    d.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 26,
        start: [0.0, 0.0, 5.0],
        end: [4.46, 0.0, 2.42],
        radius: 0.001,
    }));
    d.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 26,
        start: [0.0, 0.0, 5.0],
        end: [-4.46, 0.0, 2.42],
        radius: 0.001,
    }));
    d
}

fn assert_consistent(base: NecDeck, feed_tag: u32, feed_seg: u32, label: &str) {
    let mut dv = base.clone();
    dv.cards.push(Card::Ex(ex(0, feed_tag, feed_seg, 1.0, 0.0)));
    let segs_v = build_geometry(&dv).unwrap();
    let z_v = voltage_source_z(&dv, &segs_v, feed_tag, feed_seg);

    let mut di = base;
    di.cards.push(Card::Ex(ex(4, feed_tag, feed_seg, 1.0, 0.0)));
    let segs_i = build_geometry(&di).unwrap();
    let (z_i, i_feed) =
        current_source_z(&di, &segs_i, feed_tag, feed_seg, Complex64::new(1.0, 0.0));

    println!("{label}: Z_v={z_v:.4}  Z_i={z_i:.4}  i_feed={i_feed:.6e}");
    assert!(
        (i_feed - Complex64::new(1.0, 0.0)).norm() < 1e-4,
        "{label}: forced feed current not honored: {i_feed}"
    );
    let rel = (z_i - z_v).norm() / z_v.norm();
    assert!(
        rel < 5e-3,
        "{label}: current-source Z {z_i} disagrees with voltage-source Z {z_v} (rel {rel:.2e})"
    );
}

#[test]
fn current_source_split_dipole_matches_voltage_source() {
    assert_consistent(split_dipole(), 1, 1, "split-dipole apex feed");
}

#[test]
fn current_source_inverted_v_matches_voltage_source() {
    assert_consistent(inverted_v(), 1, 1, "inverted-V apex feed");
}

#[test]
fn current_source_split_dipole_scales_with_i0() {
    // Linearity: doubling i0 doubles the port voltage, leaving Z unchanged.
    let mut d = split_dipole();
    d.cards.push(Card::Ex(ex(4, 1, 1, 1.0, 0.0)));
    let segs = build_geometry(&d).unwrap();
    let (z1, _) = current_source_z(&d, &segs, 1, 1, Complex64::new(1.0, 0.0));
    let (z2, i2) = current_source_z(&d, &segs, 1, 1, Complex64::new(2.0, 0.0));
    assert!(
        (i2 - Complex64::new(2.0, 0.0)).norm() < 1e-4,
        "i0=2 not imposed: {i2}"
    );
    let rel = (z2 - z1).norm() / z1.norm();
    assert!(
        rel < 1e-9,
        "impedance changed with drive amplitude (rel {rel:.2e})"
    );
}
