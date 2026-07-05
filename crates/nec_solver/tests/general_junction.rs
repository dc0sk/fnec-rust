// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-002 (general junction case): a single physical conductor whose two arms
// meet at a degree-2 junction — start-to-start splits and bent inverted-V feeds —
// must solve to a physical impedance via the conductor-path Hallén solver.
//
// The unimpeachable gate is the split dipole: a λ/2 dipole cut at the feed into
// two wires that both *start* at the join is the identical antenna to the
// single-wire dipole, so it must recover the single-wire impedance exactly.
// (nec2c confirms the two models are the same antenna: 79.4 + j46.3 Ω for both.)

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

/// Solve feedpoint impedance through the general conductor-path Hallén solver.
fn solve_z_paths(deck: &NecDeck, feed_tag: u32, feed_seg: u32) -> Complex64 {
    let segs = build_geometry(deck).unwrap();
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &GroundModel::FreeSpace);
    let paths = build_conductor_paths(&segs).expect("supported degree-2 topology");
    let h = build_hallen_rhs_paths(deck, &segs, FREQ, &paths).unwrap();

    let mut path_of = vec![0usize; segs.len()];
    let mut free_ends = Vec::new();
    for (pi, p) in paths.iter().enumerate() {
        for &m in &p.segs {
            path_of[m] = pi;
        }
        free_ends.push(p.free_ends.0);
        free_ends.push(p.free_ends.1);
    }
    let sol = solve_hallen_paths(&z, &h.rhs, &h.cos_vec, &path_of, &free_ends).unwrap();
    let idx = segs
        .iter()
        .position(|s| s.tag == feed_tag && s.tag_index == feed_seg)
        .unwrap();
    Complex64::new(1.0, 0.0) / sol.currents[idx]
}

fn dipole_single() -> NecDeck {
    // 52 segments so a segment BOUNDARY sits at the feed, matching the 26+26 split
    // mesh (fed at seg 27, the boundary segment).
    let mut d = NecDeck::new();
    d.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 52,
        start: [0.0, 0.0, -5.282],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    d.cards.push(Card::Ex(ex0(1, 27)));
    d
}

#[test]
fn start_to_start_split_recovers_single_wire() {
    // Reference: the single-wire λ/2 dipole through the same path solver, same mesh.
    let z_single = solve_z_paths(&dipole_single(), 1, 27);

    // The same dipole split at the feed into two wires that BOTH start at the
    // origin (start-to-start). One arm is therefore traversed in reverse — the
    // exact case the collinear merge cannot handle. Fed at wire 1 segment 1.
    let mut split = NecDeck::new();
    split.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 26,
        start: [0.0, 0.0, 0.0],
        end: [0.0, 0.0, 5.282],
        radius: 0.001,
    }));
    split.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 26,
        start: [0.0, 0.0, 0.0],
        end: [0.0, 0.0, -5.282],
        radius: 0.001,
    }));
    split.cards.push(Card::Ex(ex0(1, 1)));
    let z_split = solve_z_paths(&split, 1, 1);

    assert!(
        z_split.re > 0.0,
        "split resistance must be positive (was -34.5 Ω before the fix): {z_split:.3}"
    );
    assert!(
        (z_split - z_single).norm() < 0.5,
        "start-to-start split {z_split:.3} must recover single wire {z_single:.3}"
    );
}

