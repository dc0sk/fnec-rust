// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-117 — a task fault must not evict the worker that reported it.
//!
//! **Why this is a test binary of its own.** It has to exec a stub worker it
//! writes itself, and `cargo test` runs the tests in one binary as threads of
//! one process. If any sibling test forks while the stub's file is still open
//! for writing, the child inherits that writable descriptor, and Linux then
//! refuses to exec the stub with `ETXTBSY` ("Text file busy") until the child
//! reaches its own exec. The siblings in `worker_infinite_vswr.rs` spawn worker
//! processes constantly, so the race fired there — passing under an isolated
//! `--test` run and failing under the full workspace gate. `O_CLOEXEC` does not
//! close the window, because it only takes effect at the child's exec, and the
//! staged-write-then-rename does not either, because the inherited descriptor
//! refers to the inode, not the name.
//!
//! Alone in its process, there is no concurrent fork to inherit anything.

use base64::Engine;
use nec_worker::WorkerPool;

const HEALTHY_DECK: &str = "corpus/dipole-freesp-51seg.nec";

fn task(id: &str, deck_path: &str) -> nec_worker::TaskMessage {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let deck = std::fs::read_to_string(root.join(deck_path))
        .unwrap_or_else(|e| panic!("read {deck_path}: {e}"));
    nec_worker::TaskMessage {
        task_id: id.to_string(),
        deck_hash: "h".to_string(),
        deck_b64: base64::engine::general_purpose::STANDARD.encode(&deck),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.2e6,
    }
}

/// A worker that answers with a complete line that is not a usable result.
///
/// Needed because the two halves of this fix mask each other: once an infinite
/// VSWR round-trips, the negative-resistance deck no longer produces a task
/// fault at all, so a pool test driven by that deck passes with the eviction bug
/// fully restored. It proved the pool survives a deck it CAN price. This stub
/// produces the fault directly.
fn stub_worker_emitting_unusable_results() -> std::path::PathBuf {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("fnec-stub-worker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let staged = dir.join("stub-worker.staging");
    let path = dir.join("stub-worker.sh");

    // Valid JSON, correct framing, missing every required field of TaskResult.
    //
    // Written to a staging name, synced, closed, and only then renamed into the
    // path that gets executed. Writing straight to the exec target races the
    // kernel: Linux returns ETXTBSY ("Text file busy") if the image is still
    // open for writing anywhere, and under full-workspace test parallelism this
    // failed where an isolated `--test` run had passed. The renamed-into path is
    // never itself opened for writing, so the race cannot occur.
    {
        let mut f = std::fs::File::create(&staged).expect("create stub");
        f.write_all(
            b"#!/bin/sh\nwhile IFS= read -r line; do\n  case \"$line\" in\n    *shutdown*) exit 0 ;;\n  esac\n  printf '{\"status\":\"ok\",\"task_id\":\"x\"}\\n'\ndone\n",
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

/// The taxonomy itself: an unusable RESULT is not a dead WORKER.
///
/// The protocol is line-framed and the parse happens after a complete line has
/// been read, so the stream is still in sync and the worker will answer the next
/// task normally. Treating the two alike is what turned one bad frequency into
/// an empty pool.
#[cfg(unix)]
#[test]
fn an_unusable_result_fails_the_task_and_keeps_the_worker() {
    let stub = stub_worker_emitting_unusable_results();
    let mut pool = WorkerPool::new_local(1, stub.to_str().expect("path")).expect("spawn stub");

    let first = pool.dispatch(&task("t-1", HEALTHY_DECK));
    assert!(first.is_err(), "an unusable result must fail its task");
    assert_eq!(
        pool.len(),
        1,
        "a task fault must NOT evict the worker: the line was complete, so the \
         stream is in sync and the worker is healthy"
    );

    // And again — the pool must still be usable, not drained one dispatch at a
    // time until "all workers in pool failed".
    let second = pool.dispatch(&task("t-2", HEALTHY_DECK));
    assert!(second.is_err(), "the stub always answers unusably");
    assert!(
        !format!("{second:?}").contains("all workers in pool failed"),
        "the pool must not have been drained: {second:?}"
    );
    assert_eq!(pool.len(), 1, "still one worker after a second task fault");

    pool.shutdown_all();
    let _ = std::fs::remove_dir_all(stub.parent().expect("dir"));
}
