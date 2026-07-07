// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-006: near-ground feedpoint impedance — the ground-image current-direction
// sign gate.
//
// fnec's Hallén operator carries a documented systematic reactance offset vs nec2c,
// so absolute impedance parity is not the gate (see fnec-validation-strategy). The
// physical, offset-cancelling quantity is the *ground-induced delta*
// ΔZ = Z(over ground) − Z(free space): the reflected image adds a well-defined,
// sign-definite contribution that must match nec2c. A prior sign inversion in the
// Z-matrix image gave ΔZ the wrong sign (a horizontal dipole's radiation resistance
// *rose* low over ground instead of dropping); this test pins the corrected sign and
// magnitude against captured nec2c references, for two geometries whose ground
// effects have opposite sign.

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, solve_hallen,
    wire_endpoints_from_segs, GroundModel,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;
// Average ground (nec2c GN 0/2 with these constants).
const GROUND: GroundModel = GroundModel::SimpleFiniteGround {
    eps_r: 13.0,
    sigma: 0.005,
};

fn feedpoint_z(deck: &NecDeck, ground: &GroundModel, feed_tag: u32, feed_seg: u32) -> Complex64 {
    let segs = build_geometry(deck).unwrap();
    let z = assemble_z_matrix_with_ground(&segs, FREQ, ground);
    let h = build_hallen_rhs(deck, &segs, FREQ).unwrap();
    let endpoints = wire_endpoints_from_segs(&segs);
    let sol = solve_hallen(&z, &h.rhs, &h.cos_vec, &endpoints, &[]).unwrap();
    let idx = segs
        .iter()
        .position(|s| s.tag == feed_tag && s.tag_index == feed_seg)
        .unwrap();
    Complex64::new(1.0, 0.0) / sol.currents[idx]
}

fn dipole(seg: u32, a: [f64; 3], b: [f64; 3], feed_seg: u32) -> NecDeck {
    let mut d = NecDeck::new();
    d.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: seg,
        start: a,
        end: b,
        radius: 0.001,
    }));
    d.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: feed_seg,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    d
}

/// Assert the ground-induced resistance delta matches nec2c in sign and magnitude
/// (relative + absolute floor, tolerant of fnec's discretization / scalar-Γ model).
fn assert_ground_delta_r(fnec_dr: f64, nec_dr: f64, label: &str) {
    assert!(
        fnec_dr.signum() == nec_dr.signum(),
        "{label}: ground resistance delta has the WRONG sign — fnec {fnec_dr:+.2} Ω vs nec2c {nec_dr:+.2} Ω \
         (the ground-image current-direction sign is inverted)"
    );
    let tol = 0.25 * nec_dr.abs() + 3.0;
    assert!(
        (fnec_dr - nec_dr).abs() < tol,
        "{label}: ground resistance delta fnec {fnec_dr:+.2} Ω vs nec2c {nec_dr:+.2} Ω (tol {tol:.2})"
    );
}

#[test]
fn horizontal_dipole_low_over_ground_resistance_drops() {
    // λ/2 horizontal dipole at 0.1λ (2.11 m) over average ground. nec2c (GN 0):
    // free space 78.94 + j45.28, over ground 51.87 + j63.40 → ΔR = −27.07 Ω, ΔX = +18.12 Ω
    // (captured 2026-07-08). Radiation resistance DROPS (image opposes) — the
    // sign the pre-fix code got backwards.
    let deck = dipole(21, [-5.28, 0.0, 2.11], [5.28, 0.0, 2.11], 11);
    let z_fs = feedpoint_z(&deck, &GroundModel::FreeSpace, 1, 11);
    let z_gr = feedpoint_z(&deck, &GROUND, 1, 11);
    let dr = z_gr.re - z_fs.re;
    let dx = z_gr.im - z_fs.im;
    println!("horizontal 0.1λ: fs={z_fs:.2} ground={z_gr:.2} ΔR={dr:+.2} ΔX={dx:+.2}");
    assert_ground_delta_r(dr, -27.07, "horizontal dipole 0.1λ");
    assert!(
        dx > 0.0,
        "horizontal-dipole ground reactance delta must be positive, got {dx:+.2}"
    );
}

#[test]
fn vertical_dipole_near_ground_resistance_rises() {
    // λ/2 vertical dipole with its base 0.5 m above average ground. nec2c (GN 2):
    // free space 79.35 + j46.22, over ground 97.32 + j44.15 → ΔR = +17.97 Ω
    // (captured 2026-07-08). Radiation resistance RISES (vertical image reinforces).
    let deck = dipole(51, [0.0, 0.0, 0.5], [0.0, 0.0, 11.064], 26);
    let z_fs = feedpoint_z(&deck, &GroundModel::FreeSpace, 1, 26);
    let z_gr = feedpoint_z(&deck, &GROUND, 1, 26);
    let dr = z_gr.re - z_fs.re;
    println!("vertical near-ground: fs={z_fs:.2} ground={z_gr:.2} ΔR={dr:+.2}");
    assert_ground_delta_r(dr, 17.97, "vertical dipole near ground");
}
