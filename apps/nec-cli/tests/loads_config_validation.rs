// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-204 / FND-207 — a `--loads-config` file is checked like an `LD` card.
//!
//! Each case below was a silent wrong answer, exit 0, on a 21-segment dipole at
//! 28 MHz whose loaded answer is 164.13 − j48.89 Ω and unloaded 64.13 − j48.89
//! (162.97 / 62.97 − j54.38 until FND-227 refined the free ends):
//!
//! - a load naming no segment (wrong tag, reversed range, out-of-range segment)
//!   solved unloaded, where the same load as an `LD` card is refused (FND-204);
//! - a misspelled key read as 0 — "all" — so `segment = 11` loaded every
//!   segment (870.40 − j432.34; 866.41 − j431.27 before FND-227 — a lumped load
//!   lands on a refined end's centre third, once); a float or negative tag read as 0 or wrapped;
//!   a misspelled table read as "no loads" and solved unloaded (FND-207).

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn tmp(name: &str, body: &str) -> common::TempDeck {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    common::TempDeck::new(&format!("fnec-loadscfg-{name}-{n}"), body)
}

const DIPOLE: &str =
    "CE\nGW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 28 0\nEN\n";
const LOAD: &str = "numerator = [100.0]\ndenominator = [1.0]\n";

fn run(loads: &str) -> std::process::Output {
    let deck = tmp("d.nec", DIPOLE);
    let cfg = tmp("l.toml", loads);
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen", "--exec", "cpu", "--loads-config"])
        .arg(&cfg)
        .arg(&deck)
        .output()
        .expect("run fnec")
}

#[test]
fn a_load_that_names_no_segment_is_refused() {
    for (label, entry, why) in [
        ("tag 9", "tag = 9\nseg_first = 11\n", "names no segment"),
        (
            "segment 99",
            "tag = 1\nseg_first = 99\n",
            "names no segment",
        ),
        (
            "reversed range",
            "tag = 1\nseg_first = 11\nseg_last = 5\n",
            "seg_last is before seg_first",
        ),
    ] {
        let out = run(&format!("[[laplace_load]]\n{entry}{LOAD}"));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && stderr.contains(why),
            "{label}: expected a refusal naming `{why}`, got exit {:?}:\n{stderr}\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

#[test]
fn a_malformed_loads_file_is_refused() {
    for (label, file, why) in [
        (
            "misspelled key",
            format!("[[laplace_load]]\ntag = 1\nsegment = 11\n{LOAD}"),
            "unknown key `segment`",
        ),
        (
            "missing seg_first",
            format!("[[laplace_load]]\ntag = 1\n{LOAD}"),
            "`seg_first` is required",
        ),
        (
            "float tag",
            format!("[[laplace_load]]\ntag = 1.0\nseg_first = 11\n{LOAD}"),
            "tag: must be a non-negative integer",
        ),
        (
            "negative tag",
            format!("[[laplace_load]]\ntag = -1\nseg_first = 11\n{LOAD}"),
            "tag: must be a non-negative integer",
        ),
        (
            "misspelled table",
            format!("[[laplace_loads]]\ntag = 1\nseg_first = 11\n{LOAD}"),
            "unknown key `laplace_loads`",
        ),
        (
            "no loads at all",
            "# nothing here\n".to_string(),
            "no loads",
        ),
    ] {
        let out = run(&file);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && stderr.contains(why),
            "{label}: expected a refusal naming `{why}`, got exit {:?}:\n{stderr}\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

/// What is documented still works: an explicit 0 means "all", as on an `LD`
/// card, and a well-formed single-segment load lands where it says.
#[test]
fn well_formed_loads_still_apply() {
    for (label, entry, want_r) in [
        ("segment 11", "tag = 1\nseg_first = 11\n", 164.13),
        ("explicit all", "tag = 0\nseg_first = 0\n", 870.40),
    ] {
        let out = run(&format!("[[laplace_load]]\n{entry}{LOAD}"));
        assert!(
            out.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let r: f64 = stdout
            .lines()
            .find_map(|l| {
                let c: Vec<&str> = l.split_whitespace().collect();
                (c.len() == 8 && c[0] == "1" && c[1] == "11").then(|| c[6].parse().ok())?
            })
            .unwrap_or_else(|| panic!("{label}: no feedpoint row:\n{stdout}"));
        assert!((r - want_r).abs() < 0.05, "{label}: R {r} vs {want_r}");
    }
}
