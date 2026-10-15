// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH10-CHK-002 — the `GC` card (a tapered wire), as nec2c's `wire` builds it.
//!
//! `GW … 0` then `GC 0 0 RDEL RAD1 RAD2`: each segment `RDEL` times as long as the
//! one before, the radii geometric from `RAD1` to `RAD2`. Refused until now (and a
//! `GW` radius of 0 with it). Each taper is held two ways: the `GC` deck equals
//! its written-out twin — one `GW` per segment, coordinates and radii in Rust's
//! shortest round-trip form — in fnec, on both solvers, and nec2c 1.3.1 prints the
//! same digits for the two (captured 2026-10-15); and refining approaches nec2c's
//! value at the finest mesh, each step nearer than the last.
use std::process::Command;

/// `fnec` on `deck` with `args`: Ok((feedpoint Z, stderr)) or Err(stderr).
fn fnec(deck: &str, args: &[&str]) -> Result<((f64, f64), String), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-gc-{}-{:?}",
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

const HALLEN: &[&str] = &["--solver", "hallen"];
const MPIE: &[&str] = &["--solver", "mpie"];

fn solve(deck: &str, args: &[&str]) -> (f64, f64) {
    fnec(deck, args)
        .unwrap_or_else(|e| panic!("{deck}\nwas refused: {e}"))
        .0
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
    format!("CE\n{geometry}\nGE 0\nEX 0 1 {feed} 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n")
}

/// A 5 m wire on the z axis, `ns` segments, tapered `rdel`, `r1` → `r2`.
fn gc(ns: u32, rdel: f64, r1: f64, r2: f64) -> String {
    format!("GW 1 {ns} 0 0 -2.5 0 0 2.5 0\nGC 0 0 {rdel} {r1} {r2}")
}

/// The same wire written out as nec2c's `wire` makes it: one GW per segment.
fn twin(ns: u32, rdel: f64, r1: f64, r2: f64) -> String {
    let (p1, p2) = ([0.0, 0.0, -2.5], [0.0, 0.0, 2.5]);
    let mut d: [f64; 3] = std::array::from_fn(|k| p2[k] - p1[k]);
    let (mut delz, rd) = if (rdel - 1.0f64).abs() >= 1e-6 {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        d = d.map(|c| c / len);
        (len * (1.0 - rdel) / (1.0 - rdel.powf(f64::from(ns))), rdel)
    } else {
        d = d.map(|c| c / f64::from(ns));
        (1.0, 1.0)
    };
    let rrad = (r2 / r1).powf(1.0 / (f64::from(ns) - 1.0));
    let (mut rad, mut s) = (r1, p1);
    let mut lines = Vec::new();
    for i in 0..ns {
        let mut e = [s[0] + d[0] * delz, s[1] + d[1] * delz, s[2] + d[2] * delz];
        if i + 1 == ns {
            e = p2;
        }
        lines.push(format!(
            "GW 1 1 {} {} {} {} {} {} {rad}",
            s[0], s[1], s[2], e[0], e[1], e[2]
        ));
        delz *= rd;
        rad *= rrad;
        s = e;
    }
    lines.join("\n")
}

/// Each step nearer nec2c's value at the finest mesh than the step before.
fn approaches(zs: &[(f64, f64)], reference: (f64, f64), what: &str) {
    let e: Vec<f64> = zs.iter().map(|z| dist(*z, reference)).collect();
    assert!(
        e.windows(2).all(|w| w[1] < w[0]),
        "{what}: refining does not approach nec2c: {zs:?} vs {reference:?}"
    );
}

/// The three meshes: 21, 63 and 189 segments, `RDEL^(1/m)` so the profile is the
/// same wire's, and the feed on the segment at the same place (the centre third
/// of the coarse feed segment).
const MESHES: [(u32, f64, u32); 3] = [(21, 1.0, 11), (63, 3.0, 32), (189, 9.0, 95)];

