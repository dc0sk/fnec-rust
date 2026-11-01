// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-307 across a real process boundary: the worker checks its GPU-resident
// solve's currents against the f64 problem, as the CLI does, and declines to
// the routed CPU solve above the shared bound, saying so in `warnings`. Before,
// the worker computed no residual of the device's currents at all. The bound is
// set through `FNEC_DEVICE_REL_RESIDUAL_MAX`, which the spawned
// `fnec worker --stdio` inherits — so this is the only test in its binary.
//
// The control (the bound at 1) tells whether this host's device answers at all:
// where it does, the forced decline must name itself and report `cpu`; where no
// adapter is, both runs are the CPU's and the decline cannot be observed (SKIP).

use base64::Engine;

const ENV: &str = nec_solver::validate::DEVICE_REL_RESIDUAL_MAX_ENV;
const DIPOLE: &str =
    "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

/// `(exec_used, warnings)` of one `exec: gpu` task on a fresh worker.
fn gpu_task(max: &str) -> (String, Vec<String>) {
    std::env::set_var(ENV, max);
    let mut worker = nec_worker::LocalWorkerHandle::spawn(env!("CARGO_BIN_EXE_fnec"))
        .expect("spawn fnec worker --stdio");
    std::env::remove_var(ENV);
    let task = nec_worker::TaskMessage {
        task_id: "fnd307".to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: base64::engine::general_purpose::STANDARD.encode(DIPOLE),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "gpu".to_string(),
        },
        frequency_hz: 14.2e6,
    };
    let result = worker.dispatch(&task).expect("dispatch");
    worker.shutdown().expect("shutdown");
    match result {
        nec_worker::TaskResult::Ok {
            exec_used,
            warnings,
            ..
        } => (exec_used, warnings),
        other => panic!("the dipole solves: {other:?}"),
    }
}

#[test]
fn the_worker_declines_a_device_answer_that_misses_the_f64_problem() {
    let declines = |ws: &[String]| ws.iter().filter(|w| w.contains("FND-307")).count();
    let (control_exec, control_warnings) = gpu_task("1");
    assert_eq!(declines(&control_warnings), 0, "{control_warnings:?}");
    let (exec, warnings) = gpu_task("1e-30");
    assert_eq!(exec, "cpu", "{warnings:?}");
    if control_exec != "gpu" {
        eprintln!("SKIP: no wgpu adapter on this host — the worker's decline not observed");
        assert_eq!(declines(&warnings), 0, "{warnings:?}");
        return;
    }
    assert_eq!(declines(&warnings), 1, "{warnings:?}");
}
