// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PAR-011: NEC-2's power budget — structure loss and efficiency — against nec2c.
//!
//! NEC-2 prints, at every frequency, the input power, the structure loss (the
//! power the series loads dissipate, ½·Σ|Iₙ|²·Re Zₙ), the network loss, and the
//! radiated power and efficiency as the input less the losses. 4nec2 scales its
//! voltage and current displays from that block. fnec had the input power and a
//! pattern-integral efficiency, but no structure loss; `structure_loss_w` is it.
//!
//! Three loaded half-wave dipoles at 14.2 MHz: a series 50 Ω + 1 µH at the feed
//! with copper wire (the `dip_ld` fixture's deck), copper wire alone, and a
//! parallel 1 kΩ ‖ 1 µH ‖ 100 pF at the feed. The reference is nec2c's converged
//! value, and shown converged: each case stores nec2c at 1×, 3× and 9× its mesh
//! (`scripts/nec2c-fine-mesh.py`), and the series is asserted settled before
//! fnec is measured against the 9× value, at 1× and 3× its own mesh.
//!
//! nec2c 1.3.1, captured 2026-10-15: (input W, structure loss W, efficiency %).

use nec_solver::{
    assemble_z_matrix_with_ground, build_deck_stamps, build_geometry, find_deck_segment,
    ground_model_from_deck, solve_hallen_routed, structure_loss_w,
};

const FREQ: f64 = 14.2e6;

/// The bound on fnec's error in the loss fraction at 1× and 3× the mesh. The 3×
/// bound is held at three times nec2c's own remaining tail (0.00052 on the series
/// RLC), so the reference uses at most a third of it.
const FRAC_TOL: [f64; 2] = [0.003, 0.002];
/// The same for the efficiency, in percentage points (nec2c's tail 0.073).
const EFF_TOL: [f64; 2] = [0.3, 0.25];

struct Case {
    name: &'static str,
    loads: &'static str,
    series: [(f64, f64, f64); 3],
}

const CASES: [Case; 3] = [
    Case {
        name: "series RLC and copper",
        loads: "LD 0 1 {f} {f} 50 1e-6 0\nLD 5 1 0 0 5.8e7\n",
        series: [
            (1.8378e-03, 7.1946e-04, 60.85),
            (1.8320e-03, 7.1518e-04, 60.96),
            (1.8288e-03, 7.1294e-04, 61.02),
        ],
    },
    Case {
        name: "copper",
        loads: "LD 5 1 0 0 5.8e7\n",
        series: [
            (4.6638e-03, 5.1220e-05, 98.90),
            (4.6314e-03, 5.0866e-05, 98.90),
            (4.6144e-03, 5.0670e-05, 98.90),
        ],
    },
    Case {
        name: "parallel RLC",
        loads: "LD 1 1 {f} {f} 1000 1e-6 1e-10\n",
        series: [
            (5.2548e-04, 3.5219e-04, 32.98),
            (5.2506e-04, 3.5137e-04, 33.08),
            (5.2480e-04, 3.5093e-04, 33.13),
        ],
    },
];

/// fnec's (structure loss over input power, efficiency) at `n` segments.
fn fnec_budget(case: &Case, n: u32) -> (f64, f64) {
    let feed = n.div_ceil(2);
    let text = format!(
        "GW 1 {n} 0 0 -5.282 0 0 5.282 0.001\nGE 0\n{}EX 0 1 {feed} 0 1 0\nFR 0 1 0 0 14.2\nEN\n",
        case.loads.replace("{f}", &feed.to_string())
    );
    let deck = nec_parser::parse(&text).expect("deck").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, FREQ, &ground_model_from_deck(&deck));
    let loads = build_deck_stamps(&deck, &segs, FREQ).diagonal;
    let routed = solve_hallen_routed(&deck, &segs, &mut z, FREQ, &loads).expect("solves");
    let currents = routed.source_currents();
    let idx = find_deck_segment(&segs, 1, feed).expect("feed");
    // ½·Re(V·I*) at the 1 V feed: NEC-2's input power.
    let input = 0.5 * currents[idx].conj().re;
    let loss = structure_loss_w(&currents, &loads);
    (loss / input, 100.0 * (input - loss) / input)
}

#[test]
fn the_structure_loss_and_efficiency_converge_to_nec2c() {
    let mut report = Vec::new();
    let mut ok = true;
    for case in &CASES {
        // The reference: the loss fraction and the efficiency, settled.
        let frac: Vec<f64> = case.series.iter().map(|s| s.1 / s.0).collect();
        let eff: Vec<f64> = case.series.iter().map(|s| s.2).collect();
        for (what, v, tol) in [
            ("loss fraction", &frac, FRAC_TOL[1]),
            ("efficiency", &eff, EFF_TOL[1]),
        ] {
            let (d1, d2) = ((v[1] - v[0]).abs(), (v[2] - v[1]).abs());
            // Settled: what the last tripling moved, carried on geometrically, is
            // within a third of the bound (a series that has stopped moving at
            // print precision has settled).
            let tail = if d1 > 0.0 && d2 < d1 {
                d2 * (d2 / d1) / (1.0 - d2 / d1)
            } else {
                d2
            };
            assert!(
                tail <= tol / 3.0,
                "{}: nec2c's {what} has not settled: {v:?}",
                case.name
            );
        }
        let (f1, e1) = fnec_budget(case, 21);
        let (f3, e3) = fnec_budget(case, 63);
        let (df1, df3) = ((f1 - frac[2]).abs(), (f3 - frac[2]).abs());
        let (de1, de3) = ((e1 - eff[2]).abs(), (e3 - eff[2]).abs());
        report.push(format!(
            "{}: loss fraction {f1:.5} / {f3:.5} vs nec2c {:.5}; efficiency {e1:.2} / {e3:.2} % vs {:.2} %",
            case.name, frac[2], eff[2]
        ));
        // Bounded at both meshes, and not moving away when refined (the copper
        // case agrees to print precision at both).
        ok &= df1 < FRAC_TOL[0] && df3 < FRAC_TOL[1] && df3 <= df1 + 1e-5;
        ok &= de1 < EFF_TOL[0] && de3 < EFF_TOL[1] && de3 <= de1 + 0.01;
    }
    assert!(ok, "{}", report.join("\n"));
}
