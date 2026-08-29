// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Worker pool — manages a set of local and/or SSH workers.
//!
//! [`WorkerPool`] provides a unified dispatch interface across N workers
//! (local subprocesses or SSH-backed remote processes).  Tasks are assigned
//! round-robin to the next available worker.

use crate::controller::LocalWorkerHandle;
use crate::hosts::HostEntry;
use crate::protocol::{TaskMessage, TaskResult};
use crate::ssh_worker::SshWorkerHandle;

/// A handle to a single worker, either local or remote.
#[derive(Debug)]
pub enum WorkerHandle {
    /// A locally spawned `fnec worker --stdio` subprocess.
    Local(LocalWorkerHandle),
    /// A remote worker connected via SSH.
    Ssh(SshWorkerHandle),
}

impl WorkerHandle {
    /// Dispatch a task to this worker and block for the result.
    fn dispatch(&mut self, task: &TaskMessage) -> Result<TaskResult, crate::DispatchError> {
        match self {
            WorkerHandle::Local(h) => h.dispatch(task),
            WorkerHandle::Ssh(h) => h.dispatch(task),
        }
    }

    /// Gracefully shut down this worker.
    fn shutdown(self) {
        match self {
            WorkerHandle::Local(h) => {
                h.shutdown().ok();
            }
            WorkerHandle::Ssh(h) => {
                h.shutdown().ok();
            }
        }
    }

    /// Return a human-readable label for this worker.
    fn label(&self) -> String {
        match self {
            WorkerHandle::Local(_) => "local".to_string(),
            WorkerHandle::Ssh(h) => format!("ssh:{}", h.hostname()),
        }
    }
}

/// One dispatch outcome: the worker's result plus the label of the worker that
/// served it, or the reason it could not be served.
pub type DispatchOutcome = Result<(TaskResult, String), String>;

/// A round-robin pool of worker handles.
///
/// Workers are created via [`WorkerPool::new_local`] (N local subprocesses)
/// or [`WorkerPool::new_ssh`] (N remote SSH workers from a config file).
/// Dispatch picks the next worker in sequence; if a worker fails the error
/// is returned immediately (no automatic retry).
pub struct WorkerPool {
    workers: Vec<WorkerHandle>,
    next_worker: usize,
}

impl WorkerPool {
    /// Create a pool of N local workers.
    ///
    /// Each worker runs `fnec worker --stdio` as a subprocess of `binary`.
    /// Returns an error if any worker fails to spawn.
    pub fn new_local(count: usize, binary: &str) -> Result<Self, String> {
        let mut workers = Vec::with_capacity(count);
        for i in 0..count {
            let handle = LocalWorkerHandle::spawn(binary)
                .map_err(|e| format!("failed to spawn local worker {i}/{count}: {e}"))?;
            workers.push(WorkerHandle::Local(handle));
        }
        Ok(Self {
            workers,
            next_worker: 0,
        })
    }

    /// Create a pool of SSH workers from a slice of host entries.
    ///
    /// Each entry is connected to via `ssh <user>@<host> <binary> worker --stdio`.
    /// If a connection fails, the error is returned — use
    /// [`WorkerPool::new_ssh_skip_failures`] to skip unreachable hosts.
    pub fn new_ssh(entries: &[HostEntry]) -> Result<Self, String> {
        let mut workers = Vec::with_capacity(entries.len());
        for entry in entries {
            let handle = SshWorkerHandle::connect(entry)
                .map_err(|e| format!("failed to connect to worker '{}': {e}", entry.hostname))?;
            workers.push(WorkerHandle::Ssh(handle));
        }
        Ok(Self {
            workers,
            next_worker: 0,
        })
    }

    /// Create a pool of SSH workers, skipping entries that fail to connect.
    ///
    /// Failures are printed to stderr.  Returns an empty pool if all entries fail.
    pub fn new_ssh_skip_failures(entries: &[HostEntry]) -> Self {
        let workers: Vec<WorkerHandle> = entries
            .iter()
            .filter_map(|entry| match SshWorkerHandle::connect(entry) {
                Ok(h) => Some(WorkerHandle::Ssh(h)),
                Err(e) => {
                    eprintln!(
                        "warning: failed to connect to worker '{}': {e}",
                        entry.hostname
                    );
                    None
                }
            })
            .collect();
        Self {
            workers,
            next_worker: 0,
        }
    }

    /// Returns the number of workers in the pool.
    pub fn len(&self) -> usize {
        self.workers.len()
    }

