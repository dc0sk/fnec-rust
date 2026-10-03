// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-002 (receive-side junction solve): a *receiving* antenna whose arms
// meet at a degree-2 junction (a start-to-start split or a bent inverted-V) must
// solve on continuous conductor paths, so the induced current stays continuous
// across the junction. This mirrors the transmit-side general-junction fix.
//
// Two independent gates, neither needing an external reference:
//   1. Degeneracy — a λ/2 dipole modelled as two arms that both START at the
//      origin (start-to-start, one arm traversed in reverse) is the identical
//      antenna, meshed identically, to the single-wire dipole. Illuminated by the
//      same plane wave, the path receive solver must reproduce the already-
//      validated per-wire solver's peak induced current at every incidence angle.
//   2. Reciprocity — for a genuinely bent inverted-V (arms not collinear, so it is
//      NOT reducible to a straight wire) the short-circuit feed current induced by
//      a plane wave from θ tracks the transmit far-field at θ: |I_feed(θ)|²/G_θ(θ)
//      is constant across angles (Rayleigh–Carson). The transmit side uses the
//      validated conductor-path delta-gap solver + farfield path.

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::{
    assemble_z_matrix_with_ground, build_conductor_paths, build_geometry, build_planewave_hallen,
    compute_radiation_pattern, solve_hallen_planewave, solve_hallen_planewave_routed,
    solve_hallen_routed, wire_endpoints_from_segs, FarFieldPoint, GroundModel, Segment,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;
const HALF_LEN: f64 = 5.282; // λ/2 dipole at 14.2 MHz (matches corpus geometry)

fn plane_wave_card(theta_deg: f64, phi_deg: f64, eta_deg: f64) -> Card {
    Card::Ex(ExCard {
        excitation_type: 1, // linear plane wave
        tag: 1,
        segment: 1,
        i4: 0,
        voltage_real: theta_deg,
        voltage_imag: phi_deg,
        polarization_deg: eta_deg,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    })
}

/// Receive solve through the production seam, which routes a bent path through the
/// section layout and the corner term (FND-162). Calling the path solver directly
/// used to test a grouping no frontend uses.
fn receive_currents_paths(deck: &NecDeck, segs: &[Segment]) -> Vec<Complex64> {
    let z = assemble_z_matrix_with_ground(segs, FREQ, &GroundModel::FreeSpace);
    assert!(
        build_conductor_paths(segs).is_some(),
        "supported degree-2 topology"
    );
    solve_hallen_planewave_routed(
        deck,
        segs,
        &z,
        FREQ,
        &nec_solver::build_deck_stamps(deck, segs, FREQ).diagonal,
    )
    .expect("path receive solve")
}

/// Receive solve through the existing per-wire plane-wave solver (validated path).
fn receive_currents_per_wire(deck: &NecDeck, segs: &[Segment]) -> Vec<Complex64> {
    let z = assemble_z_matrix_with_ground(segs, FREQ, &GroundModel::FreeSpace);
    let pw =
        build_planewave_hallen(deck, segs, FREQ, &GroundModel::FreeSpace).expect("planewave rhs");
    let endpoints = wire_endpoints_from_segs(segs);
    solve_hallen_planewave(&z, &pw.rhs, &pw.cos_vec, &pw.sin_vec, &endpoints).expect("solve")
}

fn peak(currents: &[Complex64]) -> f64 {
    currents.iter().map(|c| c.norm()).fold(0.0f64, f64::max)
}

#[test]
fn start_to_start_split_receive_matches_single_wire() {
    // Reference: the single-wire λ/2 dipole (52 seg) through the validated per-wire
    // plane-wave solver. The split below has the identical 52-segment mesh, so the
    // induced currents must agree in absolute value, angle by angle.
    let single = {
        let mut d = NecDeck::new();
        d.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 52,
            start: [0.0, 0.0, -HALF_LEN],
            end: [0.0, 0.0, HALF_LEN],
            radius: 0.001,
        }));
        d
    };

    // Same dipole split at the centre into two arms that BOTH start at the origin
    // (start-to-start): arm 2 is therefore traversed in reverse — the case the
    // per-wire solver cannot model, but the path solver can.
    let split_geom = |extra: Card| -> NecDeck {
        let mut d = NecDeck::new();
        d.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 26,
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, HALF_LEN],
            radius: 0.001,
        }));
        d.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 26,
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, -HALF_LEN],
            radius: 0.001,
        }));
        d.cards.push(extra);
        d
    };

    // Sanity: the split really is a single non-trivial (reversed) conductor path.
    let split_segs = build_geometry(&split_geom(plane_wave_card(30.0, 0.0, 0.0))).unwrap();
    let paths = build_conductor_paths(&split_segs).unwrap();
    assert_eq!(paths.len(), 1, "split dipole must be one conductor path");
    assert!(
        !paths[0].is_trivial(&split_segs),
        "split arm is traversed in reverse"
    );

    let mut max_rel = 0.0f64;
    for &theta in &[35.0f64, 55.0, 75.0, 90.0] {
        let mut single_d = single.clone();
        single_d.cards.push(plane_wave_card(theta, 0.0, 0.0));
        let ref_segs = build_geometry(&single_d).unwrap();
        let ref_peak = peak(&receive_currents_per_wire(&single_d, &ref_segs));

        let split_d = split_geom(plane_wave_card(theta, 0.0, 0.0));
        let segs = build_geometry(&split_d).unwrap();
        let got_peak = peak(&receive_currents_paths(&split_d, &segs));

        let rel = (got_peak - ref_peak).abs() / ref_peak;
        println!(
            "θ={theta:>4}  single_peak={ref_peak:.6e}  split_peak={got_peak:.6e}  rel={rel:.2e}"
        );
        max_rel = max_rel.max(rel);
    }
    // The two models are the identical antenna on the identical mesh, so agreement
    // is to machine precision (measured ~1e-11); gate well below any float-noise
    // floor so this asserts exact reproduction, not merely a close shape.
    assert!(
        max_rel < 1e-8,
        "path receive solver must reproduce the per-wire solver on the identical mesh \
         (max rel deviation {max_rel:.2e} > 1e-8)"
    );
}

