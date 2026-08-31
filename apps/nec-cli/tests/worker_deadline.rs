// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-101 — a worker that accepts a task and never answers must not wedge the
//! run.
//!
//! There was no timeout of any kind in the worker crate: `read_line` on a child
//! pipe blocks forever, and `dispatch_batch`'s `thread::scope` joins every
//! thread before returning, so ONE silent worker withheld the whole batch —
//! including other workers' finished results.
//!
//! **Its own test binary**, like `worker_task_fault.rs`: it execs a stub it
//! writes, and `cargo test` runs a binary's tests as threads of one process, so
//! a sibling forking while the stub's file is open for writing gives the child
//! that descriptor and Linux refuses the exec with `ETXTBSY`. Alone in its
//! process there is no concurrent fork.
//!
//! **No sleeps in assertions.** The stub wedges *permanently*, so there is no
//! race in either direction: with a deadline the dispatch returns, without one
//! it never does. Sabotaging the deadline would therefore HANG rather than fail,
//! which is a bad way to learn something broke — so the batch runs on a spawned
//! thread and the test thread waits on a channel with its own bound, turning
//! that hang into a red test with a message that says what happened.

use nec_worker::WorkerPool;
use std::io::Write;
use std::time::Duration;

/// A worker that reads its task and never writes anything back. Not a crash and
/// not a disconnect: the process stays alive and the pipe stays open, which is
/// the only case that hung forever (a dead socket already errored out via the
/// kernel's retransmission timeout).
fn wedging_stub() -> std::path::PathBuf {
    // Per-TEST, not per-process. Both tests in this file call this, cargo runs
    // them as threads of one process, and the loser's cleanup deletes the
    // winner's stub mid-run. Third time this shape has bitten in this session.
    let dir = std::env::temp_dir().join(format!(
        "fnec-wedge-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let staged = dir.join("stub.staging");
    let path = dir.join("wedge-worker.sh");
    {
        let mut f = std::fs::File::create(&staged).expect("create stub");
        f.write_all(b"#!/bin/sh\ncat > /dev/null\n")
            .expect("write stub");
        f.sync_all().expect("sync stub");
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o755))
            .expect("chmod stub");
    }
    // Staged then renamed: the exec target is never the file that was open for
    // writing. See worker_task_fault.rs for why that is not sufficient on its
    // own, and why this test binary is separate.
    std::fs::rename(&staged, &path).expect("stage stub into place");
    path
}

/// Remove the directory the stub lives in. Each test owns its own.
fn clean_up(stub: &std::path::Path) {
    if let Some(dir) = stub.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Spawn a pool, retrying while the stub reads as "Text file busy".
///
/// See `worker_poison_budget.rs` for why: `ETXTBSY` here is a transient
/// consequence of a sibling test's fork, not of our own writes, so it cannot be
/// ordered away and a bounded retry is the direct answer.
fn spawn_pool_retrying(count: usize, binary: &str) -> WorkerPool {
    for _ in 0..200 {
        match WorkerPool::new_local(count, binary) {
            Ok(pool) => return pool,
            Err(e) if e.contains("Text file busy") => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("spawn stub: {e}"),
        }
    }
    panic!("stub stayed 'Text file busy' for 2s, which is no longer a fork race");
}

fn task(id: &str) -> nec_worker::TaskMessage {
    use base64::Engine;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let deck = std::fs::read_to_string(root.join("corpus/dipole-freesp-51seg.nec")).expect("deck");
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

/// FND-137 — shutdown is bounded too.
///
/// `shutdown` called an unbounded `child.wait()`, and the distributed sweep
/// shuts the pool down BEFORE printing, so a worker that wedges on the shutdown
/// command hid a sweep that had already succeeded: every point solved, every
/// result computed, nothing on screen and no exit. The design doc has promised
/// "up to 2 seconds for graceful exit" in prose the whole time.
///
/// The stub ignores the shutdown command and never exits, so this is bounded by
/// the grace period or not at all.
#[cfg(unix)]
#[test]
fn a_worker_that_ignores_shutdown_is_killed_rather_than_waited_on() {
    let stub = wedging_stub();
    let stub_path = stub.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let pool = spawn_pool_retrying(1, stub.to_str().expect("path"));
        pool.shutdown_all();
        let _ = tx.send(());
    });

    rx.recv_timeout(Duration::from_secs(30))
        .expect("shutdown never returned: the grace period is not being applied");
    clean_up(&stub_path);
}

#[cfg(unix)]
#[test]
fn a_worker_that_never_answers_is_evicted_rather_than_waited_on() {
    let stub = wedging_stub();
    let stub_path = stub.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let mut pool = spawn_pool_retrying(1, stub.to_str().expect("path"));
        pool.set_deadline(Duration::from_millis(200));
        let out = pool.dispatch(&task("t-1"));
        let remaining = pool.len();
        let _ = tx.send((out.is_err(), remaining));
    });

    // Generous, and not an assertion about timing: it only has to be longer than
    // the 200 ms deadline. Without a deadline nothing is ever sent and this
    // fails with a message naming the cause, instead of the suite hanging.
    let (failed, remaining) = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("dispatch never returned: the answer deadline is not being applied");

    assert!(failed, "a wedged worker's dispatch must fail, not succeed");
    assert_eq!(
        remaining, 0,
        "a worker that never answers is a worker fault and must be evicted"
    );
    clean_up(&stub_path);
}
