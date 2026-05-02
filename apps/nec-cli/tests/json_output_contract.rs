// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use std::path::PathBuf;
use std::process::Command;

fn corpus_deck(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("corpus")
        .join(name)
}

fn run_fnec_json(deck: &PathBuf) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--output-format")
        .arg("json")
        .arg(deck)
        .output()
        .unwrap_or_else(|e| panic!("failed to run fnec: {e}"))
}

#[test]
fn json_output_parses_as_valid_json() {
    let deck = corpus_deck("dipole-freesp-51seg.nec");
    let output = run_fnec_json(&deck);
    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is not valid JSON");
    assert!(
        value.is_array(),
        "expected JSON array at top level, got: {value}"
    );
}

#[test]
fn json_output_contains_required_fields() {
    let deck = corpus_deck("dipole-freesp-51seg.nec");
    let output = run_fnec_json(&deck);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let records: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("stdout is not valid JSON");
    assert!(!records.is_empty(), "expected at least one JSON record");
    let rec = &records[0];
    for field in &[
        "freq_mhz",
        "tag",
        "seg",
        "z_re",
        "z_im",
        "z_abs",
        "z_arg_deg",
    ] {
        assert!(
            rec.get(field).is_some(),
            "missing required field '{field}' in JSON record: {rec}"
        );
    }
    // Numeric sanity: freq_mhz > 0
    let freq = rec["freq_mhz"].as_f64().expect("freq_mhz is not a number");
    assert!(freq > 0.0, "freq_mhz must be positive, got {freq}");
}

#[test]
fn json_output_is_stable_across_two_runs() {
    let deck = corpus_deck("dipole-freesp-51seg.nec");
    let out1 = run_fnec_json(&deck);
    let out2 = run_fnec_json(&deck);
    assert!(out1.status.success());
    assert!(out2.status.success());
    let s1 = String::from_utf8_lossy(&out1.stdout);
    let s2 = String::from_utf8_lossy(&out2.stdout);
    // Parse both and compare as values (ignores whitespace differences)
    let v1: serde_json::Value = serde_json::from_str(&s1).expect("run 1 not valid JSON");
    let v2: serde_json::Value = serde_json::from_str(&s2).expect("run 2 not valid JSON");
    assert_eq!(v1, v2, "JSON output differs between two consecutive runs");
}

#[test]
fn json_output_sweep_contains_multiple_records() {
    let deck = corpus_deck("frequency-sweep-dipole.nec");
    let output = run_fnec_json(&deck);
    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let records: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("stdout is not valid JSON");
    assert!(
        records.len() > 1,
        "expected multiple records for frequency-sweep deck, got {}",
        records.len()
    );
    // All records must have freq_mhz in ascending order
    let freqs: Vec<f64> = records
        .iter()
        .map(|r| r["freq_mhz"].as_f64().expect("freq_mhz missing"))
        .collect();
    let mut sorted = freqs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        freqs, sorted,
        "frequency records are not in ascending order"
    );
}

#[test]
fn text_output_is_unchanged_when_output_format_omitted() {
    let deck = corpus_deck("dipole-freesp-51seg.nec");
    let text_output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(deck)
        .output()
        .expect("failed to run fnec");
    assert!(text_output.status.success());
    let stdout = String::from_utf8_lossy(&text_output.stdout);
    // Default output must NOT be JSON
    assert!(
        !stdout.trim_start().starts_with('['),
        "default output should not be JSON"
    );
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "default output should contain text report header"
    );
}
