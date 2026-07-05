// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-004: PT (print-control) card runtime semantics — filter the segment
// current output by mode / tag / segment range.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_stdout(deck: &str, name: &str) -> String {
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

/// Distinct wire tags appearing in the CURRENTS section.
fn current_tags(stdout: &str) -> Vec<u32> {
    let mut in_sec = false;
    let mut tags = Vec::new();
    for line in stdout.lines() {
        if line.trim() == "CURRENTS" {
            in_sec = true;
            continue;
        }
        if in_sec {
            let c: Vec<&str> = line.split_whitespace().collect();
            if c.first() == Some(&"TAG") {
                continue;
            }
            match c.first().and_then(|t| t.parse::<u32>().ok()) {
                Some(t) => {
                    if !tags.contains(&t) {
                        tags.push(t);
                    }
                }
                None => break,
            }
        }
    }
    tags
}

// Two parallel dipoles (a 2-element parasitic array): tag 1 driven at its centre,
// tag 2 parasitic. Both carry currents; no junction, no collinear merge.
const BASE_TWO_WIRE: &str =
    "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGW 2 21 2.0 0 -5.282 2.0 0 5.282 0.001\nGE 0\n{PT}EX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

fn deck_with_pt(pt: &str) -> String {
    BASE_TWO_WIRE.replace("{PT}", pt)
}

#[test]
fn pt_mode_zero_prints_all_currents() {
    let out = run_stdout(&deck_with_pt("PT 0 0 0 0\n"), "pt-all");
    assert!(
        out.contains("CURRENTS"),
        "mode 0 must keep the CURRENTS section"
    );
    assert_eq!(
        current_tags(&out),
        vec![1, 2],
        "mode 0 should print currents for all wires"
    );
}

#[test]
fn pt_negative_mode_suppresses_currents() {
    let out = run_stdout(&deck_with_pt("PT -1 0 0 0\n"), "pt-suppress");
    assert!(
        !out.contains("CURRENTS"),
        "PT mode -1 must suppress the CURRENTS section; got:\n{out}"
    );
    // The rest of the report is still produced.
    assert!(out.contains("FEEDPOINTS"), "feedpoint report must remain");
}

#[test]
fn pt_positive_mode_restricts_to_tag() {
    let out = run_stdout(&deck_with_pt("PT 1 2 0 0\n"), "pt-tag2");
    assert!(
        out.contains("CURRENTS"),
        "tag filter keeps the CURRENTS section"
    );
    assert_eq!(
        current_tags(&out),
        vec![2],
        "PT mode 1 tag 2 should print only wire-2 currents"
    );
}

#[test]
fn no_pt_card_prints_all_currents() {
    let out = run_stdout(&deck_with_pt(""), "pt-none");
    assert_eq!(
        current_tags(&out),
        vec![1, 2],
        "without a PT card all currents are printed"
    );
}
