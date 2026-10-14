// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-247 — the `GR` card as NEC-2 reads it.
//!
//! `GR I1 I2`: the structure so far occurs `I2` times in all, the original
//! included, each occurrence rotated 360°/`I2` about z from the one before, tags
//! `+ I1` per occurrence (tag 0 kept); NEC-2's card has no angle, and nec2c
//! ignores a third field. fnec read `I2` as *additional* copies at an `F1` angle,
//! so a radial ground plane written `GR 1 4` was refused for a missing field, and
//! written `GR 1 4 90` solved five overlapping radial sets to −24.26 − j3702 Ω at
//! exit 0 (nec2c: 22.771 + j4.4545). Each case is held two ways: the `GR` deck
//! equals its written-out equivalent in fnec, and nec2c 1.3.1 prints the same
//! digits for both (captured 2026-10-14).
use std::process::Command;

/// `fnec --solver hallen` on `deck`: Ok(feedpoint Z) or Err(stderr).
fn fnec(deck: &str) -> Result<(f64, f64), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-gr-{}-{:?}",
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

/// A rotation by a non-quarter angle is exact to round-off only, so the
/// written-out coordinates (typed to 15 digits) differ in the last bits.
fn same(a: (f64, f64), b: (f64, f64), what: &str) {
    let d = (a.0 - b.0).hypot(a.1 - b.1);
    assert!(d <= 1e-9 * a.0.hypot(a.1), "{what}: {a:?} vs {b:?}");
}

fn deck(geometry: &str, ex: &str) -> String {
    format!("CE\n{geometry}\nGE 0\n{ex}\nFR 0 1 0 0 14.2 0\nEN\n")
}

const RADIAL: &str = "GW 2 11 0 0 1 5.3 0 1 .001";
const VERTICAL: &str = "GW 1 11 0 0 1 0 0 6.28 .001";

/// Four radials from one: `GR 1 4`, with the third field absent, padded or
/// agreeing (90°), all equal the radials written out — nec2c 22.771 + j4.4545
/// for every one of them.
#[test]
fn four_radials_from_one_as_nec2c_builds_them() {
    let written = deck(
        &format!(
            "{RADIAL}\nGW 3 11 0 0 1 0 5.3 1 .001\nGW 4 11 0 0 1 -5.3 0 1 .001\n\
             GW 5 11 0 0 1 0 -5.3 1 .001\n{VERTICAL}"
        ),
        "EX 0 1 1 0 1 0",
    );
    let z = solves(&written);
    for gr in ["GR 1 4", "GR 1 4 90", "GR 1 4 0", "GR 1 4.0 90.0"] {
        let g = solves(&deck(
            &format!("{RADIAL}\n{gr}\n{VERTICAL}"),
            "EX 0 1 1 0 1 0",
        ));
        same(g, z, gr);
    }
}

/// Tags go up by `I1` per occurrence: three dipoles from `GR 2 3` are tags 1, 3
/// and 5. A parasite after the card, near the second only, breaks the rotational
/// symmetry that would otherwise make every tag read the same: nec2c gives
/// 0.51013 + j26.137 fed on tag 3 and 8.4176 + j45.221 on tag 5, and the
/// written-out deck the same on tag 3.
#[test]
fn tags_increase_per_occurrence() {
    let dipole = "GW 1 11 1 0 -5.28 1 0 5.28 .001";
    let parasite = "GW 9 11 -0.2 2 -5.28 -0.2 2 5.28 .001";
    let gr = |tag: u32| {
        solves(&deck(
            &format!("{dipole}\nGR 2 3\n{parasite}"),
            &format!("EX 0 {tag} 6 0 1 0"),
        ))
    };
    let written = solves(&deck(
        &format!(
            "{dipole}\nGW 3 11 -0.5 0.866025403784439 -5.28 -0.5 0.866025403784439 5.28 .001\n\
             GW 5 11 -0.5 -0.866025403784439 -5.28 -0.5 -0.866025403784439 5.28 .001\n{parasite}"
        ),
        "EX 0 3 6 0 1 0",
    ));
    same(gr(3), written, "GR 2 3, tag 3, vs written out");
    let (z3, z5) = (gr(3), gr(5));
    assert!(
        (z3.0 - z5.0).hypot(z3.1 - z5.1) > 1.0,
        "tags 3 and 5 read alike: {z3:?} {z5:?}"
    );
    // GR 1 1 is one occurrence: no copy, so tag 2 names nothing.
    let e = refused(&deck(&format!("{dipole}\nGR 1 1"), "EX 0 2 6 0 1 0"));
    assert!(e.contains("no segment with tag 2"), "{e}");
}

/// The corpus deck that was written in fnec's old reading is the explicit
/// two-dipole deck under NEC-2's (`GR 1 2`; nec2c 163.20 + j68.53).
#[test]
fn the_corpus_gr_deck_is_the_explicit_pair() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let gr = std::fs::read_to_string(root.join("multi-source-gr-180.nec")).expect("deck");
    let explicit = std::fs::read_to_string(root.join("multi-source.nec")).expect("deck");
    assert!(gr.contains("GR 1 2\n"), "{gr}");
    same(
        solves(&gr),
        solves(&explicit),
        "corpus GR deck vs multi-source.nec",
    );
}

/// Refused by name: what nec2c stops on (no occurrences, a negative count, a
/// fractional field), a negative increment (nec2c makes tags 1, 0, −1), and a
/// third field that is not 0 or 360/`I2` — fnec's old reading, which nec2c
/// would silently ignore; the message gives the structure's NEC-2 spelling.
#[test]
fn what_nec2c_stops_on_and_the_old_reading_are_refused_by_name() {
    let geometry = |gr: &str| format!("{RADIAL}\n{gr}\n{VERTICAL}");
    for (gr, needle) in [
        ("GR 1 0", "at least 1"),
        ("GR 1 -2", "at least 1"),
        ("GR 1 2.5", "an integer field"),
        ("GR -1 3", "negative tag increment"),
        ("GR 1 1 180", "that structure is `GR 1 2`"),
        ("GR 1 3 90", "that structure is `GR 1 4`"),
        ("GR 1 2 45", "that structure is `GM 1 2 0 0 45 0 0 0`"),
        ("GR 1 4 45", "NEC-2's GR has no angle"),
    ] {
        let e = refused(&deck(&geometry(gr), "EX 0 1 1 0 1 0"));
        assert!(e.contains(needle), "{gr}: {e}");
    }
    // The negative controls: an agreeing angle solves.
    solves(&deck(&geometry("GR 1 3 120"), "EX 0 1 1 0 1 0"));
    solves(&deck(&geometry("GR 1 4 90"), "EX 0 1 1 0 1 0"));
}
