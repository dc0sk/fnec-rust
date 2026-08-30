// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! SSH-backed worker handle — PH6-CHK-006.
//!
//! [`SshWorkerHandle`] mirrors [`LocalWorkerHandle`] but connects via SSH
//! to a remote host and runs `fnec worker --stdio` there.  All message
//! framing, dispatch, and result collection logic is identical to the local
//! path — the only difference is that `Command::new(binary)` becomes
//! `ssh <user>@<host> <binary> worker --stdio`.

use std::process::{Command, Stdio};

use crate::hosts::HostEntry;
use crate::protocol::{TaskMessage, TaskResult};
use crate::Capability;

/// Handle to a worker process running on a remote host via SSH.
///
/// The remote worker is started with `ssh <user>@<host> <binary> worker --stdio`
/// and communicates over newline-delimited JSON on stdin/stdout.
#[derive(Debug)]
pub struct SshWorkerHandle {
    pipe: crate::pipe::WorkerPipe,
    /// How long this host gets to answer one task; see the local handle.
    deadline: std::time::Duration,
    hostname: String,
    ssh_user: Option<String>,
    binary_path: Option<String>,
}

/// Fallback thread count when the remote `nproc` probe gives no usable answer.
///
/// A conservative non-zero guess: reporting 0 would drop the node out of
/// scheduling entirely on a probe hiccup, and guessing high would over-assign it.
const DEFAULT_REMOTE_CPU_THREADS: usize = 4;

/// Thread count from the remote `nproc` output.
///
/// `None`, unparseable output, or a zero/absent count all fall back to
/// [`DEFAULT_REMOTE_CPU_THREADS`] — the probe failing must not make a node look
/// like it has no CPUs, which would take it out of scheduling entirely.
///
/// Split out from the SSH call so the parsing can be tested without a remote
/// host: the syscall is not where this goes wrong, the parsing is.
fn parse_cpu_threads(stdout: Option<&str>) -> usize {
    stdout
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_REMOTE_CPU_THREADS)
}

