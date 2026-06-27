// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH7-CHK-003 CLI gate: `--exec gpu` (GPU-resident Hallén fill+solve) must
//! produce feedpoint impedance within 2 Ω of `--exec cpu` on free-space corpus
//! decks in the supported class.
//!
//! When no wgpu adapter is available the GPU path falls back to the CPU solve,
//! so the two runs are identical and the test still passes (Δ = 0).

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run `fnec --solver hallen --exec <mode> <deck>` and return (Z_RE, Z_IM) of
/// the first FEEDPOINTS row.
fn feedpoint_impedance(deck: &str, exec: &str) -> (f64, f64) {
    let deck_path = workspace_root().join("corpus").join(deck);
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", exec])
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn fnec: {e}"));
    assert!(
        out.status.success(),
        "fnec --exec {exec} {deck} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    let mut lines = stdout.lines();
    // Find the FEEDPOINTS header, skip the column header, read the first data row.
    let row = loop {
        match lines.next() {
            Some("FEEDPOINTS") => {
                let _cols = lines.next(); // "TAG SEG V_RE ... Z_RE Z_IM"
                break lines.next().expect("missing feedpoint data row");
            }
            Some(_) => continue,
            None => panic!("no FEEDPOINTS section in output for {deck} ({exec})"),
        }
    };
    let f: Vec<&str> = row.split_whitespace().collect();
    // TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
    let z_re: f64 = f[6].parse().expect("Z_RE parse");
    let z_im: f64 = f[7].parse().expect("Z_IM parse");
    (z_re, z_im)
}

#[test]
fn gpu_resident_matches_cpu_within_2_ohm_on_corpus() {
    const DECKS: &[&str] = &[
        "dipole-freesp-51seg.nec",
        "dipole-freesp-rp-51seg.nec",
        "dipole-freesp-gm-inplace-shifted.nec",
    ];
    const TOL_OHM: f64 = 2.0;

    for deck in DECKS {
        let (cpu_r, cpu_x) = feedpoint_impedance(deck, "cpu");
        let (gpu_r, gpu_x) = feedpoint_impedance(deck, "gpu");
        let dr = (gpu_r - cpu_r).abs();
        let dx = (gpu_x - cpu_x).abs();
        eprintln!(
            "PH7-CHK-003 CLI: {deck}  Z_cpu=({cpu_r:.3}+j{cpu_x:.3})  Z_gpu=({gpu_r:.3}+j{gpu_x:.3})  ΔR={dr:.4}  ΔX={dx:.4}"
        );
        assert!(
            dr <= TOL_OHM && dx <= TOL_OHM,
            "{deck}: GPU-resident impedance differs from CPU by ΔR={dr:.4} ΔX={dx:.4} Ω (> {TOL_OHM} Ω)"
        );
    }
}
