// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH10-CHK-002 — the `GS` card (scale the structure), as nec2c reads it.
//!
//! nec2c's `datagn` multiplies every segment defined so far — ends and radius,
//! `GM`/`GR` copies included — by `GS`'s `F1`; wires after the card are not
//! scaled, two cards compound, and `I1`/`I2` are ignored. Refused since FND-236,
//! so a deck written in feet stopped solving. Each case is held two ways: the
//! `GS` deck must equal its hand-scaled equivalent in fnec (to round-off), and
//! nec2c 1.3.1 gives the two the same answer (captured 2026-10-14). Where fnec's
//! own answer is compared with nec2c it is against nec2c's converged value
//! (9× mesh, `scripts/nec2c-fine-mesh.py`, which passes `GS` through), and the
//! assertion is that refining approaches it: the 21-segment offset itself is the
//! Hallén dipole's (3.4 % here), not this card's.
use std::process::Command;

/// `fnec --solver hallen` on `deck`: Ok(feedpoint Z) or Err(stderr).
fn fnec(deck: &str) -> Result<(f64, f64), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-gs-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen"])
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let row: Vec<f64> = stdout
        .lines()
        .skip_while(|l| *l != "FEEDPOINTS")
        .nth(2)
        .expect("feedpoint row")
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    Ok((row[6], row[7]))
}

