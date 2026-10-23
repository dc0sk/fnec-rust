// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `fnec match` — the matching-network helper (GAP-012 / BL-IMPR-017).
//!
//! Given a feedpoint impedance — typed as `--z <R>[+-]j<X>` at `--freq <MHz>`,
//! or solved from a deck — and a target (50 Ω unless `--target` says otherwise),
//! prints every lossless L-network that presents the target at the feedpoint:
//! both topologies, both roots, up to four. The math is
//! [`nec_solver::lnetwork`]; this module is the argument parsing, the deck
//! route and the two output formats.
//!
//! The deck route solves the deck through the main command itself — this
//! binary, re-run with the forwarded solve options and `--output-format json` —
//! and reads the feedpoint records back. That is the one parse → validate →
//! solve sequence the main command runs, with every refusal, caveat and option
//! (`--solver`, `--vars`, `--sweep-config`, …) it has, and no second copy of it
//! here. See [`solve_deck_feedpoints`].
//!
//! No NEC cards are written (a user decision for v1); the automation guide shows
//! how to put a solution into a deck as `LD 0` + `NT` and check it.

use std::process::ExitCode;

use num_complex::Complex64;

use nec_solver::lnetwork::{self, Element, MatchError, Solution};

use crate::cli_args::{self, OutputFormat};
use crate::VERSION;

pub const MATCH_USAGE: &str = "Usage: fnec match --z <R>[+-]j<X> --freq <MHz> [--target <R>[+-]j<X>] [--output-format <text|json>]\n\
       fnec match <deck.nec> [solve options: --solver, --vars, --sweep-config, ...] [--target <R>[+-]j<X>] [--output-format <text|json>]\n\
Prints every lossless L-network (one series and one shunt L or C, both orders, both roots) that\n\
presents the target impedance (default 50 Ω) at the feedpoint. For a conjugate match to a source\n\
Z_S, give --target as Z_S*. A deck is solved by the main command (its FR card gives the frequency;\n\
one block per frequency point) and the first feedpoint's impedance is matched.";

/// One matched frequency point.
struct Block {
    freq_mhz: f64,
    z_load: Complex64,
    z_target: Complex64,
    solutions: Vec<Solution>,
    notes: Vec<String>,
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("fnec {VERSION}\n{MATCH_USAGE}");
    eprintln!("error: {msg}");
    ExitCode::from(2)
}

