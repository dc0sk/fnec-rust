// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Junction-fed feedpoint behavior across the PH9-CHK-002 / PH9-CHK-005 boundary.
//
// PH9-CHK-002 (general junction basis) now solves **degree-2** conductor chains
// on a continuous Hallén path — collinear splits, start-to-start splits, bends,
// and inverted-V apex feeds all give a physical impedance and emit no warning.
//
// The PH9-CHK-005 guardrail remains for the still-deferred out-of-scope classes:
// **degree-3+** (T/Y) junctions and closed loops. Feeding at such a node still
// gives an unreliable per-segment V/I, so the CLI must still warn there.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(deck: &str, name: &str) -> (String, String) {
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
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Parse the first feedpoint `Z_RE` from the FNEC report on stdout.
fn feedpoint_r(stdout: &str) -> f64 {
    let mut in_feed = false;
    for line in stdout.lines() {
        if line.starts_with("FEEDPOINTS") {
            in_feed = true;
            continue;
        }
        if in_feed {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
            if cols.len() >= 8 {
                if let Ok(zre) = cols[6].parse::<f64>() {
                    return zre;
                }
            }
        }
    }
    panic!("no feedpoint row found in:\n{stdout}");
}

// A straight half-wave dipole split into two wires that both START at the origin
// (start-to-start), fed at that junction. Physically the 74.24+j13.9 Ω single-wire
// dipole; PH9-CHK-002 now recovers it on a continuous conductor path.
const SPLIT_DIPOLE_JUNCTION_FED: &str =
    "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Same two wires, fed on wire 1 segment 13 — away from the junction.
const SPLIT_DIPOLE_FED_AWAY: &str =
    "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 13 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Ordinary single-wire dipole: no junction at all.
const SINGLE_WIRE_DIPOLE: &str =
    "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// A dipole modeled as two arms bent 15° at the feed, fed on one arm AWAY from the
// bend (segment 7) — a degree-2 chain. Before PH9-CHK-002 this mis-solved to a
// negative resistance; now it solves to a physical positive R.
const BENT_DIPOLE_FED_AWAY: &str =
    "GW 1 26 0 0 0 1.367 0 5.104 0.001\nGW 2 26 0 0 0 -1.367 0 -5.104 0.001\nGE 0\nEX 0 1 7 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Three wires meeting at the origin (degree-3 T/Y), fed at the node — still out of
// scope for the continuous-path fix, so the guardrail must still fire.
const TEE_JUNCTION_FED: &str =
    "GW 1 13 0 0 0 5.282 0 0 0.001\nGW 2 13 0 0 0 -5.282 0 0 0.001\nGW 3 13 0 0 0 0 0 5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn start_to_start_junction_fed_now_solves() {
    // The headline PH9-CHK-002 case: fed exactly at the degree-2 junction.
    let (stdout, stderr) = run(SPLIT_DIPOLE_JUNCTION_FED, "junction-fed");
    assert!(
        !stderr.contains("wire junction"),
        "a degree-2 junction feed must no longer warn; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("negative resistance"),
        "the split dipole now has physical R>0; stderr:\n{stderr}"
    );
    let r = feedpoint_r(&stdout);
    assert!(
        (r - 74.24).abs() < 2.0,
        "junction-fed split dipole must recover the single-wire ~74.2 Ω; got {r:.3}"
    );
}

#[test]
fn bent_dipole_fed_away_now_solves() {
    // Feed off the bend on a degree-2 bent dipole: previously negative R, now physical.
    let (stdout, stderr) = run(BENT_DIPOLE_FED_AWAY, "bent-fed-away");
    assert!(
        !stderr.contains("negative resistance"),
        "bent degree-2 dipole now solves to positive R; stderr:\n{stderr}"
    );
    assert!(
        feedpoint_r(&stdout) > 0.0,
        "bent dipole resistance must be positive"
    );
}

#[test]
fn split_dipole_fed_away_does_not_warn() {
    // A junction exists in the geometry, but the feed is not on it.
    let (_stdout, stderr) = run(SPLIT_DIPOLE_FED_AWAY, "fed-away");
    assert!(
        !stderr.contains("is on a wire junction"),
        "feed away from the junction should not warn; stderr:\n{stderr}"
    );
}

#[test]
fn single_wire_feedpoint_does_not_warn() {
    let (_stdout, stderr) = run(SINGLE_WIRE_DIPOLE, "single-wire");
    assert!(
        !stderr.contains("is on a wire junction"),
        "single-wire dipole should not warn; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("negative resistance"),
        "single-wire dipole (R>0) must not trip the negative-resistance check; stderr:\n{stderr}"
    );
}

#[test]
fn degree3_tee_junction_still_guarded() {
    // Degree-3 T/Y junction fed at the node: out of scope for PH9-CHK-002, so the
    // PH9-CHK-005 guardrail must still warn (junction + unphysical negative R).
    let (_stdout, stderr) = run(TEE_JUNCTION_FED, "tee-junction");
    assert!(
        stderr.contains("wire junction") && stderr.contains("PH9-CHK-002"),
        "a degree-3 T/Y junction feed must still warn; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("negative resistance"),
        "the still-deferred T/Y result is unphysical and must be flagged; stderr:\n{stderr}"
    );
    // The whole-geometry topology guard also flags the T/Y class explicitly.
    assert!(
        stderr.contains("T/Y junction"),
        "the topology guard must name the T/Y junction class; stderr:\n{stderr}"
    );
}

// A 1λ square loop (perimeter ≈ λ at 14.2 MHz), fed mid-wire — away from every
// corner junction. build_conductor_paths rejects the closed loop, so fnec falls
// back to the per-wire basis and reports an unreliable impedance (≈20 − j1210 Ω
// vs the nec2c truth ≈111 − j146 Ω). The feed is NOT on a junction, so only the
// whole-geometry topology guard catches it.
const SQUARE_LOOP_FED_MIDWIRE: &str =
    "GW 1 11 -2.639 0 0 2.639 0 0 0.001\nGW 2 11 2.639 0 0 2.639 0 5.278 0.001\nGW 3 11 2.639 0 5.278 -2.639 0 5.278 0.001\nGW 4 11 -2.639 0 5.278 -2.639 0 0 0.001\nGE 0\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn closed_loop_is_guarded() {
    // Regression for the previously-silent closed-loop garbage: fnec must now warn
    // that the loop geometry is unsupported, even though the feed is mid-wire (so
    // the feedpoint-at-junction guard alone would miss it).
    let (_stdout, stderr) = run(SQUARE_LOOP_FED_MIDWIRE, "square-loop");
    assert!(
        stderr.contains("closed loop") && stderr.contains("PH9-CHK-002"),
        "a closed loop must be flagged as unsupported; stderr:\n{stderr}"
    );
}
