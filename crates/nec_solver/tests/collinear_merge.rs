// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-002 (collinear case): a straight conductor split across several GW
// cards must solve as one wire. The merge is a strict no-op for geometry without
// collinear splits.

use nec_model::card::{Card, ExCard, GwCard};
use nec_model::deck::NecDeck;
use nec_solver::*;
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn ex0(tag: u32, seg: u32) -> ExCard {
    ExCard {
        excitation_type: 0,
        tag,
        segment: seg,
        i4: 0,
        voltage_real: 1.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    }
}

fn solve_z(deck: &NecDeck, feed_tag: u32, feed_seg: u32) -> Complex64 {
    let segs = build_geometry(deck).unwrap();
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &GroundModel::FreeSpace);
    let h = build_hallen_rhs(deck, &segs, FREQ).unwrap();
    let merged = merge_collinear_wire_endpoints(&segs);
    let mut comp = vec![0usize; segs.len()];
    for (ci, &(f, l)) in merged.iter().enumerate() {
        for slot in comp.iter_mut().take(l + 1).skip(f) {
            *slot = ci;
        }
    }
    let jt: Vec<(usize, usize, f64)> = detect_wire_junctions(&segs, &merged, 1e-6)
        .iter()
        .filter(|j| comp[j.seg_a] != comp[j.seg_b])
        .map(|j| (j.seg_a, j.seg_b, j.sign))
        .collect();
    let sol = solve_hallen(&z, &h.rhs, &h.cos_vec, &merged, &jt).unwrap();
    let idx = segs
        .iter()
        .position(|s| s.tag == feed_tag && s.tag_index == feed_seg)
        .unwrap();
    Complex64::new(1.0, 0.0) / sol.currents[idx]
}

#[test]
fn collinear_chain_recovers_single_wire_impedance() {
    // A λ/2 dipole modeled as ONE 52-seg wire.
    let mut single = NecDeck::new();
    single.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 52,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    single.cards.push(Card::Ex(ex0(1, 27)));
    let z_single = solve_z(&single, 1, 27);

    // The same dipole split into two collinear end-to-start wires, fed at the join.
    let mut chain = NecDeck::new();
    chain.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 26,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 0.0],
        radius: 0.001,
    }));
    chain.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 26,
        start: [0.0, 0.0, 0.0],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    chain.cards.push(Card::Ex(ex0(2, 1)));
    let z_chain = solve_z(&chain, 2, 1);

    assert!(
        (z_chain - z_single).norm() < 0.5,
        "collinear chain {z_chain:.3} should match single wire {z_single:.3}"
    );
    assert!(
        z_chain.re > 0.0,
        "chain resistance must be positive (was negative before the fix): {z_chain:.3}"
    );
}

fn geom(cards: Vec<Card>) -> Vec<Segment> {
    let mut d = NecDeck::new();
    d.cards = cards;
    build_geometry(&d).unwrap()
}

#[test]
fn merge_is_noop_for_single_wire() {
    let segs = geom(vec![Card::Gw(GwCard {
        tag: 1,
        segments: 21,
        start: [0.0, 0.0, -5.0],
        end: [0.0, 0.0, 5.0],
        radius: 0.001,
    })]);
    assert_eq!(
        merge_collinear_wire_endpoints(&segs),
        wire_endpoints_from_segs(&segs)
    );
}

#[test]
fn merge_is_noop_for_bent_geometry() {
    // Two wires meeting end-to-start but NOT collinear (a right-angle bend).
    let segs = geom(vec![
        Card::Gw(GwCard {
            tag: 1,
            segments: 10,
            start: [0.0, 0.0, 0.0],
            end: [3.0, 0.0, 0.0],
            radius: 0.001,
        }),
        Card::Gw(GwCard {
            tag: 2,
            segments: 10,
            start: [3.0, 0.0, 0.0],
            end: [3.0, 0.0, 3.0],
            radius: 0.001,
        }),
    ]);
    // A bend must NOT be merged — it is a genuine junction.
    assert_eq!(
        merge_collinear_wire_endpoints(&segs),
        wire_endpoints_from_segs(&segs)
    );
}

#[test]
fn merge_is_noop_for_stepped_radius() {
    // Collinear but different radius = a genuine electrical junction, not merged.
    let segs = geom(vec![
        Card::Gw(GwCard {
            tag: 1,
            segments: 10,
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 2.6],
            radius: 0.001,
        }),
        Card::Gw(GwCard {
            tag: 2,
            segments: 10,
            start: [0.0, 0.0, 2.6],
            end: [0.0, 0.0, 5.282],
            radius: 0.005,
        }),
    ]);
    assert_eq!(
        merge_collinear_wire_endpoints(&segs),
        wire_endpoints_from_segs(&segs)
    );
}

#[test]
fn merge_joins_collinear_same_radius_chain() {
    let segs = geom(vec![
        Card::Gw(GwCard {
            tag: 1,
            segments: 10,
            start: [0.0, 0.0, -5.0],
            end: [0.0, 0.0, 0.0],
            radius: 0.001,
        }),
        Card::Gw(GwCard {
            tag: 2,
            segments: 10,
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 5.0],
            radius: 0.001,
        }),
    ]);
    // Two 10-seg wires → one merged block spanning all 20 segments.
    assert_eq!(merge_collinear_wire_endpoints(&segs), vec![(0, 19)]);
}