/// Entry point for `fnec match …` (`args[0]` = binary, `args[1]` = "match").
pub fn run(args: &[String]) -> ExitCode {
    let mut z_arg: Option<String> = None;
    let mut freq_arg: Option<String> = None;
    let mut target_arg: Option<String> = None;
    let mut output_format = OutputFormat::Text;
    // Everything that is not this subcommand's own: the deck path and the solve
    // options forwarded to the main command.
    let mut forwarded: Vec<String> = Vec::new();

    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            flag @ ("--z" | "--freq" | "--target" | "--output-format") => {
                i += 1;
                if i >= args.len() {
                    return usage_error(&format!("missing value after {flag}"));
                }
                let value = args[i].clone();
                match flag {
                    "--z" => z_arg = Some(value),
                    "--freq" => freq_arg = Some(value),
                    "--target" => target_arg = Some(value),
                    _ => {
                        output_format = match value.as_str() {
                            "text" => OutputFormat::Text,
                            "json" => OutputFormat::Json,
                            other => {
                                return usage_error(&format!(
                                    "invalid --output-format value '{other}' (expected: text|json)"
                                ))
                            }
                        }
                    }
                }
            }
            other => forwarded.push(other.to_string()),
        }
        i += 1;
    }

    let z_target = match target_arg.as_deref().map(parse_complex) {
        None => Complex64::new(50.0, 0.0),
        Some(Ok(z)) => z,
        Some(Err(e)) => return usage_error(&format!("--target: {e}")),
    };
    if z_target.re <= 0.0 {
        return usage_error(&format!(
            "--target {} Ω: a matching network can only present a positive resistance",
            target_arg.as_deref().unwrap_or("")
        ));
    }

    // The points to match: typed, or solved from the deck.
    let points: Vec<(f64, Complex64, Vec<String>)> = if let Some(z_str) = z_arg {
        if !forwarded.is_empty() {
            return usage_error(&format!(
                "unexpected argument with --z: {} (a deck and --z are two ways to give the \
                 impedance; use one)",
                forwarded.join(" ")
            ));
        }
        let z_load = match parse_complex(&z_str) {
            Ok(z) => z,
            Err(e) => return usage_error(&format!("--z: {e}")),
        };
        let Some(freq_str) = freq_arg else {
            return usage_error(
                "--z needs --freq <MHz>: the component values depend on the frequency",
            );
        };
        let freq_mhz = match freq_str.parse::<f64>() {
            Ok(f) if f.is_finite() && f > 0.0 => f,
            _ => {
                return usage_error(&format!(
                    "--freq {freq_str}: the frequency must be a positive number of MHz"
                ))
            }
        };
        vec![(freq_mhz, z_load, Vec::new())]
    } else {
        if freq_arg.is_some() {
            return usage_error(
                "--freq goes with --z; a deck's FR card (or --sweep-config) gives its frequencies",
            );
        }
        if forwarded.is_empty() {
            return usage_error("give --z <R>[+-]j<X> --freq <MHz>, or a deck path");
        }
        // The main command's own parser checks the forwarded options and names
        // the deck, so a bad option is refused here with its sentence and never
        // reaches a child run.
        let mut check = vec![args[0].clone()];
        check.extend(forwarded.iter().cloned());
        if let Err(e) = cli_args::parse_args(&check) {
            return usage_error(&e);
        }
        match solve_deck_feedpoints(&forwarded) {
            Ok(points) => points,
            Err(DeckFailure::Exit(code)) => return code, // the solve said why on stderr
            Err(DeckFailure::Message(msg)) => {
                eprintln!("error: {msg}");
                return ExitCode::FAILURE;
            }
        }
    };

    let mut blocks: Vec<Block> = Vec::new();
    let mut failed = false;
    for (freq_mhz, z_load, mut notes) in points {
        match lnetwork::solve(z_load, z_target, freq_mhz * 1e6) {
            Ok(solutions) => {
                if solutions.is_empty() {
                    notes.push("already matched".to_string());
                }
                blocks.push(Block {
                    freq_mhz,
                    z_load,
                    z_target,
                    solutions,
                    notes,
                });
            }
            Err(e @ MatchError::NonPositiveLoadResistance(_)) => {
                eprintln!("error: at {freq_mhz:.6} MHz, {e}");
                failed = true;
            }
            Err(e) => {
                // The target and frequency were checked above; a load that is not
                // finite is a solve that produced no number.
                eprintln!("error: at {freq_mhz:.6} MHz, {e}");
                failed = true;
            }
        }
    }

    match output_format {
        OutputFormat::Text | OutputFormat::Nec2d => print_text(&blocks),
        OutputFormat::Json => println!("{}", json_document(&blocks)),
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// `<R>`, `<R>+j<X>` or `<R>-j<X>` (a `j` or `J`; spaces are not allowed).
pub fn parse_complex(s: &str) -> Result<Complex64, String> {
    let bad = || format!("'{s}' is not an impedance; write <R>, <R>+j<X> or <R>-j<X>, as 73+j42");
    let number = |t: &str| {
        t.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(bad)
    };
    let Some(j) = s.find(['j', 'J']) else {
        return Ok(Complex64::new(number(s)?, 0.0));
    };
    if j == 0 {
        return Err(bad());
    }
    let sign = match &s[j - 1..j] {
        "+" => 1.0,
        "-" => -1.0,
        _ => return Err(bad()),
    };
    let re = &s[..j - 1];
    let im = &s[j + 1..];
    if re.is_empty() || im.is_empty() || im.starts_with(['+', '-']) {
        return Err(bad());
    }
    Ok(Complex64::new(number(re)?, sign * number(im)?))
}

/// Why the deck gave no feedpoint to match.
enum DeckFailure {
    /// The solve itself refused or failed, and said why on stderr; its exit
    /// code is this subcommand's.
    Exit(ExitCode),
    /// This subcommand's own reason.
    Message(String),
}

/// One feedpoint record of the main command's JSON output.
#[derive(serde::Deserialize)]
struct FeedpointRecord {
    freq_mhz: f64,
    tag: i64,
    seg: i64,
    z_re: f64,
    z_im: f64,
}

/// The deck's feedpoint impedance at each frequency point, by the main command:
/// this binary, re-run with the forwarded options and `--output-format json`.
///
/// The main command's parse → validate → solve sequence is the one every other
/// frontend route is held to (FND-173 is what happened when `sweep --resonance`
/// kept its own), and it is not callable as a function: `fn main` reads its
/// options and prints. Rather than keep a third copy of the sequence here, the
/// deck route runs it. The child's stderr is this process's, so every warning
/// and refusal reaches the user as the main command prints it; its exit code is
/// returned as this subcommand's. A sibling branch extracts the sequence as
/// `solve_session::probe_feedpoints`; this function is its stand-in, and the
/// one place to swap when it lands.
///
/// Returns `(freq_mhz, z, notes)` per record. With several `EX` cards the main
/// command's JSON reports the first feedpoint only, and a note says so.
fn solve_deck_feedpoints(
    forwarded: &[String],
) -> Result<Vec<(f64, Complex64, Vec<String>)>, DeckFailure> {
    let exe = std::env::current_exe().map_err(|e| {
        DeckFailure::Message(format!(
            "cannot find this fnec binary to solve the deck: {e}"
        ))
    })?;
    let output = std::process::Command::new(&exe)
        .args(forwarded)
        .arg("--output-format")
        .arg("json")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| {
            DeckFailure::Message(format!(
                "cannot run {} to solve the deck: {e}",
                exe.display()
            ))
        })?;
    if !output.status.success() {
        let code = output.status.code().unwrap_or(1).clamp(1, 255) as u8;
        return Err(DeckFailure::Exit(ExitCode::from(code)));
    }
    let records: Vec<FeedpointRecord> = serde_json::from_slice(&output.stdout).map_err(|e| {
        DeckFailure::Message(format!(
            "the solve's JSON output could not be read ({e}): {}",
            String::from_utf8_lossy(&output.stdout).trim()
        ))
    })?;
    if records.is_empty() {
        return Err(DeckFailure::Message(
            "the deck solved but reports no feedpoint impedance (a receiving antenna has none); \
             there is nothing to match"
                .to_string(),
        ));
    }
    let sources = excitation_count(forwarded);
    Ok(records
        .into_iter()
        .map(|r| {
            let mut notes = Vec::new();
            if sources > 1 {
                notes.push(format!(
                    "the deck has {sources} EX cards; matched the first feedpoint the solve \
                     reports (tag {}, segment {}), as the JSON output does",
                    r.tag, r.seg
                ));
            }
            (r.freq_mhz, Complex64::new(r.z_re, r.z_im), notes)
        })
        .collect())
}

