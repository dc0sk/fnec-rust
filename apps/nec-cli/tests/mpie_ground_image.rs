// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-238 — `--solver mpie` over a perfect ground, close to it.
//!
//! The reflected term of a bent or non-horizontal wire was a kernel evaluated
//! once per segment pair at the midpoints, which is only right where the image is
//! far away next to the segment length. An L (a vertical and a radial meeting at
//! 0.05 m, 0.25 m segments) read 53.4 − j4766.9 Ω where nec2c gives 39.96 + j22.69,
//! and a vertical dipole 0.05 m up read 30 % high in X. Over a perfect ground the
//! term is now the exact image through the free-space fill. Each deck is gated
//! against nec2c's own converged value (nec2c on a 9× mesh, as in FND-232) at two
//! meshes, and the finer is nearer.
//!
//! Over a finite ground the term is still the coarse one; such a deck carries a
//! caveat naming the remedy, checked here too.

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
    let n = 21 * s;
    let top = h + 5.28;
    let feed = 3 * s - (s - 1) / 2;
    format!(
        "CE\nGW 1 {n} 0 0 {h} 0 0 {top} .001\nGW 2 {n} 0 0 {h} 5.3 0 {h} .001\nGE 0\nGN 1\n\
         EX 0 1 {feed} 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n"
    )
}

/// A centre-fed 10.56 m vertical dipole whose bottom is 0.05 m above GN 1.
fn vertical_deck(s: u32) -> String {
    let n = 21 * s;
    let feed = 11 * s - (s - 1) / 2;
    format!(
        "CE\nGW 1 {n} 0 0 0.05 0 0 10.61 .001\nGE 0\nGN 1\nEX 0 1 {feed} 0 1 0\n\
         FR 0 1 0 0 14.2 0\nEN\n"
    )
}

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

/// Over a finite ground the reflected term is still the coarse one, so a segment
/// closer to the ground than its length is a caveat — and only then: not over a
/// perfect ground, and not with the same L 3 m up.
#[test]
fn an_under_resolved_finite_ground_term_is_a_caveat() {
    const SAYS: &str = "the MPIE's reflected term is under-resolved there";
    let low = l_deck(0.1, 1).replace("GN 1", "GN 2 0 0 0 13 .005");
    let (_, stderr) = mpie(&low);
    assert!(stderr.contains(SAYS), "{stderr}");
    let (_, stderr) = mpie(&l_deck(0.1, 1));
    assert!(
        !stderr.contains(SAYS),
        "GN 1 uses the exact image: {stderr}"
    );
    let high = l_deck(3.0, 1).replace("GN 1", "GN 2 0 0 0 13 .005");
    let (_, stderr) = mpie(&high);
    assert!(!stderr.contains(SAYS), "3 m up is resolved: {stderr}");
}
