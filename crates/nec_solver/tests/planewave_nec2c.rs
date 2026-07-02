// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH8-CHK-002: validate the incident-plane-wave Hallén solve.
//
// Two independent checks:
//   1. Distribution-shape parity vs an external nec2c reference (the induced
//      current *shape* along the wire).
//   2. Internal Rayleigh–Carson reciprocity: the plane-wave receive solve's
//      short-circuit terminal current tracks the *transmit* far-field pattern
//      (uses the already-validated farfield path — no external reference).
//
// Note on absolute parity: fnec's Hallén operator and nec2c differ
// systematically in reactance/current-phase even for the *driven* dipole on
// this geometry (fnec's corpus impedance gates are regression gates against
// fnec's own golden values, not tight nec2c parity). That difference is a
// constant complex factor shared by the driven and plane-wave solves, so it is
// removed by peak-alignment before the shape comparison — it is not a
// plane-wave-specific error. See docs/ph8-chk-002-plane-wave-excitation.md.

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_planewave_hallen,
    compute_radiation_pattern, solve_hallen_planewave, wire_endpoints_from_segs, FarFieldPoint,
    GroundModel,
};
use num_complex::Complex64;

const FREQ_HZ: f64 = 14.2e6;
const HALF_LEN: f64 = 5.282; // λ/2 dipole at 14.2 MHz (matches corpus geometry)
const NSEG: u32 = 51;

// nec2c "CURRENTS AND LOCATION" (real, imag) amps for the λ/2 51-seg wire,
// EX 1 (linear plane wave) θ=30 φ=0 η=0. Captured 2026-07-02.
const NEC2C_CURRENTS: [(f64, f64); 51] = [
    (-1.1717e-3, 6.9702e-4),
    (-3.2053e-3, 1.9082e-3),
    (-5.0848e-3, 3.0298e-3),
    (-6.8821e-3, 4.1050e-3),
    (-8.6109e-3, 5.1423e-3),
    (-1.0275e-2, 6.1442e-3),
    (-1.1875e-2, 7.1112e-3),
    (-1.3409e-2, 8.0426e-3),
    (-1.4875e-2, 8.9369e-3),
    (-1.6269e-2, 9.7926e-3),
    (-1.7589e-2, 1.0608e-2),
    (-1.8831e-2, 1.1381e-2),
    (-1.9993e-2, 1.2110e-2),
    (-2.1071e-2, 1.2794e-2),
    (-2.2063e-2, 1.3429e-2),
    (-2.2967e-2, 1.4016e-2),
    (-2.3779e-2, 1.4551e-2),
    (-2.4499e-2, 1.5034e-2),
    (-2.5124e-2, 1.5464e-2),
    (-2.5653e-2, 1.5838e-2),
    (-2.6085e-2, 1.6157e-2),
    (-2.6420e-2, 1.6419e-2),
    (-2.6656e-2, 1.6623e-2),
    (-2.6794e-2, 1.6769e-2),
    (-2.6835e-2, 1.6857e-2),
    (-2.6778e-2, 1.6886e-2),
    (-2.6624e-2, 1.6856e-2),
    (-2.6376e-2, 1.6768e-2),
    (-2.6035e-2, 1.6620e-2),
    (-2.5601e-2, 1.6415e-2),
    (-2.5079e-2, 1.6152e-2),
    (-2.4469e-2, 1.5833e-2),
    (-2.3776e-2, 1.5457e-2),
    (-2.3001e-2, 1.5027e-2),
    (-2.2149e-2, 1.4544e-2),
    (-2.1222e-2, 1.4008e-2),
    (-2.0225e-2, 1.3421e-2),
    (-1.9161e-2, 1.2785e-2),
    (-1.8034e-2, 1.2101e-2),
    (-1.6850e-2, 1.1372e-2),
    (-1.5611e-2, 1.0599e-2),
    (-1.4322e-2, 9.7837e-3),
    (-1.2987e-2, 8.9282e-3),
    (-1.1611e-2, 8.0343e-3),
    (-1.0198e-2, 7.1035e-3),
    (-8.7505e-3, 6.1372e-3),
    (-7.2716e-3, 5.1361e-3),
    (-5.7624e-3, 4.0998e-3),
    (-4.2211e-3, 3.0258e-3),
    (-2.6379e-3, 1.9055e-3),
    (-9.5593e-4, 6.9600e-4),
];

