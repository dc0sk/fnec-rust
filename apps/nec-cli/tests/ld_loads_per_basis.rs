// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-124 — `LD` loads on the sinusoidal, pulse and continuity bases.
//!
//! FND-122 derived the load for the Hallén basis only; the other three kept
//! adding the load in ohms to the diagonal. On a 1050 Ω feed load that raised
//! Z_in by +7348.50 + j1415.53 on sinusoidal and by −4591.77 on pulse and
//! continuity.
//!
//! The gate is the port identity, which needs no reference solver: a series load
//! at the feed segment raises Z_in by exactly Z_L. It holds for ANY linear system
//! in which the load is stamped with the source's own scaling, so it checks the
//! stamp even on pulse and continuity, whose unloaded answer is unphysical
//! (FND-080). The off-feed case then needs a real reference, and only the
//! sinusoidal basis can meet one, so it is checked against nec2c.

use std::path::PathBuf;
use std::process::Command;

const BASE: &str = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\n";
const TAIL: &str = "EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

/// Feedpoint Z and stderr for a deck body with `loads` spliced in.
fn solve(loads: &str, args: &[&str], tag: &str) -> ((f64, f64), String) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join(format!("ld-per-basis-{tag}-{}.nec", std::process::id()));
    std::fs::write(&path, format!("{BASE}{loads}{TAIL}")).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "fnec {args:?} failed: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let z = stdout
        .lines()
        .find_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() == 8 && c[0] == "1" && c[1] == "26")
                .then(|| (c[6].parse().unwrap(), c[7].parse().unwrap()))
        })
        .unwrap_or_else(|| panic!("no feedpoint row:\n{stdout}"));
    (z, stderr)
}

const BASES: [&[&str]; 4] = [
    &["--solver", "sinusoidal"],
    &[
        "--solver",
        "pulse",
        "--pulse-rhs",
        "nec2",
        "--experimental-solver",
    ],
    &[
        "--solver",
        "pulse",
        "--pulse-rhs",
        "raw",
        "--experimental-solver",
    ],
    &["--solver", "continuity", "--experimental-solver"],
];

/// ΔZ = Z_L at the feed, resistive and reactive, on every non-Hallén basis — and
/// each pulse-RHS scaling, since the load must follow whichever one runs.
#[test]
fn a_feed_load_shifts_z_by_exactly_itself_on_every_basis() {
    for args in BASES {
        let tag = args.join("-").replace("--", "");
        let (u, _) = solve("", args, &format!("{tag}-u"));
        for (card, want) in [
            ("LD 4 1 26 26 1050 0 0\n", (1050.0, 0.0)),
            ("LD 4 1 26 26 0 250 0\n", (0.0, 250.0)),
        ] {
            let (l, stderr) = solve(card, args, &format!("{tag}-l"));
            let d = (l.0 - u.0, l.1 - u.1);
            assert!(
                (d.0 - want.0).abs() < 0.05 && (d.1 - want.1).abs() < 0.05,
                "{args:?}: {} must shift Z by {want:?}, shifted it by ({:.3}, {:.3})",
                card.trim(),
                d.0,
                d.1
            );
            assert!(
                !stderr.contains("diagonal stamp"),
                "{args:?}: the unvalidated-stamp caveat must be gone: {stderr}"
            );
        }
    }
}

/// Off the feed the identity says nothing, so this needs an external answer:
/// nec2c 1.3.1 gives 131.33 + j34.31 for 100 Ω at segment 13 (captured
/// 2026-09-26). Sinusoidal gives 130.93 + j30.40 — the same ~4 Ω reactance
/// residual as the unloaded dipole (FND-156).
#[test]
fn an_off_feed_load_on_the_sinusoidal_basis_tracks_nec2c() {
    let ((r, x), _) = solve(
        "LD 4 1 13 13 100 0 0\n",
        &["--solver", "sinusoidal"],
        "sin-off",
    );
    assert!(
        (r - 131.33).abs() < 1.5 && (x - 34.31).abs() < 6.0,
        "sinusoidal with 100 Ω at segment 13: {r:.2} + j{x:.2}, nec2c 131.33 + j34.31"
    );
}
