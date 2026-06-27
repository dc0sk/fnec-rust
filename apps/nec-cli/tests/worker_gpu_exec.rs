// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH7-CHK-004: distributed GPU dispatch through the worker pool.
//!
//! Mixed-pool test — two spawned `fnec` workers receive tasks with mixed
//! `exec` preferences (one `gpu`, one `cpu`). Both succeed and produce feedpoint
//! impedance within 2 Ω of the local CPU reference. When a wgpu adapter is
//! present the `exec=gpu` task reports `exec_used = "gpu"`; otherwise it falls
//! back to CPU, which the worker also reports honestly.

use base64::Engine;

const DIPOLE_DECK: &str = include_str!("../../../corpus/dipole-freesp-51seg.nec");

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

fn task(task_id: &str, freq: f64, exec: &str) -> nec_worker::TaskMessage {
    nec_worker::TaskMessage {
        task_id: task_id.to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: b64(DIPOLE_DECK),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: exec.to_string(),
        },
        frequency_hz: freq,
    }
}

#[test]
fn mixed_pool_gpu_and_cpu_dispatch_matches_local() {
    let fnec = env!("CARGO_BIN_EXE_fnec");
    let mut gpu_worker = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn gpu worker");
    let mut cpu_worker = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn cpu worker");

    let freq = 14.2e6;
    let r_gpu = gpu_worker
        .dispatch(&task("gpu-task", freq, "gpu"))
        .expect("gpu-task dispatch");
    let r_cpu = cpu_worker
        .dispatch(&task("cpu-task", freq, "cpu"))
        .expect("cpu-task dispatch");

    assert!(r_gpu.is_ok() && r_cpu.is_ok(), "both tasks must succeed");

    let local = nec_worker::solve::solve_deck_at_frequency(DIPOLE_DECK, freq, "hallen")
        .expect("local reference solve");

    for (label, result) in [("gpu", &r_gpu), ("cpu", &r_cpu)] {
        if let nec_worker::TaskResult::Ok {
            impedance,
            exec_used,
            ..
        } = result
        {
            eprintln!(
                "PH7-CHK-004 pool: {label}-lane exec_used={exec_used} Z=({:.3}+j{:.3}) local=({:.3}+j{:.3})",
                impedance.re_ohm, impedance.im_ohm, local.impedance_re, local.impedance_im
            );
            assert!(
                (impedance.re_ohm - local.impedance_re).abs() <= 2.0
                    && (impedance.im_ohm - local.impedance_im).abs() <= 2.0,
                "{label}-lane impedance differs from local CPU by >2 Ω"
            );
        } else {
            panic!("{label}-lane returned an error result");
        }
    }

    // The cpu-lane must always report a CPU solve.
    if let nec_worker::TaskResult::Ok { exec_used, .. } = &r_cpu {
        assert_eq!(exec_used, "cpu", "cpu lane must use CPU");
    }

    gpu_worker.shutdown().ok();
    cpu_worker.shutdown().ok();
}
