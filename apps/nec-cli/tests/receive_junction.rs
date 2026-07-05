// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-002 (receive-side junction, CLI wiring): a *receiving* antenna whose
// arms meet at a degree-2 junction now solves through the CLI on continuous
// conductor paths — the plane-wave receive path no longer fails fast on junctioned
// geometry. Gated by the same reciprocity check as the single-wire receive sweep
// (PH9-CHK-001): the junctioned antenna's normalized receive pattern must equal its
// own normalized transmit gain pattern.

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

// A λ/2 dipole split at its centre into two arms that BOTH start at the origin
// (start-to-start): arm 2 is traversed in reverse, so this is a genuine degree-2
// junction the per-wire receive solver could not handle (it fails fast). It is the
// identical antenna to the single-wire dipole, fed at the junction (wire 1 seg 1).
const RX_JUNCTION: &str = "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 1 10 1 0 0.0 0.0 0.0 10.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
const TX_JUNCTION: &str = "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nRP 0 10 1 0.0 0.0 10.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn junctioned_receive_sweep_solves_and_has_dipole_shape() {
    let out = run(RX_JUNCTION, "rxjunc");
    assert!(
        out.contains("RECEIVE_PATTERN"),
        "junctioned receive geometry must now solve and emit RECEIVE_PATTERN (was fail-fast); \
         got:\n{out}"
    );
    assert!(out.contains("N_POINTS 10"), "expected 10 sweep points");
    let rx = section_rows(&out, "THETA PHI RESPONSE_DB", 2);
    assert_eq!(rx.len(), 10, "receive pattern should have 10 rows");
    // Same physics as the single-wire z-dipole: endfire (θ=0) null, broadside
    // (θ=90) peak, monotonic rise between.
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
    for w in rx.windows(2) {
        assert!(
            w[1].1 >= w[0].1 - 1e-6,
            "receive response should rise toward broadside"
        );
    }
}

#[test]
fn junctioned_receive_matches_transmit_by_reciprocity() {
    // The junctioned antenna's normalized receive pattern must equal its own
    // normalized transmit θ-gain pattern (reciprocity) — the same gate as the
    // single-wire sweep, but with both sides solved on conductor paths.
    let rx = section_rows(&run(RX_JUNCTION, "rxjrecip"), "THETA PHI RESPONSE_DB", 2);
    let tx_raw = section_rows(
        &run(TX_JUNCTION, "txjrecip"),
        "THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO",
        3, // GAIN_V_DB = θ-polarised gain
    );
    assert_eq!(rx.len(), tx_raw.len(), "sweeps must align");
    let tx_peak = tx_raw.iter().map(|r| r.1).fold(f64::MIN, f64::max);
    let mut max_dev = 0.0f64;
    for ((th, rx_db), (_, tx_db)) in rx.iter().zip(&tx_raw) {
        if *rx_db < -100.0 || *tx_db < -100.0 {
            continue; // shared endfire null
        }
        let tx_norm = tx_db - tx_peak;
        let dev = (rx_db - tx_norm).abs();
        println!("θ={th:5.1}  rx={rx_db:8.3}  tx_norm={tx_norm:8.3}  dev={dev:.3}");
        max_dev = max_dev.max(dev);
    }
    assert!(
        max_dev < 0.2,
        "junctioned receive pattern deviates from transmit (reciprocity) by {max_dev:.3} dB"
    );
}
