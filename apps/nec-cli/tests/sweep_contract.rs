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
// VERIFIES: FR-007 (deterministic batch/sweep workflows)
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

/// The deck-free half of the flag: `--sweep-config` does not merely *override* an
/// `FR` card, it **supplies** the frequencies when the deck has none.
///
/// That capability was gated by nothing until now, which mattered the moment a
/// refusal for frequency-less decks was added (FND-070): the obvious placement
/// for that refusal — `validate::pre_solve_error`, which every frontend already
/// calls — sees only the deck and would have refused this working case. The
/// refusal is typed on the *resolved* frequency list instead, and this test is
/// what stops a future author from "simplifying" it back onto the deck.
///
/// `docs/cli-guide.md` said "overrides the `FR` card frequency list", which is
/// true and incomplete; it now says it also supplies one.
#[test]
fn sweep_config_supplies_the_frequencies_for_a_deck_with_no_fr_card() {
    const NO_FR_DECK: &str = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nEN\n";
    assert!(
        !NO_FR_DECK.contains("FR"),
        "the fixture must have no FR card, or this test proves nothing"
    );

    let deck = write_temp("no-fr-deck", NO_FR_DECK);
    let cfg = write_temp("no-fr-cfg", "[frequency]\npoints_mhz = [14.0, 14.2]\n");
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--sweep-config")
        .arg(&cfg)
        .arg(&deck)
        .output()
        .expect("failed to run fnec");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "a deck with no FR must still solve when --sweep-config supplies the \
         frequencies: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        freq_values_mhz(&stdout),
        vec![14.0, 14.2],
        "both configured points must be solved: {stdout}"
    );
}

/// The other half: with neither source, the run is refused rather than silently
/// succeeding.
///
/// It used to exit **0 having written zero bytes to stdout AND stderr** — a
/// silent success, indistinguishable from a run that worked — while the GUI and
/// `fnec_py` refused the same deck (FND-070). Both output formats are checked
/// because the `[]` that JSON mode prints for a *solved* deck with no priceable
/// feedpoint must not be reused for a deck that was never solved at all.
#[test]
fn a_deck_with_no_frequency_at_all_is_refused_in_both_output_formats() {
    const NO_FR_DECK: &str = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nEN\n";
    let deck = write_temp("no-freq-deck", NO_FR_DECK);

    for format in ["text", "json"] {
        let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--output-format", format])
            .arg(&deck)
            .output()
            .expect("failed to run fnec");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "a deck with no frequency must not report success (--output-format \
             {format}): {stderr}"
        );
        assert!(
            output.stdout.is_empty(),
            "the refusal must not also emit a report (--output-format {format}): \
             {} byte(s)",
            output.stdout.len()
        );
        assert!(
            stderr.contains("no frequency to solve at"),
            "the refusal must name its reason (--output-format {format}): {stderr}"
        );
        assert!(
            stderr.contains("--sweep-config"),
            "the CLI's remedy must name the second frequency source \
             (--output-format {format}): {stderr}"
        );
    }
}
