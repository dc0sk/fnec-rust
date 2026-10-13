// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-238 / FND-239 — `--solver mpie` over a ground, close to it.
//!
//! The reflected term of a bent or non-horizontal wire was a kernel evaluated
//! once per segment pair at the midpoints, which is only right where the image is
//! far away next to the segment length. An L (a vertical and a radial meeting at
//! 0.05 m, 0.25 m segments) read 53.4 − j4766.9 Ω where nec2c gives 39.96 + j22.69,
//! and a vertical dipole 0.05 m up read 30 % high in X. Over a perfect ground the
//! term is now the exact image through the free-space fill (FND-238); over a
//! finite one, `K ×` that image plus a remainder tabulated once per solve and
//! integrated on panels graded toward the image (FND-239). Each deck is gated
//! against nec2c's own converged value (nec2c on a 9× mesh, as in FND-232) at two
//! meshes, and the finer is nearer.
use std::process::Command;

/// nec2c 1.3.1 at 9× each deck's mesh (`scripts/nec2c-fine-mesh.py`), captured
/// 2026-10-13. The 1× / 3× / 9× series: L 0.05 m 39.963+j22.692, 40.063+j23.051,
/// 40.118+j23.214; L 0.3 m 38.944+j18.015, 39.047+j18.310, 39.098+j18.482;
/// vertical 0.05 m 111.78+j65.503, 112.50+j65.803, 112.84+j65.987.
/// A named deck builder (taking the mesh multiple) and its reference Z.
type Case = (&'static str, fn(u32) -> String, (f64, f64));

const CASES: [Case; 3] = [
    ("L, bend 0.05 m up", |s| l_deck(0.05, s), (40.118, 23.214)),
    ("L, bend 0.3 m up", |s| l_deck(0.3, s), (39.098, 18.482)),
    (
        "vertical dipole, 0.05 m up",
        vertical_deck,
        (112.84, 65.987),
    ),
];

/// A 5.28 m vertical and a 5.3 m radial meeting `h` above GN 1, `21·s` segments
/// each, fed on the vertical's third segment (its middle third at `s = 3`).
fn l_deck(h: f64, s: u32) -> String {
    radials_deck(h, s, &[(5.3, 0.0)], "GN 1")
}

/// A 5.28 m vertical rising from `h` above `ground`, with a 5.3 m radial at `h`
/// toward each `(x, y)`; `21·s` segments each, fed on the vertical's third segment.
fn radials_deck(h: f64, s: u32, radials: &[(f64, f64)], ground: &str) -> String {
    let n = 21 * s;
    let top = h + 5.28;
    let feed = 3 * s - (s - 1) / 2;
    let mut wires = format!("GW 1 {n} 0 0 {h} 0 0 {top} .001\n");
    for (i, (x, y)) in radials.iter().enumerate() {
        wires += &format!("GW {} {n} 0 0 {h} {x} {y} {h} .001\n", i + 2);
    }
    format!("CE\n{wires}GE 0\n{ground}\nEX 0 1 {feed} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n")
}

/// A centre-fed 10.56 m vertical dipole whose bottom is 0.05 m above GN 1.
fn vertical_deck(s: u32) -> String {
    dipole_deck([0.0, 0.0, 0.05], [0.0, 0.0, 10.61], s, "GN 1")
}

/// A centre-fed dipole from `a` to `b` over `ground`, `21·s` segments.
fn dipole_deck(a: [f64; 3], b: [f64; 3], s: u32, ground: &str) -> String {
    let n = 21 * s;
    let feed = 11 * s - (s - 1) / 2;
    format!(
        "CE\nGW 1 {n} {} {} {} {} {} {} .001\nGE 0\n{ground}\nEX 0 1 {feed} 0 1 0\n\
         FR 0 1 0 0 14.2 0\nEN\n",
        a[0], a[1], a[2], b[0], b[1], b[2]
    )
}

const GN2: &str = "GN 2 0 0 0 13 .005";

/// A deck builder taking the mesh multiple and the `GN` card.
type GroundedDeck = fn(u32, &str) -> String;
const FOUR: [(f64, f64); 4] = [(5.3, 0.0), (0.0, 5.3), (-5.3, 0.0), (0.0, -5.3)];

/// `fnec --solver mpie` on `deck`: (feedpoint Z, stderr).
fn mpie(deck: &str) -> ((f64, f64), String) {
    let dir = std::env::temp_dir().join(format!(
        "fnec-mpie-image-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "mpie"])
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let row: Vec<f64> = stdout
        .lines()
        .skip_while(|l| *l != "FEEDPOINTS")
        .nth(2)
        .expect("feedpoint row")
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    ((row[6], row[7]), stderr)
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

#[test]
fn near_ground_bends_and_ends_converge_to_nec2c() {
    for (name, deck, reference) in CASES {
        let ((coarse, _), (fine, _)) = (mpie(&deck(1)), mpie(&deck(3)));
        let scale = reference.0.hypot(reference.1);
        let (e1, e3) = (dist(coarse, reference), dist(fine, reference));
        eprintln!("{name}: 1x {coarse:?} ({e1:.3}) 3x {fine:?} ({e3:.3}) ref {reference:?}");
        assert!(e1 < 0.03 * scale, "{name}: 1x {coarse:?} vs {reference:?}");
        assert!(e3 < 0.012 * scale, "{name}: 3x {fine:?} vs {reference:?}");
        assert!(e3 < e1, "{name}: refining moved away from nec2c");
    }
}

/// nec2c 1.3.1 over `GN 2 0 0 0 13 .005` at 9× each deck's mesh, captured
/// 2026-10-13. The 1× / 3× / 9× series: L 0.05 m 107.66+j135.63,
/// 105.08+j131.54, 104.90+j131.07; four radials 0.1 m 38.295+j22.503,
/// 38.411+j22.379, 38.470+j22.346; vertical 0.05 m 106.81+j56.299,
/// 107.41+j56.572, 107.70+j56.748. Before FND-239 fnec read them 1063−j2214,
/// 128.37+j131.75 and 115.15+j81.13 at 1× — the vertical moving away from nec2c
/// as the mesh was refined (133.38+j111.59 at 3×).
const FINITE: [Case; 3] = [
    (
        "L, bend 0.05 m over GN 2",
        |s| radials_deck(0.05, s, &FOUR[..1], GN2),
        (104.90, 131.07),
    ),
    (
        "four radials 0.1 m over GN 2",
        |s| radials_deck(0.1, s, &FOUR, GN2),
        (38.470, 22.346),
    ),
    (
        "vertical dipole 0.05 m over GN 2",
        |s| dipole_deck([0.0, 0.0, 0.05], [0.0, 0.0, 10.61], s, GN2),
        (107.70, 56.748),
    ),
];

#[test]
fn near_finite_ground_bends_junctions_and_ends_converge_to_nec2c() {
    for (name, deck, reference) in FINITE {
        let ((coarse, w1), (fine, w3)) = (mpie(&deck(1)), mpie(&deck(3)));
        let scale = reference.0.hypot(reference.1);
        let (e1, e3) = (dist(coarse, reference), dist(fine, reference));
        eprintln!("{name}: 1x {coarse:?} ({e1:.3}) 3x {fine:?} ({e3:.3}) ref {reference:?}");
        assert!(e1 < 0.03 * scale, "{name}: 1x {coarse:?} vs {reference:?}");
        assert!(e3 < 0.012 * scale, "{name}: 3x {fine:?} vs {reference:?}");
        assert!(e3 < e1, "{name}: refining moved away from nec2c");
        // Resolved now, so no caveat says otherwise (#573 warned on these decks).
        for w in [w1, w3] {
            assert!(!w.contains("under-resolved"), "{name}: {w}");
        }
    }
}

/// Where nec2c is not a reference: over a highly conductive ground, close to it.
/// nec2c's `GN 2` does not tend to its own `GN 1` as σ grows — a horizontal
/// dipole 0.1 m up reads 2.05 + j30.42 Ω at σ = 10⁵ S/m against 0.054 + j4.17 over
/// `GN 1` (3× mesh), and the L 0.05 m up 42.32 + j65.32 against 40.06 + j23.05 —
/// while a vertical dipole 0.05 m up does (112.50 + j65.80 both). fnec's finite
/// ground is held to that limit instead, and to the sea-water value of an
/// independent formulation: the potential-form (G_A, G_Φ) path this fill replaced
/// for horizontal wires, resolved at 0.1 m, read 22.164 + j31.366 at 3× where this
/// reads 22.157 + j31.364 (nec2c: 17.24 + j46.95).
#[test]
fn a_conductive_ground_tends_to_the_perfect_one() {
    fn dipole(s: u32, ground: &str) -> String {
        dipole_deck([-5.28, 0.0, 0.1], [5.28, 0.0, 0.1], s, ground)
    }
    fn l(s: u32, ground: &str) -> String {
        radials_deck(0.05, s, &FOUR[..1], ground)
    }
    fn four(s: u32, ground: &str) -> String {
        radials_deck(0.1, s, &FOUR, ground)
    }
    let (sea, _) = mpie(&dipole(3, "GN 2 0 0 0 81 5"));
    assert!(dist(sea, (22.164, 31.366)) < 0.05, "sea water: {sea:?}");
    let decks: [(&str, GroundedDeck); 3] = [
        ("horizontal dipole 0.1 m", dipole),
        ("L 0.05 m", l),
        ("four radials 0.1 m", four),
    ];
    for (name, deck) in decks {
        let (pec, _) = mpie(&deck(1, "GN 1"));
        let (metal, _) = mpie(&deck(1, "GN 2 0 0 0 1 1e7"));
        let scale = pec.0.hypot(pec.1);
        assert!(
            dist(pec, metal) < 0.01 * scale.max(5.0),
            "{name}: σ = 10⁷ {metal:?} vs GN 1 {pec:?}"
        );
        eprintln!("{name}: GN 1 {pec:?}, σ = 10⁷ {metal:?}");
    }
}
