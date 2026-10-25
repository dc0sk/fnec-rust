// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `fnec match` — the L-network matching helper (GAP-012 / BL-IMPR-017): the
//! component values that present a target at a feedpoint, typed (`--z`) or
//! solved from a deck, as text and as `fnec-match/1` JSON.
//!
//! Every solution's input impedance is recomputed HERE from the printed
//! component values (L and C in henry and farad, at the printed frequency) and
//! must be the target — the printed `Z_IN` is not trusted. The deck route is
//! checked against the main command: the monopole's `Z_LOAD` is its
//! `--output-format json` impedance, and a solution written into the deck as
//! `LD 0` + `NT` makes the main command report the target; the main command's
//! refusals (an `LD 9`) and its negative-resistance deck come through.

use std::path::PathBuf;
use std::process::{Command, Output};

use num_complex::Complex64;

mod common;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .output()
        .expect("run fnec")
}

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name)
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// One printed `SOLUTION` line, as the test reads it: element kind + value
/// (`None` for `none`) and the numbers it prints.
#[derive(Debug, Clone)]
struct Sol {
    topology: String,
    shunt: Option<(String, f64, f64)>,
    series: Option<(String, f64, f64)>,
    q: f64,
    z_in: Complex64,
}

/// One printed `MATCH_RESULT` block.
#[derive(Debug, Clone)]
struct Block {
    freq_mhz: f64,
    z_load: Complex64,
    z_target: Complex64,
    solutions: Vec<Sol>,
    notes: Vec<String>,
}

fn num<S: AsRef<str> + std::fmt::Debug>(t: Option<S>, what: &str, line: &str) -> f64 {
    let t = t.unwrap_or_else(|| panic!("no {what} in: {line}"));
    t.as_ref()
        .parse::<f64>()
        .unwrap_or_else(|e| panic!("{what} {t:?} is not a number ({e}) in: {line}"))
}

/// `ROLE kind value X_ROLE x` or `ROLE none`, from the token after `ROLE`.
fn element(tokens: &[&str], role: &str, line: &str) -> Option<(String, f64, f64)> {
    let i = tokens
        .iter()
        .position(|t| *t == role)
        .unwrap_or_else(|| panic!("no {role} in: {line}"));
    if tokens.get(i + 1) == Some(&"none") {
        return None;
    }
    let kind = tokens[i + 1].to_string();
    let value = num(tokens.get(i + 2), "value", line);
    let x_key = format!("X_{role}");
    assert_eq!(tokens.get(i + 3), Some(&x_key.as_str()), "{line}");
    let x = num(tokens.get(i + 4), &x_key, line);
    Some((kind, value, x))
}

