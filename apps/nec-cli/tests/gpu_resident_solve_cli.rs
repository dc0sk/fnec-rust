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

/// The GPU-resident f32 solve must never hand back an answer it got wrong.
///
/// `A = MᴴM` squares `cond(M)`, so the f32 solve degrades as the system grows. The
/// corpus decks above are 51 segments, where it is accurate; at 301 segments it
/// used to diverge and the result was reported anyway — one frequency point came
/// back at 101 Ω against the CPU's 75 Ω, and another at **−1.98 Ω**, a negative
/// resistance for a passive antenna. Nothing checked, so nothing warned.
///
/// The solve now reports its own relative residual and the host rejects a
/// non-converged one, falling back to the f64 CPU solve. This test drives the size
/// where that used to break.
#[test]
fn gpu_resident_never_reports_a_diverged_solve() {
    const TOL_OHM: f64 = 2.0;
    // 301 segments, three frequency points — two of which diverged before the gate.
    let deck = "\
GW 1 301 0 0 -5.282 0 0 5.282 0.001
GE
EX 0 1 151 0 1.0 0.0
FR 0 3 0 0 14.0 0.15
EN
";
    let path = std::env::temp_dir().join("fnec-gpu-resident-301seg.nec");
    std::fs::write(&path, deck).expect("write deck");

    let run = |exec: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--solver", "hallen", "--exec", exec])
            .arg(&path)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn fnec: {e}"));
        assert!(out.status.success(), "fnec --exec {exec} failed: {out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        // SWEEP_POINTS rows: FREQ_MHZ TAG SEG Z_RE Z_IM
        let z: Vec<(f64, f64)> = stdout
            .lines()
            .skip_while(|l| !l.starts_with("SWEEP_POINTS"))
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() == 5 {
                    Some((f[3].parse().ok()?, f[4].parse().ok()?))
                } else {
                    None
                }
            })
            .collect();
        (z, stderr)
    };

    let (cpu, _) = run("cpu");
    let (gpu, gpu_stderr) = run("gpu");
    let _ = std::fs::remove_file(&path);

    if gpu_stderr.contains("no wgpu adapter available") {
        eprintln!("SKIP: no wgpu adapter on this host — the GPU solve never ran");
        return;
    }

    assert_eq!(cpu.len(), 3, "expected 3 sweep points, got {cpu:?}");
    assert_eq!(
        gpu.len(),
        cpu.len(),
        "point count differs: {gpu:?} vs {cpu:?}"
    );

    for (i, ((gr, gx), (cr, cx))) in gpu.iter().zip(cpu.iter()).enumerate() {
        assert!(
            *gr > 0.0,
            "point {i}: GPU reported a negative resistance {gr:.3} Ω (CPU {cr:.3})"
        );
        let dr = (gr - cr).abs();
        let dx = (gx - cx).abs();
        assert!(
            dr <= TOL_OHM && dx <= TOL_OHM,
            "point {i}: GPU {gr:.3}+j{gx:.3} vs CPU {cr:.3}+j{cx:.3} \
             (ΔR={dr:.3} ΔX={dx:.3} Ω, tolerance {TOL_OHM})"
        );
    }
}
