// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Contract tests for the --vars CLI flag (PH3-CHK-007).
//
// Each test writes a temporary NEC deck template and a temporary vars file,
// runs the fnec binary with `--vars <path> <deck>`, and validates the output.

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

fn write_temp(name: &str, ext: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("fnec-vars-{}-{}.{}", name, unique_suffix(), ext));
    fs::write(&path, body).expect("failed to write temp file");
    path
}

// ---------------------------------------------------------------------------
// Test 1 — TOML vars file: token substitution produces a valid, solvable deck
// ---------------------------------------------------------------------------
#[test]
fn vars_toml_substitution_produces_valid_deck() {
    let deck =
        "GW 1 51 0 0 -$HALF 0 0 $HALF 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 $FREQ 0.0\nEN\n";
    let vars_toml = "HALF = \"5.282\"\nFREQ = \"14.2\"\n";

    let deck_path = write_temp("deck-toml", "nec", deck);
    let vars_path = write_temp("vars-toml", "toml", vars_toml);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--vars")
        .arg(&vars_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&vars_path);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "expected report on stdout, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FREQ_MHZ 14.200000"),
        "expected FREQ_MHZ 14.200000 in stdout, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — JSON vars file: substitution works from .json file
// ---------------------------------------------------------------------------
#[test]
fn vars_json_substitution_produces_valid_deck() {
    let deck =
        "GW 1 51 0 0 -$HALF 0 0 $HALF 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 $FREQ 0.0\nEN\n";
    let vars_json = "{\"HALF\": \"5.282\", \"FREQ\": \"14.2\"}";

    let deck_path = write_temp("deck-json", "nec", deck);
    let vars_path = write_temp("vars-json", "json", vars_json);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--vars")
        .arg(&vars_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&vars_path);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("FREQ_MHZ 14.200000"),
        "expected FREQ_MHZ 14.200000 in stdout, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — undefined variable causes a non-zero exit and an error message
// ---------------------------------------------------------------------------
#[test]
fn vars_undefined_token_causes_error_exit() {
    let deck = "GW 1 51 0 0 -$MISSING 0 0 $MISSING 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let vars_toml = "# no MISSING key\n";

    let deck_path = write_temp("deck-undef", "nec", deck);
    let vars_path = write_temp("vars-undef", "toml", vars_toml);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--vars")
        .arg(&vars_path)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);
    let _ = fs::remove_file(&vars_path);

    assert!(
        !output.status.success(),
        "fnec should have failed on undefined variable"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains("MISSING"),
        "expected error mentioning 'MISSING' on stderr, got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — without --vars, a plain deck without tokens parses normally
// ---------------------------------------------------------------------------
#[test]
fn plain_deck_without_vars_still_works() {
    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    let deck_path = write_temp("deck-plain", "nec", deck);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed on plain deck:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("FNEC FEEDPOINT REPORT"));
}

// ---------------------------------------------------------------------------
// Test 5 — corpus variable-dipole.nec with dipole-vars.toml runs end-to-end
// ---------------------------------------------------------------------------
#[test]
fn corpus_variable_dipole_with_toml_vars() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/variable-dipole.nec");
    let vars_path = workspace_root.join("corpus/dipole-vars.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--vars")
        .arg(&vars_path)
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"));

    assert!(
        output.status.success(),
        "corpus variable-dipole failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.contains("FREQ_MHZ 14.200000"),
        "expected FREQ_MHZ 14.200000 in corpus output, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FEEDPOINTS"),
        "expected FEEDPOINTS section in corpus output, got:\n{stdout}"
    );
}