fn parse_blocks(out: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for line in out.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        match t.first().copied() {
            Some("MATCH_RESULT") => blocks.push(Block {
                freq_mhz: f64::NAN,
                z_load: Complex64::new(f64::NAN, f64::NAN),
                z_target: Complex64::new(f64::NAN, f64::NAN),
                solutions: vec![],
                notes: vec![],
            }),
            Some("FREQ_MHZ") => {
                blocks.last_mut().unwrap().freq_mhz = num(t.get(1), "FREQ_MHZ", line);
            }
            Some("Z_LOAD") => {
                blocks.last_mut().unwrap().z_load =
                    Complex64::new(num(t.get(1), "re", line), num(t.get(2), "im", line));
            }
            Some("Z_TARGET") => {
                blocks.last_mut().unwrap().z_target =
                    Complex64::new(num(t.get(1), "re", line), num(t.get(2), "im", line));
            }
            Some("SOLUTIONS") => {
                let n = num(t.get(1), "SOLUTIONS", line) as usize;
                blocks.last_mut().unwrap().solutions.reserve(n);
                // The count is checked against the lines once the block is read.
                blocks
                    .last_mut()
                    .unwrap()
                    .notes
                    .push(format!("__count {n}"));
            }
            Some("SOLUTION") => {
                let pos = |k: &str| {
                    t.iter()
                        .position(|x| *x == k)
                        .unwrap_or_else(|| panic!("no {k} in: {line}"))
                };
                let q = num(t.get(pos("Q") + 1), "Q", line);
                let zi = pos("Z_IN");
                blocks.last_mut().unwrap().solutions.push(Sol {
                    topology: t[pos("TOPOLOGY") + 1].to_string(),
                    shunt: element(&t, "SHUNT", line),
                    series: element(&t, "SERIES", line),
                    q,
                    z_in: Complex64::new(
                        num(t.get(zi + 1), "Z_IN re", line),
                        num(t.get(zi + 2), "Z_IN im", line),
                    ),
                });
            }
            Some("NOTE") => blocks.last_mut().unwrap().notes.push(t[1..].join(" ")),
            _ => {}
        }
    }
    for b in &mut blocks {
        let count = b
            .notes
            .iter()
            .position(|n| n.starts_with("__count "))
            .unwrap_or_else(|| panic!("block without SOLUTIONS line:\n{out}"));
        let n: usize = b.notes.remove(count)[8..].parse().unwrap();
        assert_eq!(
            b.solutions.len(),
            n,
            "SOLUTIONS count disagrees with the lines:\n{out}"
        );
        assert!(
            b.freq_mhz.is_finite() && b.z_load.is_finite() && b.z_target.is_finite(),
            "{out}"
        );
    }
    blocks
}

/// The one block a `--z` run prints.
fn match_z(z: &str, freq: &str, extra: &[&str]) -> (Output, Block) {
    let mut args = vec!["match", "--z", z, "--freq", freq];
    args.extend_from_slice(extra);
    let out = run(&args);
    assert!(
        out.status.success(),
        "fnec match --z {z} failed:\n{}\n{}",
        stderr(&out),
        stdout(&out)
    );
    let mut blocks = parse_blocks(&stdout(&out));
    assert_eq!(blocks.len(), 1, "{}", stdout(&out));
    (out, blocks.remove(0))
}

/// The impedance of a printed element at `omega`, from its printed value
/// (not from its printed reactance).
fn impedance(e: &(String, f64, f64), omega: f64) -> Complex64 {
    match e.0.as_str() {
        "L" => Complex64::new(0.0, omega * e.1),
        "C" => Complex64::new(0.0, -1.0 / (omega * e.1)),
        other => panic!("element kind {other}"),
    }
}

/// Z_in of a printed solution terminated in `z_load`, recomputed from its
/// component values.
fn recompute(s: &Sol, z_load: Complex64, freq_mhz: f64) -> Complex64 {
    let omega = 2.0 * std::f64::consts::PI * freq_mhz * 1e6;
    let zero = Complex64::new(0.0, 0.0);
    let z_s = s
        .series
        .as_ref()
        .map(|e| impedance(e, omega))
        .unwrap_or(zero);
    let y_p = s
        .shunt
        .as_ref()
        .map(|e| impedance(e, omega).inv())
        .unwrap_or(zero);
    match s.topology.as_str() {
        "series-first" => ((z_load + z_s).inv() + y_p).inv(),
        "shunt-first" => (z_load.inv() + y_p).inv() + z_s,
        other => panic!("topology {other}"),
    }
}

