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
        // A junction above the ground with an arm one segment long: doubled, a
        // section graph the builder refuses (one row cannot fix a section's two
        // constants).
        (
            "GW 1 10 0 0 0 0 0 3 0.001\nGW 2 1 0 0 3 0.3 0 3 0.001\nGW 3 10 0 0 3 -3 0 3 0.001\nGE 1\nGN 1\n",
            "one segment long",
        ),
        // Finite ground has no trustworthy contact model.
        ("GW 1 26 0 0 0 0 0 5.282 0.001\nGE 1\nGN 2 0 0 0 13 0.005\n", "PERFECT ground"),
    ] {
        let err = solve(&format!("CE\n{geometry}EX 0 1 2 0 1.0 0.0\n{TAIL}"), (1, 2))
            .expect_err(geometry);
        assert!(err.contains(needle), "{geometry}: {err}");
    }
}

/// A wire grounded at both ends (doubled, a closed loop) and two wires meeting
/// at one ground point (doubled, a degree-4 node) were refused as "cannot
/// represent"; they are section graphs and solve (FND-162 stage 5). Their
/// accuracy is gated in `graph_nec2c.rs` (the folded monopole against nec2c).
#[test]
fn grounded_loops_and_ground_point_junctions_solve() {
    for geometry in [
        "GW 1 10 0 0 0 0 0 3 0.001\nGW 2 10 0 0 3 3 0 3 0.001\nGW 3 10 3 0 3 3 0 0 0.001\nGE 1\nGN 1\n",
        "GW 1 10 0 0 0 0 0 3 0.001\nGW 2 10 0 0 0 2 0 3 0.001\nGE 1\nGN 1\n",
    ] {
        let z = solve(&format!("CE\n{geometry}EX 0 1 2 0 1.0 0.0\n{TAIL}"), (1, 2))
            .unwrap_or_else(|e| panic!("{geometry}: {e}"));
        assert!(z.re > 0.0 && z.re.is_finite() && z.im.is_finite(), "{geometry}: {z}");
    }
}

/// The feedpoint impedance the way a frontend prices it, at the deck's own
/// frequency (its FR card, as the CLI reads it): a current source from its solved
/// port voltage (`Z = V / i0`), a voltage source from the source current, network
/// branch included.
fn feed_z(text: &str, feed: (u32, u32)) -> Result<Complex64, String> {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let ground = ground_model_from_deck(&deck);
    if let Some(e) = pre_solve_error(&deck, &segs, &ground) {
        return Err(e);
    }
    let f = nec_solver::frequencies_hz(&deck)[0];
    let mut z = assemble_z_matrix_with_ground(&segs, f, &ground);
    let loads = build_deck_stamps(&deck, &segs, f).diagonal;
    let routed = solve_hallen_routed(&deck, &segs, &mut z, f, &loads)
        .map_err(|e: HallenSessionError| e.to_string())?;
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Ok(match routed.port_voltage {
        // The decks below drive 1 A.
        Some(v) => v,
        None => Complex64::new(1.0, 0.0) / routed.source_current(idx),
    })
}

/// A current source on wires touching PEC ground is the voltage source scaled:
/// the doubled problem is driven by the gap pair and scaled to the impressed
/// current, so `Z = V / i0` is the voltage drive's own impedance, to round-off —
/// on a straight wire (path route), a bent one, and a top-hat monopole (whose
/// doubled structure is a section graph).
#[test]
fn a_current_source_on_contact_prices_as_the_voltage_source() {
    for (label, geometry) in [
        ("monopole", "GW 1 26 0 0 0 0 0 5.282 0.001\n"),
        (
            "inverted-L",
            "GW 1 21 0 0 0 0 0 3 0.001\nGW 2 21 0 0 3 4.5 0 3 0.001\n",
        ),
        (
            "top-hat",
            "GW 1 21 0 0 0 0 0 4 0.001\nGW 2 11 0 0 4 -1.5 0 4 0.001\n\
             GW 3 11 0 0 4 1.5 0 4 0.001\n",
        ),
    ] {
        let deck = |ex: &str| format!("CE\n{geometry}GE 1\nGN 1\n{ex}{TAIL}");
        let v = feed_z(&deck("EX 0 1 1 0 1.0 0.0\n"), (1, 1)).unwrap();
        let i = feed_z(&deck("EX 4 1 1 0 1.0 0.0\n"), (1, 1)).unwrap();
        println!("{label}: EX 0 {v:.4}, EX 4 {i:.4}");
        assert!(
            (i - v).norm() <= 1e-9 * v.norm(),
            "{label}: current source {i} vs voltage source {v}"
        );
    }
}

