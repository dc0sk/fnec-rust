// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-102 — one task that kills workers must not kill the pool.
//!
//! A worker that dies holding a task looks to the controller like a dropped
//! connection, so the task was resent to the next worker, and the next, until
//! the pool was empty and every remaining task failed with "all workers in pool
//! failed". The design doc has specified "retries the task once on a different
//! node" all along; it was prose.
//!
//! The distinction that makes the budget safe is *when* the worker died. One
//! that was never reachable did not receive the task, so the task is not
//! implicated and may keep looking for a live worker — that is what lets a
//! healthy task get past dead hosts. Only a worker that died **holding** the
//! task counts against it.
//!
//! Own test binary, per the ETXTBSY analysis in `worker_task_fault.rs`.

use nec_worker::WorkerPool;
use std::io::Write;

/// A worker that answers normally, except that a task whose id contains
/// `poison` makes it exit without a word — a deterministic crash holding that
/// task, which is exactly the shape that drained the pool.
fn poison_stub() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fnec-poison-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let staged = dir.join("stub.staging");
    let path = dir.join("poison-worker.sh");
    {
        let mut f = std::fs::File::create(&staged).expect("create stub");
        f.write_all(
            b"#!/bin/sh\n\
              while IFS= read -r line; do\n\
                case \"$line\" in\n\
                  *shutdown*) exit 0 ;;\n\
                  *poison*)   exit 1 ;;\n\
                esac\n\
                printf '{\"status\":\"ok\",\"task_id\":\"t\",\"frequency_hz\":1.0,\"impedance\":{\"re_ohm\":50.0,\"im_ohm\":0.0},\"vswr_50\":1.0,\"feedpoint_current_mag\":1.0,\"feedpoint_current_phase_deg\":0.0}\\n'\n\
              done\n",
        )
        .expect("write stub");
        f.sync_all().expect("sync stub");
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o755))
            .expect("chmod stub");
    }
    std::fs::rename(&staged, &path).expect("stage stub into place");
    path
}

fn task(id: &str) -> nec_worker::TaskMessage {
    nec_worker::TaskMessage {
        task_id: id.to_string(),
        deck_hash: "h".to_string(),
        deck_b64: "Q0UKRU4K".to_string(),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.2e6,
    }
}

#[cfg(unix)]
#[test]
fn a_poison_task_is_blamed_before_it_empties_the_pool() {
    let stub = poison_stub();
    let mut pool =
        WorkerPool::new_local(4, stub.to_str().expect("path")).expect("spawn 4 poison stubs");
    assert_eq!(pool.len(), 4, "setup");

    let out = pool.dispatch(&task("poison-1"));
    let remaining = pool.len();
    pool.shutdown_all();
    let _ = std::fs::remove_dir_all(stub.parent().expect("dir"));

    let err = out.expect_err("a task that kills every worker it touches must fail");
    assert!(
        err.contains("killed") && err.contains("refusing"),
        "the failure must blame the TASK, not report an empty pool: {err}"
    );
    // The point of the budget: workers are left for the rest of the run. Before
    // it, this was 0 and every later task failed with "all workers in pool
    // failed".
    assert_eq!(
        remaining,
        4 - WorkerPool::MAX_DIED_HOLDING as usize,
        "only the budgeted number of workers may die for one task"
    );
}

/// The control, and the reason the budget keys on *died holding* rather than on
/// any failure: a healthy task must still be able to walk past dead workers.
///
/// Without the `Unreachable` distinction this test fails — a pool whose first
/// workers are unreachable would spend the task's strikes on them and blame a
/// task that nothing has even run yet.
#[cfg(unix)]
#[test]
fn a_healthy_task_still_gets_past_workers_that_were_never_reachable() {
    let stub = poison_stub();
    let mut pool = WorkerPool::new_local(3, stub.to_str().expect("path")).expect("spawn stubs");
    // Kill two workers outright, so they are gone before the task arrives: the
    // write fails, which is `Unreachable`, not `died holding`.
    pool.kill_for_test(2);

    let out = pool.dispatch(&task("healthy-1"));
    pool.shutdown_all();
    let _ = std::fs::remove_dir_all(stub.parent().expect("dir"));

    assert!(
        out.is_ok(),
        "a healthy task must reach the one live worker, not be blamed for the \
         dead ones: {out:?}"
    );
}
