// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH10-CHK-002 — the `GX` card (reflect the structure), as nec2c reads it.
//!
//! `GX I1 I2`: `I2`'s digits `XYZ` select planes; nec2c's `reflc` reflects z,
//! then y, then x, each pass copying every segment present and tagging the
//! copies `tag + I1` (tag 0 stays 0), with the increment doubling per pass.
//! Refused since FND-236, so half an antenna written with `GX` stopped solving.
//! Each case is held two ways: the `GX` deck must equal its written-out
//! equivalent in fnec, and nec2c 1.3.1 gives the two the same answer to every
//! printed digit (captured 2026-10-14; nec2c's symmetry-exploiting solve does not
//! change its numbers, which Fable's review checked across the whole current
//! table). The written-out decks are gated against nec2c by the solver's own
//! tests; what this card can break is the geometry.
use std::process::Command;

/// `fnec --solver hallen` on `deck`: Ok(feedpoint Z) or Err(stderr).
fn fnec(deck: &str) -> Result<(f64, f64), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-gx-{}-{:?}",
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

fn refused(deck: &str) -> String {
    match fnec(deck) {
        Err(e) => e,
        Ok(z) => panic!("{deck}\nsolved {z:?}, expected a refusal"),
    }
}

/// Mirroring is exact (a sign flip), so the two decks are the same numbers.
fn same(a: (f64, f64), b: (f64, f64), what: &str) {
    let d = (a.0 - b.0).hypot(a.1 - b.1);
    assert!(d <= 1e-9 * a.0.hypot(a.1), "{what}: {a:?} vs {b:?}");
}

fn deck(geometry: &str, ex: &str) -> String {
    format!("CE\n{geometry}\nGE 0\n{ex}\nFR 0 1 0 0 14.2 0\nEN\n")
}

/// Half a dipole on z ≥ 0 reflected in z = 0 is the dipole: the copy is
/// appended, so the feed beside the centre is the original's segment 1 (the
/// full deck's segment 11). nec2c: 79.432 + j45.487 for both. An even segment
/// count has no centre segment — an odd one would straddle the plane and be
/// refused.
#[test]
fn half_a_dipole_reflected_in_z_is_the_dipole() {
    let half = deck("GW 1 10 0 0 0 0 0 5.28 .001\nGX 0 001", "EX 0 1 1 0 1 0");
    let full = deck("GW 1 20 0 0 -5.28 0 0 5.28 .001", "EX 0 1 11 0 1 0");
    same(solves(&half), solves(&full), "half + GX 001 vs the dipole");
}

/// Two planes give four copies, tagged as nec2c tags them: the y copy +1, then
/// the x copies of both +2. The array is spaced unequally (1 m in x, 2 m in y)
/// and fed on tags 1, 2 (at 2 V) and 3, so which wire each tag names shows in
/// tag 1's impedance: nec2c reads 46.448 − j858.91 for the GX deck and the four
/// wires written out, 50.506 − j869.40 had the planes run x then y, and
/// 41.654 − j848.59 had the increment not doubled. (Equal feeds hid the order:
/// the fed set came out the same.)
#[test]
fn two_planes_copy_and_tag_as_nec2c_does() {
    let ex = "EX 0 1 11 0 1 0\nEX 0 2 11 0 2 0\nEX 0 3 11 0 1 0";
    let gx = deck("GW 1 21 1 2 -2.64 1 2 2.64 .001\nGX 1 110", ex);
    let written = deck(
        "GW 1 21 1 2 -2.64 1 2 2.64 .001\nGW 2 21 1 -2 -2.64 1 -2 2.64 .001\n\
         GW 3 21 -1 2 -2.64 -1 2 2.64 .001\nGW 4 21 -1 -2 -2.64 -1 -2 2.64 .001",
        ex,
    );
    same(solves(&gx), solves(&written), "GX 1 110 vs four wires");
}

/// Tag 0 stays 0 whatever the increment, as in nec2c (13.211 − j828.18 for
/// both, fed by absolute segment 11).
#[test]
fn a_tag_zero_copy_keeps_tag_zero() {
    let gx = deck(
        "GW 0 21 1 0 -2.64 1 0 2.64 .001\nGX 5 100",
        "EX 0 0 11 0 1 0",
    );
    let written = deck(
        "GW 0 21 1 0 -2.64 1 0 2.64 .001\nGW 0 21 -1 0 -2.64 -1 0 2.64 .001",
        "EX 0 0 11 0 1 0",
    );
    same(solves(&gx), solves(&written), "tag-0 GX vs two tag-0 wires");
    // Fed by absolute segment, a retagged copy would solve the same; feeding the
    // tag it would have been given tells the two apart.
    let e = refused(&deck(
        "GW 0 21 1 0 -2.64 1 0 2.64 .001\nGX 5 100",
        "EX 0 5 11 0 1 0",
    ));
    assert!(e.contains("no segment with tag 5"), "{e}");
}

