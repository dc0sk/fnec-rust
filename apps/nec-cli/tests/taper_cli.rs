// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `fnec taper` — the Leeson step-tapered-radius correction (BL-IMPR-014).
//! End-to-end check that the CLI reproduces the book's worked example (Table 8-3,
//! *Physical Design of Yagi Antennas*): ℓ′ = 95.70, d′ = 0.594.

use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .output()
        .expect("run fnec")
}

fn field(stdout: &str, key: &str) -> f64 {
    stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix(key)
                .map(|v| v.trim().parse::<f64>().unwrap())
        })
        .unwrap_or_else(|| panic!("no `{key}` line in:\n{stdout}"))
}

#[test]
fn taper_reproduces_leeson_worked_example() {
    let out = run(&["taper", "--sections", "0.8,50 0.4,50"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!((field(&s, "EQUIV_HALF_LENGTH") - 95.70).abs() < 0.05, "{s}");
    assert!((field(&s, "EQUIV_DIAMETER") - 0.594).abs() < 0.002, "{s}");
    assert!((field(&s, "KA") - 667.0).abs() < 1.0, "{s}");
    assert!((field(&s, "Z0") - 608.0).abs() < 1.5, "{s}");
}

#[test]
fn taper_usage_errors() {
    // missing --sections
    assert_eq!(run(&["taper"]).status.code(), Some(2));
    // bad section token
    assert_eq!(
        run(&["taper", "--sections", "not-a-section"]).status.code(),
        Some(2)
    );
    // non-positive radius -> algorithm error (exit 1)
    assert_eq!(run(&["taper", "--sections", "0,50"]).status.code(), Some(1));
}
