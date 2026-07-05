// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-004: near electric-field computation (NE card), validated against the
// far field it must reduce to at large range and by dipole symmetry.

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::*;
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn z_dipole() -> (Vec<Segment>, Vec<Complex64>, Complex64) {
    let mut d = NecDeck::new();
    d.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 51,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    d.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: 26,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    let segs = build_geometry(&d).unwrap();
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &GroundModel::FreeSpace);
    let h = build_hallen_rhs(&d, &segs, FREQ).unwrap();
    let ep = wire_endpoints_from_segs(&segs);
    let sol = solve_hallen(&z, &h.rhs, &h.cos_vec, &ep, &[]).unwrap();
    let i_feed = sol.currents[25];
    (segs, sol.currents, i_feed)
}

#[test]
fn near_field_far_limit_matches_gain_derived_far_field() {
    let (segs, currents, i_feed) = z_dipole();
    let lambda = 299_792_458.0 / FREQ;
    let r = 200.0 * lambda;
    let th = 60.0_f64.to_radians();
    let p = NearFieldPoint {
        x: r * th.sin(),
        y: 0.0,
        z: r * th.cos(),
    };
    let e = near_e_field(&segs, &currents, FREQ, &[p])[0].e;

    // At 200λ the field must be transverse (E_r ≈ 0).
    let r_hat = [th.sin(), 0.0, th.cos()];
    let th_hat = [th.cos(), 0.0, -th.sin()];
    let e_r = e[0] * r_hat[0] + e[2] * r_hat[2];
    let e_theta = e[0] * th_hat[0] + e[2] * th_hat[2];
    assert!(
        e_r.norm() / e_theta.norm() < 0.01,
        "near field at 200λ must be transverse"
    );

    // |E_theta| must match the gain-derived far field:
    // G = 4π r² (|E|²/2η)/P_in  →  |E_theta| = sqrt(G·2η·P_in/4π)/r.
    let eta = 4.0 * std::f64::consts::PI * 1e-7 * 299_792_458.0;
    let p_in = 0.5 * (Complex64::new(1.0, 0.0) * i_feed.conj()).re;
    let g_dbi = compute_radiation_pattern(
        &segs,
        &currents,
        FREQ,
        &[FarFieldPoint {
            theta_deg: 60.0,
            phi_deg: 0.0,
        }],
        &GroundModel::FreeSpace,
    )[0]
    .gain_theta_dbi;
    let g = 10f64.powf(g_dbi / 10.0);
    let e_expected = (g * 2.0 * eta * p_in / (4.0 * std::f64::consts::PI)).sqrt() / r;
    let rel = (e_theta.norm() - e_expected).abs() / e_expected;
    assert!(
        rel < 0.02,
        "near-field magnitude {:.4e} must match gain-derived far field {:.4e} (rel {:.4})",
        e_theta.norm(),
        e_expected,
        rel
    );
}

#[test]
fn near_field_broadside_is_axis_polarized() {
    // On the equatorial x-axis a z-dipole's E is purely z (parallel to the wire),
    // with Ex ≈ 0 and Ey = 0 by symmetry.
    let (segs, currents, _) = z_dipole();
    let e = near_e_field(
        &segs,
        &currents,
        FREQ,
        &[NearFieldPoint {
            x: 3.0,
            y: 0.0,
            z: 0.0,
        }],
    )[0]
    .e;
    assert!(e[1].norm() < 1e-12, "Ey must vanish by symmetry: {}", e[1]);
    assert!(
        e[0].norm() < 1e-6 * e[2].norm(),
        "Ex must vanish at broadside (E is z-polarized): Ex={}, Ez={}",
        e[0],
        e[2]
    );
    assert!(e[2].norm() > 0.0, "Ez must be non-zero");
}

#[test]
fn near_h_field_far_limit_impedance_and_azimuthal() {
    let (segs, currents, _) = z_dipole();
    let eta = 4.0 * std::f64::consts::PI * 1e-7 * 299_792_458.0;
    let lambda = 299_792_458.0 / FREQ;
    let r = 200.0 * lambda;
    let th = 60.0_f64.to_radians();
    let p = NearFieldPoint {
        x: r * th.sin(),
        y: 0.0,
        z: r * th.cos(),
    };
    let e = near_e_field(&segs, &currents, FREQ, &[p])[0].e;
    let hf = near_h_field(&segs, &currents, FREQ, &[p])[0].h;
    let emag = (e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr()).sqrt();
    let hmag = (hf[0].norm_sqr() + hf[1].norm_sqr() + hf[2].norm_sqr()).sqrt();
    // Far field: |E| = η·|H|.
    assert!(
        ((emag / hmag) - eta).abs() / eta < 0.02,
        "far-field |E|/|H| = {:.3} must equal η = {:.3}",
        emag / hmag,
        eta
    );
    // H is azimuthal (transverse to r̂) and, for a z-dipole in the x-z plane,
    // purely y-directed (φ̂).
    let r_hat = [th.sin(), 0.0, th.cos()];
    let h_dot_r = hf[0] * r_hat[0] + hf[2] * r_hat[2];
    assert!(
        h_dot_r.norm() / hmag < 0.01,
        "H must be transverse (azimuthal)"
    );
    assert!(
        hf[0].norm() < 1e-3 * hf[1].norm() && hf[2].norm() < 1e-3 * hf[1].norm(),
        "H must be y-directed (φ̂) in the x-z plane"
    );
}
