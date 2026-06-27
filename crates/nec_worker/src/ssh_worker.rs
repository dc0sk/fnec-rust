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
    ssh_user: Option<String>,
    binary_path: Option<String>,
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
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
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
            ssh_user: entry.ssh_user.clone(),
            binary_path: entry.binary_path.clone(),
        })
    }

    /// Build the `user@host` part used for SSH commands.
    fn user_part(&self) -> String {
        match &self.ssh_user {
            Some(u) => format!("{u}@{}", self.hostname),
            None => self.hostname.clone(),
        }
    }

    /// Send a task to the remote worker and block until the result is received.
    ///
    /// The same JSON-line protocol as [`LocalWorkerHandle::dispatch`].
    /// Connection errors (SSH auth failure, host unreachable) surface here
    /// as an `Err(String)` — the ssh child process writes errors to stderr
    /// (inherited) and closes stdout.
    ///
    /// If the connection drops mid-task, a single reconnection is attempted
    /// automatically before returning an error.
    pub fn dispatch(&mut self, task: &TaskMessage) -> Result<TaskResult, String> {
        let json = serde_json::to_string(task).map_err(|e| e.to_string())?;

        let send_ok = writeln!(self.stdin, "{json}")
            .and_then(|_| self.stdin.flush())
            .is_ok();

        if !send_ok {
            eprintln!(
                "info: ssh worker '{}' write failed, reconnecting...",
                self.hostname
            );
            self.reconnect()?;
            writeln!(self.stdin, "{json}").map_err(|e| e.to_string())?;
            self.stdin.flush().map_err(|e| e.to_string())?;
        }

        let mut line = String::new();
        if self.stdout.read_line(&mut line).is_err() || line.is_empty() {
            eprintln!(
                "info: ssh worker '{}' read failed (empty/eof), reconnecting...",
                self.hostname
            );
            self.reconnect()?;
            writeln!(self.stdin, "{json}").map_err(|e| e.to_string())?;
            self.stdin.flush().map_err(|e| e.to_string())?;
            line.clear();
            self.stdout
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            if line.is_empty() {
                return Err(format!(
                    "SSH worker '{}' still disconnected after reconnect",
                    self.hostname
                ));
            }
        }

        let result: TaskResult = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
        Ok(result)
    }

    /// Re-establish the SSH subprocess connection to the remote worker.
    ///
    /// Kills the existing child process and spawns a new SSH connection
    /// using the same parameters as [`connect`].
    pub fn reconnect(&mut self) -> Result<(), String> {
        let _ = self.child.kill();
        let _ = self.child.wait();

        let user_part = self.user_part();
        let binary = self.binary_path.as_deref().unwrap_or("fnec");

        let mut child = Command::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&user_part)
            .arg(binary)
            .arg("worker")
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("reconnect failed for '{}': {e}", self.hostname))?;

        self.stdin = child.stdin.take().expect("stdin must be piped");
        self.stdout = BufReader::new(child.stdout.take().expect("stdout must be piped"));
        self.child = child;

        Ok(())
    }

    /// Probe the remote worker's capabilities.
    ///
    /// First sends a lightweight solve task to verify the worker is
    /// responsive, then runs a quick SSH command to detect CPU thread count
    /// and GPU availability on the remote host.
    /// Override values in `hosts.toml` take precedence over detected values.
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
                exec: "cpu".to_string(),
            },
            frequency_hz: 14.2e6,
        };

        let result = self.dispatch(&task)?;
        match result {
            TaskResult::Ok { .. } => {
                let mut cap = self.detect_capability();
                cap.cpu_threads = cap.cpu_threads.max(1);
                Ok(cap)
            }
            TaskResult::Error { error_message, .. } => Err(format!(
                "capability probe failed on '{}': {error_message}",
                self.hostname
            )),
        }
    }

    /// Detect CPU thread count and GPU availability on the remote host
    /// via a separate SSH command.
    fn detect_capability(&self) -> Capability {
        let user_part = match &self.ssh_user {
            Some(u) => format!("{u}@{}", self.hostname),
            None => self.hostname.clone(),
        };

        let cpu_output = Command::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&user_part)
            .arg("nproc 2>/dev/null || echo 1")
            .output();

        let cpu_threads = cpu_output
            .ok()
            .and_then(|o| {
                String::from_utf8(o.stdout)
                    .ok()
                    .and_then(|s| s.trim().parse::<usize>().ok())
            })
            .unwrap_or(4);

        let gpu_stdout = Command::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&user_part)
            .arg("lspci 2>/dev/null | grep -qiE '(vga|3d|display|nvidia|amd)' && echo has_gpu || echo no_gpu")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_else(|_| "no_gpu".to_string());

        let gpu_available = gpu_stdout.contains("has_gpu");

        Capability {
            cpu_threads,
            gpu_available,
            wgpu_backend: if gpu_available {
                Some("Vulkan".to_string())
            } else {
                None
            },
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
        // Note: dispatch includes a reconnect attempt, so total time
        // may be up to 2 × ConnectTimeout (5s) per entry (~10s).
        let (handles, cache) = connect_all(&cfg);
        assert!(handles.is_empty());
        assert!(cache.is_empty());
    }
}
