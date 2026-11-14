// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-326 — a worker's answer is paired with the task it names.
//!
//! Both worker handles took the next line on the worker's stdout as the answer
//! to the task just written. That stdout is shared with whatever the remote
//! login shell prints, so one `.bashrc` `echo` was read as task 0's answer, task
//! 0's answer as task 1's, and every later impedance of a sweep printed against
//! the next frequency. These tests drive a real `fnec worker --stdio` behind a
//! stub that writes something first, through `WorkerPool::new_local` — the same
//! `WorkerPipe` the SSH handle uses.
//!
//! **Own test binary**, per the `ETXTBSY` analysis in `worker_task_fault.rs`:
//! every test here execs a stub it wrote. Several tests share the binary, so
//! the pool is spawned with `worker_poison_budget.rs`'s bounded retry.

use base64::Engine;
use nec_worker::{TaskMessage, TaskResult, WorkerPool};
use std::io::Write;
use std::time::{Duration, Instant};

mod common;

/// The 21-segment dipole the finding was reproduced on.
const DECK: &str =
    "CM dipole\nCE\nGW 1 21 0 0 -5.3 0 0 5.3 0.001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 4 0 0 13 0.5\nEN\n";

const SWEEP_HZ: [f64; 4] = [13.0e6, 13.5e6, 14.0e6, 14.5e6];

const FNEC: &str = env!("CARGO_BIN_EXE_fnec");

fn task(id: &str, frequency_hz: f64) -> TaskMessage {
    TaskMessage {
        task_id: id.to_string(),
        deck_hash: "na".to_string(),
        deck_b64: base64::engine::general_purpose::STANDARD.encode(DECK),
        solver_config: nec_worker::WorkerSolverConfig::default(),
        frequency_hz,
    }
}

/// The sweep as the CLI builds it: ids `na-{fidx}`.
fn sweep() -> Vec<TaskMessage> {
    SWEEP_HZ
        .iter()
        .enumerate()
        .map(|(i, &f)| task(&format!("na-{i}"), f))
        .collect()
}

/// A shell script at a fresh path: staged, synced, closed, renamed into place,
/// so the exec target is never a file open for writing.
fn stub(body: &str) -> (common::TempDir, std::path::PathBuf) {
    let dir = common::TempDir::new("stray-line");
    let staged = dir.join("stub.staging");
    let path = dir.join("stub-worker.sh");
    {
        let mut f = std::fs::File::create(&staged).expect("create stub");
        f.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
            .expect("write stub");
        f.sync_all().expect("sync stub");
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o755))
            .expect("chmod stub");
    }
    std::fs::rename(&staged, &path).expect("stage stub into place");
    (dir, path)
}

/// A stub that writes `first` to stdout and then becomes the real worker.
fn worker_after(first: &str) -> (common::TempDir, std::path::PathBuf) {
    stub(&format!("{first}\nexec '{FNEC}' \"$@\""))
}

/// Spawn a pool, retrying while the stub reads as "Text file busy" (a sibling
/// test's fork; see `worker_poison_budget.rs`).
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

/// `(frequency_hz, re, im)` of each outcome, failing on any that is not solved.
fn solved(outcomes: Vec<nec_worker::DispatchOutcome>) -> Vec<(f64, f64, f64)> {
    outcomes
        .into_iter()
        .enumerate()
        .map(|(i, o)| match o {
            Ok((
                TaskResult::Ok {
                    frequency_hz,
                    impedance,
                    ..
                },
                _,
            )) => (frequency_hz, impedance.re_ohm, impedance.im_ohm),
            other => panic!("point {i} was not solved: {other:?}"),
        })
        .collect()
}

/// The control every pairing test compares against: the bare worker.
fn bare_sweep() -> Vec<(f64, f64, f64)> {
    let mut pool = WorkerPool::new_local(1, FNEC).expect("spawn fnec");
    let out = pool.dispatch_batch(&sweep());
    pool.shutdown_all();
    solved(out)
}

/// Every point carries its own frequency and the bare worker's impedance for it.
fn assert_paired(got: &[(f64, f64, f64)]) {
    let want = bare_sweep();
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g.0, SWEEP_HZ[i], "point {i} answered another frequency");
        // Same binary, same deterministic solve.
        assert_eq!(g, w, "point {i}: {g:?} is not the bare worker's {w:?}");
    }
}

/// T2: a banner before the worker starts is skipped; every point is solved,
/// at its own frequency, with its own impedance. Without the pairing slot 0 is
/// "unusable result" and slots 1–3 carry the previous point's frequency.
#[cfg(unix)]
#[test]
fn a_banner_line_does_not_shift_the_sweep() {
    let (dir, path) = worker_after("echo 'banner from .bashrc'");
    let mut pool = spawn_pool_retrying(1, path.to_str().expect("path"));
    let out = pool.dispatch_batch(&sweep());
    pool.shutdown_all();
    drop(dir);
    assert_paired(&solved(out));
}