/// A radius taper, 0.5 → 2 mm, centre-fed: nec2c 79.247 + j41.920 at 21
/// segments for the GC deck and its twin alike, 79.655 + j42.156 at 63,
/// 79.859 + j42.174 at 189. Hallén 79.118 + j39.223, 79.635 + j41.186, 79.881 +
/// j42.123.
/// (A uniform 1 mm wire converges the same way: nec2c 79.647 + j42.268 at 189,
/// Hallén 79.609 + j41.630.) The MPIE, each segment its own radius, 78.842 +
/// j41.719, 79.485 + j42.738, 79.803 + j43.121. That the three agree says little
/// about tapers: this one is thin below the feed and thick above, and the two
/// arms' errors cancel — a taper symmetric about the feed separates them, and
/// there nec2c is no reference (`mpie_mixed_radii.rs`, FND-252). Hallén warns.
#[test]
fn a_radius_taper_is_its_twin_and_approaches_nec2c() {
    for args in [HALLEN, MPIE] {
        same(
            solve(&deck(&gc(21, 1.0, 0.0005, 0.002), 11), args),
            solve(&deck(&twin(21, 1.0, 0.0005, 0.002), 11), args),
            &format!("radius taper vs its twin, {args:?}"),
        );
    }
    let zs: Vec<(f64, f64)> = MESHES
        .iter()
        .map(|&(ns, _, feed)| solve(&deck(&gc(ns, 1.0, 0.0005, 0.002), feed), HALLEN))
        .collect();
    approaches(&zs, (79.859, 42.174), "radius taper");
    let (_, stderr) = fnec(&deck(&gc(21, 1.0, 0.0005, 0.002), 11), MPIE).expect("solves");
    assert!(!stderr.contains("radius"), "{stderr}");
    let (_, stderr) = fnec(&deck(&gc(21, 1.0, 0.0005, 0.002), 11), HALLEN).expect("solves");
    assert!(
        stderr.contains("changes radius along its length") && stderr.contains("--solver mpie"),
        "{stderr}"
    );
}

/// A length taper, RDEL 1.1 at 21 segments, 2 mm: nec2c 146.99 + j63.828 at 21
/// for the GC deck and its twin alike, 149.15 + j64.761 at 63, 150.10 + j65.227
/// at 189. Hallén 145.77 + j57.388, 148.56 + j61.671, 149.78 + j63.650; the
/// MPIE 144.54 + j61.262, 148.22 + j63.782, 149.52 +
/// j64.416. (At 5 mm the 189-segment mesh is refused: its shortest segment is
/// 1.7 radii.)
#[test]
fn a_length_taper_is_its_twin_and_approaches_nec2c() {
    for args in [HALLEN, MPIE] {
        same(
            solve(&deck(&gc(21, 1.1, 0.002, 0.002), 11), args),
            solve(&deck(&twin(21, 1.1, 0.002, 0.002), 11), args),
            &format!("length taper vs its twin, {args:?}"),
        );
        let zs: Vec<(f64, f64)> = MESHES
            .iter()
            .map(|&(ns, m, feed)| {
                solve(
                    &deck(&gc(ns, 1.1f64.powf(1.0 / m), 0.002, 0.002), feed),
                    args,
                )
            })
            .collect();
        approaches(&zs, (150.10, 65.227), &format!("length taper, {args:?}"));
    }
    // One radius throughout: neither solver warns about a radius change.
    for args in [HALLEN, MPIE] {
        let (_, stderr) = fnec(&deck(&gc(21, 1.1, 0.002, 0.002), 11), args).expect("solves");
        assert!(!stderr.contains("changes radius"), "{stderr}");
    }
}

