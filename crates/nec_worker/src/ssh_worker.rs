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

/// The options every controller-to-worker SSH connection uses.
///
/// Host keys are verified (FND-154). The list used to carry
/// `StrictHostKeyChecking=no` and `UserKnownHostsFile=/dev/null`, so the
/// controller accepted whatever machine answered at a worker's hostname and
/// never stored or compared a key: anyone on the network path could pose as a
/// worker, receive its decks and return any impedance as a solved result.
///
/// `accept-new` keeps a first connection working without a prompt — the key is
/// recorded in the user's own `known_hosts` — and refuses a host whose key has
/// CHANGED since, which is the attack. `BatchMode` stays: a worker connection
/// must never stop to ask a question nobody is there to answer.
///
/// One list, used by every `ssh` this module spawns, because it was four copies
/// of the same eight lines, which is how one of them would have been missed.
const SSH_OPTIONS: [&str; 3] = [
    "BatchMode=yes",
    "StrictHostKeyChecking=accept-new",
    "ConnectTimeout=5",
];

/// An `ssh` command to `destination` with [`SSH_OPTIONS`] applied.
fn ssh_command(destination: &str) -> Command {
    let mut cmd = Command::new("ssh");
    for option in SSH_OPTIONS {
        cmd.arg("-o").arg(option);
    }
    cmd.arg(destination);
    cmd
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

        let child = ssh_command(&user_part)
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
                match self.pipe.recv(self.deadline) {
                    Ok(line) => line,
                    Err(e) => return Err(self.who_closed_it(e)),
                }
            }
        };

        // A complete line came back, so the connection is in sync whatever it
        // says. The host is healthy; this one result is unusable (FND-117).
        let result: TaskResult = serde_json::from_str(line.trim()).map_err(|e| {
            crate::DispatchError::Task(format!("unreadable result from worker: {e}"))
        })?;
        Ok(result)
    }

    /// Tell an unreachable host from a worker that died holding the task.
    ///
    /// Both close stdout, and the pool treats them differently: a death spends one
    /// of the task's two strikes, an unreachable host does not. `ssh` exits 255
    /// when IT fails — the name does not resolve, the connection or the
    /// authentication fails — and then no worker ever received the task. Three
    /// unresolvable hosts used to fail a healthy task after the second, with the
    /// third never tried (FND-176). Any other status is the remote command's own,
    /// so it stays a death, and a task that kills workers still runs out of
    /// strikes.
    fn who_closed_it(&mut self, e: crate::DispatchError) -> crate::DispatchError {
        const SSH_OWN_FAILURE: i32 = 255;
        match e {
            crate::DispatchError::Worker(m) if m.contains("closed stdout") => {
                match self
                    .pipe
                    .exit_status_within(std::time::Duration::from_secs(2))
                {
                    Some(status) if status.code() == Some(SSH_OWN_FAILURE) => {
                        crate::DispatchError::Unreachable(format!(
                            "ssh could not reach '{}' (exit 255)",
                            self.hostname
                        ))
                    }
                    _ => crate::DispatchError::Worker(m),
                }
            }
            other => other,
        }
    }

    /// Re-establish the SSH subprocess connection to the remote worker.
    ///
    /// Kills the existing child process and spawns a new SSH connection
    /// using the same parameters as [`connect`].
    pub fn reconnect(&mut self) -> Result<(), String> {
        self.pipe.kill();

        let user_part = self.user_part();
        let binary = self.binary_path.as_deref().unwrap_or("fnec");

        let child = ssh_command(&user_part)
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
    ///
    /// Returns what was detected, and nothing else. This used to claim that
    /// "override values in `hosts.toml` take precedence over detected values";
    /// they cannot, because this handle never stores them — `connect` copies
    /// only the hostname, user and binary path (FND-104). Note too that the
    /// CLI's `--hosts` path does not call this: it builds its pool with
    /// `WorkerPool::new_ssh_skip_failures` and never probes.
    pub fn probe_capability(&mut self) -> Result<Capability, String> {
        // `connect_all` probes hosts SERIALLY, before any pool exists, so one
        // wedged host would block every probe after it. (Nothing in the CLI calls
        // `connect_all`; see its doc comment.) The
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

        let cpu_output = ssh_command(&user_part)
            .arg("nproc 2>/dev/null || echo 1")
            .output();

        let cpu_threads = parse_cpu_threads(
            cpu_output
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .as_deref(),
        );

        let gpu_stdout = ssh_command(&user_part)
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
/// **Not used by the CLI.** `fnec --hosts` builds its pool with
/// `WorkerPool::new_ssh_skip_failures`, which connects without probing, and
/// schedules by letting each worker pull the next task. This helper and the
/// [`crate::CapabilityCache`] it fills are kept as `pub` API of `nec_worker`
/// (PH6-CHK-006 shipped them), but nothing downstream consumes the capabilities
/// they gather — which is how documented scheduling controls built on top of
/// them came to be read by nothing (FND-104).
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

    /// FND-154: host keys are verified, and nothing turns verification off.
    #[test]
    fn worker_connections_verify_host_keys() {
        let cmd = ssh_command("worker.example");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.iter().any(|a| a == "StrictHostKeyChecking=accept-new"),
            "{args:?}"
        );
        assert!(
            !args
                .iter()
                .any(|a| a == "StrictHostKeyChecking=no" || a.starts_with("UserKnownHostsFile")),
            "{args:?}"
        );
        assert_eq!(args.last().map(String::as_str), Some("worker.example"));
    }

    /// Every `ssh` this module spawns goes through `ssh_command`. The options
    /// were four hand-copied lists; a fifth call site built by hand would skip
    /// the verification the test above pins.
    #[test]
    fn every_ssh_process_is_built_by_ssh_command() {
        let src = include_str!("ssh_worker.rs");
        let spawns = src.matches(concat!("Command::new(", "\"ssh\")")).count();
        assert_eq!(spawns, 1, "an ssh Command is built outside ssh_command");
    }

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
