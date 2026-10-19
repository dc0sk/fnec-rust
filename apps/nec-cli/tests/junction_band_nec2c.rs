// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-246 — wire ends join where nec2c joins them.
//!
//! nec2c's `conect` joins an end of segment `i` to another segment's end within
//! 10⁻³ of `i`'s length, by the sum of the coordinate differences (L1). fnec
//! joined only within an absolute 1 µm, so a T whose stem stops 50 µm short of
//! the bar, or a dipole drawn as two halves 50 µm apart, solved open, silently,
//! on both solvers (T: Hallén 1.68 − j2606 Ω where nec2c joins it at
//! 3.42 − j1758). nec2c's band is the searching segment's own, so between a long
//! and a short segment it can reach from one side only, and a chain of
//! near-misses never closes: nec2c aborts (`SBF - SEGMENT CONNECTION ERROR`) or
//! hangs on both, and fnec refuses them.
//!
//! References: nec2c 1.3.1, captured 2026-10-19 (decks reproduced below). Every
//! deck is 10 or 16 segments a wire, where Hallén sits about 3 % from nec2c on
//! the exactly drawn deck too; `JOINED_TOL` is that gap with room, and the
//! open answers are 50 % away. That a near-miss inside the band is the same
//! antenna as the exactly drawn one is held far tighter, against fnec's own
//! exact answer (`SAME_TOL`).
use std::process::Command;

/// The near-miss moves one end by under 1e-3 of a segment's length, so its
/// answer differs from the exact deck's by about 1e-5 (measured 0.9e-5 at 50 µm).
const SAME_TOL: f64 = 1e-4;
/// A joined deck's distance from nec2c's joined value, relative: Hallén reads
/// 2.9–3.0 % from nec2c on these meshes with the ends drawn exactly.
const JOINED_TOL: f64 = 0.04;

/// `fnec --solver <solver>` on `deck`: Ok(feedpoint Z) or Err(stderr).
fn fnec(solver: &str, deck: &str) -> Result<(f64, f64), String> {
    let dir = std::env::temp_dir().join(format!(
        "fnec-junction-band-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", solver])
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

fn solves(solver: &str, deck: &str) -> (f64, f64) {
    fnec(solver, deck).unwrap_or_else(|e| panic!("{solver}: {deck}\nwas refused: {e}"))
}

fn refused(solver: &str, deck: &str) -> String {
    match fnec(solver, deck) {
        Err(e) => e,
        Ok(z) => panic!("{solver}: {deck}\nsolved {z:?}, expected a refusal"),
    }
}

fn rel(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1) / b.0.hypot(b.1)
}

const SOLVERS: [&str; 2] = ["hallen", "mpie"];

/// A T at 7 MHz: bar 24 × 0.25 m along y at z = 4 m (a joint at y = 0), stem
/// 16 × 0.25 m up from the origin, its top `gap` short of the joint, fed at stem
/// segment 8. nec2c's band at the joint is 0.25 mm.
fn tee(gap: f64) -> String {
    let top = 4.0 - gap;
    format!(
        "CE\nGW 1 24 0 -3 4 0 3 4 .001\nGW 2 16 0 0 0 0 0 {top} .001\nGE 0\n\
         EX 0 2 8 0 1 0\nFR 0 1 0 0 7.0 0\nEN\n"
    )
}

/// A 5.28 m dipole at 28.4 MHz drawn as two halves of 10 × 0.264 m, the top
/// half's first end at `(dx, dy, dz)` off the bottom half's last, fed at
/// segment 9 of the bottom half. nec2c's band is 0.264 mm.
fn split_dipole(dx: f64, dy: f64, dz: f64) -> String {
    format!(
        "CE\nGW 1 10 0 0 -2.64 0 0 0 .001\nGW 2 10 {dx} {dy} {dz} 0 0 2.64 .001\nGE 0\n\
         EX 0 1 9 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n"
    )
}

/// nec2c 1.3.1: the T joined (gap 0, 50 µm, 0.2 mm — a junction table each time)
/// and open (0.3 mm, 1 mm).
const TEE_JOINED: (f64, f64) = (3.4152, -1758.4);
const TEE_OPEN: (f64, f64) = (1.8059, -2704.3);
/// nec2c 1.3.1: the split dipole joined (gap 0, 50 µm, 0.2 mm axial; 0.1 mm
/// sideways; 0.08 mm on each axis, L1 0.24 mm).
const DIPOLE_JOINED: (f64, f64) = (84.504, 47.723);

