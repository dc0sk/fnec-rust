// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Controller-side handle for a locally-spawned worker subprocess.
//!
//! [`LocalWorkerHandle`] is the reference implementation for the SSH-backed
//! controller: the only difference in production is that `Command::new(binary)`
//! is replaced by an SSH invocation.  All message framing, dispatch, and result
//! collection logic is identical.

use std::process::{Command, Stdio};

use crate::protocol::{TaskMessage, TaskResult};

/// A handle to a worker process spawned as a local subprocess.
///
/// Used directly in integration tests and as the building block for the
/// SSH-backed worker deployment in PH6-CHK-006.
#[derive(Debug)]
pub struct LocalWorkerHandle {
    pipe: crate::pipe::WorkerPipe,
    /// How long this worker gets to answer one task.
    ///
    /// A field rather than a constant so a test can set it to milliseconds and
    /// assert an OUTCOME — evicted, batch returned, other workers' results
    /// present — instead of waiting on the real deadline or asserting a
    /// duration. An env var would have been a second way to configure it and a
    /// way for a test to leak into a neighbour.
    deadline: std::time::Duration,
}

/// Why a dispatch failed, and therefore whether the worker survives it.
///
/// Before this distinction every failure was one `String`, and the pool treated
/// every `Err` as a dead worker — so a perfectly reproducible, deck-driven
/// failure on one frequency permanently destroyed the pool for every remaining
/// frequency (FND-117). One negative-resistance point in a sweep took out every
/// host.
#[derive(Debug, Clone)]
pub enum DispatchError {
    /// The worker is unusable: the transport broke, the process died, or the
    /// stream is no longer in sync. Evict it and try another.
    Worker(String),
    /// The worker is fine; this particular task produced something the
    /// controller cannot use. Fail the task and keep the worker.
    ///
    /// A result that fails to deserialise belongs here, not in `Worker`: the
    /// protocol is line-framed and the parse happens after a COMPLETE line has
    /// been read, so the stream is still in sync and the next task will be
    /// answered normally.
    Task(String),
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Worker(m) | Self::Task(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for DispatchError {}

impl From<DispatchError> for String {
    fn from(e: DispatchError) -> Self {
        e.to_string()
    }
}

/// An untyped error from a transport helper is a worker fault.
///
/// Every `String` error reaching a dispatch is produced by the connect,
/// reconnect or write path — the worker's health, not the task's. The one
/// failure that is NOT a worker fault, an unreadable result line, is constructed
/// explicitly as `Task` at its own site rather than falling through here.
impl From<String> for DispatchError {
    fn from(m: String) -> Self {
        DispatchError::Worker(m)
    }
}

impl LocalWorkerHandle {
    /// Spawn a worker subprocess.  `binary` is the path to the `fnec` binary.
    ///
    /// The worker is started with `fnec worker --stdio`, which runs the
    /// [`crate::worker::run_worker_stdio`] event loop on its stdin/stdout.
    pub fn spawn(binary: &str) -> Result<Self, std::io::Error> {
        let child = Command::new(binary)
            .arg("worker")
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        Ok(Self {
            pipe: crate::pipe::WorkerPipe::new(child),
            deadline: crate::pipe::DEFAULT_SOLVE_DEADLINE,
        })
    }

    /// Override the answer deadline. For tests, and for a future per-host
    /// setting; the default is [`crate::pipe::DEFAULT_SOLVE_DEADLINE`].
    pub fn set_deadline(&mut self, deadline: std::time::Duration) {
        self.deadline = deadline;
    }

    /// Send a task to the worker and block until the result is received.
    ///
    /// The result is matched to the task by `task_id`; if the worker sends a
    /// result with a different `task_id` it is returned as-is (the protocol
    /// guarantees one result per task in the single-in-flight model).
    pub fn dispatch(&mut self, task: &TaskMessage) -> Result<TaskResult, DispatchError> {
        // Our own request failed to serialise: nothing has been sent, so the
        // worker is untouched and this is the task's problem.
        let json = serde_json::to_string(task).map_err(|e| DispatchError::Task(e.to_string()))?;
        self.pipe.send(&json)?;
        let line = self.pipe.recv(self.deadline)?;
        // A COMPLETE line was read, so the stream is in sync whatever the line
        // says. The worker is healthy; this one result is unusable.
        let result: TaskResult = serde_json::from_str(line.trim())
            .map_err(|e| DispatchError::Task(format!("unreadable result from worker: {e}")))?;
        Ok(result)
    }

    /// Send the shutdown command and give the subprocess a bounded grace period.
    pub fn shutdown(mut self) -> std::io::Result<std::process::ExitStatus> {
        // `Drop` still runs and still kills; killing an already-exited child is
        // a no-op error we ignore, so the graceful path stays graceful.
        self.pipe.shutdown()
    }
}

impl Drop for LocalWorkerHandle {
    fn drop(&mut self) {
        // Killing the child closes its stdout, which is what ends the reader
        // thread — so an evicted worker takes its thread with it.
        self.pipe.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_nonexistent_binary_returns_error() {
        let result = LocalWorkerHandle::spawn("/nonexistent/fnec-binary");
        assert!(result.is_err(), "expected Err, got Ok");
    }

    #[test]
    fn spawn_empty_binary_path_returns_error() {
        let result = LocalWorkerHandle::spawn("");
        assert!(result.is_err(), "expected Err, got Ok");
    }

    #[test]
    fn shutdown_message_is_valid_json() {
        let msg = r#"{"cmd":"shutdown"}"#;
        let val: serde_json::Value = serde_json::from_str(msg).unwrap();
        assert_eq!(val.get("cmd").and_then(|v| v.as_str()), Some("shutdown"));
    }
}
