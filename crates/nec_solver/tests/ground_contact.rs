// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-082 — wires touching perfectly conducting ground, solved by images.
//!
//! Over PEC the problem is exactly free space with the geometry mirrored, so the
//! strongest gate needs no reference solver: a monopole must equal, to rounding,
//! the explicitly doubled free-space deck (the wire, its image as a second wire,
//! both gaps driven). nec2c agrees on that identity, and on the values.

use nec_solver::{
    assemble_z_matrix_with_ground, build_deck_stamps, build_geometry, ground_model_from_deck,
    solve_hallen_routed, validate::pre_solve_error, HallenSessionError,
};
use num_complex::Complex64;

const F: f64 = 14.2e6;
const TAIL: &str = "FR 0 1 0 0 14.2 0\nEN\n";
const MONOPOLE: &str = "CE\nGW 1 26 0 0 0 0 0 5.282 0.001\nGE 1\nGN 1\n";

fn solve(text: &str, feed: (u32, u32)) -> Result<Complex64, String> {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let ground = ground_model_from_deck(&deck);
    if let Some(e) = pre_solve_error(&deck, &segs, &ground) {
        return Err(e);
    }
    let mut z = assemble_z_matrix_with_ground(&segs, F, &ground);
    let loads = build_deck_stamps(&deck, &segs, F).diagonal;
    let routed = solve_hallen_routed(&deck, &segs, &mut z, F, &loads)
        .map_err(|e: HallenSessionError| e.to_string())?;
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Ok(Complex64::new(1.0, 0.0) / routed.source_current(idx))
}

/// nec2c 1.3.1: λ/4 monopole, 26 segments, base-fed on PEC — 39.584 + j23.206
/// (captured 2026-09-26). The residual is half the λ/2 dipole's (FND-156).
#[test]
fn a_monopole_on_pec_tracks_nec2c() {
    let z = solve(&format!("{MONOPOLE}EX 0 1 1 0 1.0 0.0\n{TAIL}"), (1, 1)).unwrap();
    assert!(
        (z.re - 39.584).abs() < 1.5 && (z.im - 23.206).abs() < 4.0,
        "monopole {z:.3}, nec2c 39.584 + j23.206"
    );
}

/// The identity: the monopole IS the doubled free-space deck (wire 2 is the image,
/// walked downward from the ground point, so its gap is −V in its own frame).
#[test]
fn a_monopole_equals_its_doubled_free_space_image_deck() {
    let mono = solve(&format!("{MONOPOLE}EX 0 1 1 0 1.0 0.0\n{TAIL}"), (1, 1)).unwrap();
    let doubled = solve(
        &format!(
            "CE\nGW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE\n\
             EX 0 1 1 0 1.0 0.0\nEX 0 2 1 0 -1.0 0.0\n{TAIL}"
        ),
        (1, 1),
    )
    .unwrap();
    assert!(
        (mono - doubled).norm() < 1e-9 * doubled.norm(),
        "monopole {mono} vs doubled deck {doubled}"
    );
}

/// Two ground-mounted verticals 3 m apart, one fed: nec2c 14.803 + j17.310.
#[test]
fn two_grounded_verticals_track_nec2c() {
    let z = solve(
        &format!(
            "CE\nGW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 3 0 0 3 0 5.0 0.001\nGE 1\nGN 1\n\
             EX 0 1 1 0 1.0 0.0\n{TAIL}"
        ),
        (1, 1),
    )
    .unwrap();
    assert!(
        (z.re - 14.803).abs() < 2.0 && (z.im - 17.310).abs() < 4.0,
        "array {z:.3}, nec2c 14.803 + j17.310"
    );
}

/// A series load at the base feed shifts Z by exactly itself: the image carries
/// the same load, so the per-gap identity survives the doubling.
#[test]
fn a_base_load_shifts_the_monopole_by_exactly_itself() {
    let bare = solve(&format!("{MONOPOLE}EX 0 1 1 0 1.0 0.0\n{TAIL}"), (1, 1)).unwrap();
    let loaded = solve(
        &format!("{MONOPOLE}LD 4 1 1 1 50 25 0\nEX 0 1 1 0 1.0 0.0\n{TAIL}"),
        (1, 1),
    )
    .unwrap();
    let d = loaded - bare;
    assert!(
        (d - Complex64::new(50.0, 25.0)).norm() < 0.05,
        "a 50 + j25 base load shifted Z by {d:.4}"
    );
}

/// Every contact fnec cannot represent is refused, naming the shape.
#[test]
fn unrepresentable_contacts_are_refused() {
    for (geometry, needle) in [
        // In the ground plane: the image coincides with the wire.
        ("GW 1 21 -5 0 0 5 0 0 0.001\nGE 1\nGN 1\n", "ground plane"),
        // Below it.
        ("GW 1 21 0 0 -1 0 0 5 0.001\nGE 1\nGN 1\n", "below the ground"),
        // Grounded at both ends: a closed loop with its image.
        (
            "GW 1 10 0 0 0 0 0 3 0.001\nGW 2 10 0 0 3 3 0 3 0.001\nGW 3 10 3 0 3 3 0 0 0.001\nGE 1\nGN 1\n",
            "closed loop",
        ),
        // Two wires meeting at one ground point.
        (
            "GW 1 10 0 0 0 0 0 3 0.001\nGW 2 10 0 0 0 2 0 3 0.001\nGE 1\nGN 1\n",
            "closed loop",
        ),
        // Finite ground has no trustworthy contact model.
        ("GW 1 26 0 0 0 0 0 5.282 0.001\nGE 1\nGN 2 0 0 0 13 0.005\n", "PERFECT ground"),
    ] {
        let err = solve(&format!("CE\n{geometry}EX 0 1 2 0 1.0 0.0\n{TAIL}"), (1, 2))
            .expect_err(geometry);
        assert!(err.contains(needle), "{geometry}: {err}");
    }
}

/// The drives and cards whose images the doubling does not build are refused,
/// not solved without their images.
#[test]
fn contact_with_unmirrored_drives_or_networks_is_refused() {
    let current_source = format!("{MONOPOLE}EX 4 1 1 0 1.0 0.0\n{TAIL}");
    let with_line = format!(
        "CE\nGW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 3 0 0.5 3 0 5.0 0.001\nGE 1\nGN 1\n\
         TL 1 10 2 10 50.0 1.0\nEX 0 1 1 0 1.0 0.0\n{TAIL}"
    );
    for (what, text) in [("current source", current_source), ("TL", with_line)] {
        let err = solve(&text, (1, 1)).expect_err(what);
        assert!(err.contains("touching the ground"), "{what}: {err}");
    }
}
