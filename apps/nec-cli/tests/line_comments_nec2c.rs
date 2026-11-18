// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-329: a deck line's first byte decides whether it is a card, as nec2c's
//! `load_line` (`misc.c:114-176`) decides it. A line beginning with `#` or a space
//! is discarded whole — in the geometry, the program cards and the CM/CE block —
//! before its mnemonic is read; a tab is not in nec2c's list, so a tab-led line is
//! a card named by the tab, which nec2c refuses. fnec trimmed every line first:
//! ` LD 0 1 11 11 100 0 0` became a 100 Ω load nec2c never applies (exit 0, no
//! word), `#LD` an unknown card, and a tab-led card was solved.
//!
//! Anchored on nec2c 1.3.1 absolutely, measured 2026-11-18 on these decks (the
//! 10 m, 21-segment, 1 mm dipole at 14 MHz, `XQ` included as nec2c ran them):
//!
//! - no load, ` LD …`, `#LD …`, ` GW 2 …`, `GW … 0 / # note / GC …` (a uniform
//!   taper): 64.355 − j55.751 Ω, INPUT POWER 4.4385e-3 W — one wire, no load;
//! - `LD 0 1 11 11 100 0 0` at column 1: 164.35 − j55.751 (the reading fnec gave
//!   the indented card);
//! - `GW 2 …` at column 1: 41.025 − j99.892 (the parasitic element fnec built
//!   from the indented card);
//! - `LD seg 5 50 Ω / # a note / LD seg 17 50 Ω`: 102.72 − j61.140 (both loads);
//! - `\tLD …`: exit 255, "FAULTY DATA CARD LABEL AFTER GEOMETRY SECTION";
//!   `\tGW …`: exit 255, "GEOMETRY DATA CARD ERROR";
//! - `# a note` above `CM hello there / CM indented (space-led) / CM second / CE
//!   end`: COMMENTS block `hello there`, `second`, `end`.
//!
//! The band is inherited, not tuned: (2.0 Ω, 8.0 Ω), the corpus's external gate
//! and `two_sources_one_segment_nec2c.rs`'s. fnec's no-load row is 63.687 −
//! j57.724 (0.67 / 1.97 from nec2c). Each fixed case also asserts it is far from
//! the reading fnec gave before the fix — the band holds the fix to nec2c, the
//! distance is what fails without it.

mod common;

use std::process::{Command, Output};

const BAND: (f64, f64) = (2.0, 8.0);

const NEC2C_BARE: (f64, f64) = (64.355, -55.751);
const NEC2C_LOADED: (f64, f64) = (164.35, -55.751);
const NEC2C_TWO_WIRES: (f64, f64) = (41.025, -99.892);
const NEC2C_TWO_LOADS: (f64, f64) = (102.72, -61.140);

const GW1: &str = "GW 1 21 0 0 -5 0 0 5 0.001\n";

/// The dipole as measured: `geo` is the geometry (default one wire), `geo_extra`
/// a line after it, `ctl` lines between FR and EX.
fn deck(geo: &str, geo_extra: &str, ctl: &str) -> String {
    format!("CM fnd329\nCE\n{geo}{geo_extra}GE 0\nFR 0 1 0 0 14 0\n{ctl}EX 0 1 11 0 1 0\nXQ\nEN\n")
}