#[test]
fn bent_inverted_v_receive_reciprocity() {
    // A near-resonant inverted-V fed at the apex; arms in the x–z plane so the
    // φ=0-plane far field is θ-polarised and an η=0 (θ̂-polarised) plane wave
    // couples to it. Both arms START at the apex (start-to-start), so this is a
    // genuine degree-2 junction that is NOT collinear.
    let vee = |extra: Card| -> NecDeck {
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
        d.cards.push(extra);
        d
    };

    // Transmit: drive at the apex (wire 1, seg 1) via the validated conductor-path
    // delta-gap solver, then take the θ-pol far-field gain at each (θ, 0).
    let driven = vee(Card::Ex(ExCard {
        excitation_type: 0,
        tag: 1,
        segment: 1,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }));
    let segs = build_geometry(&driven).unwrap();
    let feed_idx = segs
        .iter()
        .position(|s| s.tag == 1 && s.tag_index == 1)
        .unwrap();
    // Through the production seam, like the receive side: both then model the bend
    // (FND-162). With the old direct calls NEITHER side did, and this gate passed at
    // 5 % while the receive currents were 55–120 % off nec2c at a bend.
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &GroundModel::FreeSpace);
    let tx = solve_hallen_routed(&driven, &segs, &mut z, FREQ, &[]).unwrap();
    assert!(
        tx.route.paths,
        "a bent apex-fed V takes the conductor-path basis"
    );

    // Receive: illuminate from each θ (η=0, θ̂-polarised) and take the short-circuit
    // feed-segment current. |I_feed|²/G_θ must be constant across angles.
    let angles = [40.0f64, 55.0, 70.0, 85.0];
    let mut ratios = Vec::new();
    for &theta in &angles {
        let pt = FarFieldPoint {
            theta_deg: theta,
            phi_deg: 0.0,
        };
        let rp =
            compute_radiation_pattern(&segs, &tx.currents, FREQ, &[pt], &GroundModel::FreeSpace);
        let g_theta_lin = 10f64.powf(rp[0].gain_theta_dbi / 10.0);

        let rx_deck = vee(plane_wave_card(theta, 0.0, 0.0));
        let rx_segs = build_geometry(&rx_deck).unwrap();
        let rx = receive_currents_paths(&rx_deck, &rx_segs);
        let i_feed_sq = rx[feed_idx].norm_sqr();

        let ratio = i_feed_sq / g_theta_lin;
        println!(
            "θ={theta:>4}  |I_feed|²={i_feed_sq:.4e}  G_θ={g_theta_lin:.4e}  ratio={ratio:.4e}"
        );
        ratios.push(ratio);
    }

    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let max_dev = ratios
        .iter()
        .map(|r| (r - mean).abs() / mean)
        .fold(0.0, f64::max);
    println!("inverted-V receive reciprocity spread = {max_dev:.4}");
    assert!(
        max_dev < 0.05,
        "bent-antenna receive current does not track transmit pattern \
         (reciprocity spread {max_dev:.4} > 5%)"
    );

    // The spread alone is not a bend gate: with the receive solve put back on one
    // homogeneous term per path it was still 0.18 % (0.15 % bend-aware), because a
    // wrong receive current can be wrong by the same factor at every angle. The
    // LEVEL is fixed absolutely by reciprocity: available power |V_oc|²/(8R) equals
    // the incident density E₀²/(2η₀) times the aperture λ²G/(4π), so with
    // I_sc = V_oc/Z, |I_sc|²/G = R·λ²·E₀²/(π·η₀·|Z|²), from the transmit solve.
    // Measured: 6.535e-3 predicted, 6.52–6.54e-3 bend-aware, 6.00e-3 per-path (FND-162).
    let z_in = Complex64::new(1.0, 0.0) / tx.currents[feed_idx];
    let lambda = 299_792_458.0 / FREQ;
    let eta0 = 376.730_313_668;
    let predicted = z_in.re * lambda * lambda / (std::f64::consts::PI * eta0 * z_in.norm_sqr());
    let level = (mean - predicted).abs() / predicted;
    println!("reciprocity level: mean {mean:.4e}, predicted {predicted:.4e}, off {level:.4}");
    assert!(
        level < 0.01,
        "receive current level {mean:.4e} vs {predicted:.4e} from the transmit Z ({:.2} % off)",
        level * 100.0
    );
}
