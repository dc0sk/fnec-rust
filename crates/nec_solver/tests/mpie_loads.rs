// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH10-CHK-004 — the MPIE stamps `LD` loads, through the session every frontend
//! uses.
//!
//! It refused them. A load on a segment is its centre current times `z`, NEC's
//! own model: on the MPIE's triangle basis that current is a row `t` over the
//! bases touching the segment, so the load adds `z·t·tᵀ` — the transpose of the
//! readout, as the `EX` feed is. Held two ways: a load on the fed segment raises
//! the input impedance by exactly `z` (the feed is `V·t`, so the solve answers
//! `1/(tᵀZ⁻¹t) + z`), and four loaded decks converge to nec2c 1.3.1, captured
//! 2026-10-15 with its echo of 14.2 MHz and the segment count.

use nec_solver::{
    build_deck_geometry, ground_model_from_deck, solve_mpie_session, LaplaceLoad, MpieSessionError,
};
use num_complex::Complex64;

const F: f64 = 14.2e6;

fn session(text: &str, laplace: &[LaplaceLoad]) -> Result<Vec<Complex64>, MpieSessionError> {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_deck_geometry(&deck).expect("geometry");
    solve_mpie_session(&deck, &segs, &ground_model_from_deck(&deck), F, laplace)
}

/// The input impedance at `feed` (tag, segment) of a deck fed with 1 V there.
fn z_in(text: &str, feed: (u32, u32)) -> Complex64 {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_deck_geometry(&deck).expect("geometry");
    let currents = solve_mpie_session(&deck, &segs, &ground_model_from_deck(&deck), F, &[])
        .unwrap_or_else(|e| panic!("{text}\nwas refused: {e}"));
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed segment");
    Complex64::new(1.0, 0.0) / currents[idx]
}

/// A load on the fed segment adds exactly its own impedance, over free space and
/// both grounds, and on a start-to-start inverted V — two wires written outward
/// from the fed apex, where the basis's reference direction opposes the
/// segment's on one leg. A dipole cannot tell a dropped sign: every leg of a
/// chain has the same one. Over ground the image folds into the same matrix with
/// no image segments, so a load stamped there twice would read `2z`.
#[test]
fn a_load_on_the_fed_segment_adds_exactly_its_impedance() {
    let dipole = |ground: &str| {
        format!(
            "GW 1 21 -5.2 0 6 5.2 0 6 0.001\nGE 0\n{ground}{{LD}}EX 0 1 11 0 1 0\n\
             FR 0 1 0 0 14.2 0\nEN\n"
        )
    };
    let vee = "GW 1 10 0 0 8 3.6 0 5 0.001\nGW 2 10 0 0 8 -3.6 0 5 0.001\nGE 0\n\
               {LD}EX 0 1 1 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n"
        .to_string();
    let cases = [
        ("free-space dipole", dipole(""), (1, 11)),
        ("dipole over GN 1", dipole("GN 1\n"), (1, 11)),
        ("dipole over GN 2", dipole("GN 2 0 0 0 13 0.005\n"), (1, 11)),
        ("apex-fed inverted V", vee, (1, 1)),
    ];
    for (what, deck, (tag, seg)) in cases {
        let bare = z_in(&deck.replace("{LD}", ""), (tag, seg));
        let load = format!("LD 4 {tag} {seg} {seg} 20 50 0\n");
        let loaded = z_in(&deck.replace("{LD}", &load), (tag, seg));
        let added = loaded - bare;
        assert!(
            (added - Complex64::new(20.0, 50.0)).norm() < 1e-8 * loaded.norm(),
            "{what}: a 20 + j50 Ω load on the fed segment added {added:.9} (bare {bare:.4})"
        );
    }
}

/// A 10.4 m dipole at 14.2 MHz, 1 mm, `n` segments, fed at the centre.
fn dipole(n: u32, loads: &str) -> String {
    format!(
        "GW 1 {n} 0 0 -5.2 0 0 5.2 0.001\nGE 0\n{loads}EX 0 1 {} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
        n / 2 + 1
    )
}

