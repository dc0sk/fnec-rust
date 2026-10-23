// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH10-CHK-002 — the `GA` (wire arc) and `GH` (helix) cards, as nec2c builds
//! them, solved on the MPIE.
//!
//! Every node of an arc or helix is a bend, so every straight run is one segment
//! long; Hallén's section graph refuses that, and the per-wire fallback read a
//! half-circle arc 39.1 + j115.9 Ω against nec2c's 63.9 + j90.8. So the Hallén
//! geometry refuses both cards, naming the MPIE, which models the bends and
//! converges to nec2c. The segment tables themselves are pinned against nec2c's
//! in `nec_solver::geometry`'s unit tests; here each card equals its written-out
//! `GW` twin in fnec, and refining approaches nec2c's finest-mesh value (nec2c
//! 1.3.1, captured 2026-10-14).
use std::process::Command;

/// `fnec` on `deck` with `args`: Ok(feedpoint Z) or Err(stderr).
fn fnec(deck: &str, args: &[&str]) -> Result<(f64, f64), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-gagh-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
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

const MPIE: &[&str] = &["--solver", "mpie"];

fn mpie(deck: &str) -> (f64, f64) {
    fnec(deck, MPIE).unwrap_or_else(|e| panic!("{deck}\nwas refused: {e}"))
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn same(a: (f64, f64), b: (f64, f64), what: &str) {
    assert!(
        dist(a, b) <= 1e-9 * a.0.hypot(a.1),
        "{what}: {a:?} vs {b:?}"
    );
}

fn deck(geometry: &str, feed: u32) -> String {
    format!("CE\n{geometry}\nGE 0\nEX 0 1 {feed} 0 1 0\nFR 0 1 0 0 100 0\nEN\n")
}

/// The written-out twin of `points`: one GW per segment, all tag 1, with the
/// coordinates in Rust's shortest round-trip form, so they parse back exactly.
fn polygon(points: &[[f64; 3]], radius: f64) -> String {
    points
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            format!(
                "GW 1 1 {} {} {} {} {} {} {radius}",
                a[0], a[1], a[2], b[0], b[1], b[2]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The arc's points as nec2c's `arc` makes them (angle accumulated).
fn arc_points(r: f64, a1: f64, a2: f64, n: u32) -> Vec<[f64; 3]> {
    let to_rad = std::f64::consts::PI / 180.0;
    let mut ang = a1 * to_rad;
    let dang = (a2 - a1) * to_rad / f64::from(n);
    let mut pts = vec![[r * ang.cos(), 0.0, r * ang.sin()]];
    for _ in 0..n {
        ang += dang;
        pts.push([r * ang.cos(), 0.0, r * ang.sin()]);
    }
    pts
}

/// Refining approaches nec2c's finest-mesh value — the finest mesh nearer than
/// the coarsest, not each step nearer than the last: near a resonance the two
/// fine meshes sit within nec2c's own spread of each other (the full circle's 60
/// and 180 read 0.49 and 0.53 Ω from it, where nec2c itself moves 0.46 Ω in X
/// between them) — and the finest is near it.
fn converges(zs: &[(f64, f64)], reference: (f64, f64), within: f64, what: &str) {
    let e: Vec<f64> = zs.iter().map(|z| dist(*z, reference)).collect();
    assert!(
        e.last().unwrap() < e.first().unwrap(),
        "{what}: refining moved away from nec2c: {zs:?} vs {reference:?}"
    );
    let scale = reference.0.hypot(reference.1);
    assert!(
        *e.last().unwrap() < within * scale,
        "{what}: finest {zs:?} vs {reference:?}"
    );
}

/// A half-circle arc, R = 0.5 m, fed beside its middle. nec2c 63.878 + j90.771 at
/// 10 segments for the GA deck and its GW twin, 66.766 + j99.738 at 30, 67.831 +
/// j101.95 at 90; fnec's MPIE is 1.3 % from the last at 90 (measured).
#[test]
fn a_half_circle_arc_is_its_polygon_and_converges_to_nec2c() {
    let ga = mpie(&deck("GA 1 10 0.5 0 180 .001", 5));
    let twin = mpie(&deck(&polygon(&arc_points(0.5, 0.0, 180.0, 10), 0.001), 5));
    same(ga, twin, "GA half circle vs its GW polygon");
    let zs: Vec<(f64, f64)> = [(10, 5), (30, 14), (90, 41)]
        .iter()
        .map(|&(n, k)| mpie(&deck(&format!("GA 1 {n} 0.5 0 180 .001"), k)))
        .collect();
    converges(&zs, (67.831, 101.95), 0.02, "half circle");
}

/// A full circle closes on itself (its last end meets its first start to 1e-16
/// m) and solves as a loop: nec2c 138.33 − j10.378 at 20 segments, 139.77 −
/// j0.7821 at 60, 140.01 − j0.32661 at 180.
#[test]
fn a_full_circle_closes_and_converges_to_nec2c() {
    let ga = mpie(&deck("GA 1 20 0.5 0 360 .001", 5));
    let twin = mpie(&deck(&polygon(&arc_points(0.5, 0.0, 360.0, 20), 0.001), 5));
    same(ga, twin, "GA full circle vs its GW polygon");
    let zs: Vec<(f64, f64)> = [(20, 5), (60, 14), (180, 41)]
        .iter()
        .map(|&(n, k)| mpie(&deck(&format!("GA 1 {n} 0.5 0 360 .001"), k)))
        .collect();
    converges(&zs, (140.01, -0.32661), 0.02, "full circle");
}

/// A gentle two-turn helix (S 0.3, HL 0.6, radius 0.1, wire 0.5 mm): nec2c
/// 12.024 − j117.25 at 40 segments, 12.027 − j110.81 at 120. (A tight coil is no
/// gate: nec2c's own answer for one crosses a resonance as its mesh is refined.)
///
/// FND-296: the reference was the 120-segment point, which nec2c's own next
/// tripling moves 1.2 Ω — past the 1 % (1.1 Ω) band. It is now nec2c at 1080
/// segments, the feed held at the same point (segment 20 / 59 / 176 / 527),
/// `EK 0` (3.5 radii a segment at 360, 1.2 at 1080 — under the thin-wire limit),
/// captured 2026-10-23: 12.024 − j117.26, 12.026 − j110.82, 11.997 − j109.68,
/// 11.957 − j109.46 at 40 / 120 / 360 / 1080; the last step 0.22 Ω, a tail of
/// about 0.05. fnec's MPIE reads 12.096 − j117.67, 12.048 − j111.32, 12.005 −
/// j110.05 at 40 / 120 / 360 (measured 2026-10-23): 1.7 % from that limit at 120,
/// 0.53 % at 360, heading for about −j109.73. So the gate is held at 360, where
/// the 1 % band is nearly twice fnec's distance.
#[test]
fn a_helix_is_its_polygon_and_converges_to_nec2c() {
    let helix = |n: u32| format!("GH 1 {n} 0.3 0.6 0.1 0.1 0.1 0.1 .0005");
    let points: Vec<[f64; 3]> = {
        let zinc = 0.6 / 40.0;
        let mut z = 0.0;
        let mut pts = Vec::new();
        for i in 0..=40 {
            if i > 0 {
                z += zinc;
            }
            let t = 2.0 * std::f64::consts::PI * z / 0.3;
            pts.push([0.1 * t.cos(), 0.1 * t.sin(), z]);
        }
        pts
    };
    same(
        mpie(&deck(&helix(40), 20)),
        mpie(&deck(&polygon(&points, 0.0005), 20)),
        "GH vs its GW polygon",
    );
    let zs = [mpie(&deck(&helix(40), 20)), mpie(&deck(&helix(360), 176))];
    converges(&zs, (11.957, -109.46), 0.01, "helix");
}

/// The cards compose as nec2c composes them: a quarter arc turned into the xy
/// plane by GM and completed by GR is the full circle turned the same way, and
/// GS scales an arc, wire radius included.
#[test]
fn arcs_compose_with_gm_gr_and_gs() {
    let turn = "GM 0 0 90 0 0 0 0 0";
    let quarters = mpie(&deck(&format!("GA 1 5 0.5 0 90 .001\n{turn}\nGR 0 4"), 3));
    let circle = mpie(&deck(&format!("GA 1 20 0.5 0 360 .001\n{turn}"), 3));
    assert!(
        dist(quarters, circle) <= 1e-6 * circle.0.hypot(circle.1),
        "GA quarter + GM + GR 4 vs the full circle: {quarters:?} vs {circle:?}"
    );
    same(
        mpie(&deck("GA 1 10 0.25 0 180 .0005\nGS 0 0 2", 5)),
        mpie(&deck("GA 1 10 0.5 0 180 .001", 5)),
        "GS 2 on an arc",
    );
}

/// Refused by name, each beside the deck that solves: either card on the Hallén
/// geometry (the default solver), and the values nec2c hangs on, builds wrong or
/// reads otherwise than NEC-2 defines.
#[test]
fn what_cannot_be_built_or_solved_is_refused_by_name() {
    let e = fnec(&deck("GA 1 10 0.5 0 180 .001", 5), &[]).expect_err("Hallén GA");
    assert!(
        e.contains("a wire arc (GA)") && e.contains("--solver mpie"),
        "{e}"
    );
    let e = fnec(
        &deck("GH 1 40 0.3 0.6 0.1 0.1 0.1 0.1 .0005", 20),
        &["--solver", "hallen"],
    )
    .expect_err("Hallén GH");
    assert!(e.contains("a helix (GH)"), "{e}");
    for (card, needle) in [
        ("GA 1 0 0.5 0 180 .001", "at least 1"),
        ("GA 1 10 0 0 180 .001", "an arc radius must be positive"),
        ("GA 1 10 -0.5 0 180 .001", "an arc radius must be positive"),
        ("GA 1 10 0.5 0 0 .001", "spans no angle"),
        ("GA 1 10 0.5 0 400 .001", "more than 360 degrees"),
        ("GA 1 1 0.5 0 360 .001", "chord through the centre"),
        ("GA 1 10 0.5 0 180 0", "GA field 6 = '0'"),
        (
            "GH 1 8 0 0.4 0.1 0.1 0.1 0.1 .0005",
            "spacing between turns must be positive",
        ),
        ("GH 1 8 0.2 0 0.1 0.1 0.1 0.1 .0005", "flat spiral"),
        ("GH 1 8 0.2 0.4 0.1 0.1 0.1 0.2 .0005", "nec2c ignores B2"),
        ("GH 1 8 0.2 0.4 0.1 0 0.05 0.05 .0005", "in a tapered helix"),
        (
            "GH 1 8 0.2 0.4 0.1 -0.1 0.1 0.1 .0005",
            "must not be negative",
        ),
        (
            "GH 1 8 0.2 0.4 0 0 0 0 .0005",
            "a straight wire on the z axis",
        ),
    ] {
        let e = fnec(&deck(card, 1), MPIE).expect_err(card);
        assert!(e.contains(needle), "{card}: {e}");
    }
    // The spellings beside them that solve.
    mpie(&deck("GA 1 3 0.5 0 180 .001", 2));
    mpie(&deck("GH 1 8 0.2 0.4 0.1 0.1 0.1 0.1 .0005", 4));
    mpie(&deck("GH 1 8 0.2 0.4 0.1 0.1 0.05 0.05 .0005", 4));
    mpie(&deck("GH 1 8 0.2 0.4 0 0 0.1 0.1 .0005", 4));
}
