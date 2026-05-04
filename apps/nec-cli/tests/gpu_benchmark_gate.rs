// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Gate G5 (PH5-CHK-005): CPU-vs-GPU benchmark regression gate.
//!
//! Runs `corpus/dipole-freesp-rp-large-grid.nec` (37×73 = 2701 RP points)
//! under `--exec cpu` and `--exec gpu`, takes the median of three wall-clock
//! measurements for each, and asserts that the GPU path is no more than 25%
//! slower than the CPU path (GPU ≥ 0.8× CPU speed).
//!
//! Both paths currently run identical CPU-backed code (the GPU path falls
//! back to the CPU stub). The gate exists as a regression guard so that any
//! future GPU dispatch wiring cannot silently introduce >20% overhead.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

fn deck_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("corpus/dipole-freesp-rp-large-grid.nec")
}

fn run_timed(exec_mode: &str) -> std::time::Duration {
    let deck = deck_path();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(["--solver", "hallen", "--exec", exec_mode])
        .env_remove("FNEC_ACCEL_STUB_GPU")
        .arg(&deck);

    let start = Instant::now();
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn fnec: {e}"));
    let elapsed = start.elapsed();

    assert!(
        out.status.success(),
        "fnec --exec {exec_mode} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    elapsed
}

fn median_us(times: &mut [u64]) -> u64 {
    times.sort_unstable();
    times[times.len() / 2]
}

/// Gate G5: GPU path must not be more than 25% slower than the CPU path
/// on the large RP grid (37×73 = 2701 observation points).
///
/// Uses the median of 3 repetitions to reduce OS scheduling noise.
#[test]
fn gpu_exec_not_more_than_25_percent_slower_than_cpu() {
    const REPS: usize = 3;
    let mut cpu_us = [0u64; REPS];
    let mut gpu_us = [0u64; REPS];

    for i in 0..REPS {
        cpu_us[i] = run_timed("cpu").as_micros() as u64;
        gpu_us[i] = run_timed("gpu").as_micros() as u64;
    }

    let cpu_med = median_us(&mut cpu_us);
    let gpu_med = median_us(&mut gpu_us);

    let ratio = gpu_med as f64 / cpu_med as f64;
    let limit = 1.25_f64;

    eprintln!(
        "G5 gate: cpu_median={cpu_med}µs  gpu_median={gpu_med}µs  ratio={ratio:.3}  limit={limit:.2}×"
    );

    assert!(
        ratio <= limit,
        "G5 regression: GPU median={gpu_med}µs exceeds {limit:.2}× CPU median={cpu_med}µs \
         (ratio={ratio:.3}). GPU dispatch path has too much overhead."
    );
}
