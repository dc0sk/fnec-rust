// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-004: RP XNDA-driven normalized gain output (NORMALIZED_PATTERN).
// FND-298: the request is XNDA's N (hundreds) digit, as nec2c splits XNDA; the
// thousands digit X only picks nec2c's column labels.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(deck: &str, name: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-{name}-{now}.nec"));
    fs::write(&path, deck).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .arg(&path)
        .current_dir(&root)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn norm_gains(stdout: &str) -> Vec<f64> {
    let mut in_sec = false;
    let mut v = Vec::new();
    for line in stdout.lines() {
        if line.trim() == "NORMALIZED_PATTERN" {
            in_sec = true;
            continue;
        }
        if in_sec {
            let c: Vec<&str> = line.split_whitespace().collect();
            if c.first() == Some(&"THETA") || c.first() == Some(&"N_POINTS") {
                continue;
            }
            match c.first().and_then(|t| t.parse::<f64>().ok()) {
                Some(_) if c.len() == 3 => v.push(c[2].parse::<f64>().unwrap()),
                _ => break,
            }
        }
    }
    v
}

const DIPOLE: &str = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\n{RP}\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn xnda_n_digit_emits_normalized_pattern_with_zero_db_peak() {
    // XNDA = 0500 → N digit 5 (the total gain) → normalized-gain output.
    let out = run(
        &DIPOLE.replace("{RP}", "RP 0 19 1 0500 0.0 0.0 5.0 0.0"),
        "rpnorm",
    );
    assert!(
        out.contains("RADIATION_PATTERN") && out.contains("NORMALIZED_PATTERN"),
        "both absolute and normalized patterns must be present"
    );
    let g = norm_gains(&out);
    assert_eq!(g.len(), 19);
    let peak = g.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (peak - 0.0).abs() < 1e-6,
        "normalized pattern peak must be 0 dB, got {peak}"
    );
    assert!(
        g.iter().all(|&x| x <= 1e-6),
        "all normalized gains must be <= 0 dB"
    );
}

#[test]
fn the_x_digit_4nec2_writes_does_not_normalize() {
    // XNDA = 1000, as every 4nec2 deck writes it: X = 1 picks nec2c's column
    // labels and nothing else, so the pattern is the absolute one only. fnec
    // before 0.26 read X as "normalize" (FND-298).
    let out = run(
        &DIPOLE.replace("{RP}", "RP 0 19 1 1000 0.0 0.0 5.0 0.0"),
        "rpplain",
    );
    assert!(
        out.contains("RADIATION_PATTERN"),
        "absolute pattern present"
    );
    assert!(
        !out.contains("NORMALIZED_PATTERN"),
        "XNDA 1000 (N = 0) → no NORMALIZED_PATTERN section"
    );
}

#[test]
fn xnda_zero_x_digit_does_not_normalize() {
    // XNDA = 0 (X digit 0) — no normalization requested.
    let out = run(
        &DIPOLE.replace("{RP}", "RP 0 19 1 0 0.0 0.0 5.0 0.0"),
        "rpx0",
    );
    assert!(
        !out.contains("NORMALIZED_PATTERN"),
        "XNDA with X=0 must not normalize"
    );
}
