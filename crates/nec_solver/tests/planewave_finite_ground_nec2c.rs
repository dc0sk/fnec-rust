// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-170, finite ground — an incident plane wave over `GN 0` / `GN 2`.
//!
//! The reflected wave's coefficients are nec2c's (`etmns`): `rrv` for the field's
//! vertical part, `rrh` for its horizontal (φ̂) part. fnec's finite-ground MATRIX
//! is its own (a normal-incidence scalar Γ on the image, not nec2c's per-pair
//! model), so the gates separate the two:
//!
//! - **The reflected wave**, 3 λ up, where the image coupling is small and the
//!   reflected wave full strength: the ratio of the current over ground to the same
//!   deck's free-space current, against nec2c's same ratio. The receive forcing's
//!   own discretization error, which is the same in free space, cancels in it.
//!   One deck per polarization, so each coefficient is tested alone (their ratios
//!   differ: 1.157 θ̂, 1.232 φ̂ at the peak).
//! - **The matrix**, 5 m up (0.24 λ): a pinned band, which measures fnec's
//!   finite-ground matrix model, not the plane wave.
//!
//! Expectations are nec2c 1.3.1, captured 2026-10-02 at 14.2 MHz over
//! `GN 2 0 0 0 13 0.005` (echo checked: `FREQUENCY : 1.4200E+01`,
//! `RELATIVE DIELECTRIC CONST: 13.000`, `CONDUCTIVITY: 5.000E-03`). Whole-table
//! absolute error, 21 → 41: 3 λ θ̂ 10.14 → 5.62 %, φ̂ 10.38 → 5.70 % (the deck's
//! free-space level, 10.2 → 5.7 %); 5 m θ̂ 8.30 → 3.52 %, φ̂ 8.45 → 3.57 %. Ratio
//! error over the central half: 3 λ 0.27 → 0.16 % (both); 5 m 2.88 → 2.37 % (θ̂),
//! 2.86 → 2.34 % (φ̂).

use nec_solver::validate::pre_solve_error;
use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, ground_model_from_deck,
    solve_hallen_planewave_routed,
};
use num_complex::Complex64;

const FREQ: f64 = 14.2e6;
const AVERAGE: &str = "GE 1\nGN 2 0 0 0 13 0.005\n";
const FREE: &str = "GE 0\n";
const THETA_POL: &str = "EX 1 1 1 0 45 0 0\n";
const PHI_POL: &str = "EX 1 1 1 0 45 90 90\n";

/// The receive currents the way a frontend gets them: refusals first, then the
/// matrix over the deck's own ground, then the receive seam.
fn receive(geometry: &str, ground: &str, wave: &str) -> Vec<Complex64> {
    let d = nec_parser::parse(&format!(
        "CE\n{geometry}{ground}{wave}FR 0 1 0 0 14.2 0\nEN\n"
    ))
    .expect("parses")
    .deck;
    let segs = build_geometry(&d).expect("geometry");
    let g = ground_model_from_deck(&d);
    assert_eq!(pre_solve_error(&d, &segs, &g), None);
    let z = assemble_z_matrix_with_ground(&segs, FREQ, &g);
    solve_hallen_planewave_routed(&d, &segs, &z, FREQ).expect("receive solve")
}

fn dipole(n: u32, h: f64) -> String {
    format!("GW 1 {n} -5 0 {h} 5 0 {h} .001\n")
}

type Pins = [(usize, f64, f64); 3];

