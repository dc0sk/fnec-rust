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
// VERIFIES: FR-008 (stable structured automation output)
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

// ---------------------------------------------------------------------------
// FND-291 — every feedpoint, not the first alone
// ---------------------------------------------------------------------------

mod common;

/// Two dipoles 2.5 m apart, driven 1 V and j1 V: the two feedpoints differ, so
/// an array that repeated the first would be visible (a symmetric pair, like
/// `corpus/multi-source.nec`, could not tell).
const TWO_SOURCES: &str = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 2.5 0 -2.5 2.5 0 2.5 0.001\nGE 0\nEX 0 1 11 0 1 0\nEX 0 2 11 0 0 1\nFR 0 1 0 0 29.9 0\nEN\n";

fn run_fnec(args: &[&str], deck: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(deck)
        .output()
        .unwrap_or_else(|e| panic!("failed to run fnec: {e}"))
}

fn json_records(out: &std::process::Output) -> Vec<serde_json::Value> {
    assert!(
        out.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is not valid JSON")
}

const FEED_FIELDS: [&str; 6] = ["tag", "seg", "z_re", "z_im", "z_abs", "z_arg_deg"];

/// The text report's FEEDPOINTS rows: `(tag, seg, z_re, z_im)`.
fn text_feedpoints(stdout: &str) -> Vec<(i64, i64, f64, f64)> {
    stdout
        .lines()
        .skip_while(|l| !l.starts_with("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM"))
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            (
                t[0].parse().expect("tag"),
                t[1].parse().expect("seg"),
                t[6].parse().expect("z_re"),
                t[7].parse().expect("z_im"),
            )
        })
        .collect()
}

/// FND-291: the record carries both feedpoints, each the text report's row of
/// the same build, the first identical to the top-level fields — and the pair is
/// nec2c's, so the second element is the second source's impedance and not some
/// other row's.
///
/// The oracle for "which numbers" is nec2c 1.3.1 on this deck (+ `XQ`):
/// 81.320 + j210.13 at tag 1 seg 11 and 61.129 + j37.061 at tag 2 seg 11
/// (absolute 32). fnec at this 21-segment mesh is 2.6 % and 2.7 % off in |ΔZ|/|Z|
/// (76.480 + j206.985, 60.677 + j35.150); both codes converge on the same pair
/// as the mesh refines (at 61 segments 1.2 % and 1.3 %), so the bound is the
/// coarse mesh's, not an offset. A record that repeated the first feedpoint
/// would put 76.48 at the second — 25 % from nec2c.
#[test]
fn json_carries_every_feedpoint_of_a_two_source_deck() {
    let deck = common::TempDeck::new("fnec-json-two-sources.nec", TWO_SOURCES);
    let records = json_records(&run_fnec(&["--output-format", "json"], &deck));
    assert_eq!(records.len(), 1, "one record per frequency: {records:?}");
    let rec = &records[0];
    let feeds = rec["feedpoints"].as_array().expect("a feedpoints array");
    assert_eq!(feeds.len(), 2, "both feedpoints: {rec}");

    // The first element IS the top level, bit for bit.
    for f in FEED_FIELDS {
        assert_eq!(feeds[0][f], rec[f], "feedpoints[0].{f} vs top level: {rec}");
    }

    // Each element is the text report's row of the same build (6 decimals).
    let text = run_fnec(&[], &deck);
    assert!(text.status.success());
    let rows = text_feedpoints(&String::from_utf8_lossy(&text.stdout));
    assert_eq!(rows.len(), 2, "the text report's FEEDPOINTS rows");
    for (f, (tag, seg, re, im)) in feeds.iter().zip(&rows) {
        assert_eq!(f["tag"].as_i64(), Some(*tag), "{f}");
        assert_eq!(f["seg"].as_i64(), Some(*seg), "{f}");
        assert!(
            (f["z_re"].as_f64().unwrap() - re).abs() < 1e-6,
            "{f} vs {re}"
        );
        assert!(
            (f["z_im"].as_f64().unwrap() - im).abs() < 1e-6,
            "{f} vs {im}"
        );
    }

    // nec2c, absolutely.
    for (f, (tag, want_re, want_im)) in feeds.iter().zip([(1, 81.320, 210.13), (2, 61.129, 37.061)])
    {
        assert_eq!(f["tag"].as_i64(), Some(tag), "{f}");
        assert_eq!(f["seg"].as_i64(), Some(11), "{f}");
        let (re, im) = (f["z_re"].as_f64().unwrap(), f["z_im"].as_f64().unwrap());
        let err = ((re - want_re).powi(2) + (im - want_im).powi(2)).sqrt();
        let mag = (want_re * want_re + want_im * want_im).sqrt();
        assert!(
            err / mag < 0.035,
            "tag {tag}: {re} + j{im} vs nec2c {want_re} + j{want_im} ({:.2} %)",
            100.0 * err / mag
        );
        // `z_abs` and `z_arg_deg` describe this element's own impedance.
        assert!(
            (f["z_abs"].as_f64().unwrap() - re.hypot(im)).abs() < 1e-9,
            "{f}"
        );
        assert!(
            (f["z_arg_deg"].as_f64().unwrap() - im.atan2(re).to_degrees()).abs() < 1e-9,
            "{f}"
        );
    }
}

/// The controls: one source gives a one-element array equal to the top level;
/// a sweep gives one array per record; a receive deck still prints a bare `[]`
/// (no record is written for a point with no feedpoint, so there is no record to
/// hold an empty array); and the unvalidated-solver caveat stays on the record.
#[test]
fn json_feedpoints_controls() {
    // One source.
    let records = json_records(&run_fnec(
        &["--output-format", "json"],
        &corpus_deck("dipole-freesp-51seg.nec"),
    ));
    for rec in &records {
        let feeds = rec["feedpoints"].as_array().expect("always present");
        assert_eq!(feeds.len(), 1, "{rec}");
        for f in FEED_FIELDS {
            assert_eq!(feeds[0][f], rec[f], "{f}: {rec}");
        }
    }

    // A sweep: every record has its own one-element array, at its own impedance.
    let records = json_records(&run_fnec(
        &["--output-format", "json"],
        &corpus_deck("frequency-sweep-dipole.nec"),
    ));
    assert!(records.len() > 1);
    for rec in &records {
        let feeds = rec["feedpoints"].as_array().expect("always present");
        assert_eq!(feeds.len(), 1, "{rec}");
        assert_eq!(feeds[0]["z_re"], rec["z_re"], "{rec}");
    }
    assert_ne!(records[0]["z_re"], records[1]["z_re"], "the points differ");

    // A receive deck: the whole document is the empty array, as before.
    let out = run_fnec(
        &["--output-format", "json"],
        &corpus_deck("dipole-ex1-freesp-51seg.nec"),
    );
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[]");

    // An unvalidated solver: the caveat is the record's, not each feedpoint's.
    let records = json_records(&run_fnec(
        &[
            "--output-format",
            "json",
            "--solver",
            "pulse",
            "--experimental-solver",
        ],
        &corpus_deck("dipole-freesp-51seg.nec"),
    ));
    let rec = &records[0];
    assert!(rec["caveat"].is_string(), "{rec}");
    let feeds = rec["feedpoints"].as_array().expect("always present");
    assert_eq!(feeds.len(), 1, "{rec}");
    assert!(feeds[0].get("caveat").is_none(), "{rec}");
}