/// The **receive** twin of `collinear_chain_recovers_single_wire_impedance`.
///
/// A straight conductor split across two collinear `GW` cards, lit by an incident
/// plane wave, must carry the same induced current as the same conductor written
/// as one card. It did not: it was **refused outright** (FND-142). The delta-gap
/// builder merged collinear splits and the plane-wave builder did not, so
/// `detect_wire_junctions` saw the join as a junction and
/// `build_planewave_hallen` rejected the deck — while
/// `solve_hallen_planewave`, which consumes what that builder produces, was
/// already being handed the *merged* endpoint list by its caller.
///
/// The two decks are segmented **identically** (25 + 25 against 50 over the same
/// span, so every segment midpoint coincides). That is what makes this an
/// equality rather than a similarity: nothing differs but the card boundary and
/// the bookkeeping it drives, so any residual is the basis disagreeing with
/// itself.
#[test]
fn collinear_chain_recovers_single_wire_plane_wave_currents() {
    // Broadside incidence (θ = 90°, φ = 0, η = 0) puts ê along ẑ, parallel to the
    // dipole: maximum coupling. A near-axial wave would drive the conductor to
    // almost nothing and let this pass on two vectors of noise.
    let ex1 = ExCard {
        excitation_type: 1,
        tag: 0,
        segment: 0,
        i4: 0,
        voltage_real: 90.0, // θ_inc, degrees
        voltage_imag: 0.0,  // φ_inc, degrees
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    };

    let gw = |tag: u32, segments: u32, z0: f64, z1: f64| {
        Card::Gw(GwCard {
            tag,
            segments,
            start: [0.0, 0.0, z0],
            end: [0.0, 0.0, z1],
            radius: 0.001,
        })
    };

    let mut whole = NecDeck::new();
    whole.cards.push(gw(1, 50, -5.282, 5.282));
    whole.cards.push(Card::Ex(ex1.clone()));

    let mut split = NecDeck::new();
    split.cards.push(gw(1, 25, -5.282, 0.0));
    split.cards.push(gw(2, 25, 0.0, 5.282));
    split.cards.push(Card::Ex(ex1));

    let induced = |deck: &NecDeck| -> Vec<Complex64> {
        let segs = build_geometry(deck).unwrap();
        let z = assemble_z_matrix_with_ground(&segs, FREQ, &GroundModel::FreeSpace);
        solve_hallen_planewave_routed(deck, &segs, &z, FREQ)
            .expect("a collinear split is one conductor, not a junction")
    };

    let i_whole = induced(&whole);
    let i_split = induced(&split);
    assert_eq!(i_whole.len(), 50);
    assert_eq!(i_split.len(), 50);

    // The split really is a split: two `GW` cards that merge to one conductor.
    let split_segs = build_geometry(&split).unwrap();
    assert_eq!(wire_endpoints_from_segs(&split_segs).len(), 2);
    assert_eq!(merge_collinear_wire_endpoints(&split_segs), vec![(0, 49)]);

    // Floor: an all-zero pair would satisfy every ratio below. The measured peak
    // is ~1.23 mA; this only has to exclude nothing.
    let peak = i_whole.iter().map(|c| c.norm()).fold(0.0, f64::max);
    assert!(
        peak > 1e-4,
        "fixture induces no current, so the comparison is vacuous: peak = {peak:e}"
    );

    let worst = i_whole
        .iter()
        .zip(&i_split)
        .map(|(a, b)| (a - b).norm())
        .fold(0.0, f64::max);
    assert!(
        worst / peak < 1e-9,
        "split conductor disagrees with the same conductor written whole: \
         worst |Δ| = {worst:e} against peak |I| = {peak:e} (relative {:e})",
        worst / peak
    );
}

/// A genuine junction is still refused — the merge must not have widened the
/// gate it was narrowing. A bent (non-collinear) pair shares an endpoint, so the
/// merge is a no-op and the junction test still fires.
#[test]
fn a_bent_junction_is_still_refused_on_the_plane_wave_path() {
    let ex1 = ExCard {
        excitation_type: 1,
        tag: 0,
        segment: 0,
        i4: 0,
        voltage_real: 90.0,
        voltage_imag: 0.0,
        polarization_deg: 0.0,
        polarization_ratio: 0.0,
        theta_inc: 0.0,
        phi_inc: 0.0,
    };
    let mut bent = NecDeck::new();
    bent.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 21,
        start: [-5.0, 0.0, 0.0],
        end: [0.0, 0.0, 3.0],
        radius: 0.001,
    }));
    bent.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 21,
        start: [0.0, 0.0, 3.0],
        end: [5.0, 0.0, 0.0],
        radius: 0.001,
    }));
    bent.cards.push(Card::Ex(ex1));
    let segs = build_geometry(&bent).unwrap();
    assert_eq!(
        merge_collinear_wire_endpoints(&segs),
        wire_endpoints_from_segs(&segs),
        "a bend is not a collinear continuation, so the merge must be a no-op here"
    );
    let err = build_planewave_hallen(&bent, &segs, FREQ)
        .expect_err("a bent junction is not modelled by the per-conductor basis");
    assert_eq!(err, PlaneWaveError::JunctionedGeometryNotSupported);
}