fn dipole_deck() -> NecDeck {
    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: NSEG,
        start: [0.0, 0.0, -HALF_LEN],
        end: [0.0, 0.0, HALF_LEN],
        radius: 0.001,
    }));
    deck
}

fn plane_wave_card(theta_deg: f64, phi_deg: f64, eta_deg: f64) -> Card {
    Card::Ex(ExCard {
        excitation_type: 1,
        tag: 1,
        segment: 1,
        i4: 0,
        voltage_real: theta_deg,
        voltage_imag: phi_deg,
        polarization_deg: eta_deg,
        polarization_ratio: 0.0,
    })
}

fn solve_plane_wave(theta_deg: f64, phi_deg: f64, eta_deg: f64) -> Vec<Complex64> {
    let mut deck = dipole_deck();
    deck.cards
        .push(plane_wave_card(theta_deg, phi_deg, eta_deg));
    let segs = build_geometry(&deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ_HZ, &GroundModel::FreeSpace);
    let pw = build_planewave_hallen(&deck, &segs, FREQ_HZ).expect("planewave rhs");
    let endpoints = wire_endpoints_from_segs(&segs);
    solve_hallen_planewave(&z, &pw.rhs, &pw.cos_vec, &pw.sin_vec, &endpoints).expect("solve")
}

#[test]
fn planewave_currents_match_nec2c_shape() {
    let currents = solve_plane_wave(30.0, 0.0, 0.0);
    assert_eq!(currents.len(), 51);

    // Remove the constant fnec-vs-nec2c operator offset by aligning on the peak
    // (center) segment, then compare the induced-current *distribution*.
    let peak = 25usize; // seg 26 (1-based)
    let ref_peak = Complex64::new(NEC2C_CURRENTS[peak].0, NEC2C_CURRENTS[peak].1);
    let align = ref_peak / currents[peak];

    let mut max_rel = 0.0f64;
    for (i, &(re, im)) in NEC2C_CURRENTS.iter().enumerate() {
        let refc = Complex64::new(re, im);
        let ours = currents[i] * align;
        max_rel = max_rel.max((ours - refc).norm() / ref_peak.norm());
    }
    println!("shape max rel (vs peak) = {max_rel:.4}");
    assert!(
        max_rel < 0.05,
        "plane-wave induced-current shape deviates from nec2c by {max_rel:.4} (>5% of peak)"
    );
}

#[test]
fn planewave_broadside_current_is_symmetric() {
    // Broadside (θ=90) uniform illumination on a symmetric wire → the induced
    // current must be symmetric about the center, I[m] == I[N-1-m].
    let currents = solve_plane_wave(90.0, 0.0, 0.0);
    let n = currents.len();
    let peak = currents[n / 2].norm().max(1e-30);
    let mut max_asym = 0.0f64;
    for i in 0..n {
        let j = n - 1 - i;
        max_asym = max_asym.max((currents[i] - currents[j]).norm() / peak);
    }
    println!("broadside asymmetry = {max_asym:.2e}");
    assert!(
        max_asym < 1e-6,
        "broadside plane-wave current is not symmetric (asym={max_asym:.2e})"
    );
}