/// Max over the pinned segments of the relative error of the ground / free-space
/// current ratio against nec2c's.
fn ratio_err(n: u32, h: f64, wave: &str, ground_ref: &Pins, free_ref: &Pins) -> f64 {
    let g = receive(&dipole(n, h), AVERAGE, wave);
    let f = receive(&dipole(n, h), FREE, wave);
    ground_ref
        .iter()
        .zip(free_ref)
        .map(|(&(s, gr, gi), &(_, fr, fi))| {
            let want = Complex64::new(gr, gi) / Complex64::new(fr, fi);
            (g[s - 1] / f[s - 1] - want).norm() / want.norm()
        })
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

const H3L: f64 = 63.34;

/// θ̂ polarization (in the plane of incidence): the field along the dipole is the
/// vertical-polarization part, so this isolates `rrv`. Kill criterion: under 1 %
/// at 41, shrinking.
#[test]
fn the_vertical_coefficient_matches_nec2c_three_wavelengths_up() {
    let e21 = ratio_err(
        21,
        H3L,
        THETA_POL,
        &[
            (6, -3.58450e-03, 4.73480e-02),
            (11, -5.45090e-03, 6.18380e-02),
            (16, -4.44040e-03, 4.52910e-02),
        ],
        &[
            (6, 1.45810e-02, 3.83710e-02),
            (11, 1.84640e-02, 5.03800e-02),
            (16, 1.31930e-02, 3.70540e-02),
        ],
    );
    let e41 = ratio_err(
        41,
        H3L,
        THETA_POL,
        &[
            (11, -3.28670e-03, 4.67590e-02),
            (21, -5.14320e-03, 6.20140e-02),
            (31, -4.15090e-03, 4.46830e-02),
        ],
        &[
            (11, 1.46000e-02, 3.77970e-02),
            (21, 1.87730e-02, 5.03980e-02),
            (31, 1.31990e-02, 3.64670e-02),
        ],
    );
    assert_converges("θ̂ ratio, 3 λ over GN 2", e21, e41, 0.01);
}

/// φ̂ polarization (E perpendicular to the plane of incidence, along the dipole):
/// isolates `rrh`. Kill criterion: under 1 % at 41, shrinking.
#[test]
fn the_horizontal_coefficient_matches_nec2c_three_wavelengths_up() {
    let e21 = ratio_err(
        21,
        H3L,
        PHI_POL,
        &[
            (6, 2.00290e-02, -7.56350e-02),
            (11, 2.66240e-02, -1.00890e-01),
            (16, 2.00290e-02, -7.56350e-02),
        ],
        &[
            (6, -2.19160e-02, -5.95850e-02),
            (11, -2.92940e-02, -7.94370e-02),
            (16, -2.19160e-02, -5.95850e-02),
        ],
    );
    let e41 = ratio_err(
        41,
        H3L,
        PHI_POL,
        &[
            (11, 1.93750e-02, -7.47140e-02),
            (21, 2.61590e-02, -1.01250e-01),
            (31, 1.93750e-02, -7.47140e-02),
        ],
        &[
            (11, -2.19250e-02, -5.86610e-02),
            (21, -2.97750e-02, -7.94480e-02),
            (31, -2.19250e-02, -5.86610e-02),
        ],
    );
    assert_converges("φ̂ ratio, 3 λ over GN 2", e21, e41, 0.01);
}

/// 5 m up (0.24 λ) the image coupling is strong, and fnec's finite-ground matrix
/// (a normal-incidence scalar Γ) is not nec2c's: the ratio is 2.4 % off at 41 in
/// both polarizations. Pinned as a band — it measures the matrix model, and moves
/// if that model changes: re-measure then, don't widen it.
#[test]
fn five_metres_up_the_matrix_model_gap_is_pinned() {
    let theta = ratio_err(
        41,
        5.0,
        THETA_POL,
        &[
            (11, 1.62720e-03, 5.45940e-02),
            (21, 1.63140e-03, 7.23120e-02),
            (31, 9.63390e-04, 5.20490e-02),
        ],
        &[
            (11, 3.20240e-03, 4.03920e-02),
            (21, 3.60470e-03, 5.36600e-02),
            (31, 2.23930e-03, 3.87180e-02),
        ],
    );
    let phi = ratio_err(
        41,
        5.0,
        PHI_POL,
        &[
            (11, 8.26140e-03, -9.31790e-02),
            (21, 1.10700e-02, -1.26290e-01),
            (31, 8.26140e-03, -9.31790e-02),
        ],
        &[
            (11, -4.26660e-03, -6.24790e-02),
            (21, -5.85600e-03, -8.46420e-02),
            (31, -4.26660e-03, -6.24790e-02),
        ],
    );
    println!(
        "5 m ratio gap: θ̂ {:.2} %, φ̂ {:.2} %",
        theta * 100.0,
        phi * 100.0
    );
    for (label, e) in [("θ̂", theta), ("φ̂", phi)] {
        assert!((0.015..0.035).contains(&e), "{label}: {:.2} %", e * 100.0);
    }
}

/// A ground with εr = 1 and σ = 0 is no ground: both reflection coefficients
/// vanish, the matrix's image Γ too, so the receive currents must be the
/// free-space ones — exact, independent of any model.
#[test]
fn a_ground_of_vacuum_receives_exactly_as_free_space() {
    for wave in [THETA_POL, PHI_POL] {
        let vacuum = receive(&dipole(21, 5.0), "GE 1\nGN 2 0 0 0 1 0\n", wave);
        let free = receive(&dipole(21, 5.0), FREE, wave);
        for (a, b) in vacuum.iter().zip(&free) {
            assert!((a - b).norm() <= 1e-9 * b.norm().max(1e-12), "{a} vs {b}");
        }
    }
}