fn solves(deck: &str) -> (f64, f64) {
    fnec(deck).unwrap_or_else(|e| panic!("{deck}\nwas refused: {e}"))
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Equal to round-off: the scaled ends are `x·s`, the hand-scaled ones `(x·s)`
/// typed as a literal, so the two decks differ only in the last bits.
fn same(a: (f64, f64), b: (f64, f64), what: &str) {
    assert!(
        dist(a, b) <= 1e-9 * a.0.hypot(a.1),
        "{what}: {a:?} vs {b:?}"
    );
}

/// `n` segments a wire, fed on the segment whose midpoint is the deck's
/// segment 11 midpoint at `n / 21` times the mesh.
fn deck(wires: &str, n: u32) -> String {
    let s = n / 21;
    let feed = 11 * s - (s - 1) / 2;
    format!(
        "CE\n{}\nGE 0\nEX 0 1 {feed} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
        wires.replace("{n}", &n.to_string())
    )
}

/// Refining from 21 to 63 segments must approach nec2c's converged value. No
/// bound on the 21-segment distance: that is the Hallén solver's own offset
/// (3.4 % for the dipole, 6.8 % for the coupled pair), gated by its own tests;
/// what this card can break is the geometry, which the equivalences hold.
fn converges(wires: &str, reference: (f64, f64), what: &str) {
    let (z1, z3) = (solves(&deck(wires, 21)), solves(&deck(wires, 63)));
    let (e1, e3) = (dist(z1, reference), dist(z3, reference));
    assert!(
        e3 < e1,
        "{what}: refining moved away from nec2c: {z1:?} {z3:?} vs {reference:?}"
    );
}

/// A half-wave dipole written in feet: nec2c reads `GS 0 0 .3048` as the metres
/// deck, 79.042 + j45.909 Ω for both at 21 segments, 79.588 + j46.578 at 9×.
/// The radius is scaled too: unscaled, 0.00328 m would be a different antenna.
#[test]
fn a_deck_in_feet_scaled_by_gs_is_the_metres_deck() {
    let feet = "GW 1 {n} 0 0 -17.33 0 0 17.33 .00328\nGS 0 0 .3048";
    let metres = "GW 1 {n} 0 0 -5.282184 0 0 5.282184 .000999744";
    for n in [21, 63] {
        same(
            solves(&deck(feet, n)),
            solves(&deck(metres, n)),
            "feet vs metres",
        );
    }
    converges(feet, (79.588, 46.578), "feet");
    // The radius is part of the scale: a deck scaled without it differs.
    let unscaled_radius = "GW 1 {n} 0 0 -5.282184 0 0 5.282184 .00328";
    assert!(dist(solves(&deck(feet, 21)), solves(&deck(unscaled_radius, 21))) > 1.0);
}

/// A wire after the card is not scaled: nec2c 21.821 + j62.867 at 21 segments
/// (22.433 + j64.384 at 9×) for the driven 2.64 m half scaled to 5.28 m beside a
/// parasite written in metres.
#[test]
fn gs_scales_only_the_segments_defined_before_it() {
    let scaled = "GW 1 {n} 0 0 -2.64 0 0 2.64 .0005\nGS 0 0 2\nGW 2 {n} 2 0 -5.28 2 0 5.28 .001";
    let by_hand = "GW 1 {n} 0 0 -5.28 0 0 5.28 .001\nGW 2 {n} 2 0 -5.28 2 0 5.28 .001";
    same(
        solves(&deck(scaled, 21)),
        solves(&deck(by_hand, 21)),
        "GS between two wires",
    );
    converges(scaled, (22.433, 64.384), "GS between two wires");
}

/// Two cards compound (nec2c: GS 2 then GS 1.5 is the ×3 deck, 78.942 + j45.277),
/// a factor of 1 changes nothing, and I1/I2 are ignored as nec2c ignores them.
#[test]
fn gs_cards_compound_and_their_integer_fields_are_ignored() {
    let x3 = deck("GW 1 {n} 0 0 -5.28 0 0 5.28 .001", 21);
    let twice = deck(
        "GW 1 {n} 0 0 -1.76 0 0 1.76 .000333333333333\nGS 0 0 2\nGS 0 0 1.5",
        21,
    );
    same(solves(&twice), solves(&x3), "GS 2 then GS 1.5");
    let plain = deck("GW 1 {n} 0 0 -2.64 0 0 2.64 .0005", 21);
    let one = deck("GW 1 {n} 0 0 -2.64 0 0 2.64 .0005\nGS 0 0 1", 21);
    assert_eq!(solves(&one), solves(&plain), "GS 0 0 1 changed the answer");
    let fields = deck("GW 1 {n} 0 0 -2.64 0 0 2.64 .0005\nGS 3 7 2", 21);
    let zeros = deck("GW 1 {n} 0 0 -2.64 0 0 2.64 .0005\nGS 0 0 2", 21);
    assert_eq!(
        solves(&fields),
        solves(&zeros),
        "GS I1/I2 changed the answer"
    );
}

/// A GM copy made before the card is scaled with everything else; a GM after it
/// translates in the new units. nec2c gives the GS deck and its hand-expanded
/// equivalent the same 6.0368 + j35.926 Ω.
#[test]
fn gm_copies_before_gs_are_scaled_and_later_moves_are_in_the_new_units() {
    let scaled =
        "GW 1 {n} 0 0 -2.64 0 0 2.64 .0005\nGM 1 1 0 0 0 1 0 0\nGS 0 0 2\nGM 2 1 0 0 0 1 0 0";
    let by_hand =
        "GW 1 {n} 0 0 -5.28 0 0 5.28 .001\nGW 2 {n} 2 0 -5.28 2 0 5.28 .001\nGM 2 1 0 0 0 1 0 0";
    same(
        solves(&deck(scaled, 21)),
        solves(&deck(by_hand, 21)),
        "GM / GS / GM",
    );
}

/// What nec2c stops on is refused, by name: a factor that is not positive (its
/// "SEGMENT DATA ERROR" at GE; a missing F1 reads as 0 there) and a geometry
/// card after GE ("FAULTY DATA CARD LABEL AFTER GEOMETRY SECTION"), where fnec
/// used to solve the extra wire (FND-245).
#[test]
fn what_nec2c_stops_on_is_refused_by_name() {
    let wire = "CE\nGW 1 21 0 0 -2.64 0 0 2.64 .0005\n";
    let tail = "EX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    for (cards, needle) in [
        ("GS 0 0 0\nGE 0\n", "must be positive"),
        ("GS 0 0 -1\nGE 0\n", "must be positive"),
        ("GS 0 0\nGE 0\n", "GS needs 3 fields"),
        ("GE 0\nGS 0 0 2\n", "GS after GE"),
        ("GE 0\nGW 2 21 2 0 -2.64 2 0 2.64 .0005\n", "GW after GE"),
        ("GE 0\nGE 0\n", "GE after GE"),
    ] {
        let e = fnec(&format!("{wire}{cards}{tail}")).expect_err(cards);
        assert!(e.contains(needle), "{cards}: {e}");
    }
    // The negative control: the same cards in NEC order solve.
    solves(&format!("{wire}GS 0 0 2\nGE 0\n{tail}"));
    solves(&format!(
        "{wire}GW 2 21 2 0 -2.64 2 0 2.64 .0005\nGE 0\n{tail}"
    ));
}
