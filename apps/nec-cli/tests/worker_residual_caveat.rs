// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-302 across a real process boundary: the worker's caveat crosses the wire
// in its `warnings` when the residual is above the threshold. The threshold is
// set through `FNEC_HALLEN_REL_RESIDUAL_CAVEAT`, which the spawned
// `fnec worker --stdio` inherits — so this is the only test in its binary: the
// variable is process-wide, and a sibling test would see it.

use base64::Engine;

const DIPOLE: &str =
    "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

fn warnings_from_a_fresh_worker() -> Vec<String> {
    let mut worker = nec_worker::LocalWorkerHandle::spawn(env!("CARGO_BIN_EXE_fnec"))
        .expect("spawn fnec worker --stdio");
    let task = nec_worker::TaskMessage {
        task_id: "fnd302".to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: base64::engine::general_purpose::STANDARD.encode(DIPOLE),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.2e6,
    };
    let result = worker.dispatch(&task).expect("dispatch");
    worker.shutdown().expect("shutdown");
    match result {
        nec_worker::TaskResult::Ok { warnings, .. } => warnings,
        other => panic!("the dipole solves: {other:?}"),
    }
}

#[test]
fn the_worker_sends_the_residual_caveat_when_the_threshold_is_crossed() {
    std::env::set_var("FNEC_HALLEN_REL_RESIDUAL_CAVEAT", "1e-30");
    let fired = warnings_from_a_fresh_worker();
    std::env::remove_var("FNEC_HALLEN_REL_RESIDUAL_CAVEAT");
    assert_eq!(
        fired.iter().filter(|w| w.contains("FND-302")).count(),
        1,
        "{fired:?}"
    );
    let quiet = warnings_from_a_fresh_worker();
    assert!(quiet.iter().all(|w| !w.contains("FND-302")), "{quiet:?}");
}
