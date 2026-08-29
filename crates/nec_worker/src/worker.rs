use base64::Engine;
use std::io::{BufRead, Write};

use crate::protocol::{ErrorCode, Impedance, TaskMessage, TaskResult};
use crate::solve::SolveError;

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
                error_message: format!(
                    "transport: the task line could not be decoded, so no deck was read: {e}"
                ),
                // Nothing has been parsed yet, so there are no deck caveats to
                // report — these three failures happen before there is a deck.
                //
                // The code stays `ParseError` because `ErrorCode` is an
                // externally-tagged serde enum with no `#[serde(other)]`: a new
                // variant fails an older controller's whole result line, which
                // the pool then reads as a dead worker and evicts. So the
                // *message* carries the distinction instead, opening with
                // "transport:" and saying no deck was read — the protocol doc
                // records this as a known imprecision (FND-060).
                warnings: Vec::new(),
            };
        }
    };

    let task_id = task.task_id.clone();
    let freq_hz = task.frequency_hz;
    let basis = task.solver_config.basis.clone();
    let exec = task.solver_config.exec.clone();

    // `ground_model` reads as though it selects one and does not: the worker
    // derives ground from the deck's own `GN` card, which is authoritative and is
    // what the local solve uses. Refusing a value it cannot honour turns a silent
    // ignore into a statement — the FND-013 trap, which is that a field looking
    // like a control while being discarded is worse than no field (FND-019).
    if task.solver_config.ground_model != "none" {
        return TaskResult::Error {
            task_id,
            frequency_hz: freq_hz,
            error_code: ErrorCode::UnsupportedConfig,
            error_message: format!(
                "solver_config.ground_model = '{}' is not honoured: the worker takes the \
                 ground model from the deck's GN card, not from the task. Send 'none' and \
                 put the ground in the deck",
                task.solver_config.ground_model
            ),
            warnings: Vec::new(),
        };
    }

    let deck_bytes = match decode_b64(&task.deck_b64) {
        Ok(b) => b,
        Err(e) => {
            return TaskResult::Error {
                task_id,
                frequency_hz: freq_hz,
                error_code: ErrorCode::ParseError,
                error_message: format!(
                    "transport: the task's base64 deck payload could not be decoded, so no deck was read: {e}"
                ),
                warnings: Vec::new(),
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
                error_message: format!(
                    "transport: the decoded payload is not valid UTF-8, so no deck was read: {e}"
                ),
                warnings: Vec::new(),
            };
        }
    };

    let deck_str = deck_str.to_string();

    // Reporting form: a deck can be both flawed and refused, and the plain
    // `Result` loses the flaw when it reports the refusal (FND-059).
    let (outcome, warnings) =
        crate::solve::solve_deck_reporting_warnings(&deck_str, freq_hz, &basis, &exec);
    match outcome {
        Ok(fp) => {
            // A non-finite impedance serialises as JSON `null` in a plain `f64`
            // field, and the resulting line cannot be deserialised at all — so
            // the controller sees a broken worker rather than a bad task. Report
            // it as what it is (FND-117). `Impedance` is two plain `f64`s, so
            // this must be caught before the line is built, not after.
            if !fp.impedance_re.is_finite() || !fp.impedance_im.is_finite() {
                return TaskResult::Error {
                    task_id,
                    frequency_hz: freq_hz,
                    error_code: ErrorCode::SingularMatrix,
                    error_message: format!(
                        "the solve produced a non-finite feedpoint impedance \
                         ({} + j{}), so it did not converge",
                        fp.impedance_re, fp.impedance_im
                    ),
                    warnings,
                };
            }
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
                exec_used: fp.exec_used,
                warnings: fp.warnings,
            }
        }
        Err(e) => {
            // Exhaustive on purpose. This was a catch-all onto `ParseError`, so a
            // deck that parsed cleanly — and that the local CLI solves — crossed
            // the wire as `parse_error`, sending the reader hunting for a syntax
            // mistake that is not there. A plane-wave (`EX 1`) receive deck is the
            // live case: it returns `NoFeedpoint`, which is a statement about what
            // this worker supports, not about the deck's syntax (FND-049).
            //
            // Listing every variant means a new `SolveError` forces a decision
            // here at compile time instead of silently inheriting the wrong code —
            // the mistake the catch-all made once already.
            //
            // Mapped onto existing `ErrorCode`s rather than adding one: the enum is
            // serialised on the wire, so a new variant breaks an older controller's
            // deserialisation outright, unlike the additive `warnings` field.
            let error_code = match &e {
                SolveError::ParseError(_) => ErrorCode::ParseError,
                SolveError::SingularMatrix(_) => ErrorCode::SingularMatrix,
                SolveError::GeometryError(_)
                | SolveError::UnsupportedConfig(_)
                | SolveError::NoFeedpoint => ErrorCode::UnsupportedConfig,
                // The first producer of this code. It has been in the enum — and
                // so deserialisable by every released controller — since the
                // crate's first commit, which is why a too-big deck can be given
                // its own code without the wire break a new variant would mean.
                SolveError::ResourceExhausted(_) => ErrorCode::ResourceExhausted,
            };
            TaskResult::Error {
                task_id,
                frequency_hz: freq_hz,
                error_code,
                error_message: e.to_string(),
                warnings,
            }
        }
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
    // `gamma` is NaN when both sums overflowed to infinity, which happens for a
    // finite but astronomical |Z| (roughly 1e154 and up). That IS an open
    // circuit, so the honest answer is infinite SWR — and saying so here is what
    // makes the wire invariant "null means infinite" true by construction rather
    // than by luck (FND-117).
    if gamma.is_nan() || gamma >= 1.0 {
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

    /// Build a task line for `deck`, so a test can drive the wire boundary rather
    /// than the solver underneath it — which is where the mislabel was visible.
    fn task_line(deck: &str) -> String {
        let b64 = base64::engine::general_purpose::STANDARD.encode(deck);
        format!(
            r#"{{"task_id":"t1","deck_hash":"abc","deck_b64":"{b64}",
                "solver_config":{{"basis":"hallen","ground_model":"none"}},
                "frequency_hz":14.2e6}}"#
        )
    }

    /// FND-125: an over-large deck is well-formed and its geometry is supported —
    /// it is simply too big. That is a different remedy from "this config is not
    /// supported", so it gets `ResourceExhausted`, which had been on the wire
    /// with no producer since this crate's first commit.
    ///
    /// The wire boundary is the thing under test, not the geometry check: the
    /// `SolveError` variant is one `match` away from silently inheriting
    /// `UnsupportedConfig`, which is exactly how FND-049 happened.
    #[test]
    fn an_oversized_deck_crosses_the_wire_as_resource_exhausted() {
        // 100 000 segments: past MAX_SEGMENTS by 10x, and ~13 MB rather than a
        // memory bomb should the geometry guard ever be removed.
        let deck = "CE\nGW 1 100000 0 0 -5.282 0 0 5.282 0.001\nGE\n\
                    EX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let result = process_task(&task_line(deck));
        let TaskResult::Error {
            error_code,
            error_message,
            ..
        } = &result
        else {
            panic!("expected a refusal for an oversized deck: {result:?}");
        };
        assert_eq!(*error_code, ErrorCode::ResourceExhausted);
        // The message too: `ResourceExhausted` must not become the new catch-all
        // that `ParseError` once was.
        assert!(
            error_message.contains("segments"),
            "message should name what ran out: {error_message}"
        );
    }

    /// FND-049: the catch-all stamped `ParseError` on every error it had not
    /// named, so a deck that **parsed cleanly** crossed the wire as `parse_error`.
    ///
    /// A plane-wave receive deck is the live case: it has no driven feedpoint, so
    /// the worker returns `NoFeedpoint` — a statement about what this worker
    /// supports, not about the deck's syntax. The local CLI solves the same deck.
    #[test]
    fn a_cleanly_parsed_deck_is_not_reported_as_a_parse_error() {
        // `EX 1` is an incident plane wave: no driven source for the worker to
        // price, but nothing wrong with the text.
        let deck = "CM plane-wave receive deck\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\n\
                    EX 1 1 1 0 0.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let result = process_task(&task_line(deck));
        let TaskResult::Error {
            error_code,
            error_message,
            ..
        } = &result
        else {
            panic!("expected an error for a deck with no driven feedpoint: {result:?}");
        };
        assert_ne!(
            *error_code,
            ErrorCode::ParseError,
            "a deck that parsed cleanly must not cross the wire as parse_error; \
             message was: {error_message}"
        );
        assert_eq!(*error_code, ErrorCode::UnsupportedConfig);
    }

    /// FND-059: a deck can be **both flawed and refused**, and the plain `Result`
    /// shape reported the refusal while losing the flaw. Here the deck carries an
    /// unrecognised card *and* has no driven feedpoint: the reader was told the
    /// solve stopped and never that a line was ignored on the way there — which is
    /// often the reason it stopped.
    #[test]
    fn a_refused_deck_still_reports_the_caveats_it_earned() {
        let deck = "CM flawed and refused\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\n\
                    ZZ 1 2 3\nEX 1 1 1 0 0.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let result = process_task(&task_line(deck));
        let TaskResult::Error {
            error_code,
            error_message,
            warnings,
            ..
        } = &result
        else {
            panic!("expected a refusal for a deck with no driven feedpoint: {result:?}");
        };
        assert_eq!(*error_code, ErrorCode::UnsupportedConfig);
        // The message, not just the code: `UnsupportedConfig` is also what an
        // earlier RHS failure yields, so asserting the code alone would let a
        // reclassification silently certify the wrong exit as the tested one.
        assert!(
            error_message.contains("no driven feedpoint"),
            "expected the no-feedpoint exit, got: {error_message}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("ZZ")),
            "the ignored card must survive the refusal: {warnings:?}"
        );
    }

    /// The residual half of FND-059: a flaw found *later* than the parse must also
    /// survive a refusal. A skipped `LD` is a stamp-level caveat, and stamps used
    /// to be computed after the RHS build — so a deck with a bad `LD` **and** an
    /// `EX` naming a missing segment reported the refusal with the flaw missing.
    /// Found by review after the first fix, by walking every error exit and asking
    /// which caveats existed yet at each one.
    #[test]
    fn a_flaw_found_after_parsing_also_survives_a_refusal() {
        let deck = "CM skipped load, and a source on a segment that is not there\nCE\n\
                    GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nLD 9 1 1 51 0.0 0.0 0.0\n\
                    EX 0 1 999 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let result = process_task(&task_line(deck));
        let TaskResult::Error {
            error_message,
            warnings,
            ..
        } = &result
        else {
            panic!("expected a refusal for an EX naming a missing segment: {result:?}");
        };
        assert!(
            error_message.contains("999"),
            "the refusal must name the missing segment: {error_message}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("LD")),
            "the skipped load must survive the refusal: {warnings:?}"
        );
    }

    /// ...and a deck refused before anything was parsed reports none, rather than
    /// inventing caveats for a deck that never existed.
    #[test]
    fn a_task_refused_before_parsing_reports_no_caveats() {
        let result = process_task(
            r#"{"task_id":"t1","deck_hash":"a","deck_b64":"!!!bad!!!",
            "solver_config":{"basis":"hallen","ground_model":"none"},"frequency_hz":14e6}"#,
        );
        let TaskResult::Error { warnings, .. } = &result else {
            panic!("expected an error: {result:?}");
        };
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// FND-019: `ground_model` reads as though it selects one and never did. The
    /// worker takes ground from the deck's `GN` card, so a controller that set
    /// this field got its choice silently discarded — the FND-013 trap.
    #[test]
    fn a_ground_model_the_worker_cannot_honour_is_refused_not_ignored() {
        let deck = "CM d\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\n\
                    EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(deck);
        let line = format!(
            r#"{{"task_id":"t1","deck_hash":"a","deck_b64":"{b64}",
                "solver_config":{{"basis":"hallen","ground_model":"sommerfeld"}},
                "frequency_hz":14.2e6}}"#
        );
        let TaskResult::Error {
            error_code,
            error_message,
            ..
        } = process_task(&line)
        else {
            panic!("a ground model the worker cannot honour must be refused");
        };
        assert_eq!(error_code, ErrorCode::UnsupportedConfig);
        assert!(error_message.contains("sommerfeld"), "{error_message}");
        assert!(error_message.contains("GN card"), "{error_message}");
    }

    /// The negative control: `"none"` is what every controller sends, and it must
    /// keep working — refusing it would break every distributed run.
    #[test]
    fn the_default_ground_model_is_accepted() {
        let deck = "CM d\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\n\
                    EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        assert!(
            process_task(&task_line(deck)).is_ok(),
            "the ordinary distributed path must be unaffected"
        );
    }

    /// FND-060: a transport fault and a deck fault shared `ErrorCode::ParseError`
    /// *and* a message shape, so a truncated SSH payload read to the user as a
    /// typo in their antenna file. The code cannot change — a new `ErrorCode`
    /// fails an older controller's whole result line — so the message carries it.
    #[test]
    fn a_transport_fault_does_not_read_as_a_deck_fault() {
        for (line, what) in [
            ("not json at all", "a corrupt task line"),
            (
                r#"{"task_id":"t1","deck_hash":"a","deck_b64":"!!!bad!!!",
                   "solver_config":{"basis":"hallen","ground_model":"none"},"frequency_hz":14e6}"#,
                "an undecodable payload",
            ),
        ] {
            let TaskResult::Error { error_message, .. } = process_task(line) else {
                panic!("{what} must be an error");
            };
            assert!(
                error_message.starts_with("transport:"),
                "{what} must name itself a transport fault: {error_message}"
            );
            assert!(
                error_message.contains("no deck was read"),
                "{what} must say no deck was read: {error_message}"
            );
        }
    }

    /// ...and a genuine deck fault must NOT claim to be a transport one, or the
    /// distinction is decoration.
    #[test]
    fn a_deck_fault_is_not_dressed_as_a_transport_fault() {
        let deck = "CM bad field\nCE\nGW 1 51 zz 0 -5.282 0 0 5.282 0.001\nGE 0\n\
                    EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let TaskResult::Error { error_message, .. } = process_task(&task_line(deck)) else {
            panic!("expected a parse error");
        };
        assert!(!error_message.starts_with("transport:"), "{error_message}");
    }

    /// The negative control: a genuine syntax error still earns `ParseError`, so
    /// the fix narrowed the code rather than abandoning it.
    #[test]
    fn a_genuinely_unparseable_deck_is_still_a_parse_error() {
        let deck = "CM bad field\nCE\nGW 1 51 zz 0 -5.282 0 0 5.282 0.001\nGE 0\n\
                    EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let result = process_task(&task_line(deck));
        let TaskResult::Error { error_code, .. } = &result else {
            panic!("expected a parse error: {result:?}");
        };
        assert_eq!(*error_code, ErrorCode::ParseError);
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
