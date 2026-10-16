// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PAR-011: the far field's complex components, `R·E_θ` and `R·E_φ`, against nec2c.
//!
//! NEC-2's pattern table prints both components' magnitude and phase on every row,
//! and its polarization columns derive from them. fnec computed the components and
//! then kept only the gains; they are now kept, scaled as NEC-2 scales them
//! (`ffld`: `-j·(η₀/4π)·k` times the moment sum, the range's `e^{-jkR}` dropped).
//! The scale and the phase convention are what this gates: the complex field,
//! magnitude and phase together, against nec2c on four problems, which are
//! a free-space dipole, a dipole over perfect ground, the same dipole over a
//! finite ground by reflection coefficient (`GN 0`), and two crossed dipoles fed in
//! quadrature, whose field is elliptically polarized with both components large.
//!
//! The reference is nec2c's converged value, and shown converged: each case stores
//! nec2c at 1×, 3× and 9× its mesh (`scripts/nec2c-fine-mesh.py`), and the test
//! asserts the series settles — contraction ratio below 0.7 and the geometric tail
//! within a third of the tolerance — before it measures fnec against the 9× value,
//! at two meshes, with the error shrinking.
//!
//! nec2c 1.3.1, captured 2026-10-15. Each row: θ, φ, |E_θ| (V), ∠E_θ (°), |E_φ|, ∠E_φ.

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, compute_radiation_pattern,
    ground_model_from_deck, solve_hallen, wire_endpoints_from_segs, FarFieldPoint,
};
use num_complex::Complex64;

type Row = (f64, f64, f64, f64, f64, f64);

