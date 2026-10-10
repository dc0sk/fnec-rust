// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-170 — an incident plane wave over perfect ground.
//!
//! The receive forcing now carries the ground-reflected wave (nec2c's `etmns`):
//! evaluated at each segment's mirror point, with the mirrored polarization scaled
//! by `rrv` / `rrh` (both −1 on perfect ground). The matrix already carried the
//! images. Before, the straight dipole below was 55 % off nec2c at the peak.
//!
//! Every expectation is nec2c 1.3.1, captured 2026-10-02 at 14.2 MHz over `GN 1`
//! (its echo checked: `FREQUENCY : 1.4200E+01 MHz`, `PERFECT GROUND`). Whole
//! current table, max |ΔI| / peak, 21 → 41 segments per wire, with each deck's
//! own free-space error beside it — the receive forcing of a straight or bent wire
//! converges slowly in free space too, and the ground adds nothing to that:
//!
//! | deck | free space | over perfect ground |
//! |---|---|---|
//! | horizontal dipole, θ = 45° | 10.24 → 5.67 % | 10.91 → 5.91 % |
//! | vertical dipole, θ = 60° | 10.36 → 5.71 % | 9.77 → 5.38 % |
//! | inverted-V, θ = 45° | 9.41 → 5.41 % | 7.95 → 4.61 % |
//! | horizontal dipole, elliptic | 10.30 → 5.69 % | 10.91 → 5.91 % |
//! | T (junction route), θ = 45° | — | 1.05 → 0.56 % |
//!
//! At 42 → 84 the dipole is 5.55 → 3.22 % in free space and 5.79 → 3.32 % over
//! ground. The gates below pin a handful of segments per deck and require the
//! error to shrink with the mesh.

use nec_solver::validate::pre_solve_error;
use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck,
    solve_hallen_planewave_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;

fn deck(geometry: &str, ground: &str, wave: &str) -> nec_model::deck::NecDeck {
    nec_parser::parse(&format!(
        "CE\n{geometry}{ground}{wave}FR 0 1 0 0 14.2 0\nEN\n"
    ))
    .expect("parses")
    .deck
}

/// The receive currents the way a frontend gets them: refusals first, then the
/// matrix over the deck's own ground, then the receive seam.
fn receive(geometry: &str, ground: &str, wave: &str) -> Result<Vec<Complex64>, String> {
    let d = deck(geometry, ground, wave);
    let segs = build_geometry(&d).expect("geometry");
    let g = ground_model_from_deck(&d);
    if let Some(why) = pre_solve_error(&d, &segs, &g) {
        return Err(why);
    }
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &g);
    solve_hallen_planewave_routed(
        &d,
        &segs,
        &z,
        FREQ,
        &nec_solver::build_deck_stamps(&d, &segs, FREQ).diagonal,
    )
    // nec2c numbers the deck's segments (FND-227).
    .map(|currents| nec_solver::deck_values(&segs, &currents))
    .map_err(|e| e.to_string())
}

const PEC: &str = "GE 1\nGN 1\n";

/// Max over the pinned segments (1-based) of |ΔI| / peak |I_nec2c|.
fn err(got: &[Complex64], peak: f64, refs: &[(usize, f64, f64)]) -> f64 {
    refs.iter()
        .map(|&(seg, re, im)| (got[seg - 1] - Complex64::new(re, im)).norm() / peak)
        .fold(0.0, f64::max)
}

fn assert_converges(label: &str, coarse: f64, fine: f64, bound: f64) {
    println!("{label}: {:.2} % -> {:.2} %", coarse * 100.0, fine * 100.0);
    assert!(
        fine < bound,
        "{label}: {:.2} % at the finer mesh",
        fine * 100.0
    );
    assert!(
        fine < coarse,
        "{label}: must shrink, {coarse:.4} -> {fine:.4}"
    );
}

fn dipole(n: u32) -> String {
    format!("GW 1 {n} -5 0 5 5 0 5 .001\n")
}

