// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-001: incident-plane-wave receive-pattern sweep. Validated by
// reciprocity — the receive pattern must match the transmit gain pattern.

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
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&path)
        .current_dir(&root)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Parse a `SECTION` table's numeric rows, returning column `col` (0-indexed)
/// keyed by the θ column (col 0).
fn section_rows(stdout: &str, section: &str, col: usize) -> Vec<(f64, f64)> {
    let mut rows = Vec::new();
    let mut in_sec = false;
    for line in stdout.lines() {
        if line.trim() == section {
            in_sec = true;
            continue;
        }
        if in_sec {
            let c: Vec<&str> = line.split_whitespace().collect();
            if c.is_empty() {
                continue;
            }
            match c[0].parse::<f64>() {
                Ok(theta) if c.len() > col => {
                    if let Ok(v) = c[col].parse::<f64>() {
                        rows.push((theta, v));
                    }
                }
                _ => {
                    if !rows.is_empty() {
                        break;
                    }
                }
            }
        }
    }
    rows
}

const RX_SWEEP: &str = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 1 10 1 0 0.0 0.0 0.0 10.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
const TX_RP: &str = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nRP 0 10 1 0.0 0.0 10.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn receive_pattern_sweep_emits_all_angles() {
    let out = run(RX_SWEEP, "rxsweep");
    assert!(
        out.contains("RECEIVE_PATTERN"),
        "missing RECEIVE_PATTERN section"
    );
    assert!(out.contains("N_POINTS 10"), "expected 10 sweep points");
    let rx = section_rows(&out, "THETA PHI RESPONSE_DB", 2);
    assert_eq!(rx.len(), 10, "receive pattern should have 10 rows");
    // z-dipole: endfire (θ=0) is a receive null; broadside (θ=90) is the peak.
    assert!(
        rx[0].1 < -100.0,
        "θ=0 should be the endfire null, got {}",
        rx[0].1
    );
    assert!(
        (rx[9].1).abs() < 1e-6,
        "θ=90 should be the 0 dB peak, got {}",
        rx[9].1
    );
    // Monotonic rise from endfire to broadside.
    for w in rx.windows(2) {
        assert!(
            w[1].1 >= w[0].1 - 1e-6,
            "receive response should rise toward broadside"
        );
    }
}

#[test]
fn receive_pattern_matches_transmit_by_reciprocity() {
    // The normalized receive pattern must equal the normalized transmit gain
    // pattern (Rayleigh–Carson reciprocity).
    let rx = section_rows(&run(RX_SWEEP, "rxrecip"), "THETA PHI RESPONSE_DB", 2);
    let tx_raw = section_rows(
        &run(TX_RP, "txrecip"),
        "THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO",
        3, // GAIN_V_DB = θ-polarised gain
    );
    assert_eq!(rx.len(), tx_raw.len(), "sweeps must align");
    let tx_peak = tx_raw.iter().map(|r| r.1).fold(f64::MIN, f64::max);
    let mut max_dev = 0.0f64;
    for ((th, rx_db), (_, tx_db)) in rx.iter().zip(&tx_raw) {
        if *rx_db < -100.0 || *tx_db < -100.0 {
            continue; // shared null
        }
        let tx_norm = tx_db - tx_peak;
        let dev = (rx_db - tx_norm).abs();
        println!("θ={th:5.1}  rx={rx_db:8.3}  tx_norm={tx_norm:8.3}  dev={dev:.3}");
        max_dev = max_dev.max(dev);
    }
    assert!(
        max_dev < 0.2,
        "receive pattern deviates from transmit (reciprocity) by {max_dev:.3} dB"
    );
}