/// How many `EX` cards the deck has, for the several-sources note; 0 when the
/// deck cannot be read here (the solve already reported whatever is wrong).
fn excitation_count(forwarded: &[String]) -> usize {
    let Ok(parsed) = cli_args::parse_args(
        &std::iter::once("fnec".to_string())
            .chain(forwarded.iter().cloned())
            .collect::<Vec<_>>(),
    ) else {
        return 0;
    };
    let Ok(mut text) = std::fs::read_to_string(&parsed.path) else {
        return 0;
    };
    if let Some(vars_path) = parsed.vars_path {
        let Ok(vars) = crate::vars_config::load_vars(&vars_path) else {
            return 0;
        };
        let Ok(substituted) = nec_parser::template::substitute(&text, &vars) else {
            return 0;
        };
        text = substituted;
    }
    nec_parser::parse(&text)
        .map(|r| {
            r.deck
                .cards
                .iter()
                .filter(|c| matches!(c, nec_model::card::Card::Ex(_)))
                .count()
        })
        .unwrap_or(0)
}

fn element_text(role: &str, e: Option<Element>) -> String {
    match e {
        None => format!("{role} none"),
        // Eleven significant digits: a network of Q ≈ 20 amplifies a rounding
        // of the component value by Q², and a reader recomputing Z_in from the
        // printed value should land within 1e-6 Ω of the target, as the
        // contract test does.
        Some(e) => format!(
            "{role} {} {:.10e} X_{role} {:.6}",
            e.kind(),
            e.value(),
            e.reactance_ohm()
        ),
    }
}

