// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Wire protocol types shared between controller and worker.
//!
//! All messages are newline-delimited JSON (one object per line, no embedded newlines).
//! Maximum message size: 4 MiB.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_result_ok_roundtrip() {
        let result = TaskResult::Ok {
            task_id: "t-42".into(),
            frequency_hz: 14.2e6,
            impedance: Impedance {
                re_ohm: 74.24,
                im_ohm: 13.90,
            },
            vswr_50: 1.5,
            feedpoint_current_mag: 0.5,
            feedpoint_current_phase_deg: 10.0,
            exec_used: "cpu".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_id(), "t-42");
        assert!(back.is_ok());
    }

    #[test]
    fn task_result_error_roundtrip() {
        let result = TaskResult::Error {
            task_id: "t-err".into(),
            frequency_hz: 14.2e6,
            error_code: ErrorCode::SingularMatrix,
            error_message: "matrix is singular".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_id(), "t-err");
        assert!(!back.is_ok());
    }

    #[test]
    fn task_result_error_code_variants_roundtrip() {
        let codes = vec![
            ErrorCode::SingularMatrix,
            ErrorCode::ParseError,
            ErrorCode::UnsupportedConfig,
            ErrorCode::ResourceExhausted,
            ErrorCode::Internal,
        ];
        for code in codes {
            let json = serde_json::to_string(&code).unwrap();
            let back: ErrorCode = serde_json::from_str(&json).unwrap();
            assert_eq!(code, back);
        }
    }

    #[test]
    fn task_message_roundtrip() {
        let msg = TaskMessage {
            task_id: "t99".into(),
            deck_hash: "abc123".into(),
            deck_b64: "R0VORVJJQyB...".into(),
            solver_config: WorkerSolverConfig {
                basis: "hallen".into(),
                ground_model: "perfect".into(),
                exec: "cpu".into(),
            },
            frequency_hz: 7.15e6,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: TaskMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_id, "t99");
        assert_eq!(back.solver_config.basis, "hallen");
        assert!((back.frequency_hz - 7.15e6).abs() < 1e-9);
    }

    #[test]
    fn worker_solver_config_defaults() {
        let cfg = WorkerSolverConfig::default();
        assert_eq!(cfg.basis, "hallen");
        assert_eq!(cfg.ground_model, "none");
    }

    #[test]
    fn worker_solver_config_roundtrip() {
        let cfg = WorkerSolverConfig {
            basis: "sinusoidal".into(),
            ground_model: "sommerfeld".into(),
            exec: "cpu".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: WorkerSolverConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }
}

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
    /// Execution preference: `"cpu"` | `"gpu"` (PH7-CHK-004). When `"gpu"` and the
    /// node has a usable wgpu adapter and the deck is in the GPU-resident
    /// supported class, the worker solves on the GPU; otherwise it falls back to
    /// the CPU solve. Defaults to `"cpu"` for wire back-compat.
    #[serde(default = "default_exec")]
    pub exec: String,
}

fn default_basis() -> String {
    "hallen".to_string()
}
fn default_ground() -> String {
    "none".to_string()
}
fn default_exec() -> String {
    "cpu".to_string()
}

impl Default for WorkerSolverConfig {
    fn default() -> Self {
        Self {
            basis: default_basis(),
            ground_model: default_ground(),
            exec: default_exec(),
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
        /// Execution path the worker actually used: `"cpu"` | `"gpu"`
        /// (PH7-CHK-004). Defaults to `"cpu"` for wire back-compat.
        #[serde(default = "default_exec")]
        exec_used: String,
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