/// The FND-170 deck: a 10 m horizontal dipole 5 m over perfect ground, lit at
/// θ = 45° in its own plane. It was 55 % off; its free-space error at these
/// meshes is 10.2 → 5.7 %. Kill criterion: under 7 % at 41, shrinking.
#[test]
fn a_horizontal_dipole_over_perfect_ground_receives_like_nec2c() {
    let wave = "EX 1 1 1 0 45 0 0\n";
    let e21 = err(
        &receive(&dipole(21), PEC, wave).unwrap(),
        9.326_519e-2,
        &[
            (1, -2.5663e-04, 8.8169e-03),
            (6, -2.5745e-03, 7.1509e-02),
            (11, -3.6177e-03, 9.3195e-02),
            (16, -2.5847e-03, 6.8179e-02),
            (21, -2.5922e-04, 7.9846e-03),
        ],
    );
    let e41 = err(
        &receive(&dipole(41), PEC, wave).unwrap(),
        9.325_960e-2,
        &[
            (1, -1.1172e-04, 4.8483e-03),
            (11, -2.1247e-03, 7.0439e-02),
            (21, -3.0718e-03, 9.3209e-02),
            (31, -2.1350e-03, 6.7077e-02),
            (41, -1.1319e-04, 4.3782e-03),
        ],
    );
    assert_converges("horizontal dipole over GN 1", e21, e41, 0.07);
}

/// A vertical dipole (3 m to 13 m) lit at θ = 60°: the field along the wire is
/// the normal component, which the ground image keeps rather than reverses — so
/// this is the deck that fails if the reflected field vector is not mirrored.
#[test]
fn a_vertical_dipole_over_perfect_ground_receives_like_nec2c() {
    let wire = |n: u32| format!("GW 1 {n} 0 0 3 0 0 13 .001\n");
    let wave = "EX 1 1 1 0 60 0 0\n";
    let e21 = err(
        &receive(&wire(21), PEC, wave).unwrap(),
        4.854_655e-2,
        &[
            (1, -3.7970e-03, -2.5742e-03),
            (6, -3.1434e-02, -1.9846e-02),
            (11, -4.1871e-02, -2.4568e-02),
            (16, -3.1373e-02, -1.6935e-02),
            (21, -3.7767e-03, -1.8324e-03),
        ],
    );
    let e41 = err(
        &receive(&wire(41), PEC, wave).unwrap(),
        4.866_477e-2,
        &[
            (1, -2.0962e-03, -1.4122e-03),
            (11, -3.1103e-02, -1.9468e-02),
            (21, -4.2089e-02, -2.4429e-02),
            (31, -3.1043e-02, -1.6529e-02),
            (41, -2.0847e-03, -9.9276e-04),
        ],
    );
    assert_converges("vertical dipole over GN 1", e21, e41, 0.07);
}

/// An inverted-V, apex 10 m: the bent-path route, whose corner term now takes
/// the ground images too.
#[test]
fn an_inverted_v_over_perfect_ground_receives_like_nec2c() {
    let vee = |n: u32| {
        format!("GW 1 {n} 0 0 10 -3.7477 0 6.2523 .001\nGW 2 {n} 0 0 10 3.7477 0 6.2523 .001\n")
    };
    let wave = "EX 1 1 1 0 45 0 0\n";
    let e21 = err(
        &receive(&vee(21), PEC, wave).unwrap(),
        1.106_177e-1,
        &[
            (1, -2.1642e-02, -1.0848e-01),
            (11, -1.5829e-02, -8.0164e-02),
            (21, -1.0583e-03, -5.3670e-03),
            (22, 2.1637e-02, 1.0819e-01),
            (32, 1.5776e-02, 7.6928e-02),
            (42, 1.0526e-03, 5.0114e-03),
        ],
    );
    let e41 = err(
        &receive(&vee(41), PEC, wave).unwrap(),
        1.105_096e-1,
        &[
            (1, -2.1988e-02, -1.0830e-01),
            (21, -1.6077e-02, -8.0062e-02),
            (41, -5.9637e-04, -2.9746e-03),
            (42, 2.1985e-02, 1.0816e-01),
            (62, 1.6023e-02, 7.6825e-02),
            (82, 5.9314e-04, 2.7754e-03),
        ],
    );
    assert_converges("inverted-V over GN 1", e21, e41, 0.07);
}