/// Two λ/4 monopoles on PEC, 5 m apart at 30 MHz, the first fed at its base, a
/// network from base to base.
fn pair(n: u32, network: &str) -> String {
    format!(
        "CE\nGW 1 {n} 0 0 0 0 0 2.5 0.001\nGW 2 {n} 5 0 0 5 0 2.5 0.001\nGE 1\nGN 1\n\
         {network}EX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 30 0\nEN\n"
    )
}

/// The same pair drawn by hand as its free-space double, images drawn UPWARD —
/// each monopole and its image one wire from −2.5 to 2.5 m, the original base at
/// segment n + 1 and the image's at n, both driven +1 V in that frame, and the
/// image network between the image bases. Drawn independently of the doubling
/// code (whose mirrored frames give −V), so it checks rather than repeats it.
fn pair_drawn_doubled(n: u32, network: impl Fn(u32) -> String) -> String {
    format!(
        "CE\nGW 1 {m} 0 0 -2.5 0 0 2.5 0.001\nGW 2 {m} 5 0 -2.5 5 0 2.5 0.001\nGE 0\n\
         {orig}{image}EX 0 1 {up} 0 1.0 0.0\nEX 0 1 {n} 0 1.0 0.0\nFR 0 1 0 0 30 0\nEN\n",
        m = 2 * n,
        up = n + 1,
        orig = network(n + 1),
        image = network(n),
    )
}

/// A network card for the ports at one segment of each monopole.
type NetworkCard = fn(u32) -> String;

/// A TL or NT on wires touching PEC ground: the contact deck's feed impedance
/// equals the hand-drawn free-space double's — crossed line and shunts included.
#[test]
fn a_network_on_contact_equals_its_drawn_double() {
    let cards: [(&str, NetworkCard); 4] = [
        ("TL", |s| format!("TL 1 {s} 2 {s} 50 0\n")),
        ("crossed TL", |s| format!("TL 1 {s} 2 {s} -50 0\n")),
        ("TL with shunts", |s| {
            format!("TL 1 {s} 2 {s} 50 0 0.002 0.01 0 -0.02\n")
        }),
        ("NT", |s| {
            format!("NT 1 {s} 2 {s} 0.01 -0.02 -0.005 0.01 0.01 -0.02\n")
        }),
    ];
    for (label, card) in cards {
        let on_ground = feed_z(&pair(21, &card(1)), (1, 1)).unwrap();
        let drawn = feed_z(&pair_drawn_doubled(21, card), (1, 22)).unwrap();
        println!("{label}: contact {on_ground:.4}, drawn double {drawn:.4}");
        assert!(
            (on_ground - drawn).norm() <= 1e-9 * drawn.norm(),
            "{label}: contact {on_ground} vs drawn double {drawn}"
        );
    }
}

/// Against nec2c 1.3.1 (captured 2026-10-03, 30 MHz, `GN 1`, echo checked). The
/// same pair without a network is 2.39 → 1.48 Ω off at 21 → 41 segments; each
/// network deck is closer than that, and converging. Kill criterion: under the
/// no-network error at 41, shrinking.
#[test]
fn networks_on_contact_track_nec2c() {
    for (label, card, z21, z41) in [
        (
            "TL",
            "TL 1 1 2 1 50 0\n",
            (24.220, 19.630),
            (24.361, 19.668),
        ),
        (
            "crossed TL",
            "TL 1 1 2 1 -50 0\n",
            (16.027, 3.9332),
            (16.042, 3.9711),
        ),
        (
            "NT",
            "NT 1 1 2 1 0.01 -0.02 -0.005 0.01 0.01 -0.02\n",
            (17.429, 14.286),
            (17.438, 14.323),
        ),
    ] {
        let e21 = (feed_z(&pair(21, card), (1, 1)).unwrap() - Complex64::new(z21.0, z21.1)).norm();
        let e41 = (feed_z(&pair(41, card), (1, 1)).unwrap() - Complex64::new(z41.0, z41.1)).norm();
        println!("{label}: {e21:.3} -> {e41:.3} Ω");
        assert!(e41 < 1.48 && e41 < e21, "{label}: {e21:.3} -> {e41:.3} Ω");
    }
}

/// Still refused, as everywhere (FND-123): a current source with a network.
#[test]
fn a_current_source_with_a_network_is_still_refused() {
    let text = pair(21, "TL 1 1 2 1 50 0\n").replace("EX 0 1 1 0", "EX 4 1 1 0");
    let err = feed_z(&text, (1, 1)).unwrap_err();
    assert!(
        err.contains("current source") && err.contains("TL/NT"),
        "{err}"
    );
}