#[test]
fn planewave_reciprocity_matches_transmit_pattern() {
    // Rayleigh–Carson reciprocity: the short-circuit terminal (center-segment)
    // current induced by a plane wave from θ is proportional to the transmit
    // far-field at θ. So |I_center(θ)|² / G_θ(θ) is constant across angles.
    //
    // Transmit pattern from the *validated* farfield path on the driven dipole.
    let mut driven = dipole_deck();
    driven.cards.push(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: 26,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
    }));
    let segs = build_geometry(&driven).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ_HZ, &GroundModel::FreeSpace);
    let h = nec_solver::build_hallen_rhs(&driven, &segs, FREQ_HZ).expect("rhs");
    let endpoints = wire_endpoints_from_segs(&segs);
    let tx = nec_solver::solve_hallen(&z, &h.rhs, &h.cos_vec, &endpoints, &[]).expect("tx solve");

    let angles = [40.0f64, 55.0, 70.0, 90.0];
    let mut ratios = Vec::new();
    for &theta in &angles {
        // Transmit θ-pol gain at (θ, 0).
        let pt = FarFieldPoint {
            theta_deg: theta,
            phi_deg: 0.0,
        };
        let rp =
            compute_radiation_pattern(&segs, &tx.currents, FREQ_HZ, &[pt], &GroundModel::FreeSpace);
        let g_theta_lin = 10f64.powf(rp[0].gain_theta_dbi / 10.0);

        // Receive short-circuit current at the center segment (θ-pol → η=0).
        let rx = solve_plane_wave(theta, 0.0, 0.0);
        let i_center_sq = rx[25].norm_sqr();

        let ratio = i_center_sq / g_theta_lin;
        println!(
            "θ={theta:>4}  |I_center|²={i_center_sq:.4e}  G_θ={g_theta_lin:.4e}  ratio={ratio:.4e}"
        );
        ratios.push(ratio);
    }

    // Ratio should be constant across angles (reciprocity).
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let max_dev = ratios
        .iter()
        .map(|r| (r - mean).abs() / mean)
        .fold(0.0, f64::max);
    println!("reciprocity ratio spread = {max_dev:.4}");
    assert!(
        max_dev < 0.05,
        "receive current does not track transmit pattern (reciprocity spread {max_dev:.4} > 5%)"
    );
}

// ── Elliptic polarization (EX types 2/3) — PH8-CHK-002 breadth ──────────────

// nec2c tilted-wire elliptic reference: GW 1 41 -3 -3 0 3 3 0 0.001,
// EX 2 (right-elliptic) θ=45 φ=0 η=0 axial-ratio 0.5. Captured 2026-07-02.
const NEC2C_TILT_ELLIPTIC: [(f64, f64); 41] = [
    (-2.1163e-4, 5.8367e-4),
    (-5.8295e-4, 1.5849e-3),
    (-9.3076e-4, 2.4952e-3),
    (-1.2672e-3, 3.3508e-3),
    (-1.5940e-3, 4.1592e-3),
    (-1.9113e-3, 4.9224e-3),
    (-2.2183e-3, 5.6409e-3),
    (-2.5142e-3, 6.3140e-3),
    (-2.7979e-3, 6.9409e-3),
    (-3.0682e-3, 7.5205e-3),
    (-3.3238e-3, 8.0516e-3),
    (-3.5637e-3, 8.5329e-3),
    (-3.7867e-3, 8.9634e-3),
    (-3.9915e-3, 9.3422e-3),
    (-4.1771e-3, 9.6682e-3),
    (-4.3424e-3, 9.9408e-3),
    (-4.4866e-3, 1.0159e-2),
    (-4.6086e-3, 1.0324e-2),
    (-4.7075e-3, 1.0433e-2),
    (-4.7828e-3, 1.0488e-2),
    (-4.8335e-3, 1.0488e-2),
    (-4.8592e-3, 1.0434e-2),
    (-4.8594e-3, 1.0326e-2),
    (-4.8335e-3, 1.0165e-2),
    (-4.7813e-3, 9.9516e-3),
    (-4.7025e-3, 9.6870e-3),
    (-4.5969e-3, 9.3723e-3),
    (-4.4644e-3, 9.0088e-3),
    (-4.3050e-3, 8.5981e-3),
    (-4.1188e-3, 8.1417e-3),
    (-3.9059e-3, 7.6413e-3),
    (-3.6665e-3, 7.0988e-3),
    (-3.4008e-3, 6.5160e-3),
    (-3.1090e-3, 5.8948e-3),
    (-2.7914e-3, 5.2369e-3),
    (-2.4480e-3, 4.5441e-3),
    (-2.0787e-3, 3.8175e-3),
    (-1.6831e-3, 3.0577e-3),
    (-1.2596e-3, 2.2633e-3),
    (-8.0417e-4, 1.4290e-3),
    (-2.9764e-4, 5.2304e-4),
];

