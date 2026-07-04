// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-005: guardrail for the junction-fed feedpoint limitation. Feeding a
// segment that sits at a wire junction gives an unreliable (often unphysical)
// impedance in fnec's per-segment V/I; the CLI must warn instead of silently
// reporting it. Accurate junction-fed impedance is PH9-CHK-002.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_stderr(deck: &str, name: &str) -> String {
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
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// A straight half-wave dipole split into two wires joined at the origin, fed at
// that junction. Physically identical to the 74.24+j13.9 Ω single-wire dipole,
// but the junction feed makes fnec's per-segment V/I unphysical.
const SPLIT_DIPOLE_JUNCTION_FED: &str =
    "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Same two wires, but fed on wire 1 segment 13 — away from the junction.
const SPLIT_DIPOLE_FED_AWAY: &str =
    "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 13 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Ordinary single-wire dipole: no junction at all.
const SINGLE_WIRE_DIPOLE: &str =
    "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn junction_fed_feedpoint_emits_warning() {
    let stderr = run_stderr(SPLIT_DIPOLE_JUNCTION_FED, "junction-fed");
    assert!(
        stderr.contains("wire junction") && stderr.contains("PH9-CHK-002"),
        "junction-fed feedpoint should warn; stderr:\n{stderr}"
    );
}

#[test]
fn feedpoint_away_from_junction_does_not_warn() {
    // A junction exists in the geometry, but the feed is not on it.
    let stderr = run_stderr(SPLIT_DIPOLE_FED_AWAY, "fed-away");
    assert!(
        !stderr.contains("is on a wire junction"),
        "feed away from the junction should not warn; stderr:\n{stderr}"
    );
}

#[test]
fn single_wire_feedpoint_does_not_warn() {
    let stderr = run_stderr(SINGLE_WIRE_DIPOLE, "single-wire");
    assert!(
        !stderr.contains("is on a wire junction"),
        "single-wire dipole should not warn; stderr:\n{stderr}"
    );
}
