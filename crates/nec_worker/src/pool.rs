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
    fn dispatch(&mut self, task: &TaskMessage) -> Result<TaskResult, String> {
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
    pub fn dispatch(&mut self, task: &TaskMessage) -> Result<(TaskResult, String), String> {
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
                Err(e) => {
                    eprintln!("warning: worker '{label}' failed, removing from pool: {e}");
                    self.workers.remove(idx);
                    // idx now points to the next worker (shifted down by one after removal).
                }
            }
        }

        Err("all workers in pool failed".to_string())
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
}