/// A T (4 m stem from 2 m up, 3 m bar halves): the section-graph route.
/// Kill criterion: under 1 % at 41, shrinking. Against nec2c on a 27× mesh
/// (segment j → 27j − 13, the same point), captured 2026-10-10: since FND-227
/// refined the free ends fnec outruns nec2c on the deck's own mesh, and the
/// error against that stopped shrinking (0.32 → 0.41 %); against 27N it is
/// 0.49 → 0.32 %.
#[test]
fn a_t_over_perfect_ground_receives_like_nec2c() {
    let tee = |n: u32| {
        format!(
            "GW 1 {n} 0 0 2 0 0 6 .001\nGW 2 {n} 0 0 6 -3 0 6 .001\nGW 3 {n} 0 0 6 3 0 6 .001\n"
        )
    };
    let wave = "EX 1 1 1 0 45 0 0\n";
    let e21 = err(
        &receive(&tee(21), PEC, wave).unwrap(),
        7.209_240e-3,
        &[
            (1, -2.4346e-05, -4.7334e-04),
            (11, -2.9385e-04, -5.5604e-03),
            (21, -3.4364e-04, -6.5913e-03),
            (22, 6.2104e-03, -3.5316e-03),
            (43, -6.5473e-03, -2.9800e-03),
            (63, -3.8098e-04, -1.6713e-04),
        ],
    );
    let e41 = err(
        &receive(&tee(41), PEC, wave).unwrap(),
        7.211_120e-3,
        &[
            (1, -1.3809e-05, -2.6841e-04),
            (21, -2.9436e-04, -5.5642e-03),
            (41, -3.4313e-04, -6.5830e-03),
            (42, 6.2119e-03, -3.5482e-03),
            (83, -6.5517e-03, -2.9964e-03),
            (123, -2.1728e-04, -9.5563e-05),
        ],
    );
    assert_converges("T over GN 1", e21, e41, 0.01);
}

/// An elliptically polarized wave (right-hand, axial ratio 0.5, η = 20°,
/// φ = 30°): the reflection applies to the complex polarization vector.
#[test]
fn an_elliptic_wave_over_perfect_ground_receives_like_nec2c() {
    let wave = "EX 2 1 1 0 45 30 20 0 0 0.5\n";
    let e21 = err(
        &receive(&dipole(21), PEC, wave).unwrap(),
        7.163_075e-2,
        &[
            (1, 4.1646e-03, 5.2893e-03),
            (6, 3.3685e-02, 4.3195e-02),
            (11, 4.3937e-02, 5.6573e-02),
            (16, 3.2270e-02, 4.1520e-02),
            (21, 3.8077e-03, 4.8670e-03),
        ],
    );
    let e41 = err(
        &receive(&dipole(41), PEC, wave).unwrap(),
        7.162_344e-2,
        &[
            (1, 2.3064e-03, 2.8938e-03),
            (11, 3.3417e-02, 4.2341e-02),
            (21, 4.4263e-02, 5.6309e-02),
            (31, 3.1988e-02, 4.0650e-02),
            (41, 2.1047e-03, 2.6551e-03),
        ],
    );
    assert_converges("elliptic wave over GN 1", e21, e41, 0.07);
}

/// A ground type fnec does not model solves in free space and says so; its
/// receive answer must be exactly the free-space one — no reflected wave over a
/// matrix without images.
#[test]
fn a_deferred_ground_receives_exactly_as_free_space() {
    let wave = "EX 1 1 1 0 45 0 0\n";
    let deferred = receive(&dipole(21), "GE 1\nGN 3\n", wave).unwrap();
    let free = receive(&dipole(21), "GE 0\n", wave).unwrap();
    for (a, b) in deferred.iter().zip(&free) {
        assert!((a - b).norm() <= 1e-12 * b.norm().max(1e-12), "{a} vs {b}");
    }
}

/// Still refused, each with its reason: a wave from below the ground plane —
/// single or as a receive-sweep row, over perfect or finite ground — and a wire
/// lying in the ground plane. Grazing incidence (θ = 90°) solves, and so do wires
/// touching the ground (planewave_ground_contact.rs).
#[test]
fn what_fnec_cannot_answer_over_ground_is_refused_by_name() {
    let below = receive(&dipole(21), PEC, "EX 1 1 1 0 135 0 0\n").unwrap_err();
    assert!(
        below.contains("below") && below.contains("FND-170"),
        "{below}"
    );
    // A sweep from θ = 0° in 20° steps, ten rows: its last rows are below.
    let sweep = receive(&dipole(21), PEC, "EX 1 10 1 0 0 0 0 20 0\n").unwrap_err();
    assert!(sweep.contains("below"), "{sweep}");
    assert!(receive(&dipole(21), PEC, "EX 1 1 1 0 90 0 0\n").is_ok());

    let in_plane = "GW 1 21 -5 0 0 5 0 0 .001\n";
    let flat = receive(in_plane, PEC, "EX 1 1 1 0 45 0 0\n").unwrap_err();
    assert!(flat.contains("in the ground plane"), "{flat}");

    // Over finite ground the reflected wave is modelled
    // (planewave_finite_ground_nec2c.rs), but a wave from below is refused there too.
    let below_finite = receive(
        &dipole(21),
        "GE 1\nGN 2 0 0 0 13 0.005\n",
        "EX 1 1 1 0 135 0 0\n",
    )
    .unwrap_err();
    assert!(below_finite.contains("below"), "{below_finite}");
}
