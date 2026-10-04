// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-209 — the negative-resistance caveat reads the run's loads, not the deck.
//!
//! A dipole with a −200 Ω load reports Re Z ≈ −122 Ω, which is the load's own
//! resistance at the feed. The caveat said "the reason is not identified" and
//! chose its remedy from the deck's cards: with the load from `--loads-config`
//! it told the user to "re-run with `--solver mpie`", which then refused the
//! run; with the same load as an `LD` card it said the MPIE was unavailable.
//! Both spellings now name the negative load, and neither points at the MPIE.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-negload-{name}-{n}"), body)
}

const DIPOLE: &str =
    "CE\nGW 1 21 0 0 -5.282 0 0 5.282 .001\nGE 0\n{LD}EX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";

fn stderr(extra: &[&str], deck: &common::TempDeck) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu"])
        .args(extra)
        .arg(deck)
        .output()
        .expect("run fnec");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_negative_load_is_named_whichever_way_it_is_spelled() {
    let ld = tmp("ld.nec", &DIPOLE.replace("{LD}", "LD 4 1 11 11 -200 0\n"));
    let bare = tmp("bare.nec", &DIPOLE.replace("{LD}", ""));
    let cfg = tmp(
        "n.toml",
        "[[laplace_load]]\ntag = 1\nseg_first = 11\nnumerator = [-200.0]\ndenominator = [1.0]\n",
    );
    let by_card = stderr(&[], &ld);
    let by_file = stderr(&["--loads-config", cfg.to_str().unwrap()], &bare);
    for (label, err) in [("LD card", &by_card), ("--loads-config", &by_file)] {
        assert!(
            err.contains("has negative resistance")
                && err.contains("a load in this run has negative resistance"),
            "{label}: the caveat does not name the negative load:\n{err}"
        );
        assert!(
            !err.contains("--solver mpie"),
            "{label}: the caveat points at an MPIE that refuses this run:\n{err}"
        );
    }
}
