// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-227 — a worker on another mesh is refused, not blended into a sweep.
//!
//! Free wire ends are refined into thirds since FND-227, so an older worker
//! answers a different discretisation of the same deck. A worker is a separately
//! installed binary and a mixed-version pool is normal (`protocol.rs`), so the
//! pool refuses a result whose `mesh` is not this build's: that task fails,
//! naming the worker, and the worker stays in the pool.

use base64::Engine;

fn task() -> nec_worker::TaskMessage {
    let deck = include_str!("../../../corpus/dipole-freesp-51seg.nec");
    nec_worker::TaskMessage {
        task_id: "t-mesh".to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: base64::engine::general_purpose::STANDARD.encode(deck.as_bytes()),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.2e6,
    }
}

/// A stand-in for an fnec from before the refinement: it answers every task
/// with a well-formed result that carries no `mesh`.
fn old_worker() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fnec-worker-mesh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("old-fnec");
    std::fs::write(
        &path,
        "#!/bin/sh\nwhile read -r line; do\n  case \"$line\" in *shutdown*) exit 0;; esac\n  \
         echo '{\"status\":\"ok\",\"task_id\":\"t-mesh\",\"frequency_hz\":14200000.0,\
         \"impedance\":{\"re_ohm\":78.83,\"im_ohm\":42.44},\"vswr_50\":1.9,\
         \"feedpoint_current_mag\":0.011,\"feedpoint_current_phase_deg\":-28.3}'\ndone\n",
    )
    .expect("write stand-in");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

#[test]
fn a_worker_on_another_mesh_is_refused_and_kept() {
    let old = old_worker();
    // A test thread forking while the script was being written can still hold it
    // open for writing ("Text file busy"); that clears within moments.
    let mut pool = (0..100)
        .find_map(
            |_| match nec_worker::WorkerPool::new_local(1, old.to_str().expect("utf-8")) {
                Ok(p) => Some(p),
                Err(e) if e.contains("Text file busy") => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    None
                }
                Err(e) => panic!("spawn: {e}"),
            },
        )
        .expect("the stand-in worker spawns");
    let err = pool
        .dispatch(&task())
        .expect_err("a result on another mesh must not be an answer");
    assert!(
        err.contains("different mesh") && err.contains("unrefined free ends"),
        "{err}"
    );
    assert_eq!(
        pool.len(),
        1,
        "the worker stays: the task failed, not the host"
    );
    pool.shutdown_all();
    let _ = std::fs::remove_dir_all(old.parent().expect("dir"));
}

/// The negative control: this build's own worker is on this build's mesh.
#[test]
fn a_worker_on_the_same_mesh_answers() {
    let mut pool = nec_worker::WorkerPool::new_local(1, env!("CARGO_BIN_EXE_fnec")).expect("spawn");
    let (result, _) = pool.dispatch(&task()).expect("same build, same mesh");
    assert!(result.is_ok(), "{result:?}");
    pool.shutdown_all();
}
