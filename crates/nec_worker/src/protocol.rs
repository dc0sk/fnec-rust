// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Wire protocol types shared between controller and worker.
//!
//! All messages are newline-delimited JSON (one object per line, no embedded newlines).
//! Maximum message size: 4 MiB.

use serde::{Deserialize, Serialize};

/// Solver configuration included with each task message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerSolverConfig {
    /// Solver basis: `"hallen"` | `"sinusoidal"` | `"pulse"` | `"continuity"`.
    #[serde(default = "default_basis")]
    pub basis: String,
    /// Ground model: `"none"` | `"perfect"` | `"sommerfeld"`.
    #[serde(default = "default_ground")]
    pub ground_model: String,
}

fn default_basis() -> String {
    "hallen".to_string()
}
fn default_ground() -> String {
    "none".to_string()
}

impl Default for WorkerSolverConfig {
    fn default() -> Self {
        Self {
            basis: default_basis(),
            ground_model: default_ground(),
        }
    }
}

/// A task dispatched by the controller to the worker.
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskMessage {
    /// Opaque UUID-v4 task identifier assigned by the controller.
    pub task_id: String,
    /// SHA-256 hex digest of the deck bytes (informational; worker does not verify).
    pub deck_hash: String,
    /// Full NEC deck bytes, base64-encoded (STANDARD alphabet, no line wrapping).
    pub deck_b64: String,
    /// Solver configuration.
    pub solver_config: WorkerSolverConfig,
    /// Frequency in Hz for this task.
    pub frequency_hz: f64,
}

/// Feedpoint impedance from a successful solve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Impedance {
    pub re_ohm: f64,
    pub im_ohm: f64,
}

/// Machine-readable error codes (see design doc §3.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    SingularMatrix,
    ParseError,
    UnsupportedConfig,
    ResourceExhausted,
    Internal,
}

/// A result emitted by the worker for a completed (or failed) task.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskResult {
    Ok {
        task_id: String,
        frequency_hz: f64,
        impedance: Impedance,
        vswr_50: f64,
        feedpoint_current_mag: f64,
        feedpoint_current_phase_deg: f64,
    },
    Error {
        task_id: String,
        frequency_hz: f64,
        error_code: ErrorCode,
        error_message: String,
    },
}

impl TaskResult {
    pub fn task_id(&self) -> &str {
        match self {
            TaskResult::Ok { task_id, .. } => task_id,
            TaskResult::Error { task_id, .. } => task_id,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, TaskResult::Ok { .. })
    }
}