/// T3: a VALID result naming another task, at a frequency not in the sweep, is
/// skipped too. A matcher that skips only unparseable lines takes it as point
/// 0's answer — the case that separates the id check from the junk filter.
#[cfg(unix)]
#[test]
fn a_stale_result_line_does_not_shift_the_sweep() {
    let stale = r#"{"status":"ok","task_id":"stale","frequency_hz":99000000.0,"impedance":{"re_ohm":1.0,"im_ohm":2.0},"vswr_50":50.0,"feedpoint_current_mag":1.0,"feedpoint_current_phase_deg":0.0,"mesh":"free-end-thirds"}"#;
    let (dir, path) = worker_after(&format!("printf '%s\\n' '{stale}'"));
    let mut pool = spawn_pool_retrying(1, path.to_str().expect("path"));
    let out = pool.dispatch_batch(&sweep());
    pool.shutdown_all();
    drop(dir);
    assert_paired(&solved(out));
}

/// T4, the negative control for strictness: a task the worker could not read
/// is answered with `task_id: "unknown"` (FND-060), and that answer reaches the
/// task at once. A matcher that skipped `"unknown"` would wait out the deadline
/// and evict a healthy worker. The task's frequency is NaN, which JSON writes
/// as `null`, so the real worker cannot decode the task line.
#[cfg(unix)]
#[test]
fn an_unknown_answer_reaches_its_task_at_once() {
    let mut pool = WorkerPool::new_local(1, FNEC).expect("spawn fnec");
    pool.set_deadline(Duration::from_secs(30));
    let start = Instant::now();
    let unreadable = pool.dispatch(&task("na-0", f64::NAN));
    let took = start.elapsed();
    // And a task that decodes but carries a deck that is not base64: the
    // worker's error names the real id.
    let mut bad_deck = task("na-1", 13.0e6);
    bad_deck.deck_b64 = "not base64 !!".to_string();
    let named = pool.dispatch(&bad_deck);
    let kept = pool.len();
    pool.shutdown_all();
    match unreadable {
        Ok((TaskResult::Error { task_id, .. }, _)) => assert_eq!(task_id, "unknown"),
        other => panic!("the unreadable task must get the worker's error: {other:?}"),
    }
    assert!(
        took < Duration::from_secs(10),
        "took {took:?}: waited, not paired"
    );
    match named {
        Ok((TaskResult::Error { task_id, .. }, _)) => assert_eq!(task_id, "na-1"),
        other => panic!("a bad deck must get the worker's error: {other:?}"),
    }
    assert_eq!(kept, 1, "the worker stays");
}

/// T5: the right id at another frequency is the worker's bug, not a stray: that
/// task fails as unusable, and the worker stays.
#[cfg(unix)]
#[test]
fn the_right_id_at_another_frequency_fails_the_task_and_keeps_the_worker() {
    let (dir, path) = stub(
        r#"while IFS= read -r line; do
  case "$line" in *shutdown*) exit 0 ;; esac
  id=$(printf '%s' "$line" | sed 's/.*"task_id":"\([^"]*\)".*/\1/')
  printf '{"status":"ok","task_id":"%s","frequency_hz":1.0,"impedance":{"re_ohm":50.0,"im_ohm":0.0},"vswr_50":1.0,"feedpoint_current_mag":1.0,"feedpoint_current_phase_deg":0.0,"mesh":"free-end-thirds"}\n' "$id"
done"#,
    );
    let mut pool = spawn_pool_retrying(1, path.to_str().expect("path"));
    let out = pool.dispatch(&task("na-0", 13.0e6));
    let kept = pool.len();
    pool.shutdown_all();
    drop(dir);
    let err = out.expect_err("an answer at another frequency is not this task's number");
    assert!(
        err.contains("unusable result") && err.contains("13000000"),
        "{err}"
    );
    assert_eq!(kept, 1, "a Task fault keeps the worker");
}

/// T6: a worker that only babbles is evicted by the stray cap, well inside its
/// deadline, and the task carries a strike for each — two such workers and the
/// task is blamed. Run on a thread with its own bound, so a missing cap reads as
/// a red test and not a 30 s wait (as `worker_deadline.rs` does).
#[cfg(unix)]
#[test]
fn a_worker_that_only_babbles_is_evicted_by_the_cap() {
    let (dir, path) = stub("exec yes banner");
    let mut pool = spawn_pool_retrying(2, path.to_str().expect("path"));
    pool.set_deadline(Duration::from_secs(30));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let start = Instant::now();
        let out = pool.dispatch(&task("na-0", 13.0e6));
        let _ = tx.send((out, start.elapsed(), pool.len()));
        pool.shutdown_all();
    });
    let (out, took, left) = rx
        .recv_timeout(Duration::from_secs(25))
        .expect("the cap did not fire: the dispatch waited on its deadline");
    drop(dir);
    let err = out.expect_err("a worker that never answers cannot solve the task");
    assert!(
        err.contains("killed 2"),
        "two strikes, the task blamed: {err}"
    );
    assert!(err.contains("none the answer"), "{err}");
    assert!(took < Duration::from_secs(15), "took {took:?}");
    assert_eq!(left, 0, "both babbling workers are evicted");
}