#[test]
fn bent_inverted_v_matches_nec2c_resistance() {
    // A near-resonant 30° inverted-V fed at the apex (both arms start at the top,
    // ~5.15 m each). nec2c: 57.7 - j4.3 Ω. Radiation resistance is the direction-
    // independent physical quantity, so we gate resistance tightly against nec2c
    // (within ~15%); fnec's Hallén carries a known systematic reactance offset vs
    // nec2c (see the fnec-validation-strategy note), so reactance is gated only for
    // sanity (bounded, not the -17 - j1283 Ω garbage produced before the fix).
    let mut vee = NecDeck::new();
    vee.cards.push(Card::Gw(GwCard {
        tag: 1,
        segments: 26,
        start: [0.0, 0.0, 5.0],
        end: [4.46, 0.0, 2.42],
        radius: 0.001,
    }));
    vee.cards.push(Card::Gw(GwCard {
        tag: 2,
        segments: 26,
        start: [0.0, 0.0, 5.0],
        end: [-4.46, 0.0, 2.42],
        radius: 0.001,
    }));
    vee.cards.push(Card::Ex(ex0(1, 1)));
    let z = solve_z_paths(&vee, 1, 1);

    // nec2c radiation resistance is 57.7 Ω; fnec gives 55.5 Ω (~4%).
    assert!(
        (z.re - 57.7).abs() < 9.0,
        "inverted-V resistance must match nec2c 57.7 Ω within ~15%; got {z:.3}"
    );
    assert!(
        z.im.abs() < 60.0,
        "inverted-V near resonance must have bounded reactance; got {z:.3}"
    );
}

fn geom(cards: Vec<Card>) -> Vec<Segment> {
    let mut d = NecDeck::new();
    d.cards = cards;
    build_geometry(&d).unwrap()
}

#[test]
fn single_wire_is_one_trivial_path() {
    let segs = geom(vec![Card::Gw(GwCard {
        tag: 1,
        segments: 21,
        start: [0.0, 0.0, -5.0],
        end: [0.0, 0.0, 5.0],
        radius: 0.001,
    })]);
    let paths = build_conductor_paths(&segs).unwrap();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].is_trivial());
    assert_eq!(paths[0].free_ends, (0, 20));
}

#[test]
fn collinear_end_to_start_is_one_trivial_path() {
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
    let paths = build_conductor_paths(&segs).unwrap();
    assert_eq!(paths.len(), 1);
    assert!(
        paths[0].is_trivial(),
        "collinear end-to-start reduces to a single wire"
    );
}

#[test]
fn start_to_start_is_one_nontrivial_path() {
    // Both wires start at the origin → one arm traversed in reverse (sign flip).
    let segs = geom(vec![
        Card::Gw(GwCard {
            tag: 1,
            segments: 10,
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 5.0],
            radius: 0.001,
        }),
        Card::Gw(GwCard {
            tag: 2,
            segments: 10,
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, -5.0],
            radius: 0.001,
        }),
    ]);
    let paths = build_conductor_paths(&segs).unwrap();
    assert_eq!(paths.len(), 1);
    assert!(
        !paths[0].is_trivial(),
        "start-to-start requires a sign flip → non-trivial"
    );
    // Exactly one of the two arms is reversed.
    let flipped = paths[0].signs.iter().filter(|&&s| s < 0.0).count();
    assert_eq!(flipped, 10, "one 10-segment arm is traversed in reverse");
    // Free ends are the two physical tips (the +z tip and the -z tip).
    let (a, b) = paths[0].free_ends;
    assert!(
        (a == 9 && b == 19) || (a == 19 && b == 9),
        "free ends must be the two tips, got ({a}, {b})"
    );
}

#[test]
fn t_junction_is_unsupported() {
    // Three wires meeting at the origin (degree-3) — out of scope for this slice.
    let segs = geom(vec![
        Card::Gw(GwCard {
            tag: 1,
            segments: 5,
            start: [0.0, 0.0, 0.0],
            end: [5.0, 0.0, 0.0],
            radius: 0.001,
        }),
        Card::Gw(GwCard {
            tag: 2,
            segments: 5,
            start: [0.0, 0.0, 0.0],
            end: [-5.0, 0.0, 0.0],
            radius: 0.001,
        }),
        Card::Gw(GwCard {
            tag: 3,
            segments: 5,
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 5.0],
            radius: 0.001,
        }),
    ]);
    assert!(
        build_conductor_paths(&segs).is_none(),
        "degree-3 T/Y junction must fall back (return None)"
    );
}