/// Segment `i` of the 21-segment mesh, at the same place on the mesh `3^k` finer.
fn at(i: u32, k: u32) -> u32 {
    (0..k).fold(i, |i, _| 3 * i - 1)
}

/// Refining approaches nec2c's finest value, each step nearer, and the finest
/// mesh lands within `rel` of it.
fn converges(what: &str, zs: &[Complex64], nec2c: (f64, f64), rel: f64) {
    let r = Complex64::new(nec2c.0, nec2c.1);
    let e: Vec<f64> = zs.iter().map(|z| (z - r).norm()).collect();
    assert!(
        e.windows(2).all(|w| w[1] < w[0]) && e[e.len() - 1] < rel * r.norm(),
        "{what}: MPIE {zs:.3?} vs nec2c {r:.3}"
    );
}

/// An off-centre series load (`LD 0`, 20 Ω + 1 µH at 0.74 of the length): nec2c
/// 94.647 + j77.084, 95.479 + j77.841, 95.860 + j78.218 at 21/63/189 segments;
/// the MPIE 93.73 + j75.81, 95.09 + j77.46, 95.68 + j77.98.
#[test]
fn an_off_centre_series_load_converges_to_nec2c() {
    let zs: Vec<Complex64> = (0..3)
        .map(|k| {
            let s = at(16, k);
            z_in(
                &dipole(21 * 3u32.pow(k), &format!("LD 0 1 {s} {s} 20 1e-6 0\n")),
                (1, 21 * 3u32.pow(k) / 2 + 1),
            )
        })
        .collect();
    converges("series load", &zs, (95.860, 78.218), 0.005);
}

/// Two parallel-LC traps (`LD 1`, 1 µH ∥ 57 pF, resonant near 21 MHz), at 0.74
/// and 0.26 of the length: nec2c 97.651 + j251.30, 100.02 + j256.37, 101.09 +
/// j258.65; the MPIE 95.07 + j245.23, 98.93 + j254.03, 100.60 + j257.55. The
/// reactance is large, so the finest gap is the largest here: 1.2 Ω.
#[test]
fn a_trap_dipole_converges_to_nec2c() {
    let zs: Vec<Complex64> = (0..3)
        .map(|k| {
            let (a, b) = (at(16, k), at(6, k));
            let loads = format!("LD 1 1 {a} {a} 0 1e-6 57e-12\nLD 1 1 {b} {b} 0 1e-6 57e-12\n");
            z_in(
                &dipole(21 * 3u32.pow(k), &loads),
                (1, 21 * 3u32.pow(k) / 2 + 1),
            )
        })
        .collect();
    converges("trap dipole", &zs, (101.09, 258.65), 0.006);
}

/// Copper (`LD 5`, σ = 5.8e7 S/m on every segment), a distributed load stamped as
/// the segment's centre current times its loss, as NEC does: nec2c 76.257 +
/// j22.954, 76.467 + j23.345, 76.576 + j23.562; the MPIE 76.05 + j22.11, 76.37 +
/// j23.13, 76.53 + j23.41.
#[test]
fn a_copper_dipole_converges_to_nec2c() {
    let zs: Vec<Complex64> = (0..3)
        .map(|k| {
            z_in(
                &dipole(21 * 3u32.pow(k), "LD 5 0 0 0 5.8e7\n"),
                (1, 21 * 3u32.pow(k) / 2 + 1),
            )
        })
        .collect();
    converges("copper", &zs, (76.576, 23.562), 0.005);
}

