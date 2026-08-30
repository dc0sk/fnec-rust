// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The child-process pipe a worker handle talks over, with deadlines.
//!
//! Both `LocalWorkerHandle` and `SshWorkerHandle` carried the same three fields
//! — a `Child`, its `ChildStdin`, a `BufReader<ChildStdout>` — and their own
//! copy of write-then-`read_line`. That is the repo's dominant defect class in
//! progress, and it had already diverged: a task that failed to serialise was a
//! `Task` fault on the local path and a `Worker` fault on the SSH one, so an
//! unserialisable task evicted a healthy remote worker (FND-136). One core, so
//! they cannot differ again.
//!
//! **Why a reader thread.** `read_line` on a child pipe has no timeout in std,
//! and a worker that accepts a task and never answers wedged it forever
//! (FND-101) — with `dispatch_batch`'s `thread::scope` then withholding the
//! whole batch, including other workers' finished results. The fix is one
//! long-lived thread per worker, spawned once, pumping lines into a channel;
//! `dispatch` becomes a write plus `recv_timeout`.
//!
//! One thread per WORKER, not per dispatch. A thread per dispatch would leak one
//! for every wedged worker; this one exits on its own, because a timeout evicts
//! the handle, `Drop` kills the child, and killing the child closes the pipe,
//! which ends the thread's `read_line` at EOF.
//!
//! **Residual, recorded rather than hidden:** only the READ is bounded. The
//! write can still block if the child stops draining stdin and the task line
//! exceeds the pipe buffer (~64 KiB, which a many-wire deck's base64 can pass).
//! Covering that means moving the write into this thread too and making dispatch
//! a full channel round-trip. It is not done here.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use crate::DispatchError;

/// How long to wait for a worker's answer to one solve task.
///
/// Matched to the kernel's TCP retransmission bound, which is what already
/// limited the *dead-socket* case: the guarantee this makes is "never worse than
/// a dead socket, and bounded where it was infinite". It is also more than five
/// times the slowest legitimate point measured on this hardware — an MPIE sweep
/// point runs in minutes — because a deadline that fires on a slow but healthy
/// solve would be a worse defect than the hang it replaces. A false fire costs
/// one eviction and one retried task: duplicated work, never a wrong number.
pub const DEFAULT_SOLVE_DEADLINE: Duration = Duration::from_secs(15 * 60);

/// How long to wait for a capability probe, which solves a trivial deck.
///
/// Short on purpose and separate from the solve deadline: `connect_all` probes
/// hosts SERIALLY at startup, before any pool exists, so one wedged host used to
/// block the run before it began.
pub const PROBE_DEADLINE: Duration = Duration::from_secs(30);

/// How long a worker gets to exit after being told to, before it is killed.
///
/// The design doc has promised "up to 2 seconds for graceful exit" in prose
/// while `shutdown` called an unbounded `wait()`. Worse, the distributed sweep
/// shuts the pool down BEFORE printing, so a worker wedged here hid a sweep that
/// had already succeeded (FND-137).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// A worker subprocess, its stdin, and a thread pumping its stdout into a
/// channel.
#[derive(Debug)]
pub struct WorkerPipe {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl WorkerPipe {
    /// Take ownership of a spawned child and start pumping its stdout.
    pub fn new(mut child: Child) -> Self {
        let stdin = child.stdin.take().expect("stdin must be piped");
        let stdout = child.stdout.take().expect("stdout must be piped");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    // A closed receiver means the handle is gone; stop reading.
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
        }
    }

    /// Write one line to the worker.
    ///
    /// A failed write is `Unreachable`, not `Worker`: the pipe was already
    /// broken, so nothing was delivered and the task is not implicated in this
    /// worker's death. That distinction is what the retry budget turns on — a
    /// task blamed for workers that never received it would be blamed for a dead
    /// host it had nothing to do with (FND-102).
    pub fn send(&mut self, line: &str) -> Result<(), DispatchError> {
        writeln!(self.stdin, "{line}").map_err(|e| DispatchError::Unreachable(e.to_string()))?;
        self.stdin
            .flush()
            .map_err(|e| DispatchError::Unreachable(e.to_string()))
    }

    /// Wait for one line, up to `deadline`.
    ///
    /// A timeout is a WORKER fault: the peer accepted the task and did not
    /// answer, so it is not usable, whatever the task's own merits.
    pub fn recv(&mut self, deadline: Duration) -> Result<String, DispatchError> {
        match self.lines.recv_timeout(deadline) {
            Ok(line) => Ok(line),
            Err(RecvTimeoutError::Timeout) => Err(DispatchError::Worker(format!(
                "worker did not answer within {}s",
                deadline.as_secs()
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(DispatchError::Worker(
                "worker closed stdout unexpectedly".to_string(),
            )),
        }
    }

    /// Ask the worker to exit, wait `SHUTDOWN_GRACE`, then kill it.
    ///
    /// Bounded on purpose: an unbounded `wait()` here hangs a run that has
    /// already produced every answer it was asked for (FND-137).
    pub fn shutdown(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let _ = writeln!(self.stdin, r#"{{"cmd":"shutdown"}}"#);
        let _ = self.stdin.flush();
        let deadline = std::time::Instant::now() + SHUTDOWN_GRACE;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            if std::time::Instant::now() >= deadline {
                let _ = self.child.kill();
                return self.child.wait();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Kill the child. Closing its stdout is what ends the reader thread.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