/// Every solution of the block, recomputed from its printed components,
/// presents the target within `tol` Ω; and the printed `Z_IN` agrees.
fn assert_recomputed(b: &Block, tol: f64) {
    assert!(!b.solutions.is_empty());
    for s in &b.solutions {
        let z = recompute(s, b.z_load, b.freq_mhz);
        assert!(
            (z - b.z_target).norm() <= tol,
            "{s:?} recomputed to {z}, not the target {}",
            b.z_target
        );
        assert!(
            (s.z_in - b.z_target).norm() <= 1e-5,
            "printed Z_IN {}: {s:?}",
            s.z_in
        );
    }
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

/// The solution with these elements (kind + value within 1e-5 relative;
/// `None` = `none`).
fn find<'a>(b: &'a Block, shunt: Option<(&str, f64)>, series: Option<(&str, f64)>) -> &'a Sol {
    let same = |e: &Option<(String, f64, f64)>, want: Option<(&str, f64)>| match (e, want) {
        (None, None) => true,
        (Some((k, v, _)), Some((wk, wv))) => k == wk && rel(*v, wv) <= 1e-5,
        _ => false,
    };
    b.solutions
        .iter()
        .find(|s| same(&s.shunt, shunt) && same(&s.series, series))
        .unwrap_or_else(|| panic!("no solution shunt {shunt:?} series {series:?} in {b:#?}"))
}

// ---------------------------------------------------------------------------
// M1: 25 Ω → 50 Ω at 14.2 MHz: exactly two series-first networks.
// ---------------------------------------------------------------------------

#[test]
// VERIFIES: FR-012
fn m1_real_load_below_target_two_series_first_networks() {
    let (_, b) = match_z("25", "14.2", &[]);
    assert_eq!(b.freq_mhz, 14.2);
    assert_eq!(b.z_load, Complex64::new(25.0, 0.0));
    assert_eq!(b.z_target, Complex64::new(50.0, 0.0));
    assert_eq!(b.solutions.len(), 2, "{b:#?}");
    assert!(
        b.solutions.iter().all(|s| s.topology == "series-first"),
        "{b:#?}"
    );
    let a = find(&b, Some(("C", 2.241619e-10)), Some(("L", 2.802024e-7)));
    assert!((a.series.as_ref().unwrap().2 - 25.0).abs() <= 1e-5, "{a:?}");
    assert!((a.shunt.as_ref().unwrap().2 + 50.0).abs() <= 1e-5, "{a:?}");
    assert!((a.q - 1.0).abs() <= 1e-3, "{a:?}");
    let c = find(&b, Some(("L", 5.604047e-7)), Some(("C", 4.483238e-10)));
    assert!((c.series.as_ref().unwrap().2 + 25.0).abs() <= 1e-5, "{c:?}");
    assert!((c.shunt.as_ref().unwrap().2 - 50.0).abs() <= 1e-5, "{c:?}");
    assert_recomputed(&b, 1e-6);
}

// ---------------------------------------------------------------------------
// M2: 73 + j42 → 50: exactly two shunt-first networks; at half the frequency the
// reactances are the same, so every L and C doubles.
// ---------------------------------------------------------------------------

#[test]
fn m2_complex_load_two_shunt_first_networks_and_frequency_scaling() {
    let (_, b) = match_z("73+j42", "14.2", &[]);
    assert_eq!(b.z_load, Complex64::new(73.0, 42.0));
    assert_eq!(b.solutions.len(), 2, "{b:#?}");
    assert!(
        b.solutions.iter().all(|s| s.topology == "shunt-first"),
        "{b:#?}"
    );
    let a = find(&b, Some(("C", 1.784000e-10)), Some(("L", 5.442819e-7)));
    assert!(
        (a.shunt.as_ref().unwrap().2 + 62.8256).abs() <= 1e-3,
        "{a:?}"
    );
    assert!(
        (a.series.as_ref().unwrap().2 - 48.5615).abs() <= 1e-3,
        "{a:?}"
    );
    assert!((a.q - 0.971).abs() <= 1e-3, "{a:?}");
    find(&b, Some(("L", 2.750851e-6)), Some(("C", 2.308021e-10)));
    assert_recomputed(&b, 1e-6);

    let (_, h) = match_z("73+j42", "7.1", &[]);
    assert_eq!(h.solutions.len(), 2, "{h:#?}");
    let ha = find(&h, Some(("C", 3.568001e-10)), Some(("L", 1.088564e-6)));
    assert!(
        rel(
            ha.shunt.as_ref().unwrap().1,
            2.0 * a.shunt.as_ref().unwrap().1
        ) <= 1e-6
    );
    assert!(
        rel(
            ha.series.as_ref().unwrap().1,
            2.0 * a.series.as_ref().unwrap().1
        ) <= 1e-6
    );
    assert!((ha.shunt.as_ref().unwrap().2 - a.shunt.as_ref().unwrap().2).abs() <= 1e-5);
    find(&h, Some(("L", 5.501702e-6)), Some(("C", 4.616041e-10)));
    assert_recomputed(&h, 1e-6);
}

