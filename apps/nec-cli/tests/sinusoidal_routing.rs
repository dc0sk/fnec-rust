// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-203 — `--solver sinusoidal` takes only what its basis can solve.
//!
//! The sinusoidal arm is the plain merged-conductor solve. It has no
//! conductor-path basis for a bend or a split (FND-121) and no section graph for
//! a junction or loop (FND-162), and it answered those decks anyway: a split-V
//! 10.27 − j731.22 Ω where Hallén gives 270.43 + j443.20 and nec2c 268.56 +
//! j452.26, a T 4.19 − j1010.07 against nec2c's 107.54 − j366.35 — exit 0, its
//! residual check passing. It now refuses them by name, and still solves the
//! straight wires and collinear chains its basis is exact for.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-sinroute-{name}-{n}"), body)
}

fn sinusoidal(deck: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "sinusoidal", "--exec", "cpu"])
        .arg(deck)
        .output()
        .expect("run fnec")
}

const T_JUNCTION: &str = "CE\nGW 1 21 0 0 0 0 0 5 .001\nGW 2 21 -5 0 5 0 0 5 .001\nGW 3 21 0 0 5 5 0 5 .001\nGE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
/// A T whose third arm is one segment long: the section graph declines it, so
/// the Hallén route is the plain basis with an unsupported topology (warned
/// there) — not a path route. The refusal must cover this half too.
const T_ONE_SEGMENT_ARM: &str = "CE\nGW 1 21 0 0 0 0 0 5 .001\nGW 2 21 -5 0 5 0 0 5 .001\nGW 3 1 0 0 5 0.5 0 5 .001\nGE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
const SQUARE_LOOP: &str = "CE\nGW 1 11 0 0 0 5 0 0 .001\nGW 2 11 5 0 0 5 5 0 .001\nGW 3 11 5 5 0 0 5 0 .001\nGW 4 11 0 5 0 0 0 0 .001\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";

#[test]
fn bent_split_junction_and_loop_decks_are_refused_by_name() {
    let split_v = std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/split-v-conductor-path-freesp.nec"
    ));
    let t = tmp("t.nec", T_JUNCTION);
    let ring = tmp("loop.nec", SQUARE_LOOP);
    let stub = tmp("t1.nec", T_ONE_SEGMENT_ARM);
    for (label, path) in [
        ("split-V", split_v.as_path()),
        ("T junction", t.path()),
        ("square loop", ring.path()),
        ("T with a one-segment arm", stub.path()),
    ] {
        let out = sinusoidal(path);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "{label}: --solver sinusoidal answered a deck its basis cannot solve:\n{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            stderr.contains("--solver sinusoidal solves straight wires and collinear chains only"),
            "{label}: refused without naming why:\n{stderr}"
        );
    }
}

/// The class it does solve is untouched: a straight dipole and a collinear split
/// (one conductor after the merge, FND-158) still answer on the sinusoidal basis.
#[test]
fn straight_and_collinear_decks_still_solve() {
    let straight = tmp(
        "straight.nec",
        "CE\nGW 1 21 0 0 -5.282 0 0 5.282 .001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    let split = tmp(
        "split.nec",
        "CE\nGW 1 10 0 0 -5.282 0 0 0 .001\nGW 2 10 0 0 0 0 0 5.282 .001\nGE 0\nEX 0 1 10 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    for (label, d) in [("straight", &straight), ("collinear split", &split)] {
        let out = sinusoidal(d.path());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains("SOLVER_MODE sinusoidal"),
            "{label}: did not run on the sinusoidal basis:\n{stdout}"
        );
    }
}