struct Case {
    name: &'static str,
    /// The deck at the 1× mesh, `{n}` the segments per wire and `{f}` the feed.
    deck: &'static str,
    n: u32,
    /// nec2c at 1×, 3×, 9×.
    series: [&'static [Row]; 3],
    /// The bound on fnec's error at 1× and 3× the mesh, as a fraction of the
    /// largest field in the case.
    tol: [f64; 2],
}

const FREE: [&[Row]; 3] = [
    &[
        (30.0, 0.0, 2.8325e-01, 58.02, 0.0, 0.0),
        (60.0, 0.0, 5.5658e-01, 57.87, 0.0, 0.0),
        (90.0, 0.0, 6.8339e-01, 57.79, 0.0, 0.0),
    ],
    &[
        (30.0, 0.0, 2.8229e-01, 57.72, 0.0, 0.0),
        (60.0, 0.0, 5.5481e-01, 57.56, 0.0, 0.0),
        (90.0, 0.0, 6.8129e-01, 57.49, 0.0, 0.0),
    ],
    &[
        (30.0, 0.0, 2.8175e-01, 57.57, 0.0, 0.0),
        (60.0, 0.0, 5.5379e-01, 57.41, 0.0, 0.0),
        (90.0, 0.0, 6.8006e-01, 57.34, 0.0, 0.0),
    ],
];

const PEC: [&[Row]; 3] = [
    &[
        (20.0, 0.0, 4.5786e-01, -19.79, 0.0, 0.0),
        (50.0, 0.0, 7.7627e-01, -19.64, 0.0, 0.0),
        (20.0, 90.0, 2.4041e-12, 160.18, 5.0130e-01, 160.18),
        (50.0, 90.0, 4.5801e-12, 160.18, 1.3962e+00, 160.18),
    ],
    &[
        (20.0, 0.0, 4.5689e-01, -20.15, 0.0, 0.0),
        (50.0, 0.0, 7.7448e-01, -20.01, 0.0, 0.0),
        (20.0, 90.0, 2.3991e-12, 159.81, 5.0027e-01, 159.81),
        (50.0, 90.0, 4.5706e-12, 159.81, 1.3933e+00, 159.81),
    ],
    &[
        (20.0, 0.0, 4.5631e-01, -20.34, 0.0, 0.0),
        (50.0, 0.0, 7.7343e-01, -20.19, 0.0, 0.0),
        (20.0, 90.0, 2.3961e-12, 159.63, 4.9963e-01, 159.63),
        (50.0, 90.0, 4.5648e-12, 159.63, 1.3915e+00, 159.63),
    ],
];

const GN0: [&[Row]; 3] = [
    &[
        (20.0, 0.0, 3.9808e-01, 14.12, 0.0, 0.0),
        (50.0, 0.0, 5.3814e-01, -20.73, 0.0, 0.0),
        (20.0, 90.0, 2.0903e-12, -165.91, 4.2986e-01, -169.60),
        (50.0, 90.0, 3.1762e-12, 159.09, 1.1660e+00, 155.94),
    ],
    &[
        (20.0, 0.0, 3.9701e-01, 13.78, 0.0, 0.0),
        (50.0, 0.0, 5.3659e-01, -21.08, 0.0, 0.0),
        (20.0, 90.0, 2.0848e-12, -166.25, 4.2872e-01, -169.95),
        (50.0, 90.0, 3.1678e-12, 158.74, 1.1629e+00, 155.59),
    ],
    &[
        (20.0, 0.0, 3.9638e-01, 13.60, 0.0, 0.0),
        (50.0, 0.0, 5.3570e-01, -21.25, 0.0, 0.0),
        (20.0, 90.0, 2.0815e-12, -166.43, 4.2805e-01, -170.12),
        (50.0, 90.0, 3.1628e-12, 158.57, 1.1611e+00, 155.42),
    ],
];

const QUADRATURE: [&[Row]; 3] = [
    &[
        (15.0, 10.0, 6.4843e-01, -112.01, 6.8435e-01, -21.56),
        (40.0, 10.0, 4.7485e-01, -111.09, 6.8052e-01, -22.46),
        (65.0, 10.0, 2.3813e-01, -109.88, 6.7618e-01, -23.58),
        (15.0, 45.0, 6.5001e-01, -76.79, 6.8268e-01, 13.21),
        (40.0, 45.0, 4.9479e-01, -76.82, 6.5331e-01, 13.18),
        (65.0, 45.0, 2.6012e-01, -76.91, 6.1939e-01, 13.09),
        (15.0, 80.0, 6.4836e-01, -41.54, 6.8442e-01, 47.97),
        (40.0, 80.0, 4.7449e-01, -42.45, 6.8094e-01, 48.72),
        (65.0, 80.0, 2.3772e-01, -43.70, 6.7698e-01, 49.55),
    ],
    &[
        (15.0, 10.0, 6.4642e-01, -112.32, 6.8225e-01, -21.86),
        (40.0, 10.0, 4.7332e-01, -111.39, 6.7842e-01, -22.77),
        (65.0, 10.0, 2.3732e-01, -110.17, 6.7409e-01, -23.88),
        (15.0, 45.0, 6.4800e-01, -77.09, 6.8057e-01, 12.91),
        (40.0, 45.0, 4.9323e-01, -77.12, 6.5125e-01, 12.88),
        (65.0, 45.0, 2.5928e-01, -77.21, 6.1738e-01, 12.79),
        (15.0, 80.0, 6.4635e-01, -41.85, 6.8232e-01, 47.67),
        (40.0, 80.0, 4.7296e-01, -42.75, 6.7884e-01, 48.41),
        (65.0, 80.0, 2.3691e-01, -44.01, 6.7489e-01, 49.24),
    ],
    &[
        (15.0, 10.0, 6.4526e-01, -112.47, 6.8103e-01, -22.01),
        (40.0, 10.0, 4.7244e-01, -111.54, 6.7720e-01, -22.92),
        (65.0, 10.0, 2.3687e-01, -110.33, 6.7287e-01, -24.04),
        (15.0, 45.0, 6.4684e-01, -77.24, 6.7935e-01, 12.76),
        (40.0, 45.0, 4.9233e-01, -77.28, 6.5006e-01, 12.72),
        (65.0, 45.0, 2.5880e-01, -77.36, 6.1623e-01, 12.64),
        (15.0, 80.0, 6.4519e-01, -42.00, 6.8110e-01, 47.51),
        (40.0, 80.0, 4.7208e-01, -42.90, 6.7762e-01, 48.26),
        (65.0, 80.0, 2.3646e-01, -44.16, 6.7368e-01, 49.09),
    ],
];

fn cases() -> [Case; 4] {
    [
        Case {
            name: "free-space dipole",
            deck: "GW 1 {n} 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 {f} 0 1 0\nFR 0 1 0 0 14.2\nEN\n",
            n: 21,
            series: FREE,
            tol: [0.040, 0.0175],
        },
        Case {
            name: "dipole over perfect ground",
            deck: "GW 1 {n} -5.282 0 10 5.282 0 10 0.001\nGE 1\nGN 1\nEX 0 1 {f} 0 1 0\nFR 0 1 0 0 14.2\nEN\n",
            n: 21,
            series: PEC,
            tol: [0.040, 0.0175],
        },
        Case {
            name: "dipole over GN 0",
            deck: "GW 1 {n} -5.282 0 10 5.282 0 10 0.001\nGE 1\nGN 0 0 0 0 13 0.005\nEX 0 1 {f} 0 1 0\nFR 0 1 0 0 14.2\nEN\n",
            n: 21,
            series: GN0,
            tol: [0.040, 0.0175],
        },
        Case {
            name: "crossed dipoles in quadrature",
            deck: "GW 1 {n} -5.282 0 0 5.282 0 0 0.001\nGW 2 {n} 0 -5.282 0.05 0 5.282 0.05 0.001\nGE 0\nEX 0 1 {f} 0 1 0\nEX 0 2 {f} 0 0 1\nFR 0 1 0 0 14.2\nEN\n",
            n: 21,
            series: QUADRATURE,
            tol: [0.040, 0.0175],
        },
    ]
}

fn field(mag: f64, deg: f64) -> Complex64 {
    Complex64::from_polar(mag, deg.to_radians())
}

/// A row's two components.
fn components(r: &Row) -> (Complex64, Complex64) {
    (field(r.2, r.3), field(r.4, r.5))
}

/// The largest distance between two tables' fields, over the case's largest field.
fn distance(a: &[(Complex64, Complex64)], b: &[(Complex64, Complex64)], scale: f64) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x.0 - y.0).norm().max((x.1 - y.1).norm()))
        .fold(0.0, f64::max)
        / scale
}