// ---------------------------------------------------------------------------
// M3: 5 − j300 → 50 at 3.6 MHz: both topologies, four networks.
// ---------------------------------------------------------------------------

#[test]
fn m3_low_resistance_load_four_networks() {
    let (_, b) = match_z("5-j300", "3.6", &[]);
    assert_eq!(b.solutions.len(), 4, "{b:#?}");
    assert_eq!(
        b.solutions
            .iter()
            .filter(|s| s.topology == "series-first")
            .count(),
        2
    );
    assert_eq!(
        b.solutions
            .iter()
            .filter(|s| s.topology == "shunt-first")
            .count(),
        2
    );
    find(&b, Some(("C", 2.652582e-9)), Some(("L", 1.392606e-5)));
    find(&b, Some(("L", 7.368284e-7)), Some(("L", 1.259977e-5)));
    find(&b, Some(("L", 1.939085e-5)), Some(("L", 4.188855e-5)));
    find(&b, Some(("L", 1.008229e-5)), Some(("C", 4.665948e-11)));
    assert_recomputed(&b, 1e-6);
}

// ---------------------------------------------------------------------------
// M4: 50 + j30 → 50: a one-element match, reported once, with no rounding
// residue printed as a component.
// ---------------------------------------------------------------------------

#[test]
fn m4_one_element_match_once_without_residue_components() {
    let (_, b) = match_z("50+j30", "14.2", &[]);
    assert_eq!(b.solutions.len(), 2, "{b:#?}");
    let alone = find(&b, None, Some(("C", 3.736032e-10)));
    assert!(
        (alone.series.as_ref().unwrap().2 + 30.0).abs() <= 1e-5,
        "{alone:?}"
    );
    find(&b, Some(("C", 1.977899e-10)), Some(("L", 3.362428e-7)));
    for s in &b.solutions {
        for e in [&s.shunt, &s.series].into_iter().flatten() {
            assert!(e.1 > 1e-20, "residue component {e:?} in {s:?}");
        }
    }
    assert_recomputed(&b, 1e-6);
}

// ---------------------------------------------------------------------------
// M5: already matched.
// ---------------------------------------------------------------------------

#[test]
fn m5_already_matched_load_has_no_network_and_says_so() {
    let (out, b) = match_z("50", "14.2", &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(b.solutions.is_empty(), "{b:#?}");
    assert!(b.notes.iter().any(|n| n == "already matched"), "{b:#?}");
    assert!(stdout(&out).contains("SOLUTIONS 0\n"));
}

// ---------------------------------------------------------------------------
// M6: refusals and usage errors.
// ---------------------------------------------------------------------------

#[test]
fn m6_non_positive_resistance_is_refused_and_usage_errors_exit_2() {
    for z in ["-2.1-j444", "0+j100"] {
        let out = run(&["match", "--z", z, "--freq", "14.2"]);
        assert_eq!(out.status.code(), Some(1), "--z {z}:\n{}", stderr(&out));
        let err = stderr(&out);
        assert!(err.contains("cannot match"), "--z {z}: {err}");
        assert!(
            !stdout(&out).contains("SOLUTION "),
            "a match was printed for --z {z}:\n{}",
            stdout(&out)
        );
    }
    let usage: &[&[&str]] = &[
        &["match", "--z", "73+j42"],
        &["match", "--z", "73j42", "--freq", "14.2"],
        &["match", "--z", "73+42j", "--freq", "14.2"],
        &["match", "--z", "73+j42", "--freq", "14.2", "--target", "0"],
        &[
            "match", "--z", "73+j42", "--freq", "14.2", "--target", "-50",
        ],
        &["match", "--z", "73+j42", "--freq", "0"],
        &["match", "--z", "73+j42", "--freq", "14.2", "--bogus"],
        &[
            "match",
            "--z",
            "73+j42",
            "--freq",
            "14.2",
            "--output-format",
            "yaml",
        ],
        &["match", "--z"],
        &["match"],
    ];
    for args in usage {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}:\n{}", stderr(&out));
        assert!(
            stderr(&out).contains("Usage: fnec match"),
            "{args:?}: {}",
            stderr(&out)
        );
    }
}