/// Inside the band the T is the exactly drawn T, as nec2c makes it; beyond it
/// the stem is open, as in nec2c.
#[test]
fn a_t_whose_stem_stops_short_inside_the_band_joins_the_bar() {
    for solver in SOLVERS {
        let exact = solves(solver, &tee(0.0));
        assert!(
            rel(exact, TEE_JOINED) < JOINED_TOL,
            "{solver} exact {exact:?}"
        );
        for gap in [50e-6, 0.2e-3] {
            let z = solves(solver, &tee(gap));
            assert!(
                rel(z, exact) < SAME_TOL,
                "{solver} gap {gap}: {z:?} vs exact {exact:?}"
            );
        }
        for gap in [0.3e-3, 1e-3] {
            let z = solves(solver, &tee(gap));
            assert!(
                rel(z, TEE_OPEN) < JOINED_TOL,
                "{solver} gap {gap}: {z:?}, open in nec2c"
            );
        }
    }
}

/// Axial and sideways near-misses inside the band join; the band is measured
/// as nec2c measures it, by the sum of the coordinate differences: 0.1 mm on
/// each axis is 0.17 mm away in a straight line but 0.3 mm by that sum, past the
/// 0.264 mm band, and nec2c leaves it open (22.29 − j1585.6 Ω).
#[test]
fn a_dipole_drawn_in_two_halves_joins_by_nec2cs_measure() {
    for solver in SOLVERS {
        let exact = solves(solver, &split_dipole(0.0, 0.0, 0.0));
        assert!(
            rel(exact, DIPOLE_JOINED) < JOINED_TOL,
            "{solver} exact {exact:?}"
        );
        for (dx, dy, dz) in [
            (0.0, 0.0, 50e-6),
            (0.0, 0.0, 0.2e-3),
            (0.1e-3, 0.0, 0.0),
            (80e-6, 80e-6, 80e-6),
        ] {
            let z = solves(solver, &split_dipole(dx, dy, dz));
            assert!(
                rel(z, exact) < SAME_TOL,
                "{solver} ({dx}, {dy}, {dz}): {z:?} vs exact {exact:?}"
            );
        }
        for (dx, dy, dz) in [(0.0, 0.0, 0.3e-3), (0.1e-3, 0.1e-3, 0.1e-3)] {
            let z = solves(solver, &split_dipole(dx, dy, dz));
            assert!(
                z.1 < -1000.0,
                "{solver} ({dx}, {dy}, {dz}): {z:?} should be open"
            );
        }
    }
}

/// A half of 10 × 0.264 m (band 0.264 mm) meeting a half of 20 × 0.132 m (band
/// 0.132 mm), fed at segment 5. At 0.1 mm both see each other and nec2c joins
/// them (200.16 + j87.75); at 0.2 mm only the longer one does, and nec2c aborts
/// with `SBF - SEGMENT CONNECTION ERROR FOR SEGMENT 10`.
#[test]
fn ends_only_the_longer_segment_reaches_are_refused() {
    let deck = |gap: f64| {
        let h = gap / 2.0;
        format!(
            "CE\nGW 1 10 0 0 -2.64 0 0 -{h} .001\nGW 2 20 0 0 {h} 0 0 2.64 .001\nGE 0\n\
             EX 0 1 5 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n"
        )
    };
    for solver in SOLVERS {
        let z = solves(solver, &deck(0.1e-3));
        assert!(rel(z, (200.16, 87.75)) < JOINED_TOL, "{solver}: {z:?}");
        let e = refused(solver, &deck(0.2e-3));
        assert!(
            e.contains("segment 10 of tag 1 and segment 1 of tag 2")
                && e.contains("from one side only"),
            "{solver}: {e}"
        );
    }
}

/// Three one-segment wires ending at x = 0, 0.8 mm and 1.6 mm: each end within
/// the 1 mm band of the next, the outer two 1.6 mm apart. nec2c hangs on every
/// card order of it.
#[test]
fn a_chain_of_near_misses_is_refused() {
    let deck = "CE\nGW 1 1 0 0 0 0 0 1 .001\nGW 2 1 0.0008 0 2 0.0008 0 1 .001\n\
                GW 3 1 0.0016 1 1 0.0016 0 1 .001\nGE 0\nEX 0 1 1 0 1 0\nFR 0 1 0 0 100 0\nEN\n";
    for solver in SOLVERS {
        let e = refused(solver, deck);
        assert!(
            e.contains("chain of near-misses")
                && e.contains("segment 1 of tag 1 and segment 1 of tag 3"),
            "{solver}: {e}"
        );
    }
}
