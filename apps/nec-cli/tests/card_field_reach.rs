// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Every field position `nec_model::card::FIELD_DISPOSITION` calls consumed
//! reaches the report: `fnec` on the row's deck and on the same deck with that
//! position changed must give a different solution, or refuse the changed deck.
//! A position the table says is kept on the typed card and read by no solve must
//! leave the solution exactly as it was.
//!
//! This is the half of the field guard that needs the solver
//! (`crates/nec_parser/tests/card_matrix.rs` is the parser's half): it catches a
//! field the typed card carries and the solver never reads, the class of
//! FND-280 (LD 2/3's values), FND-283 (GN's radial screen), FND-285 (`RP 1`
//! solved as `RP 0`, mode 0 → 1 changing nothing) and FND-298 (XNDA's digits).
//! The solution is the report less the sections that echo the deck (`SOURCES`,
//! `LOADS`), which differ for any value written.
//!
//! What it does NOT catch: a field read wrongly. A position that changes the
//! answer passes whatever answer it gives; only the nec2c-anchored gates catch
//! FND-282's tag 0 or FND-298's digit swap.

mod common;

use common::TempDeck;
use nec_model::card::{
    deck_with, field_name, perturbed_line, Disposition, FieldCase, Probe, FIELD_DISPOSITION,
};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
enum Run {
    /// Exit 0, with the report's solution.
    Solved(String),
    /// A non-zero exit, with its stderr.
    Refused(String),
}

/// The report less the sections that echo the deck, and the build line.
fn solution(stdout: &str) -> String {
    let mut out = String::new();
    let mut echo = false;
    for line in stdout.lines() {
        if matches!(line, "SOURCES" | "LOADS") {
            echo = true;
        }
        if line.is_empty() {
            echo = false;
        }
        if !echo && !line.starts_with("FNEC_BUILD ") {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

static DECKS: AtomicUsize = AtomicUsize::new(0);

fn fnec(deck: &str, args: &[&str]) -> Run {
    let n = DECKS.fetch_add(1, Ordering::Relaxed);
    let path = TempDeck::new(
        &format!("card_field_reach_{}_{n}.nec", std::process::id()),
        deck,
    );
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    if out.status.success() {
        Run::Solved(solution(&String::from_utf8_lossy(&out.stdout)))
    } else {
        Run::Refused(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[derive(Default, Debug)]
struct Findings {
    failures: Vec<String>,
}

/// Runs a row's deck with the probed card written as given.
type Runner<'a> = &'a (dyn Fn(&FieldCase, &str) -> Run + Sync);

fn check_row(row: &FieldCase, run: Runner) -> Findings {
    let mut out = Findings::default();
    let at = format!("{} [{}]", row.card, row.case);
    let base = match run(row, row.line) {
        Run::Solved(s) => s,
        Run::Refused(e) => {
            out.failures
                .push(format!("{at}: the baseline deck does not solve:\n{e}"));
            return out;
        }
    };
    for (i, probes) in row.fields.iter().enumerate() {
        let pos = i + 1;
        for Probe { value, disposition } in probes.iter().copied() {
            let consumed = matches!(disposition, Disposition::Consumed);
            let kept = matches!(disposition, Disposition::Kept { .. });
            if !consumed && !kept {
                continue;
            }
            let here = format!(
                "{at} {} = {value} ({disposition:?})",
                field_name(row.card, pos)
            );
            let line = perturbed_line(row.line, pos, value);
            let got = run(row, &line);
            let reaches = match &got {
                Run::Refused(_) => true,
                Run::Solved(s) => *s != base,
            };
            match disposition {
                Disposition::Consumed if !reaches => out.failures.push(format!(
                    "{here}: the solution is unchanged — the field never reaches the solve\n    {line}"
                )),
                Disposition::Kept { .. } if reaches => out.failures.push(format!(
                    "{here}: the table says no solve reads it, but it changes the outcome: \
                     {}\n    {line}",
                    match &got {
                        Run::Refused(e) => format!("refused: {e}"),
                        Run::Solved(_) => "the solution differs".to_string(),
                    }
                )),
                _ => {}
            }
        }
    }
    out
}

/// Check the rows on four threads (each `fnec` run is its own process).
fn check_reach(rows: &[FieldCase], run: Runner) -> Findings {
    let next = AtomicUsize::new(0);
    let all = Mutex::new(Findings::default());
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(row) = rows.get(i) else { break };
                let f = check_row(row, run);
                let mut all = all.lock().unwrap();
                all.failures.extend(f.failures);
            });
        }
    });
    all.into_inner().unwrap()
}

fn production(row: &FieldCase, line: &str) -> Run {
    fnec(&deck_with(row, line), row.fnec_args)
}

#[test]
fn every_consumed_field_reaches_the_report_and_every_kept_one_does_not() {
    let f = check_reach(FIELD_DISPOSITION, &production);
    assert!(
        f.failures.is_empty(),
        "{} failure(s):\n{}",
        f.failures.len(),
        f.failures.join("\n")
    );
}

/// The guard's standing self-test: FND-285's defect, the RP mode read and never
/// solved, with the table claiming the mode is consumed. The runner writes every
/// RP card's I1 as 0 — what the solve saw before FND-285 — so `RP 1` solves as
/// `RP 0`, and the guard must say so.
#[test]
fn self_test_a_consumed_field_the_solver_never_reads_fails_the_guard() {
    let mut row = *FIELD_DISPOSITION
        .iter()
        .find(|r| r.card == "RP")
        .expect("an RP row");
    row.fields[0] = &[Probe {
        value: "1",
        disposition: Disposition::Consumed,
    }];
    row.fields[1..].iter_mut().for_each(|p| *p = &[]);
    let mode_never_read = |row: &FieldCase, line: &str| -> Run {
        let line = perturbed_line(line, 1, "0");
        production(row, &line)
    };
    let f = check_reach(&[row], &mode_never_read);
    assert!(
        f.failures
            .iter()
            .any(|m| m.starts_with("RP [")
                && m.contains("I1 = 1 (Consumed): the solution is unchanged")),
        "{:#?}",
        f.failures
    );
    // And the production runner passes the same row: the sabotage is what failed.
    assert!(check_reach(&[row], &production).failures.is_empty());
}
