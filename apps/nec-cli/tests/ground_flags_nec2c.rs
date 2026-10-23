// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-241 / FND-242 / FND-243 — what `GE` and `GN` mean, as nec2c reads them.
//!
//! nec2c takes the ground from the last `GN` card alone; `GE I1` only decides
//! whether a wire end at z = 0 is joined to its image (1) or left open (0, -1),
//! and "at z = 0" is within 10⁻³ of the segment's length. fnec read `GE 1` with no
//! `GN` as a perfect ground (nec2c: free space), solved `GN 3` in free space with a
//! warning, solved a wire standing on the ground joined whatever `GE` said (nec2c:
//! an open end, 58 − j7611 Ω), and counted a base 1 µm up as hanging over the
//! plane (29.26 − j5497 Ω; nec2c 39.88 + j23.11). The references below are nec2c
//! 1.3.1, captured 2026-10-14.
use std::process::Command;

/// `fnec --solver hallen` on `deck`: Ok((feedpoint Z, stderr)) or Err(stderr).
fn fnec(deck: &str) -> Result<((f64, f64), String), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-ground-flags-{}-{:?}",
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
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return Err(stderr);
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
    Ok(((row[6], row[7]), stderr))
}

fn solves(deck: &str) -> (f64, f64) {
    fnec(deck)
        .unwrap_or_else(|e| panic!("{deck}\nwas refused: {e}"))
        .0
}

