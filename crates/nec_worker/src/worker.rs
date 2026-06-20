use base64::Engine;
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
    base64::engine::general_purpose::STANDARD.decode(s)
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn vswr_matched_load_is_1() {
        let v = vswr(50.0, 0.0, 50.0);
        assert!((v - 1.0).abs() < 1e-9);
    }

    #[test]
    fn vswr_open_circuit_is_infinite() {
        let v = vswr(1e100, 0.0, 50.0);
        assert!(v.is_infinite());
    }

    #[test]
    fn vswr_short_circuit_is_infinite() {
        let v = vswr(0.0, 0.0, 50.0);
        assert!(v.is_infinite());
    }

    #[test]
    fn vswr_known_mismatch() {
        let v = vswr(100.0, 0.0, 50.0);
        assert!((v - 2.0).abs() < 1e-9);
    }

    #[test]
    fn vswr_reactive_load() {
        let v = vswr(50.0, 50.0, 50.0);
        assert!(v > 1.0);
        assert!(v.is_finite());
    }

    #[test]
    fn vswr_negative_resistance_is_infinite() {
        let v = vswr(-10.0, 0.0, 50.0);
        assert!(v.is_infinite());
    }

    #[test]
    fn decode_b64_roundtrip() {
        let data = b"hello world";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let decoded = decode_b64(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_b64_invalid_input() {
        assert!(decode_b64("!!!not-base64!!!").is_err());
    }

    #[test]
    fn decode_b64_empty_string() {
        let decoded = decode_b64("").unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn is_shutdown_detects_cmd_shutdown() {
        assert!(is_shutdown(r#"{"cmd":"shutdown"}"#));
        assert!(is_shutdown(r#"  {"cmd":"shutdown"}  "#));
    }

    #[test]
    fn is_shutdown_rejects_other_json() {
        assert!(!is_shutdown(r#"{"task_id":"t1"}"#));
        assert!(!is_shutdown(r#"not json"#));
        assert!(!is_shutdown(""));
    }

    #[test]
    fn process_task_malformed_json_returns_error() {
        let result = process_task("not json at all");
        assert!(!result.is_ok());
        assert_eq!(result.task_id(), "unknown");
    }

    #[test]
    fn process_task_missing_fields_returns_error() {
        let result = process_task(r#"{"task_id":"t1"}"#);
        assert!(!result.is_ok());
    }

    #[test]
    fn process_task_invalid_base64_returns_error() {
        let input = r#"{
            "task_id":"t1",
            "deck_hash":"abc",
            "deck_b64":"!!!invalid!!!",
            "solver_config":{"basis":"hallen","ground_model":"none"},
            "frequency_hz":14e6
        }"#;
        let result = process_task(input);
        assert!(!result.is_ok());
        if let TaskResult::Error { error_code, .. } = &result {
            assert_eq!(*error_code, ErrorCode::ParseError);
        }
    }

    #[test]
    fn worker_stdio_loop_handles_empty_input() {
        let input = b"";
        let mut output = Vec::new();
        run_worker_stdio(&input[..], &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn worker_stdio_loop_handles_shutdown() {
        let input = b"{\"cmd\":\"shutdown\"}\n";
        let mut output = Vec::new();
        run_worker_stdio(&input[..], &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn worker_stdio_loop_skips_empty_lines() {
        let input = b"\n\n{\"cmd\":\"shutdown\"}\n";
        let mut output = Vec::new();
        run_worker_stdio(&input[..], &mut output);
        assert!(output.is_empty());
    }
}
