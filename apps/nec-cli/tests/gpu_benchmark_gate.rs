// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Gate G5 (PH5-CHK-005): CPU-vs-GPU benchmark regression gate.
//!
//! Runs `corpus/dipole-freesp-rp-large-grid.nec` (37×73 = 2701 RP points)
//! under `--exec cpu` and `--exec gpu`, takes the best (minimum) of several
//! wall-clock measurements for each, and asserts that the GPU path is no more
//! than 50% slower than the CPU path.
//!
//! On a host with a real wgpu adapter the `--exec gpu` path dispatches actual
//! wgpu kernels (RP far-field batch / Z-matrix fill). Each measurement is a
//! fresh process spawn, so the GPU path pays a fixed wgpu device-initialization
//! cost (tens of ms) on every invocation. For a workload that solves in a few
//! hundred ms that fixed cost is a structural floor on the ratio, not a
//! dispatch regression — so the gate guards against *gross* overhead (>50%)
//! rather than fine-grained deltas, and uses best-of-N timing to reject the
//! positive-only scheduling noise that made a tight median-based gate flaky.
//!
//! When no wgpu adapter is present the GPU path falls back to the CPU stub and
//! the timing comparison is meaningless; the gate detects that and skips.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

fn deck_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("corpus/dipole-freesp-rp-large-grid.nec")
}

struct RunResult {
    elapsed: std::time::Duration,
    stderr: String,
}

fn run_timed(exec_mode: &str) -> RunResult {
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

    RunResult {
        elapsed,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Best-case (minimum) timing. Wall-clock noise is positive-only (scheduling,
/// page faults, device-init jitter only ever *add* time), so the minimum over
/// several repetitions is the most stable estimator of each path's true cost.
fn best_us(times: &[u64]) -> u64 {
    times.iter().copied().min().expect("non-empty timing set")
}

/// Gate G5: GPU path must not be more than 50% slower than the CPU path
/// on the large RP grid (37×73 = 2701 observation points).
///
/// Uses the best of several repetitions to reject OS scheduling noise.
#[test]
fn gpu_exec_not_more_than_50_percent_slower_than_cpu() {
    const REPS: usize = 7;
    let mut cpu_us = [0u64; REPS];
    let mut gpu_us = [0u64; REPS];
    let mut gpu_fallback = false;

    for i in 0..REPS {
        let cpu = run_timed("cpu");
        let gpu = run_timed("gpu");
        cpu_us[i] = cpu.elapsed.as_micros() as u64;
        gpu_us[i] = gpu.elapsed.as_micros() as u64;
        if gpu.stderr.contains("no wgpu adapter available") {
            gpu_fallback = true;
        }
    }

    // When running in CI without a hardware GPU the wgpu path falls back to
    // the CPU stub.  The timing comparison is meaningless (and noisy) in that
    // environment, so we only enforce the gate when a real adapter was used.
    if gpu_fallback {
        eprintln!("G5 gate: no hardware GPU adapter — timing gate skipped (software fallback)");
        return;
    }

    let cpu_best = best_us(&cpu_us);
    let gpu_best = best_us(&gpu_us);

    let ratio = gpu_best as f64 / cpu_best as f64;
    let limit = 1.5_f64;

    eprintln!(
        "G5 gate: cpu_best={cpu_best}µs  gpu_best={gpu_best}µs  ratio={ratio:.3}  limit={limit:.2}×"
    );

    assert!(
        ratio <= limit,
        "G5 regression: GPU best={gpu_best}µs exceeds {limit:.2}× CPU best={cpu_best}µs \
         (ratio={ratio:.3}). GPU dispatch path has too much overhead."
    );
}