// ---------------------------------------------------------------------------
// M7: the deck route goes through the main command.
// ---------------------------------------------------------------------------

/// Z_LOAD is the main command's `--output-format json` impedance.
#[test]
fn m7a_deck_z_load_is_the_main_commands_impedance() {
    let deck = corpus("monopole-pec-26seg.nec");
    let main = run(&["--output-format", "json", deck.to_str().unwrap()]);
    assert!(main.status.success(), "{}", stderr(&main));
    let records: Vec<serde_json::Value> = serde_json::from_str(&stdout(&main)).unwrap();
    let z_main = Complex64::new(
        records[0]["z_re"].as_f64().unwrap(),
        records[0]["z_im"].as_f64().unwrap(),
    );

    let out = run(&["match", deck.to_str().unwrap(), "--output-format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let blocks = v["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 1, "{v}");
    let z_match = Complex64::new(
        blocks[0]["z_load"]["re"].as_f64().unwrap(),
        blocks[0]["z_load"]["im"].as_f64().unwrap(),
    );
    assert!(
        (z_match - z_main).norm() <= 1e-9,
        "match Z_LOAD {z_match} is not the main command's {z_main}"
    );
    assert_eq!(
        blocks[0]["freq_mhz"].as_f64().unwrap(),
        records[0]["freq_mhz"].as_f64().unwrap()
    );

    let text = run(&["match", deck.to_str().unwrap()]);
    let b = parse_blocks(&stdout(&text)).remove(0);
    assert!((b.z_load - z_main).norm() <= 1e-6, "{b:#?}");
    assert_recomputed(&b, 1e-6);
}

/// FND-291 / D4: a two-source deck is matched at its first feedpoint — the
/// record's top-level impedance — and the note says how many the solve reported,
/// counted from the record's own `feedpoints` (it re-parsed the deck to count
/// `EX` cards, a second answer to "how many"). A one-source deck has no note.
#[test]
fn m7e_a_two_source_deck_is_matched_at_its_first_feedpoint_and_says_so() {
    let two = common::TempDeck::new(
        "fnec-match-m7e-two-sources.nec",
        "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 2.5 0 -2.5 2.5 0 2.5 0.001\nGE 0\n\
         EX 0 1 11 0 1 0\nEX 0 2 11 0 0 1\nFR 0 1 0 0 29.9 0\nEN\n",
    );
    let main = run(&["--output-format", "json", two.to_str().unwrap()]);
    assert!(main.status.success(), "{}", stderr(&main));
    let records: Vec<serde_json::Value> = serde_json::from_str(&stdout(&main)).unwrap();
    let z_first = Complex64::new(
        records[0]["z_re"].as_f64().unwrap(),
        records[0]["z_im"].as_f64().unwrap(),
    );

    let out = run(&["match", two.to_str().unwrap(), "--output-format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let block = &v["blocks"][0];
    let z_load = Complex64::new(
        block["z_load"]["re"].as_f64().unwrap(),
        block["z_load"]["im"].as_f64().unwrap(),
    );
    assert!((z_load - z_first).norm() <= 1e-9, "{z_load} vs {z_first}");
    let notes: Vec<&str> = block["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("2 feedpoints") && n.contains("tag 1, segment 11")),
        "{notes:?}"
    );

    // The control: one source, no note.
    let one = run(&[
        "match",
        corpus("dipole-freesp-51seg.nec").to_str().unwrap(),
        "--output-format",
        "json",
    ]);
    assert!(one.status.success(), "{}", stderr(&one));
    let v: serde_json::Value = serde_json::from_str(&stdout(&one)).unwrap();
    assert_eq!(v["blocks"][0]["notes"], serde_json::json!([]), "{v}");
}

/// The first series-first solution, written into the deck as `LD 0` (the series
/// element) and `NT` (the shunt element as a one-port admittance on the feed
/// segment), makes the main command report the target.
#[test]
fn m7b_solution_written_as_ld0_and_nt_presents_the_target() {
    let deck = corpus("monopole-pec-26seg.nec");
    let out = run(&["match", deck.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let b = parse_blocks(&stdout(&out)).remove(0);
    let s = b
        .solutions
        .iter()
        .find(|s| s.topology == "series-first" && s.shunt.is_some() && s.series.is_some())
        .unwrap_or_else(|| panic!("no two-element series-first solution in {b:#?}"));
    let omega = 2.0 * std::f64::consts::PI * b.freq_mhz * 1e6;
    let series = s.series.as_ref().unwrap();
    let ld = match series.0.as_str() {
        "C" => format!("LD 0 1 1 1 0 0 {:.9e}", series.1),
        "L" => format!("LD 0 1 1 1 0 {:.9e} 0", series.1),
        k => panic!("{k}"),
    };
    let shunt = s.shunt.as_ref().unwrap();
    let b_shunt = match shunt.0.as_str() {
        "C" => omega * shunt.1,
        "L" => -1.0 / (omega * shunt.1),
        k => panic!("{k}"),
    };
    let nt = format!("NT 1 1 1 1 0 {b_shunt:.9e} 0 0 0 0");
    let body = std::fs::read_to_string(&deck).unwrap();
    assert!(
        body.contains("EX 0 1 1 0 1.0 0.0\n"),
        "fixture changed:\n{body}"
    );
    let matched = body.replace(
        "EX 0 1 1 0 1.0 0.0\n",
        &format!("{ld}\n{nt}\nEX 0 1 1 0 1.0 0.0\n"),
    );
    let path = common::TempDeck::new("fnec-match-m7b-monopole-matched.nec", &matched);

    let main = run(&["--output-format", "json", path.to_str().unwrap()]);
    assert!(main.status.success(), "{}\n{matched}", stderr(&main));
    let records: Vec<serde_json::Value> = serde_json::from_str(&stdout(&main)).unwrap();
    let z = Complex64::new(
        records[0]["z_re"].as_f64().unwrap(),
        records[0]["z_im"].as_f64().unwrap(),
    );
    assert!(
        (z - b.z_target).norm() <= 1e-3,
        "the matched deck solves to {z}, not {} — deck:\n{matched}",
        b.z_target
    );
}

/// A deck the main command refuses is refused with the main command's sentence.
#[test]
fn m7c_deck_refused_by_the_main_command_is_refused_here() {
    let body = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 9 1 26 26 1.0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let path = common::TempDeck::new("fnec-match-m7c-ld9.nec", body);
    let out = run(&["match", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("LD 9 1 26 26") && err.contains("not supported"),
        "the refusal must be the main command's, naming the card: {err}"
    );
    assert!(!stdout(&out).contains("MATCH_RESULT"), "{}", stdout(&out));
}

/// A deck that solves to a negative feedpoint resistance has no passive match.
#[test]
fn m7d_negative_resistance_deck_is_refused_naming_the_resistance() {
    let deck = corpus("u-one-segment-negative-r-freesp.nec");
    let out = run(&["match", deck.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("-2.117 Ω") && err.contains("cannot match"),
        "{err}"
    );
    assert!(!stdout(&out).contains("SOLUTION "), "{}", stdout(&out));
}

// ---------------------------------------------------------------------------
// M8: a complex target is presented, reactance included.
// ---------------------------------------------------------------------------

#[test]
fn m8_complex_target_is_presented() {
    let target = Complex64::new(25.0, 10.0);
    for z in ["73+j42", "25", "5-j300", "200-j100"] {
        let (_, b) = match_z(z, "14.2", &["--target", "25+j10"]);
        assert_eq!(b.z_target, target, "{b:#?}");
        assert!(!b.solutions.is_empty(), "{b:#?}");
        for s in &b.solutions {
            let zin = recompute(s, b.z_load, b.freq_mhz);
            assert!(
                (zin - target).norm() <= 1e-6,
                "--z {z}: {s:?} recomputed to {zin}, not {target}"
            );
        }
    }
    let (_, b) = match_z("73+j42", "14.2", &["--target", "50-j20"]);
    assert_eq!(b.z_target, Complex64::new(50.0, -20.0));
    assert_recomputed(&b, 1e-6);
}

// ---------------------------------------------------------------------------
// M9: JSON.
// ---------------------------------------------------------------------------

#[test]
// VERIFIES: FR-012
fn m9_json_parses_matches_text_and_is_stable() {
    for (z, freq) in [
        ("73+j42", "14.2"),
        ("5-j300", "3.6"),
        ("50+j30", "14.2"),
        ("50", "14.2"),
    ] {
        let (text_out, b) = match_z(z, freq, &[]);
        let j1 = run(&["match", "--z", z, "--freq", freq, "--output-format", "json"]);
        let j2 = run(&["match", "--z", z, "--freq", freq, "--output-format", "json"]);
        assert!(j1.status.success(), "{}", stderr(&j1));
        let v1: serde_json::Value = serde_json::from_str(&stdout(&j1)).expect("valid JSON");
        let v2: serde_json::Value = serde_json::from_str(&stdout(&j2)).expect("valid JSON");
        assert_eq!(v1, v2, "two runs differ");
        assert_eq!(v1["schema"], "fnec-match/1");
        let blocks = v1["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        let blk = &blocks[0];
        assert_eq!(blk["freq_mhz"].as_f64().unwrap(), b.freq_mhz);
        assert_eq!(blk["z_load"]["re"].as_f64().unwrap(), b.z_load.re);
        assert_eq!(blk["z_load"]["im"].as_f64().unwrap(), b.z_load.im);
        assert_eq!(blk["z_target"]["re"].as_f64().unwrap(), 50.0);
        let sols = blk["solutions"].as_array().unwrap();
        assert_eq!(
            sols.len(),
            b.solutions.len(),
            "JSON vs text:\n{}",
            stdout(&text_out)
        );
        for (js, ts) in sols.iter().zip(&b.solutions) {
            assert_eq!(js["topology"], ts.topology.as_str());
            for (role, te) in [("shunt", &ts.shunt), ("series", &ts.series)] {
                match te {
                    None => assert!(js[role].is_null(), "{js}"),
                    Some((k, v, x)) => {
                        assert_eq!(js[role]["kind"], k.as_str());
                        assert!(rel(js[role]["value"].as_f64().unwrap(), *v) <= 1e-6, "{js}");
                        assert!((js[role]["x"].as_f64().unwrap() - x).abs() <= 1e-5, "{js}");
                    }
                }
            }
            assert!((js["q"].as_f64().unwrap() - ts.q).abs() <= 1e-3);
            assert!((js["z_in"]["re"].as_f64().unwrap() - ts.z_in.re).abs() <= 1e-5);
        }
        let notes: Vec<&str> = blk["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        assert_eq!(
            notes,
            b.notes.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }
}