    /// Returns true if the pool has no workers.
    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }

    /// Dispatch a task to the next worker in round-robin order.
    ///
    /// Returns the worker's label on success, or an error description on failure.
    /// If a worker fails, subsequent calls skip it (the pool removes failed workers).
    pub fn dispatch(&mut self, task: &TaskMessage) -> DispatchOutcome {
        if self.workers.is_empty() {
            return Err("worker pool is empty — no workers available".to_string());
        }

        // Try from next_worker until we find one that works or exhaust the pool.
        let initial_len = self.workers.len();
        let mut idx = self.next_worker % initial_len;

        for _ in 0..initial_len {
            if self.workers.is_empty() {
                break;
            }
            idx %= self.workers.len();
            let label = self.workers[idx].label();
            match self.workers[idx].dispatch(task) {
                Ok(result) => {
                    self.next_worker = (idx + 1) % self.workers.len();
                    return Ok((result, label));
                }
                // A task fault says nothing about the worker's health, so the
                // worker stays and the failure is returned for THIS task only.
                // Treating the two alike meant one negative-resistance frequency
                // -- whose infinite VSWR the controller could not parse -- evicted
                // every host in the pool, one dispatch at a time, and every
                // remaining frequency then failed with "all workers in pool
                // failed" (FND-117).
                Err(crate::DispatchError::Task(e)) => {
                    self.next_worker = (idx + 1) % self.workers.len();
                    return Err(format!("worker '{label}' returned an unusable result: {e}"));
                }
                Err(crate::DispatchError::Worker(e)) => {
                    eprintln!("warning: worker '{label}' failed, removing from pool: {e}");
                    self.workers.remove(idx);
                    // idx now points to the next worker (shifted down by one after removal).
                }
            }
        }

        Err("all workers in pool failed".to_string())
    }

    /// Dispatch a batch of tasks across all workers **concurrently**, returning
    /// one result per task in the order given.
    ///
    /// [`dispatch`](Self::dispatch) blocks until one worker answers, so driving a
    /// sweep through it leaves every other worker idle: M frequency points across
    /// N workers cost `M × latency` instead of `M/N × latency`. Each worker here
    /// gets a thread and pulls the next task itself, so a fast node takes more work
    /// than a slow one without any scheduling policy.
    ///
    /// Failure handling matches `dispatch`: a worker that errors is removed from
    /// the pool, and the task it was holding is retried on a survivor rather than
    /// lost. Only when every worker is gone does a task come back as an error.
    ///
    /// Results are indexed by task, so output order does not depend on which
    /// worker finished first. Which worker *served* a given task does — the label
    /// in the returned tuple is a diagnostic, not a stable contract.
    pub fn dispatch_batch(&mut self, tasks: &[TaskMessage]) -> Vec<DispatchOutcome> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;

        let n = tasks.len();
        if n == 0 {
            return Vec::new();
        }
        if self.workers.is_empty() {
            return (0..n)
                .map(|_| Err("worker pool is empty — no workers available".to_string()))
                .collect();
        }

        let cursor = AtomicUsize::new(0);
        let slots: Mutex<Vec<Option<DispatchOutcome>>> = Mutex::new((0..n).map(|_| None).collect());
        // Tasks whose worker died mid-flight; retried on a survivor below.
        let orphaned: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        let survivors: Mutex<Vec<WorkerHandle>> = Mutex::new(Vec::new());

        let workers = std::mem::take(&mut self.workers);
        std::thread::scope(|scope| {
            for mut worker in workers {
                let (cursor, slots, orphaned, survivors) = (&cursor, &slots, &orphaned, &survivors);
                scope.spawn(move || {
                    loop {
                        let i = cursor.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            survivors.lock().expect("survivors lock").push(worker);
                            return;
                        }
                        let label = worker.label();
                        match worker.dispatch(&tasks[i]) {
                            Ok(result) => {
                                slots.lock().expect("slots lock")[i] = Some(Ok((result, label)));
                            }
                            // Same taxonomy as `dispatch`, and it must be here
                            // too: the batch path is the one a sweep actually
                            // uses, so a single unusable result would otherwise
                            // take out a worker per bad frequency (FND-117).
                            // Not retried, either — a task fault is
                            // deterministic, so a retry just fails again.
                            Err(crate::DispatchError::Task(e)) => {
                                slots.lock().expect("slots lock")[i] = Some(Err(format!(
                                    "worker '{label}' returned an unusable result: {e}"
                                )));
                            }
                            Err(crate::DispatchError::Worker(e)) => {
                                eprintln!(
                                    "warning: worker '{label}' failed, removing from pool: {e}"
                                );
                                // Drop this worker, but not the task it was holding.
                                orphaned.lock().expect("orphaned lock").push(i);
                                return;
                            }
                        }
                    }
                });
            }
        });

        self.workers = survivors.into_inner().expect("survivors lock");
        self.next_worker = 0;
        let mut slots = slots.into_inner().expect("slots lock");

        // Retry whatever the dead workers were holding, plus anything they never
        // reached, on the survivors. Sequential: by this point the pool is smaller
        // and this is the exceptional path.
        let orphaned: Vec<usize> = orphaned.into_inner().expect("orphaned lock");
        let mut retry: Vec<usize> = (0..n)
            .filter(|i| slots[*i].is_none() || orphaned.contains(i))
            .collect();
        retry.sort_unstable();
        retry.dedup();
        for i in retry {
            slots[i] = Some(self.dispatch(&tasks[i]));
        }

        slots
            .into_iter()
            .map(|s| s.unwrap_or_else(|| Err("task was never dispatched".to_string())))
            .collect()
    }

    /// Shut down all workers gracefully.
    pub fn shutdown_all(mut self) {
        for w in self.workers.drain(..) {
            w.shutdown();
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        for w in self.workers.drain(..) {
            w.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pool_dispatch_fails() {
        let mut pool = WorkerPool {
            workers: vec![],
            next_worker: 0,
        };
        let task = TaskMessage {
            task_id: "t".into(),
            deck_hash: "x".into(),
            deck_b64: String::new(),
            solver_config: crate::protocol::WorkerSolverConfig {
                basis: "hallen".into(),
                ground_model: "none".into(),
                exec: "cpu".into(),
            },
            frequency_hz: 14.0e6,
        };
        assert!(pool.dispatch(&task).is_err());
    }

    #[test]
    fn empty_pool_len() {
        let pool = WorkerPool {
            workers: vec![],
            next_worker: 0,
        };
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn new_local_nonexistent_binary_returns_error() {
        let result = WorkerPool::new_local(1, "/nonexistent/fnec-binary");
        assert!(result.is_err(), "expected Err, got Ok");
    }

    #[test]
    fn new_local_zero_workers_returns_empty() {
        let pool = WorkerPool::new_local(0, "fnec").unwrap();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn new_local_multiple_with_nonexistent_binary_returns_error() {
        let result = WorkerPool::new_local(3, "/nonexistent/fnec-binary");
        assert!(result.is_err(), "expected Err, got Ok");
    }
}