fn refused(deck: &str) -> String {
    match fnec(deck) {
        Err(e) => e,
        Ok((z, _)) => panic!("{deck}\nsolved {z:?}, expected a refusal"),
    }
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// A 5.28 m vertical from z = 3 m, 21 segments, centre-fed, 28.4 MHz.
fn raised(cards: &str) -> String {
    format!("CE\nGW 1 21 0 0 3 0 0 8.28 .001\n{cards}\nEX 0 1 11 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n")
}

/// A 2.64 m monopole whose base is at `z0`, `n` segments, base-fed, 28.4 MHz.
fn monopole(z0: f64, n: u32, cards: &str) -> String {
    format!("CE\nGW 1 {n} 0 0 {z0} 0 0 2.64 .001\n{cards}\nEX 0 1 1 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n")
}

const GN2: &str = "GN 2 0 0 0 13 .005";

/// nec2c solves the raised vertical over the GN ground whatever GE says
/// (75.987 + j46.842 over GN 1, 77.21 + j47.07 over GN 2, for GE 0, -1 and 1);
/// fnec must not move either, and must not warn about the flag.
#[test]
fn ge_does_not_select_the_ground_the_gn_card_does() {
    for gn in ["GN 1", GN2] {
        let joined = solves(&raised(&format!("GE 1\n{gn}")));
        for ge in ["GE 0", "GE -1"] {
            let (z, stderr) = fnec(&raised(&format!("{ge}\n{gn}"))).expect("solves");
            assert_eq!(z, joined, "{ge} / {gn} moved the answer of a raised wire");
            assert!(!stderr.contains("GE I1"), "{ge} / {gn}: {stderr}");
        }
    }
    // The ground is there: free space reads otherwise (nec2c 79.68 + j45.60).
    let free = solves(&raised("GE 0"));
    assert!(dist(free, solves(&raised("GE 0\nGN 1"))) > 1.0);
}

/// nec2c keeps the last GN, and GN -1 resets to free space.
#[test]
fn the_last_gn_card_is_the_ground() {
    let pec = solves(&raised("GE 1\nGN 1"));
    assert_eq!(solves(&raised(&format!("GE 1\n{GN2}\nGN 1"))), pec);
    assert_eq!(
        solves(&raised("GE 0\nGN 1\nGN -1")),
        solves(&raised("GE 0"))
    );
}

/// NEC-2 defines GE -1..=1 and GN -1..=2; nec2c reads other values by code
/// (GE by sign; GN 3/4 as perfect-ground currents under a finite-ground
/// pattern). And GE 1 with no ground is self-contradictory: nec2c solves free
/// space joining z = 0 ends to an absent image (a monopole: 16.28 − j1905).
#[test]
fn undefined_ground_values_are_refused_by_name() {
    for (cards, needle) in [
        ("GE 2\nGN 1", "not a NEC-2 ground flag"),
        ("GE -2\nGN 1", "not a NEC-2 ground flag"),
        ("GE 1\nGN 3 0 0 0 13 .005", "not a NEC-2 ground type"),
        ("GE 1\nGN 4", "not a NEC-2 ground type"),
        ("GE 1\nGN -2", "not a NEC-2 ground type"),
        ("GE 1", "no GN card defines one"),
        ("GE 1\nGN 1\nGN -1", "no GN card defines one"),
    ] {
        let e = refused(&raised(cards));
        assert!(e.contains(needle), "{cards}: {e}");
    }
    // Each remedy the GE 1 message names solves.
    let e = refused(&raised("GE 1"));
    assert!(e.contains("Add `GN 1`") && e.contains("set `GE 0`"), "{e}");
    solves(&raised("GE 1\nGN 1"));
    solves(&raised("GE 0"));
}

/// nec2c leaves an end at the ground open under GE 0 / -1 (and refuses a deck
/// with no GE): a quarter-wave monopole reads 58.04 − j7610.7 Ω at 21 segments
/// and 56.32 − j15556 at 63 — no antenna. fnec solved it joined. Refused now, and
/// the remedy the message names converges to nec2c (39.88 + j23.11 at 21,
/// 40.08 + j23.21 at 63).
#[test]
fn an_unjoined_ground_contact_is_refused_and_its_remedy_converges_to_nec2c() {
    // Over a finite ground too, and first: FND-240's refusal names `GN 1` as a
    // remedy, which this one would then refuse in turn.
    for cards in [
        "GE 0\nGN 1",
        "GE -1\nGN 1",
        "GN 1",
        "GE 0\nGN 2 0 0 0 13 .005",
    ] {
        let e = refused(&monopole(0.0, 21, cards));
        assert!(e.contains("ends on the ground plane"), "{cards}: {e}");
        assert!(e.contains("Set `GE 1`"), "{cards}: {e}");
    }
    let (r21, r63): ((f64, f64), (f64, f64)) = ((39.88, 23.11), (40.08, 23.21));
    let z21 = solves(&monopole(0.0, 21, "GE 1\nGN 1"));
    let z63 = solves(&monopole(0.0, 63, "GE 1\nGN 1"));
    let scale = r63.0.hypot(r63.1);
    assert!(dist(z21, r21) < 0.03 * scale, "21: {z21:?} vs {r21:?}");
    assert!(dist(z63, r63) < 0.012 * scale, "63: {z63:?} vs {r63:?}");
    assert!(
        dist(z63, r63) < dist(z21, r21),
        "refining moved away from nec2c"
    );
    // The message quotes fnec's own 21-segment answer; keep it true.
    let e = refused(&monopole(0.0, 21, "GE 0\nGN 1"));
    assert!(
        e.contains(&format!("fnec {:.2} + j{:.2}", z21.0, z21.1)),
        "{z21:?}: {e}"
    );
}

/// nec2c's contact band: an end within 10⁻³ of its segment's length of z = 0 is
/// on the ground, snapped to it under GE 1 (39.88 + j23.11 for a base at 1 µm or
/// 0.1 mm, as at 0). fnec used 1 nm: 29.26 − j5497 Ω at 1 µm, exit 0.
#[test]
fn an_end_within_nec2c_contact_band_is_on_the_ground() {
    let on = solves(&monopole(0.0, 21, "GE 1\nGN 1"));
    for z0 in [1e-6, 1e-4, -1e-4] {
        let z = solves(&monopole(z0, 21, "GE 1\nGN 1"));
        assert!(
            dist(z, on) < 1e-3 * on.0.hypot(on.1),
            "base {z0}: {z:?} vs {on:?}"
        );
        // Unjoined, the same base is refused, not solved hanging.
        let e = refused(&monopole(z0, 21, "GE 0\nGN 1"));
        assert!(e.contains("ends on the ground plane"), "{z0}: {e}");
        // And over a finite ground it is the contact FND-240 refuses.
        let e = refused(&monopole(z0, 21, &format!("GE 1\n{GN2}")));
        assert!(
            e.contains("touches or crosses the finite ground"),
            "{z0}: {e}"
        );
    }
    // Outside the band (2.64/21 m segments: 1.26e-4 m) the end is not snapped,
    // and not refused when unjoined either: both sides of the band, on both
    // routes. The unjoined end is split into thirds (FND-227), so a band measured
    // on the wrong length, or widened, moves only that route's edge.
    let off = solves(&monopole(2e-4, 21, "GE 1\nGN 1"));
    assert!(dist(off, on) > 100.0, "a base 0.2 mm up snapped: {off:?}");
    solves(&monopole(2e-4, 21, "GE 0\nGN 1"));
}

/// A horizontal 5 m dipole 5 m up, 21 segments, centre-fed, 29.9 MHz.
fn horizontal(gn: &str) -> String {
    format!(
        "CE\nGW 1 21 -2.5 0 5 2.5 0 5 .001\nGE 0\n{gn}\nEX 0 1 11 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n"
    )
}

/// FND-283: GN's radial-wire screen (I2 = NRADL, F3/F4) changes nec2c's
/// currents — 75.133 + j30.984 → 71.558 + j24.854 Ω with 16 radials of 20 m
/// over 13/0.005 — and fnec read neither, solving 74.891 + j28.357 for both.
/// Refused over GN 0; over GN 2 nec2c itself stops (exit 255, "RADIAL WIRE G.S.
/// APPROXIMATION MAY NOT BE USED WITH SOMMERFELD GROUND OPTION").
#[test]
fn a_radial_screen_over_a_finite_ground_is_refused() {
    let e = refused(&horizontal("GN 0 16 0 0 13 .005 20 .001"));
    assert!(
        e.contains("16 wires") && e.contains("--solver mpie") && e.contains("field 2"),
        "{e}"
    );
    let e = refused(&horizontal("GN 2 16 0 0 13 .005 20 .001"));
    assert!(e.contains("RADIAL WIRE G.S."), "{e}");
    // Control: the same ground without the screen solves.
    solves(&horizontal("GN 0 0 0 0 13 .005"));
}

/// What nec2c ignores, fnec ignores, with a warning, and the answer does not
/// move: nec2c gives 73.487 + j22.724 for GN 1 with and without
/// `16 0 0 0 0 20 .001`, 79.004 + j41.448 (free space) for GN −1 with them,
/// 75.133 + j30.984 for GN 0 with I3/I4 set or a second medium in F3–F6.
#[test]
fn gn_fields_nec2c_ignores_leave_the_answer_unchanged() {
    for (plain, with, needle) in [
        ("GN 1", "GN 1 16 0 0 0 0 20 .001", "over a perfect ground"),
        ("GN -1", "GN -1 16 0 0 13 .005", "in free space"),
        (
            "GN 0 0 0 0 13 .005",
            "GN 0 0 3 4 13 .005",
            "never reads GN I3/I4",
        ),
        (
            "GN 0 0 0 0 13 .005",
            "GN 0 0 0 0 13 .005 20 .001 5 2",
            "second medium",
        ),
    ] {
        let want = solves(&horizontal(plain));
        let (got, stderr) = fnec(&horizontal(with)).unwrap_or_else(|e| panic!("{with}: {e}"));
        assert_eq!(got, want, "{with} vs {plain}");
        assert!(stderr.contains(needle), "{with}: {stderr}");
    }
}
