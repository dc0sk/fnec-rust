// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Worker stdio loop — reads JSON task lines from `reader`, writes result lines
//! to `writer`.  Run via `fnec worker --stdio`.

use std::io::{BufRead, Write};

use crate::protocol::{ErrorCode, Impedance, TaskMessage, TaskResult};
use crate::solve::{solve_deck_at_frequency, SolveError};

/// Run the worker stdio event loop.
///
/// Reads newline-delimited JSON task messages from `reader` and writes
/// newline-delimited JSON result messages to `writer`.  Blocks until EOF or
/// until a `{"cmd":"shutdown"}` message is received.
pub fn run_worker_stdio<R: BufRead, W: Write>(reader: R, mut writer: W) {
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for shutdown command before attempting task deserialization.
        if is_shutdown(trimmed) {
            break;
        }

        let result = process_task(trimmed);
        let json = match serde_json::to_string(&result) {
            Ok(s) => s,
            Err(e) => format!(
                r#"{{"status":"error","task_id":"unknown","frequency_hz":0.0,"error_code":"internal","error_message":"serialization error: {e}"}}"#
            ),
        };
        let _ = writeln!(writer, "{json}");
        let _ = writer.flush();
    }
}

fn is_shutdown(line: &str) -> bool {
    // Deserialize the raw JSON value and look for {"cmd":"shutdown"}.
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
        return val.get("cmd") == Some(&serde_json::Value::String("shutdown".to_string()));
    }
    false
}

fn process_task(line: &str) -> TaskResult {
    let task: TaskMessage = match serde_json::from_str(line) {
        Ok(t) => t,
        Err(e) => {
            return TaskResult::Error {
                task_id: "unknown".to_string(),
                frequency_hz: 0.0,
                error_code: ErrorCode::ParseError,
                error_message: format!("failed to deserialize task: {e}"),
            };
        }
    };

    let task_id = task.task_id.clone();
    let freq_hz = task.frequency_hz;
    let basis = task.solver_config.basis.clone();

    let deck_bytes = match decode_b64(&task.deck_b64) {
        Ok(b) => b,
        Err(e) => {
            return TaskResult::Error {
                task_id,
                frequency_hz: freq_hz,
                error_code: ErrorCode::ParseError,
                error_message: format!("base64 decode failed: {e}"),
            };
        }
    };

    let deck_str = match std::str::from_utf8(&deck_bytes) {
        Ok(s) => s,
        Err(e) => {
            return TaskResult::Error {
                task_id,
                frequency_hz: freq_hz,
                error_code: ErrorCode::ParseError,
                error_message: format!("deck is not valid UTF-8: {e}"),
            };
        }
    };

    // We need an owned String because the borrow of deck_bytes ends here.
    let deck_str = deck_str.to_string();

    match solve_deck_at_frequency(&deck_str, freq_hz, &basis) {
        Ok(fp) => {
            let vswr = vswr(fp.impedance_re, fp.impedance_im, 50.0);
            TaskResult::Ok {
                task_id,
                frequency_hz: freq_hz,
                impedance: Impedance {
                    re_ohm: fp.impedance_re,
                    im_ohm: fp.impedance_im,
                },
                vswr_50: vswr,
                feedpoint_current_mag: fp.current_mag,
                feedpoint_current_phase_deg: fp.current_phase_deg,
            }
        }
        Err(SolveError::SingularMatrix(m)) => TaskResult::Error {
            task_id,
            frequency_hz: freq_hz,
            error_code: ErrorCode::SingularMatrix,
            error_message: m,
        },
        Err(SolveError::UnsupportedConfig(m)) => TaskResult::Error {
            task_id,
            frequency_hz: freq_hz,
            error_code: ErrorCode::UnsupportedConfig,
            error_message: m,
        },
        Err(e) => TaskResult::Error {
            task_id,
            frequency_hz: freq_hz,
            error_code: ErrorCode::ParseError,
            error_message: e.to_string(),
        },
    }
}

fn decode_b64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.decode(s)
}

fn vswr(z_re: f64, z_im: f64, z0: f64) -> f64 {
    let num_sq = (z_re - z0).powi(2) + z_im.powi(2);
    let den_sq = (z_re + z0).powi(2) + z_im.powi(2);
    if den_sq < 1e-100 {
        return f64::INFINITY;
    }
    let gamma = num_sq.sqrt() / den_sq.sqrt();
    if gamma >= 1.0 {
        return f64::INFINITY;
    }
    (1.0 + gamma) / (1.0 - gamma)
}
