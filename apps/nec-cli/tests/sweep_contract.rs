// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Contract tests for the --sweep-config CLI flag (PH3-CHK-006).
//
// Each test writes a temporary NEC deck and a temporary TOML sweep-config file,
// runs the fnec binary with `--sweep-config <path> <deck>`, and validates that:
//   - stdout contains exactly one FREQ_MHZ block per configured frequency point,
//   - block ordering is stable (ascending frequency),
//   - output is machine-parseable.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos()
}

fn write_temp(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("fnec-sweep-{}-{}.tmp", name, unique_suffix()));
    fs::write(&path, body).expect("failed to write temp file");
    path
}

/// Minimal NEC deck with a single half-wave dipole.  The FR card is a single
/// point at 14 MHz; when --sweep-config is supplied the FR card frequency is
/// ignored.
const DIPOLE_DECK: &str =
    "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.0 0.0\nEN\n";

/// Count how many per-block `FREQ_MHZ <number>` lines appear in stdout.
/// Excludes the `FREQ_MHZ TAG SEG ...` table-header line in the SWEEP_POINTS summary.
fn count_freq_blocks(stdout: &str) -> usize {
    freq_values_mhz(stdout).len()
}

/// Return all `FREQ_MHZ` values from stdout, in order.
fn freq_values_mhz(stdout: &str) -> Vec<f64> {
    stdout
        .lines()
        .filter_map(|l| {
            l.strip_prefix("FREQ_MHZ ")
                .and_then(|rest| rest.trim().parse::<f64>().ok())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Test 1 — single explicit point produces exactly one output block
// ---------------------------------------------------------------------------
#[test]
fn sweep_single_explicit_point_produces_one_block() {
    let sweep_toml = "[frequency]\npoints_mhz = [14.2]\n";

    let deck_path = write_temp("deck-single", DIPOLE_DECK);
    let sweep_path = write_temp("sweep-single", sweep_toml);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--sweep-config")
        .arg(&sweep_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&sweep_path);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        count_freq_blocks(&stdout),
        1,
        "expected exactly 1 FREQ_MHZ block for single explicit point, got:\n{stdout}"
    );

    let freqs = freq_values_mhz(&stdout);
    assert!(
        (freqs[0] - 14.2).abs() < 0.0001,
        "expected FREQ_MHZ ~14.2, got {}",
        freqs[0]
    );
}

// ---------------------------------------------------------------------------
// Test 2 — multi-point explicit list produces one block per point
// ---------------------------------------------------------------------------
#[test]
fn sweep_explicit_list_produces_block_per_point() {
    let sweep_toml = "[frequency]\npoints_mhz = [14.0, 15.0, 16.0]\n";

    let deck_path = write_temp("deck-list", DIPOLE_DECK);
    let sweep_path = write_temp("sweep-list", sweep_toml);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--sweep-config")
        .arg(&sweep_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&sweep_path);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        count_freq_blocks(&stdout),
        3,
        "expected 3 FREQ_MHZ blocks for 3-point list, got:\n{stdout}"
    );

    let freqs = freq_values_mhz(&stdout);
    for (got, expected) in freqs.iter().zip([14.0_f64, 15.0, 16.0]) {
        assert!(
            (got - expected).abs() < 0.0001,
            "expected FREQ_MHZ ~{expected}, got {got}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3 — range-based sweep produces correct point count
// ---------------------------------------------------------------------------
#[test]
fn sweep_range_produces_correct_point_count() {
    // 14 to 18 MHz in 1 MHz steps → 5 points: 14, 15, 16, 17, 18
    let sweep_toml = "[frequency]\nstart_mhz = 14.0\nend_mhz = 18.0\nstep_mhz = 1.0\n";

    let deck_path = write_temp("deck-range", DIPOLE_DECK);
    let sweep_path = write_temp("sweep-range", sweep_toml);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--sweep-config")
        .arg(&sweep_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&sweep_path);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        count_freq_blocks(&stdout),
        5,
        "expected 5 FREQ_MHZ blocks for 14-18 MHz at 1 MHz step, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — explicit list output block ordering is stable (FIFO, as specified)
// ---------------------------------------------------------------------------
#[test]
fn sweep_output_block_ordering_is_stable() {
    // Supply points in ascending order and verify output is in same order.
    let sweep_toml = "[frequency]\npoints_mhz = [14.0, 15.0, 16.0, 17.0, 18.0]\n";

    let deck_path = write_temp("deck-order", DIPOLE_DECK);
    let sweep_path = write_temp("sweep-order", sweep_toml);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--sweep-config")
        .arg(&sweep_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&sweep_path);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let freqs = freq_values_mhz(&stdout);
    assert_eq!(freqs.len(), 5, "expected 5 blocks, got {}", freqs.len());

    // Verify blocks are in ascending order (same as input).
    let sorted = {
        let mut v = freqs.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    };
    assert_eq!(
        freqs, sorted,
        "FREQ_MHZ output ordering is not ascending: {freqs:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — output is machine-parseable: each FREQ_MHZ is followed by report
//           headers and structured data lines
// ---------------------------------------------------------------------------
#[test]
fn sweep_output_is_machine_parseable() {
    let sweep_toml = "[frequency]\npoints_mhz = [14.2, 14.4]\n";

    let deck_path = write_temp("deck-parseable", DIPOLE_DECK);
    let sweep_path = write_temp("sweep-parseable", sweep_toml);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--sweep-config")
        .arg(&sweep_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&sweep_path);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Every FREQ_MHZ value must parse as f64.
    let freqs = freq_values_mhz(&stdout);
    assert_eq!(
        freqs.len(),
        2,
        "expected 2 FREQ_MHZ entries, got {}\nstdout:\n{stdout}",
        freqs.len()
    );

    // The primary report header must appear once at the top.
    assert!(
        stdout.starts_with("FNEC FEEDPOINT REPORT\n"),
        "stdout must start with 'FNEC FEEDPOINT REPORT\\n', got:\n{stdout}"
    );

    // Each sweep block must have FEEDPOINTS section.
    assert_eq!(
        stdout.matches("FEEDPOINTS\n").count(),
        2,
        "expected 2 FEEDPOINTS sections (one per frequency), got:\n{stdout}"
    );
}
