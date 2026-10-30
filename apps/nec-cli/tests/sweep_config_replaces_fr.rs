// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-210 — `--sweep-config` replaces the deck's `FR` cards, so the deck's `FR`
//! must not refuse the run or describe it.
//!
//! The pre-solve checks read the deck's governing `FR`: a deck whose `FR` was
//! negative was refused ("-14.2 MHz is not a usable frequency") although the
//! sweep file supplied 14.2 MHz and that card never runs, and a deck with two
//! `FR` cards was warned that "fnec runs ... at the frequencies the last card
//! asks for (14 MHz)" before solving 14.2.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-swfr-{name}-{n}"), body)
}

const SWEEP: &str = "[frequency]\npoints_mhz = [14.2]\n";

fn deck_with(fr: &str) -> String {
    format!("CE\nGW 1 21 0 0 -5.3 0 0 5.3 .001\nGE 0\nEX 0 1 11 0 1 0\n{fr}EN\n")
}

fn run(deck: &common::TempDeck, sweep: Option<&common::TempDeck>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(["--solver", "hallen", "--exec", "cpu"]);
    if let Some(s) = sweep {
        cmd.arg("--sweep-config").arg(s);
    }
    cmd.arg(deck).output().expect("run fnec")
}

#[test]
fn a_replaced_unusable_fr_card_does_not_refuse_the_sweep() {
    let deck = tmp("neg.nec", &deck_with("FR 0 1 0 0 -14.2 0\n"));
    let sweep = tmp("sw.toml", SWEEP);
    let out = run(&deck, Some(&sweep));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("FREQ_MHZ 14.200000"),
        "the sweep's 14.2 MHz did not run:\n{}\n{stdout}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Without the sweep file the deck's own FR governs, and is still refused.
    let alone = run(&deck, None);
    assert!(
        !alone.status.success()
            && String::from_utf8_lossy(&alone.stderr).contains("not a usable frequency"),
        "a negative FR on its own must still be refused"
    );
}

#[test]
fn replaced_fr_cards_are_not_described_as_the_run() {
    let deck = tmp("two.nec", &deck_with("FR 0 1 0 0 7 0\nFR 0 1 0 0 14 0\n"));
    let sweep = tmp("sw.toml", SWEEP);
    let with_sweep = run(&deck, Some(&sweep));
    let stderr = String::from_utf8_lossy(&with_sweep.stderr);
    assert!(with_sweep.status.success(), "{stderr}");
    assert!(
        !stderr.contains("superseded"),
        "the FR caveat names frequencies --sweep-config never runs:\n{stderr}"
    );
    // The caveat itself still fires when the deck's FR cards do govern.
    let alone = run(&deck, None);
    assert!(String::from_utf8_lossy(&alone.stderr).contains("superseded"));
}

/// FND-290 gave the GUI and the worker this route's frequency-list check, and
/// with it a network refusal that says which listed frequency failed and where
/// the list came from: a line λ/2 long at 30 MHz is unbuildable there only, and
/// the deck (whose `FR` is 29.9) does not contain 30 MHz. Negative control: the
/// list that stops at 29.9 solves.
#[test]
fn a_network_refused_at_a_listed_frequency_names_it_and_the_list() {
    let deck = tmp(
        "tl.nec",
        "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 1 0 -2.5 1 0 2.5 0.001\nGE 0\n\
         EX 0 1 11 0 1.0 0.0\nTL 1 11 2 11 50 4.996540966666667 0 0 0 0\n\
         FR 0 1 0 0 29.9 0\nEN\n",
    );
    let both = tmp(
        "both.toml",
        "[frequency]\nstart_mhz = 29.9\nend_mhz = 30.0\nstep_mhz = 0.1\n",
    );
    let out = run(&deck, Some(&both));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(
        stderr.contains("error: TL 1 11 2 11: its electrical length")
            && stderr.contains("(at 30 MHz, from --sweep-config)"),
        "{stderr}"
    );
    let short = tmp("short.toml", "[frequency]\npoints_mhz = [29.9]\n");
    let ok = run(&deck, Some(&short));
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

/// FND-316: a `--sweep-config` range whose step is below its resolution is
/// refused through the binary, exit 1, before anything is solved; the same deck
/// over a range at an ordinary step solves (the control).
#[test]
fn a_range_step_below_the_resolution_is_refused_by_the_binary() {
    let deck = tmp("res.nec", &deck_with(""));
    let sweep = tmp(
        "res.toml",
        "[frequency]\nstart_mhz = 1000.0\nend_mhz = 1000.000000000001\nstep_mhz = 1e-13\n",
    );
    let out = run(&deck, Some(&sweep));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(1)
            && stderr.contains("below the resolution")
            && out.stdout.is_empty(),
        "{:?}\n{stderr}",
        out.status
    );
    let ok = tmp(
        "ok.toml",
        "[frequency]\nstart_mhz = 14.2\nend_mhz = 14.3\nstep_mhz = 0.1\n",
    );
    let out = run(&deck, Some(&ok));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