/// fnec's `(R·E_θ, R·E_φ)` at the case's angles, on the deck at `mult` × its mesh.
fn fnec_fields(case: &Case, mult: u32) -> Vec<(Complex64, Complex64)> {
    let n = case.n * mult;
    let feed = n.div_ceil(2);
    let text = case
        .deck
        .replace("{n}", &n.to_string())
        .replace("{f}", &feed.to_string());
    let deck = nec_parser::parse(&text).expect("deck").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let ground = ground_model_from_deck(&deck);
    let z = assemble_z_matrix_with_ground(&segs, 14.2e6, &ground);
    let h = build_hallen_rhs(&deck, &segs, 14.2e6).expect("rhs");
    let sol = solve_hallen(
        &z,
        &h.rhs,
        &h.cos_vec,
        &h.sin_vec,
        &wire_endpoints_from_segs(&segs),
        &[],
    )
    .expect("solve");
    let points: Vec<FarFieldPoint> = case.series[0]
        .iter()
        .map(|r| FarFieldPoint {
            theta_deg: r.0,
            phi_deg: r.1,
        })
        .collect();
    compute_radiation_pattern(&segs, &sol.currents, 14.2e6, &points, &ground)
        .iter()
        .map(|r| (r.e_theta, r.e_phi))
        .collect()
}

#[test]
fn the_far_field_components_converge_to_nec2c() {
    let mut report = Vec::new();
    let mut ok = true;
    for case in cases() {
        let series: Vec<Vec<(Complex64, Complex64)>> = case
            .series
            .iter()
            .map(|t| t.iter().map(components).collect())
            .collect();
        let scale = series[2]
            .iter()
            .map(|c| c.0.norm().max(c.1.norm()))
            .fold(0.0, f64::max);
        // The reference has settled: the series contracts, and what is left of it
        // is within a third of the tightest bound.
        let d1 = distance(&series[1], &series[0], scale);
        let d2 = distance(&series[2], &series[1], scale);
        let r = d2 / d1;
        let tail = d2 * r / (1.0 - r);
        assert!(
            r < 0.7 && tail <= case.tol[1] / 3.0,
            "{}: nec2c has not settled: steps {d1:.5} then {d2:.5} (r = {r:.2}, tail {tail:.5})",
            case.name
        );
        let e1 = distance(&fnec_fields(&case, 1), &series[2], scale);
        let e3 = distance(&fnec_fields(&case, 3), &series[2], scale);
        report.push(format!(
            "{}: fnec {:.2} % from converged nec2c at 1x, {:.2} % at 3x (nec2c tail {:.3} %)",
            case.name,
            100.0 * e1,
            100.0 * e3,
            100.0 * tail
        ));
        ok &= e1 < case.tol[0] && e3 < case.tol[1] && e3 < e1;
    }
    assert!(ok, "{}", report.join("\n"));
}