/// A Y of three 3.5 m arms with a 50 Ω load on arm 2 and the feed on arm 1, both
/// at 0.46 of the arm — the topology the MPIE exists for: nec2c 26.929 − j579.25,
/// 26.245 − j570.72, 25.945 − j566.92 at 14/42/126 segments an arm; the MPIE
/// 27.73 − j589.00, 26.55 − j574.58, 26.07 − j568.71.
#[test]
fn a_loaded_arm_of_a_y_converges_to_nec2c() {
    let zs: Vec<Complex64> = (0..3)
        .map(|k| {
            let (m, s) = (14 * 3u32.pow(k), at(7, k));
            z_in(
                &format!(
                    "GW 1 {m} 0 0 0 0 0 3.5 0.001\nGW 2 {m} 0 0 0 3.031 0 -1.75 0.001\n\
                     GW 3 {m} 0 0 0 -3.031 0 -1.75 0.001\nGE 0\nLD 0 2 {s} {s} 50 0 0\n\
                     EX 0 1 {s} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n"
                ),
                (1, s),
            )
        })
        .collect();
    converges("loaded Y", &zs, (25.945, -566.92), 0.005);
}

/// The case the MPIE is for: the series-loaded dipole 1.06 m (0.05 λ) over `GN 2`
/// (ε 13, σ 0.005), where the surface wave is in the currents. nec2c (its
/// Sommerfeld solution, echoed) 83.129 + j86.067, 83.935 + j86.980, 84.295 +
/// j87.414 at 21/63/189 segments; the MPIE 82.28 + j84.74, 83.54 + j86.55,
/// 84.10 + j87.15. Hallén reads 51.08 + j81.59 at 189 and warns about the
/// ground. The unloaded dipole there is gated by the parity sweep's
/// `R-lowground`.
#[test]
fn a_loaded_dipole_low_over_finite_ground_converges_to_nec2c() {
    let zs: Vec<Complex64> = (0..3)
        .map(|k| {
            let (n, s) = (21 * 3u32.pow(k), at(16, k));
            z_in(
                &format!(
                    "GW 1 {n} -5.2 0 1.06 5.2 0 1.06 0.001\nGE 0\nGN 2 0 0 0 13 0.005\n\
                     LD 0 1 {s} {s} 20 1e-6 0\nEX 0 1 {} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
                    n / 2 + 1
                ),
                (1, n / 2 + 1),
            )
        })
        .collect();
    converges("loaded low dipole over GN 2", &zs, (84.295, 87.414), 0.005);
}

/// A load that names no segment is refused by the session itself, not only by
/// the pre-solve check a caller may skip: `build_loads` gives it zero load, which
/// would solve the antenna unloaded. The same for a `--loads-config` load; and the
/// cards the MPIE has no solve for stay refused by name.
#[test]
fn loads_the_session_cannot_apply_are_refused_by_name() {
    let refused = |text: &str, laplace: &[LaplaceLoad]| match session(text, laplace) {
        Err(MpieSessionError::Load(e)) => e,
        other => panic!("{text}: expected a load refusal, got {other:?}"),
    };
    let e = refused(&dipole(21, "LD 0 7 1 1 50 0 0\n"), &[]);
    assert!(e.contains("names no segment"), "{e}");
    let stray = LaplaceLoad {
        tag: 7,
        seg_first: 1,
        seg_last: 0,
        numerator: vec![50.0],
        denominator: vec![1.0],
    };
    let e = refused(&dipole(21, ""), &[stray]);
    assert!(
        e.contains("laplace_load[0]") && e.contains("names no segment"),
        "{e}"
    );
    let empty = LaplaceLoad {
        tag: 1,
        seg_first: 11,
        seg_last: 0,
        numerator: vec![],
        denominator: vec![1.0],
    };
    let e = refused(&dipole(21, ""), &[empty]);
    assert!(
        e.contains("--loads-config") && e.contains("empty numerator"),
        "{e}"
    );
    assert!(session(&dipole(21, "LD 0 1 5 5 50 0 0\n"), &[]).is_ok());
    assert!(matches!(
        session(&dipole(21, "TL 1 5 1 17 50 0 0 0 0 0\n"), &[]),
        Err(MpieSessionError::Unsupported(_))
    ));
}
