// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-006: `fnec --ground-solver sommerfeld` must correct the near-ground
// feedpoint impedance of a low horizontal dipole to the surface-wave-inclusive
// (nec2c GN2) value, flipping the radiation-resistance delta positive where the
// default `--ground-solver rcm` gives the wrong-signed reflection-coefficient result.
//
// nec2c reference (14.2 MHz, horizontal λ/2 dipole 0.025 λ over εr=13/σ=0.005),
// ΔR = R(ground) − R(free space): GN2 (truth) = +9.0, GN0 (RCM) = −24.

use std::process::Command;

fn feedpoint_r(extra_args: &[&str], deck: &str) -> f64 {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "fnec-somm-cli-{}.nec",
        std::process::id() as u64 + fastrand_seed()
    ));
    std::fs::write(&path, deck).expect("write deck");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.arg("--solver").arg("hallen");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.arg(&path).output().expect("run fnec");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .find_map(|line| {
            let p: Vec<&str> = line.split_whitespace().collect();
            // FEEDPOINTS row: TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
            if p.len() >= 8 && p[0] == "1" && p[1] == "11" {
                p[6].parse::<f64>().ok()
            } else {
                None
            }
        })
        .expect("no feedpoint row")
}

// Tiny unique-ish suffix so parallel test cases don't collide on the temp file.
fn fastrand_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64
}

const FREE: &str =
    "GW 1 21 -5.278 0 0 5.278 0 0 0.001\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
const GROUND: &str =
    "GW 1 21 -5.278 0 0.528 5.278 0 0.528 0.001\nGN 0 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn ground_solver_sommerfeld_flips_low_dipole_delta_r_positive() {
    let r_free = feedpoint_r(&[], FREE);
    let r_rcm = feedpoint_r(&["--ground-solver", "rcm"], GROUND);
    let r_somm = feedpoint_r(&["--ground-solver", "sommerfeld"], GROUND);

    let dr_rcm = r_rcm - r_free;
    let dr_somm = r_somm - r_free;

    // Default RCM undershoots: ΔR strongly negative.
    assert!(
        dr_rcm < -20.0,
        "rcm ΔR should be strongly negative; got {dr_rcm:.1} (free {r_free:.1}, rcm {r_rcm:.1})"
    );
    // Sommerfeld flips it positive, matching nec2c GN2 truth (+9).
    assert!(
        dr_somm > 0.0 && (dr_somm - 9.0).abs() < 7.0,
        "sommerfeld ΔR should be positive near nec2c GN2 +9; got {dr_somm:.1} (somm {r_somm:.1})"
    );
    // The surface-wave correction is a large, opposite-sign shift from RCM.
    assert!(
        dr_somm - dr_rcm > 30.0,
        "surface-wave correction should be a large positive shift; got {:.1}",
        dr_somm - dr_rcm
    );
}

#[test]
fn ground_solver_rcm_is_the_unchanged_default() {
    // Omitting --ground-solver must equal --ground-solver rcm (no behavior change).
    let r_default = feedpoint_r(&[], GROUND);
    let r_rcm = feedpoint_r(&["--ground-solver", "rcm"], GROUND);
    assert!(
        (r_default - r_rcm).abs() < 1e-6,
        "default must equal rcm; got {r_default:.4} vs {r_rcm:.4}"
    );
}
