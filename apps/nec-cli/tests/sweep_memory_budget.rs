// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-187: a parallel CPU sweep's points in flight are bounded by memory.
//!
//! Each point holds its own matrices — about `POINT_MATRICES` (7) × 16·N² bytes,
//! measured per solver — so on a small host the default one-point-per-core pool
//! could need many times the memory there is. The concurrency itself is gated in
//! the executor's unit tests (one slot never overlaps two points); this gates the
//! CLI's side: the budget reaches the sweep, it says what it did, and every point
//! still comes out, in order.

use std::process::Command;

/// A 51-segment dipole swept over four frequencies: about 0.29 MB per point.
const SWEEP: &str = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 .001\nGE 0\nEX 0 1 26 0 1 0\n\
                     FR 0 4 0 0 14.0 0.1\nEN\n";

fn run(budget_mb: &str) -> (String, String) {
    let path = std::env::temp_dir().join(format!(
        "fnec-sweep-budget-{}-{budget_mb}.nec",
        std::process::id()
    ));
    std::fs::write(&path, SWEEP).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--exec", "cpu"])
        .arg(&path)
        .env("RAYON_NUM_THREADS", "8")
        .env("FNEC_SWEEP_MEMORY_BUDGET_MB", budget_mb)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn frequencies(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|l| l.strip_prefix("FREQ_MHZ "))
        // Each point's block opens with its frequency; a summary table's header
        // also starts with FREQ_MHZ, and is not a point.
        .filter(|v| v.trim().parse::<f64>().is_ok())
        .map(str::to_owned)
        .collect()
}

/// 11 MB less the process's fixed 10 MB fits three 0.29 MB points: three at a
/// time, not the eight the pool would take.
#[test]
fn a_small_budget_caps_the_points_in_flight_and_says_so() {
    let (stdout, stderr) = run("11");
    assert!(
        stderr.contains("the sweep solves 3 of its points at a time, not 8"),
        "{stderr}"
    );
    assert_eq!(
        frequencies(&stdout),
        ["14.000000", "14.100000", "14.200000", "14.300000"],
        "every point, in frequency order"
    );
}

/// A budget the points fit in changes nothing and says nothing.
#[test]
fn an_ample_budget_is_silent() {
    let (stdout, stderr) = run("100000");
    assert!(!stderr.contains("points at a time"), "{stderr}");
    assert_eq!(frequencies(&stdout).len(), 4);
}

/// A malformed override is reported, not silently ignored.
#[test]
fn a_malformed_budget_is_reported() {
    let (_, stderr) = run("lots");
    assert!(
        stderr.contains("FNEC_SWEEP_MEMORY_BUDGET_MB") && stderr.contains("not a whole number"),
        "{stderr}"
    );
}