/// Both at once (RDEL 1.05, 0.5 → 2 mm; nec2c 95.595 + j44.110 for the GC deck
/// and its twin alike), and the cards compose as nec2c composes them: GS scales a
/// taper, its radii included.
#[test]
fn both_tapers_at_once_and_a_scaled_taper() {
    same(
        solve(&deck(&gc(21, 1.05, 0.0005, 0.002), 11), HALLEN),
        solve(&deck(&twin(21, 1.05, 0.0005, 0.002), 11), HALLEN),
        "length and radius taper vs its twin",
    );
    same(
        solve(
            &deck(
                "GW 1 21 0 0 -1.25 0 0 1.25 0\nGC 0 0 1.05 .00025 .001\nGS 0 0 2",
                11,
            ),
            HALLEN,
        ),
        solve(&deck(&gc(21, 1.05, 0.0005, 0.002), 11), HALLEN),
        "GS 2 on a taper",
    );
}

/// Refused by name, each beside a spelling that solves: what nec2c stops on (a
/// radius-0 GW with no GC after it, a GC after anything else, a zero radius), what
/// it builds wrong (a ratio of 0 or below, a negative radius, one segment given
/// two radii — nec2c silently uses RAD1), and a taper too short for the thin-wire
/// kernel (RDEL 0.6 ends at 73 µm on a 1 mm wire; fed on its third segment,
/// nec2c 125.33 + j56.663 and fnec's twin 123.13 + j37.435).
#[test]
fn what_nec2c_stops_on_or_builds_wrong_is_refused_by_name() {
    // Fed on a plain dipole beside the taper, so a one-segment taper can be built.
    let fed = |geometry: &str| {
        format!(
            "CE\n{geometry}\nGW 2 11 1 0 -2.5 1 0 2.5 .001\nGE 0\nEX 0 2 6 0 1 0\n\
             FR 0 1 0 0 29.9 0\nEN\n"
        )
    };
    let refused = |d: &str| match fnec(d, HALLEN) {
        Err(e) => e,
        Ok((z, _)) => panic!("{d}\nsolved {z:?}, expected a refusal"),
    };
    for (geometry, needle) in [
        (
            "GW 1 9 0 0 -1 0 0 1 0",
            "follow it with `GC 0 0 RDEL RAD1 RAD2`",
        ),
        (
            "GW 1 9 0 0 -1 0 0 1 .001\nGC 0 0 1 .001 .002",
            "a GC tapers the GW directly before it",
        ),
        (
            "GW 1 9 0 0 -1 0 0 1 0\nCM\nGC 0 0 1 .001 .002",
            "follow it with",
        ),
        (
            "GW 1 9 0 0 -1 0 0 1 0\nGC 0 0 0 .001 .002",
            "must be positive",
        ),
        (
            "GW 1 9 0 0 -1 0 0 1 0\nGC 0 0 -1.1 .001 .002",
            "must be positive",
        ),
        ("GW 1 9 0 0 -1 0 0 1 0\nGC 0 0 1 0 .002", "GC field 4 = '0'"),
        (
            "GW 1 9 0 0 -1 0 0 1 0\nGC 0 0 1 .001 -.002",
            "GC field 5 = '-.002'",
        ),
        (
            "GW 1 1 0 0 -1 0 0 1 0\nGC 0 0 1 .001 .002",
            "one segment cannot taper",
        ),
        (
            "GW 1 21 0 0 -2.5 0 0 2.5 0\nGC 0 0 0.6 .001 .001",
            "shorter than 2 radii",
        ),
    ] {
        let e = refused(&fed(geometry));
        assert!(e.contains(needle), "{geometry}: {e}");
    }
    let e =
        refused("CE\nGW 1 9 0 0 -1 0 0 1 0\nGC 0 0 1 .001 .002\nGE 0\nGC 0 0 1 .001 .002\nEN\n");
    assert!(e.contains("after GE"), "{e}");
    // The spellings beside them that solve.
    for geometry in [
        "GW 1 9 0 0 -1 0 0 1 0\nGC 0 0 1 .001 .002",
        "GW 1 9 0 0 -1 0 0 1 0\n\nGC 5 5 1.2 .001 .002",
        "GW 1 1 0 0 -1 0 0 1 0\nGC 0 0 1 .001 .001",
        "GW 1 21 0 0 -2.5 0 0 2.5 0\nGC 0 0 0.8 .001 .001",
    ] {
        solve(&fed(geometry), HALLEN);
    }
}