fn ex_card(ex_type: u32, theta: f64, phi: f64, eta: f64, ar: f64) -> Card {
    Card::Ex(ExCard {
        excitation_type: ex_type,
        tag: 1,
        segment: 1,
        i4: 0,
        voltage_real: theta,
        voltage_imag: phi,
        polarization_deg: eta,
        polarization_ratio: ar,
    })
}

fn solve_ex_on(geom: GwCard, ex: Card) -> Vec<Complex64> {
    let mut deck = NecDeck::new();
    deck.cards.push(Card::Gw(geom));
    deck.cards.push(ex);
    let segs = build_geometry(&deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, FREQ_HZ, &GroundModel::FreeSpace);
    let pw = build_planewave_hallen(&deck, &segs, FREQ_HZ).expect("planewave rhs");
    let endpoints = wire_endpoints_from_segs(&segs);
    solve_hallen_planewave(&z, &pw.rhs, &pw.cos_vec, &pw.sin_vec, &endpoints).expect("solve")
}

fn z_dipole() -> GwCard {
    GwCard {
        tag: 1,
        segments: NSEG,
        start: [0.0, 0.0, -HALF_LEN],
        end: [0.0, 0.0, HALF_LEN],
        radius: 0.001,
    }
}

fn tilted_wire() -> GwCard {
    GwCard {
        tag: 1,
        segments: 41,
        start: [-3.0, -3.0, 0.0],
        end: [3.0, 3.0, 0.0],
        radius: 0.001,
    }
}

#[test]
fn elliptic_reduces_to_linear_on_z_wire() {
    // For a z-oriented wire only the θ̂ component couples (φ̂·ẑ = 0), so an
    // elliptic wave (any axial ratio) induces the same current as linear.
    let lin = solve_ex_on(z_dipole(), ex_card(1, 30.0, 0.0, 0.0, 0.0));
    let ell = solve_ex_on(z_dipole(), ex_card(2, 30.0, 0.0, 0.0, 0.5));
    let peak = lin[NSEG as usize / 2].norm().max(1e-30);
    let max = (0..lin.len())
        .map(|i| (lin[i] - ell[i]).norm() / peak)
        .fold(0.0, f64::max);
    assert!(max < 1e-9, "elliptic != linear on z-wire (max {max:.2e})");
}

#[test]
fn elliptic_axial_ratio_zero_equals_linear() {
    // AR = 0 collapses the ellipse to a line: type 2 with AR=0 must equal type 1.
    let lin = solve_ex_on(tilted_wire(), ex_card(1, 45.0, 0.0, 0.0, 0.0));
    let ell0 = solve_ex_on(tilted_wire(), ex_card(2, 45.0, 0.0, 0.0, 0.0));
    let peak = lin[20].norm().max(1e-30);
    let max = (0..lin.len())
        .map(|i| (lin[i] - ell0[i]).norm() / peak)
        .fold(0.0, f64::max);
    assert!(max < 1e-9, "elliptic(AR=0) != linear (max {max:.2e})");
}

#[test]
fn elliptic_matches_nec2c_shape_on_tilted_wire() {
    // Tilted wire where both θ̂ and φ̂ couple, so the axial ratio matters. Compare
    // the induced-current distribution to nec2c (shape, aligning the constant
    // fnec-vs-nec2c operator factor on the peak segment).
    let ell = solve_ex_on(tilted_wire(), ex_card(2, 45.0, 0.0, 0.0, 0.5));
    assert_eq!(ell.len(), 41);
    let peak = 20usize;
    let ref_peak = Complex64::new(NEC2C_TILT_ELLIPTIC[peak].0, NEC2C_TILT_ELLIPTIC[peak].1);
    let align = ref_peak / ell[peak];
    let mut max_rel = 0.0f64;
    for (i, &(re, im)) in NEC2C_TILT_ELLIPTIC.iter().enumerate() {
        let refc = Complex64::new(re, im);
        let ours = ell[i] * align;
        max_rel = max_rel.max((ours - refc).norm() / ref_peak.norm());
    }
    println!("elliptic tilted-wire shape max rel = {max_rel:.4}");
    assert!(
        max_rel < 0.07,
        "elliptic currents deviate from nec2c by {max_rel:.4} (>7%)"
    );
}