/// GX and GS commute here (nec2c: 79.432 + j45.487 either way, the 5.28 m
/// dipole), a GX with no plane changes nothing, and a wire after a GX is legal.
#[test]
fn gx_composes_with_gs_and_the_other_cards() {
    let full = solves(&deck("GW 1 20 0 0 -5.28 0 0 5.28 .001", "EX 0 1 11 0 1 0"));
    for order in ["GS 0 0 2\nGX 0 001", "GX 0 001\nGS 0 0 2"] {
        let z = solves(&deck(
            &format!("GW 1 10 0 0 0 0 0 2.64 .0005\n{order}"),
            "EX 0 1 1 0 1 0",
        ));
        same(z, full, order);
    }
    let plain = deck("GW 1 21 0 0 -2.64 0 0 2.64 .001", "EX 0 1 11 0 1 0");
    let noop = deck("GW 1 21 0 0 -2.64 0 0 2.64 .001\nGX 0 0", "EX 0 1 11 0 1 0");
    assert_eq!(solves(&noop), solves(&plain), "GX 0 0 changed the answer");
    let after = deck(
        "GW 1 21 1 0 -2.64 1 0 2.64 .001\nGX 1 100\nGW 3 21 0 2 -2.64 0 2 2.64 .001",
        "EX 0 1 11 0 1 0",
    );
    let after_written = deck(
        "GW 1 21 1 0 -2.64 1 0 2.64 .001\nGW 2 21 -1 0 -2.64 -1 0 2.64 .001\n\
         GW 3 21 0 2 -2.64 0 2 2.64 .001",
        "EX 0 1 11 0 1 0",
    );
    same(solves(&after), solves(&after_written), "GW after GX");
}

/// Refused by name: a segment in or across its plane (nec2c: "LIES IN PLANE OF
/// SYMMETRY"; fnec refuses every crossing, where nec2c lets a small one through
/// to overlapping copies), non-integer, negative or out-of-range fields, GX after
/// GE (FND-245), and a z-reflection over a ground, which buries the copy (nec2c:
/// "EXTENDS BELOW GROUND"). Each beside the deck that solves.
#[test]
fn what_nec2c_stops_on_is_refused_by_name() {
    let dipole = "GW 1 21 0 0 -2.64 0 0 2.64 .001";
    let ex = "EX 0 1 11 0 1 0";
    for (geometry, needle) in [
        // A crossing nec2c lets through (|z| 0.5 mm each side): it then solves
        // overlapping copies and prints -NaN. fnec refuses every crossing.
        (
            "GW 1 21 1 0 1 1 0 6.28 .001\nGW 2 1 3 0 -.0005 3 0 .0005 .0001\nGX 0 001".into(),
            "segment 22 (tag 2) lies in or crosses the plane z = 0",
        ),
        (
            format!("{dipole}\nGX 0 100"),
            "lies in or crosses the plane x = 0",
        ),
        (
            format!("{dipole}\nGX 0 001"),
            "lies in or crosses the plane z = 0",
        ),
        (
            "GW 1 21 1 0 -2.64 1 0 2.64 .001\nGX 0 1000".into(),
            "GX field 2 = '1000'",
        ),
        (
            "GW 1 21 1 0 -2.64 1 0 2.64 .001\nGX 0 -1".into(),
            "GX field 2 = '-1'",
        ),
        (
            "GW 1 21 1 0 -2.64 1 0 2.64 .001\nGX -1 100".into(),
            "GX field 1 = '-1'",
        ),
        (
            "GW 1 21 1 0 -2.64 1 0 2.64 .001\nGX 0 1.5".into(),
            "GX field 2 = '1.5': an integer field",
        ),
    ] {
        let e = refused(&deck(&geometry, ex));
        assert!(e.contains(needle), "{geometry}: {e}");
    }
    let e = refused(&format!(
        "CE\nGW 1 21 1 0 -2.64 1 0 2.64 .001\nGE 0\nGX 0 100\n{ex}\nFR 0 1 0 0 14.2 0\nEN\n"
    ));
    assert!(e.contains("GX after GE"), "{e}");
    let e = refused(
        "CE\nGW 1 10 0 0 1 0 0 3 .001\nGX 0 001\nGE 1\nGN 1\nEX 0 1 5 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    assert!(e.contains("below the ground plane"), "{e}");
    // The negative control: the plane the wire does not touch reflects fine.
    solves(&deck("GW 1 21 1 0 -2.64 1 0 2.64 .001\nGX 0 100", ex));
}
