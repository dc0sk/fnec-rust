// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-252 — Hallén takes a radius step on the section graph.
//!
//! The path solve splits a path only where the tangent turns, so a straight
//! stepped wire was one section with one homogeneous `(C, D)`: the potential it
//! forced smooth across the step made the current jump instead, and the solve
//! carried a tenth of the step's reactance or less. The section graph puts a node
//! at the step — Kirchhoff and equal potential — and every drive now takes it
//! there. Its impedance against the closed form and the Leeson equivalent is
//! gated in `apps/nec-cli/tests/mpie_mixed_radii.rs`; these hold the drives and
//! the loads to the same solve.

use nec_solver::{
    assemble_z_matrix_with_ground, build_deck_stamps, build_geometry, ground_model_from_deck,
    hallen_route, solve_current_source_hallen, solve_hallen_routed, CurrentSourceError,
};
use num_complex::Complex64;

const F: f64 = 14.2e6;

/// The 10.4 m dipole thinned from 5 mm to 4 mm within 1.26 m of its feed, with
/// `cards` (an `LD`) and the drive `ex`.
fn stepped(cards: &str, ex: &str) -> String {
    format!(
        "GW 1 72 0 0 -5.2 0 0 -1.26 0.005\nGW 2 45 0 0 -1.26 0 0 1.26 0.004\n\
         GW 3 72 0 0 1.26 0 0 5.2 0.005\nGE 0\n{cards}{ex}\nFR 0 1 0 0 14.2 0\nEN\n"
    )
}

/// The routed solve's feedpoint impedance: `V / I` at tag 2 segment 23 for a
/// voltage source, the solved port voltage over the impressed current for a
/// current source.
fn z_routed(text: &str) -> Complex64 {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let mut z = assemble_z_matrix_with_ground(&segs, F, &ground_model_from_deck(&deck));
    let loads = build_deck_stamps(&deck, &segs, F).diagonal;
    let routed = solve_hallen_routed(&deck, &segs, &mut z, F, &loads).expect("solves");
    let idx = segs
        .iter()
        .position(|s| s.tag == 2 && s.tag_index == 23)
        .expect("feed");
    match routed.port_voltage {
        Some(v) => v / Complex64::new(1.0, 0.0),
        None => Complex64::new(1.0, 0.0) / routed.source_current(idx),
    }
}

/// A current source on the stepped wire is the unit-gap voltage solve scaled to
/// its impressed current, on the same graph: `EX 4` and `EX 0` price the same
/// port. The current-source entry used to solve it on the conductor paths itself,
/// with one `(C, D)` across each step, so the two drives disagreed once the voltage
/// solve moved to the graph; it now hands the deck to the routed solve.
#[test]
fn a_current_source_on_a_stepped_wire_prices_as_the_voltage_source() {
    let v = z_routed(&stepped("", "EX 0 2 23 0 1 0"));
    let i = z_routed(&stepped("", "EX 4 2 23 0 1 0"));
    assert!(
        (v - i).norm() < 1e-9 * v.norm(),
        "EX 0 {v:.9} vs EX 4 {i:.9}"
    );
    // 78.552 + j37.405 on the graph; 78.59 + j28.10 on one (C, D) across the step.
    assert!((v.im - 37.405).abs() < 0.01, "{v:.4}");
    let deck = nec_parser::parse(&stepped("", "EX 4 2 23 0 1 0"))
        .expect("parses")
        .deck;
    let segs = build_geometry(&deck).expect("geometry");
    let z = assemble_z_matrix_with_ground(&segs, F, &ground_model_from_deck(&deck));
    assert!(matches!(
        solve_current_source_hallen(&deck, &segs, &z, F),
        Err(CurrentSourceError::NeedsRoutedSolve)
    ));
    let route = hallen_route(&deck, &segs);
    assert!(route.paths && !route.unsupported_topology, "{route:?}");
}

/// A load on the fed segment adds exactly its impedance on the graph too: the
/// loads are columns of its system, entered once (FND-198).
#[test]
fn a_load_on_the_fed_segment_of_a_stepped_wire_adds_its_impedance() {
    let bare = z_routed(&stepped("", "EX 0 2 23 0 1 0"));
    let loaded = z_routed(&stepped("LD 4 2 23 23 20 50\n", "EX 0 2 23 0 1 0"));
    let added = loaded - bare;
    assert!(
        (added - Complex64::new(20.0, 50.0)).norm() < 1e-6 * loaded.norm(),
        "a 20 + j50 Ω load on the fed segment added {added:.9}"
    );
}
