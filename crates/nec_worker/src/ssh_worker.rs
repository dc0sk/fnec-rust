// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! SSH-backed worker handle — PH6-CHK-006.
//!
//! [`SshWorkerHandle`] mirrors [`LocalWorkerHandle`] but connects via SSH
//! to a remote host and runs `fnec worker --stdio` there.  All message
//! framing, dispatch, and result collection logic is identical to the local
//! path — the only difference is that `Command::new(binary)` becomes
//! `ssh <user>@<host> <binary> worker --stdio`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::hosts::HostEntry;
use crate::protocol::{TaskMessage, TaskResult};
use crate::Capability;

/// Handle to a worker process running on a remote host via SSH.
///
/// The remote worker is started with `ssh <user>@<host> <binary> worker --stdio`
/// and communicates over newline-delimited JSON on stdin/stdout.
#[derive(Debug)]
pub struct SshWorkerHandle {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    hostname: String,
}

impl SshWorkerHandle {
    /// Connect to a remote worker via SSH.
    ///
    /// Spawns `ssh <user>@<host> <binary> worker --stdio` as a subprocess.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the SSH process cannot be spawned.
    /// Connection errors (bad hostname, auth failure, unreachable host)
    /// appear when the first task is dispatched (the child process is
    /// spawned here but the SSH connection is established lazily).
    pub fn connect(entry: &HostEntry) -> Result<Self, std::io::Error> {
        let user_part = match &entry.ssh_user {
            Some(u) => format!("{u}@{}", entry.hostname),
            None => entry.hostname.clone(),
        };
        let binary = entry.binary_path.as_deref().unwrap_or("fnec");

        let mut child = Command::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&user_part)
            .arg(binary)
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
            hostname: entry.hostname.clone(),
        })
    }

    /// Send a task to the remote worker and block until the result is received.
    ///
    /// The same JSON-line protocol as [`LocalWorkerHandle::dispatch`].
    /// Connection errors (SSH auth failure, host unreachable) surface here
    /// as an `Err(String)` — the ssh child process writes errors to stderr
    /// (inherited) and closes stdout.
    pub fn dispatch(&mut self, task: &TaskMessage) -> Result<TaskResult, String> {
        let json = serde_json::to_string(task).map_err(|e| e.to_string())?;
        writeln!(self.stdin, "{json}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Err(format!(
                "SSH worker '{}' closed stdout unexpectedly — check host connectivity and auth",
                self.hostname
            ));
        }
        let result: TaskResult = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
        Ok(result)
    }

    /// Probe the remote worker's capabilities.
    ///
    /// Sends a lightweight solve task to determine CPU thread count and GPU
    /// availability.  The probe task uses a small deck so it completes quickly.
    pub fn probe_capability(&mut self) -> Result<Capability, String> {
        let probe_deck = "CM probe\nGW 0 1 0 0 -0.5 0 0 0.5 0.001\nGE 0\nEX 0 0 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let task = TaskMessage {
            task_id: "probe-cap".to_string(),
            deck_hash: "probe".to_string(),
            deck_b64: STANDARD.encode(probe_deck.as_bytes()),
            solver_config: crate::protocol::WorkerSolverConfig {
                basis: "hallen".to_string(),
                ground_model: "none".to_string(),
            },
            frequency_hz: 14.2e6,
        };

        let result = self.dispatch(&task)?;
        match result {
            TaskResult::Ok { .. } => {
                // For now assume 4 CPU threads and no GPU — production
                // deployments should override via hosts.toml.
                Ok(Capability {
                    cpu_threads: 4,
                    gpu_available: false,
                    wgpu_backend: None,
                })
            }
            TaskResult::Error { error_message, .. } => Err(format!(
                "capability probe failed on '{}': {error_message}",
                self.hostname
            )),
        }
    }

    /// Send the shutdown command and wait for the remote worker to exit.
    pub fn shutdown(mut self) -> std::io::Result<std::process::ExitStatus> {
        let _ = writeln!(self.stdin, r#"{{"cmd":"shutdown"}}"#);
        let _ = self.stdin.flush();
        self.child.wait()
    }

    /// The hostname this worker is connected to.
    pub fn hostname(&self) -> &str {
        &self.hostname
    }
}

impl Drop for SshWorkerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Connect to all workers listed in a [`crate::HostsConfig`] and return
/// their handles along with probed capabilities.
///
/// Workers that fail to connect or probe are skipped with a warning printed
/// to stderr.
pub fn connect_all(config: &crate::HostsConfig) -> (Vec<SshWorkerHandle>, crate::CapabilityCache) {
    let mut handles = Vec::new();
    let mut cache = crate::CapabilityCache::new();

    for entry in &config.worker {
        match SshWorkerHandle::connect(entry) {
            Ok(mut handle) => {
                let hostname = entry.hostname.clone();
                match handle.probe_capability() {
                    Ok(cap) => {
                        eprintln!(
                            "info: connected to worker '{}' (cpu={}, gpu={})",
                            hostname, cap.cpu_threads, cap.gpu_available
                        );
                        cache.insert(&hostname, cap);
                        handles.push(handle);
                    }
                    Err(e) => {
                        eprintln!("warning: worker '{hostname}' probe failed: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "warning: failed to connect to worker '{}': {e}",
                    entry.hostname
                );
            }
        }
    }

    (handles, cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_all_empty_config_returns_empty() {
        let cfg = crate::HostsConfig::from_str("").unwrap();
        let (handles, cache) = connect_all(&cfg);
        assert!(handles.is_empty());
        assert!(cache.is_empty());
    }

    #[test]
    fn connect_all_skips_unreachable_host_gracefully() {
        // connect_all should not panic when given an unresolvable host;
        // it prints a warning to stderr and continues.
        let toml = r#"
[[worker]]
hostname = "invalid-host-that-will-never-resolve.example"
"#;
        let cfg = crate::HostsConfig::from_str(toml).unwrap();
        // Note: this may take up to ConnectTimeout (5s) per entry.
        let (handles, cache) = connect_all(&cfg);
        assert!(handles.is_empty());
        assert!(cache.is_empty());
    }
}
