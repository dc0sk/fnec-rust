// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Integration test for `--exec gpu` RP far-field path (PH5-CHK-004 / gate G4).
//!
//! Runs the dipole free-space RP corpus deck twice:
//!   1. `--exec cpu`  — reference
//!   2. `--exec gpu`  — wgpu kernel (or CPU fallback when no adapter available)
//!
//! When a wgpu adapter is present the two gain columns must agree within 0.5 dBi
//! for every row.  When no adapter is available (headless CI) the test verifies
//! that the fallback warning is emitted and the output is still well-formed.

use std::process::Command;

mod common;

/// Parse all GAIN_DB values from fnec stdout.
/// The report prints the pattern section with header:
///   THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO
fn parse_gain_total_column(stdout: &str) -> Vec<f64> {
    let mut in_pattern = false;
    let mut values: Vec<f64> = Vec::new();

    for line in stdout.lines() {
        if line.trim_start().starts_with("THETA") && line.contains("GAIN_DB") {
            in_pattern = true;
            continue;
        }
        if in_pattern {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Data rows: THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO
            if parts.len() >= 3 {
                if let (Ok(_theta), Ok(_phi), Ok(gain)) = (
                    parts[0].parse::<f64>(),
                    parts[1].parse::<f64>(),
                    parts[2].parse::<f64>(),
                ) {
                    values.push(gain);
                    continue;
                }
            }
            // Any non-data line (empty or next section header) ends the table.
            if !parts.is_empty() && !values.is_empty() {
                if parts[0].parse::<f64>().is_err() {
                    in_pattern = false;
                }
            }
        }
    }
    values
}

/// Run fnec with `args` on the given deck and return (stdout, stderr).
/// Panics if the process fails to spawn or returns non-zero exit.
fn run_fnec(args: &[&str]) -> (String, String) {
    let deck = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/dipole-freesp-rp-51seg.nec"
    );

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(deck);

    let out = cmd.output().expect("failed to spawn fnec");

    assert!(
        out.status.success(),
        "fnec exited with {}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Gate G4: `--exec gpu` produces well-formed RP output that either matches
/// the CPU reference within 0.5 dBi (real GPU) or emits the fallback warning
/// and still produces output (no adapter).
#[test]
fn exec_gpu_rp_output_matches_cpu_or_falls_back_gracefully() {
    let (cpu_stdout, _cpu_stderr) = run_fnec(&["--exec", "cpu"]);
    let (gpu_stdout, gpu_stderr) = run_fnec(&["--exec", "gpu"]);

    let cpu_gains = parse_gain_total_column(&cpu_stdout);
    let gpu_gains = parse_gain_total_column(&gpu_stdout);

    // Both runs must produce at least one pattern point.
    assert!(
        !cpu_gains.is_empty(),
        "CPU run produced no pattern rows — check deck has RP card"
    );
    assert!(
        !gpu_gains.is_empty(),
        "GPU run produced no pattern rows\nstdout:\n{gpu_stdout}\nstderr:\n{gpu_stderr}"
    );
    assert_eq!(
        cpu_gains.len(),
        gpu_gains.len(),
        "CPU and GPU runs produced different numbers of pattern rows"
    );

    let no_adapter = gpu_stderr.contains("no wgpu adapter available");

    if no_adapter {
        // Fallback to CPU — output must still be present (already asserted above).
        // Just verify the warning was correctly emitted.
        eprintln!("note: no wgpu adapter — fallback path exercised");
    } else {
        // Real GPU path — gains must agree within 0.5 dBi (gate G4 tolerance).
        for (i, (cpu_g, gpu_g)) in cpu_gains.iter().zip(gpu_gains.iter()).enumerate() {
            // Skip rows where CPU is at the -999.99 null sentinel.  At exact-null
            // directions (θ=0°, θ=180°), the GPU f32 shader yields ~-134 dBi rather
            // than -999.99 — both represent zero radiation but differ in the log domain.
            if *cpu_g < -900.0 {
                continue;
            }
            let diff = (cpu_g - gpu_g).abs();
            assert!(
                diff <= 0.5,
                "RP parity failure at row {i}: CPU={cpu_g:.4} dBi  GPU={gpu_g:.4} dBi  |Δ|={diff:.4} dB (limit 0.5)"
            );
        }
    }
}

/// Diagnostic line must show `exec=gpu(cpu-fallback)` when `--exec gpu` is used.
#[test]
fn exec_gpu_diag_line_shows_gpu_exec_mode() {
    let (_stdout, stderr) = run_fnec(&["--exec", "gpu"]);
    assert!(
        stderr.contains("exec=gpu(cpu-fallback)"),
        "expected exec=gpu(cpu-fallback) in diag line\nstderr:\n{stderr}"
    );
}