fn run(name: &str, text: &str, args: &[&str]) -> Output {
    let deck = common::TempDeck::new(&format!("fnd329_{name}_{}.nec", std::process::id()), text);
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--exec", "cpu"])
        .args(args)
        .arg(&*deck)
        .output()
        .expect("run fnec")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The single FEEDPOINTS row's Z, and stderr; panics unless fnec exits 0.
fn solve(name: &str, text: &str) -> ((f64, f64), String) {
    let out = run(name, text, &[]);
    let err = stderr(&out);
    assert!(out.status.success(), "{name}: fnec failed:\n{err}");
    let report = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<Vec<f64>> = report
        .lines()
        .skip_while(|l| *l != "FEEDPOINTS")
        .skip(2)
        .take_while(|l| !l.is_empty())
        .map(|l| l.split_whitespace().map(|c| c.parse().unwrap()).collect())
        .collect();
    assert_eq!(rows.len(), 1, "{name}: one feedpoint:\n{report}");
    ((rows[0][6], rows[0][7]), err)
}

fn assert_band(name: &str, z: (f64, f64), nec2c: (f64, f64)) {
    assert!(
        (z.0 - nec2c.0).abs() < BAND.0 && (z.1 - nec2c.1).abs() < BAND.1,
        "{name}: Z {} {:+} against nec2c {nec2c:?}",
        z.0,
        z.1
    );
}

fn indent_warned(err: &str, line: usize, card: &str) -> bool {
    err.lines().any(|l| {
        l.starts_with(&format!("warning: line {line}: ` {card}"))
            && l.contains("begins with a space")
            && l.contains("remove the indent")
    })
}

/// An indented LD is a comment: no load, as nec2c — with a warning naming it.
#[test]
fn an_indented_load_is_not_applied_as_nec2c() {
    let (z, err) = solve("space_ld", &deck(GW1, "", " LD 0 1 11 11 100 0 0\n"));
    assert_band("space_ld", z, NEC2C_BARE);
    // The unfixed reading applied the 100 Ω load: R 163.7.
    assert!(
        z.0 < NEC2C_LOADED.0 - 50.0,
        "the load was applied: R {}",
        z.0
    );
    assert!(indent_warned(&err, 6, "LD 0 1 11 11 100 0 0"), "{err}");
}

/// `#` is nec2c's advertised comment: `#LD` is no card, and no unknown card.
#[test]
fn a_hash_load_is_a_comment_as_nec2c() {
    let (z, err) = solve("hash_ld", &deck(GW1, "", "#LD 0 1 11 11 100 0 0\n"));
    assert_band("hash_ld", z, NEC2C_BARE);
    assert!(!err.contains("line 6"), "a `#` line is silent:\n{err}");
}

/// The geometry section reads lines the same way: an indented GW builds no wire.
#[test]
fn an_indented_wire_is_not_built_as_nec2c() {
    let (z, err) = solve("space_gw", &deck(GW1, " GW 2 21 1 0 -5 1 0 5 0.001\n", ""));
    assert_band("space_gw", z, NEC2C_BARE);
    // The unfixed reading built the parasitic element: X −100.7.
    assert!(
        z.1 > NEC2C_TWO_WIRES.1 + 30.0,
        "the wire was built: X {}",
        z.1
    );
    assert!(indent_warned(&err, 4, "GW 2 21"), "{err}");
}

/// A `#` note between two loads is no card between them: both loads stand.
#[test]
fn a_hash_note_between_loads_keeps_both_as_nec2c() {
    let (z, err) = solve(
        "hash_between",
        &deck(
            GW1,
            "",
            "LD 0 1 5 5 50 0 0\n# a note\nLD 0 1 17 17 50 0 0\n",
        ),
    );
    assert_band("hash_between", z, NEC2C_TWO_LOADS);
    assert!(!err.contains("line 7"), "a `#` line is silent:\n{err}");
}

/// A tapered wire's GC is read through the same `load_line`: a `#` note between
/// the GW and its GC is skipped, and the (uniform) taper solves as the plain wire.
#[test]
fn a_hash_note_between_a_taper_and_its_gc_as_nec2c() {
    let (z, _) = solve(
        "taper_hash",
        &deck(
            "GW 1 21 0 0 -5 0 0 5 0\n# a note\nGC 0 0 1 0.001 0.001\n",
            "",
            "",
        ),
    );
    assert_band("taper_hash", z, NEC2C_BARE);
}

/// A tab-led line is refused by name, as nec2c refuses it (exit 255), in both
/// sections; nothing is solved.
#[test]
fn a_tab_led_line_is_refused_as_nec2c() {
    for (name, text, line) in [
        ("tab_ld", deck(GW1, "", "\tLD 0 1 11 11 100 0 0\n"), 6),
        ("tab_gw", deck(GW1, "\tGW 2 21 1 0 -5 1 0 5 0.001\n", ""), 4),
    ] {
        let out = run(name, &text, &[]);
        assert_eq!(out.status.code(), Some(1), "{name}: {}", stderr(&out));
        let err = stderr(&out);
        assert!(
            err.contains(&format!("line {line} begins with a tab"))
                && err.contains("remove the tab")
                && err.contains("start the line with `#`"),
            "{name}:\n{err}"
        );
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains("FEEDPOINTS"),
            "{name}: solved"
        );
    }
}

/// The NEC-2D output's COMMENTS block reads the CM/CE cards as nec2c does: a `#`
/// line above them and a space-led line among them are no cards.
#[test]
fn the_nec2d_comment_block_skips_comment_lines_as_nec2c() {
    let text = format!(
        "# a note\nCM hello there\n CM indented\nCM second\nCE end\n{GW1}GE 0\n\
         FR 0 1 0 0 14 0\nEX 0 1 11 0 1 0\nXQ\nEN\n"
    );
    let out = run("hash_cm", &text, &["--output-format", "nec2d"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let report = String::from_utf8_lossy(&out.stdout);
    let block: Vec<&str> = report
        .lines()
        .skip_while(|l| !l.contains("- - - - COMMENTS - - - -"))
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take_while(|l| !l.contains("STRUCTURE SPECIFICATION"))
        .collect();
    assert_eq!(block, ["hello there", "second", "end"], "{report}");
}

/// Negative controls: the same cards at column 1 are cards, as in nec2c.
#[test]
fn column_one_cards_are_still_cards_as_nec2c() {
    let (z, err) = solve("col1_ld", &deck(GW1, "", "LD 0 1 11 11 100 0 0\n"));
    assert_band("col1_ld", z, NEC2C_LOADED);
    assert!(!err.contains("begins with a space"), "{err}");
    let (z, _) = solve("col1_gw", &deck(GW1, "GW 2 21 1 0 -5 1 0 5 0.001\n", ""));
    assert_band("col1_gw", z, NEC2C_TWO_WIRES);
    let (z, _) = solve("bare", &deck(GW1, "", ""));
    assert_band("bare", z, NEC2C_BARE);
}