fn print_text(blocks: &[Block]) {
    for (n, b) in blocks.iter().enumerate() {
        if n > 0 {
            println!();
        }
        println!("MATCH_RESULT");
        println!("FREQ_MHZ {:.6}", b.freq_mhz);
        println!("Z_LOAD {:.6} {:.6}", b.z_load.re, b.z_load.im);
        println!("Z_TARGET {:.6} {:.6}", b.z_target.re, b.z_target.im);
        println!("SOLUTIONS {}", b.solutions.len());
        for (k, s) in b.solutions.iter().enumerate() {
            println!(
                "SOLUTION {} TOPOLOGY {} {} {} Q {:.3} Z_IN {:.6} {:.6}",
                k + 1,
                s.topology.as_str(),
                element_text("SHUNT", s.shunt),
                element_text("SERIES", s.series),
                s.q,
                s.z_in.re,
                s.z_in.im
            );
        }
        for note in &b.notes {
            println!("NOTE {note}");
        }
    }
}

fn json_complex(z: Complex64) -> serde_json::Value {
    serde_json::json!({ "re": z.re, "im": z.im })
}

fn json_element(e: Option<Element>) -> serde_json::Value {
    match e {
        None => serde_json::Value::Null,
        Some(e) => serde_json::json!({
            "kind": e.kind(),
            "value": e.value(),
            "x": e.reactance_ohm(),
        }),
    }
}

/// The `fnec-match/1` document.
fn json_document(blocks: &[Block]) -> String {
    let blocks: Vec<serde_json::Value> = blocks
        .iter()
        .map(|b| {
            serde_json::json!({
                "freq_mhz": b.freq_mhz,
                "z_load": json_complex(b.z_load),
                "z_target": json_complex(b.z_target),
                "solutions": b.solutions.iter().map(|s| serde_json::json!({
                    "topology": s.topology.as_str(),
                    "shunt": json_element(s.shunt),
                    "series": json_element(s.series),
                    "q": s.q,
                    "z_in": json_complex(s.z_in),
                })).collect::<Vec<_>>(),
                "notes": b.notes,
            })
        })
        .collect();
    serde_json::json!({ "schema": "fnec-match/1", "blocks": blocks }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_impedance_spellings() {
        assert_eq!(parse_complex("73+j42").unwrap(), Complex64::new(73.0, 42.0));
        assert_eq!(
            parse_complex("73-j42").unwrap(),
            Complex64::new(73.0, -42.0)
        );
        assert_eq!(parse_complex("25").unwrap(), Complex64::new(25.0, 0.0));
        assert_eq!(
            parse_complex("-2.1-j444").unwrap(),
            Complex64::new(-2.1, -444.0)
        );
        assert_eq!(parse_complex("0+j100").unwrap(), Complex64::new(0.0, 100.0));
        assert_eq!(parse_complex("1e2+J5").unwrap(), Complex64::new(100.0, 5.0));
        for bad in [
            "", "j42", "73j42", "73+42j", "73+j", "+j42", "73+j-4", "abc", "nan", "73 + j42",
        ] {
            assert!(parse_complex(bad).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn text_block_lists_elements_or_none() {
        let sols = lnetwork::solve(
            Complex64::new(50.0, 30.0),
            Complex64::new(50.0, 0.0),
            14.2e6,
        )
        .unwrap();
        let one = sols.iter().find(|s| s.shunt.is_none()).unwrap();
        assert_eq!(element_text("SHUNT", one.shunt), "SHUNT none");
        assert!(
            element_text("SERIES", one.series).starts_with("SERIES C 3.7360315"),
            "{}",
            element_text("SERIES", one.series)
        );
        assert!(element_text("SERIES", one.series).ends_with("X_SERIES -30.000000"));
    }

    #[test]
    fn json_document_carries_the_schema_and_null_for_an_absent_element() {
        let b = Block {
            freq_mhz: 14.2,
            z_load: Complex64::new(50.0, 30.0),
            z_target: Complex64::new(50.0, 0.0),
            solutions: lnetwork::solve(
                Complex64::new(50.0, 30.0),
                Complex64::new(50.0, 0.0),
                14.2e6,
            )
            .unwrap(),
            notes: vec![],
        };
        let v: serde_json::Value = serde_json::from_str(&json_document(&[b])).unwrap();
        assert_eq!(v["schema"], "fnec-match/1");
        assert_eq!(v["blocks"].as_array().unwrap().len(), 1);
        let sols = v["blocks"][0]["solutions"].as_array().unwrap();
        assert_eq!(sols.len(), 2);
        assert!(sols.iter().any(|s| s["shunt"].is_null()));
    }
}