/// Whether the remote GPU probe reported an adapter.
///
/// The remote command prints exactly `has_gpu` or `no_gpu`; anything else (an SSH
/// failure, a shell error) is treated as no GPU, so an unreachable probe never
/// promotes a node to GPU-capable.
fn parse_gpu_available(stdout: &str) -> bool {
    stdout.contains("has_gpu")
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

        let child = Command::new("ssh")
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

        Ok(Self {
            pipe: crate::pipe::WorkerPipe::new(child),
            deadline: crate::pipe::DEFAULT_SOLVE_DEADLINE,
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
    pub fn dispatch(&mut self, task: &TaskMessage) -> Result<TaskResult, crate::DispatchError> {
        // A serialisation failure is a TASK fault, as it is on the local path.
        // This used to be a bare `to_string`, which reached `From<String> for
        // DispatchError` and came out a WORKER fault — so an unserialisable task
        // evicted a healthy remote host, and falsified that impl's own doc
        // comment (FND-136). Both paths now say it once, in `WorkerPipe`.
        let json =
            serde_json::to_string(task).map_err(|e| crate::DispatchError::Task(e.to_string()))?;

        if self.pipe.send(&json).is_err() {
            eprintln!(
                "info: ssh worker '{}' write failed, reconnecting...",
                self.hostname
            );
            self.reconnect()?;
            self.pipe.send(&json)?;
        }

        let line = match self.pipe.recv(self.deadline) {
            Ok(line) => line,
            Err(e) => {
                // Reconnect only for a DROPPED connection. A timeout means the
                // host accepted the task and went quiet, and resending it is how
                // one wedging task takes down a second process on the same host
                // before the pool has even moved on (FND-102).
                let dropped = matches!(&e, crate::DispatchError::Worker(m)
                    if m.contains("closed stdout"));
                if !dropped {
                    return Err(e);
                }
                eprintln!(
                    "info: ssh worker '{}' read failed (eof), reconnecting...",
                    self.hostname
                );
                self.reconnect()?;
                self.pipe.send(&json)?;
                self.pipe.recv(self.deadline)?
            }
        };

        // A complete line came back, so the connection is in sync whatever it
        // says. The host is healthy; this one result is unusable (FND-117).
        let result: TaskResult = serde_json::from_str(line.trim()).map_err(|e| {
            crate::DispatchError::Task(format!("unreadable result from worker: {e}"))
        })?;
        Ok(result)
    }

    /// Re-establish the SSH subprocess connection to the remote worker.
    ///
    /// Kills the existing child process and spawns a new SSH connection
    /// using the same parameters as [`connect`].
    pub fn reconnect(&mut self) -> Result<(), String> {
        self.pipe.kill();

        let user_part = self.user_part();
        let binary = self.binary_path.as_deref().unwrap_or("fnec");

        let child = Command::new("ssh")
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

        self.pipe = crate::pipe::WorkerPipe::new(child);

        Ok(())
    }

    /// Probe the remote worker's capabilities.
    ///
    /// First sends a lightweight solve task to verify the worker is
    /// responsive, then runs a quick SSH command to detect CPU thread count
    /// and GPU availability on the remote host.
    /// Override values in `hosts.toml` take precedence over detected values.
    pub fn probe_capability(&mut self) -> Result<Capability, String> {
        // `connect_all` probes hosts SERIALLY at startup, before any pool
        // exists, so one wedged host used to block the run before it began. The
        // probe solves a one-segment deck, so it earns a far shorter deadline
        // than a real task (FND-101).
        let solve_deadline = self.deadline;
        self.deadline = crate::pipe::PROBE_DEADLINE;
        let restore = |h: &mut Self| h.deadline = solve_deadline;
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

        let dispatched = self.dispatch(&task);
        restore(self);
        match dispatched? {
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

        let cpu_threads = parse_cpu_threads(
            cpu_output
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .as_deref(),
        );

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

        let gpu_available = parse_gpu_available(&gpu_stdout);

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
        self.pipe.shutdown()
    }

    /// Override the answer deadline; see the local handle.
    pub fn set_deadline(&mut self, deadline: std::time::Duration) {
        self.deadline = deadline;
    }

    /// The hostname this worker is connected to.
    pub fn hostname(&self) -> &str {
        &self.hostname
    }
}

impl Drop for SshWorkerHandle {
    fn drop(&mut self) {
        self.pipe.kill();
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

    // ── Capability-probe parsing (review-260719 FIND-016) ───────────────────
    //
    // `probe_capability` itself needs a reachable host, so it stays untested. What
    // can actually be wrong is the parsing of what comes back, and that is now
    // separable from the SSH call — including the failure defaults, which decide
    // whether a node stays in scheduling at all.

    #[test]
    fn cpu_thread_count_is_parsed_from_nproc_output() {
        assert_eq!(parse_cpu_threads(Some("16\n")), 16);
        assert_eq!(parse_cpu_threads(Some("  8  ")), 8);
        assert_eq!(parse_cpu_threads(Some("1")), 1);
    }

    #[test]
    fn an_unusable_nproc_answer_falls_back_rather_than_reporting_none() {
        // A node reported as having 0 threads would be dropped from scheduling
        // entirely, so every unusable answer must land on the default instead.
        for answer in [None, Some(""), Some("   "), Some("not-a-number"), Some("0")] {
            assert_eq!(
                parse_cpu_threads(answer),
                DEFAULT_REMOTE_CPU_THREADS,
                "unusable nproc answer {answer:?} must fall back"
            );
        }
        // The remote command is `nproc 2>/dev/null || echo 1`, so a failing nproc
        // legitimately yields 1 — that is a real answer, not a fallback.
        assert_eq!(parse_cpu_threads(Some("1\n")), 1);
    }

    #[test]
    fn gpu_availability_is_read_from_the_probe_marker() {
        assert!(parse_gpu_available("has_gpu\n"));
        // Anything else means no GPU — an unreachable or erroring probe must never
        // promote a node to GPU-capable.
        for answer in [
            "no_gpu\n",
            "",
            "bash: lspci: command not found\n",
            "HAS_GPU",
        ] {
            assert!(
                !parse_gpu_available(answer),
                "{answer:?} must not read as a GPU"
            );
        }
    }
}
