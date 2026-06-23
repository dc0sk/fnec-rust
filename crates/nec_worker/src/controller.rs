// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Controller-side handle for a locally-spawned worker subprocess.
//!
//! [`LocalWorkerHandle`] is the reference implementation for the SSH-backed
//! controller: the only difference in production is that `Command::new(binary)`
//! is replaced by an SSH invocation.  All message framing, dispatch, and result
//! collection logic is identical.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::protocol::{TaskMessage, TaskResult};

/// A handle to a worker process spawned as a local subprocess.
///
/// Used directly in integration tests and as the building block for the
/// SSH-backed worker deployment in PH6-CHK-006.
#[derive(Debug)]
pub struct LocalWorkerHandle {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl LocalWorkerHandle {
    /// Spawn a worker subprocess.  `binary` is the path to the `fnec` binary.
    ///
    /// The worker is started with `fnec worker --stdio`, which runs the
    /// [`crate::worker::run_worker_stdio`] event loop on its stdin/stdout.
    pub fn spawn(binary: &str) -> Result<Self, std::io::Error> {
        let mut child = Command::new(binary)
            .arg("worker")
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().expect("stdin must be piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout must be piped"));
        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    /// Send a task to the worker and block until the result is received.
    ///
    /// The result is matched to the task by `task_id`; if the worker sends a
    /// result with a different `task_id` it is returned as-is (the protocol
    /// guarantees one result per task in the single-in-flight model).
    pub fn dispatch(&mut self, task: &TaskMessage) -> Result<TaskResult, String> {
        let json = serde_json::to_string(task).map_err(|e| e.to_string())?;
        writeln!(self.stdin, "{json}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Err("worker closed stdout unexpectedly".to_string());
        }
        let result: TaskResult = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
        Ok(result)
    }

    /// Send the shutdown command and wait for the subprocess to exit gracefully.
    pub fn shutdown(mut self) -> std::io::Result<std::process::ExitStatus> {
        let _ = writeln!(self.stdin, r#"{{"cmd":"shutdown"}}"#);
        let _ = self.stdin.flush();
        self.child.wait()
    }
}

impl Drop for LocalWorkerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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
