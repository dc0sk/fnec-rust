// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-233 — `--solver sinusoidal` solves a collinear chain whatever the order of
//! its `GW` cards.
//!
//! The sinusoidal basis holds a conductor as one index range, and the collinear
//! merge only welds wires adjacent in the segment array: a dipole split over three
//! cards solved when written bottom-to-top and was refused as "a bend, split,
//! junction or loop" when the bottom wire was written last. The arm now solves the
//! segments in chain order (`nec_solver::SegmentOrder`) and restores the currents.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn run(solver: &str, deck_text: &str) -> (bool, String, String) {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "fnec-sin-chain-order-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck_text).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", solver])
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The cards in `order`, then `tail`.
fn deck(wires: &[&str], order: &[usize], tail: &str) -> String {
    let mut d: String = order.iter().map(|&i| format!("{}\n", wires[i])).collect();
    d.push_str(tail);
    d
}

const ORDERS: [[usize; 3]; 3] = [[0, 1, 2], [1, 2, 0], [2, 0, 1]];

/// A λ/2 dipole at 29.9 MHz split over three collinear cards of unequal segment
/// lengths, fed on the middle card and loaded on the top one. Unequal lengths and
/// the load are load-bearing: the free-end rows weight by the end segments'
/// lengths, and a load column is shaped over the merged conductor — both read the
/// chain off the array, so a solve that permuted only the finished matrix, or
/// stamped the loads in deck order, answers differently per order.
const SPLIT_DIPOLE: [&str; 3] = [
    "GW 1 7 0 0 -2.5 0 0 -0.7 0.001",
    "GW 2 9 0 0 -0.7 0 0 0.9 0.001",
    "GW 3 5 0 0 0.9 0 0 2.5 0.001",
];
const LOADED_TAIL: &str = "GE 0\nLD 4 3 2 2 30 0 0\nEX 0 2 4 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";

/// The FEEDPOINTS row for `(tag, seg)`: (Re Z, Im Z).
fn z_at(stdout: &str, tag: u32, seg: u32) -> (f64, f64) {
    let row = stdout
        .lines()
        .find(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            c.len() == 8 && c[0] == tag.to_string() && c[1] == seg.to_string()
        })
        .unwrap_or_else(|| panic!("no feedpoint row for {tag}/{seg}:\n{stdout}"));
    let f: Vec<f64> = row
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    (f[6], f[7])
}

/// The CURRENTS table: (tag, seg) → the row's text, sorted by key.
type CurrentsTable = Vec<((u32, u32), String)>;

fn currents(stdout: &str) -> CurrentsTable {
    let mut rows: CurrentsTable = stdout
        .lines()
        .skip_while(|l| !l.starts_with("CURRENTS"))
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            Some((
                (c.first()?.parse().ok()?, c.get(1)?.parse().ok()?),
                l.to_string(),
            ))
        })
        .collect();
    rows.sort();
    rows
}

fn diag_field<'a>(stderr: &'a str, key: &str) -> &'a str {
    let line = stderr
        .lines()
        .find(|l| l.starts_with("diag:"))
        .unwrap_or_else(|| panic!("no diag line:\n{stderr}"));
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no {key} in {line}"))
}

/// Every order of the split dipole is solved by the sinusoidal basis itself —
/// not refused, not handed to the Hallén fallback — on the plain route, with the
/// same impedance and the same current on every deck segment.
#[test]
fn every_order_of_a_split_dipole_solves_the_same() {
    let mut first: Option<((f64, f64), CurrentsTable)> = None;
    for order in ORDERS {
        let (ok, out, err) = run("sinusoidal", &deck(&SPLIT_DIPOLE, &order, LOADED_TAIL));
        assert!(ok, "order {order:?} refused:\n{err}");
        assert_eq!(diag_field(&err, "mode"), "sinusoidal", "order {order:?}");
        assert_eq!(diag_field(&err, "route"), "plain", "order {order:?}");
        let z = z_at(&out, 2, 4);
        let table = currents(&out);
        // Every deck segment, once: 7 + 9 + 5.
        let keys: Vec<(u32, u32)> = table.iter().map(|(k, _)| *k).collect();
        let expected: Vec<(u32, u32)> = [(1, 7), (2, 9), (3, 5)]
            .iter()
            .flat_map(|&(t, n)| (1..=n).map(move |s| (t, s)))
            .collect();
        assert_eq!(keys, expected, "order {order:?}");
        match &first {
            None => first = Some((z, table)),
            Some((z0, t0)) => {
                assert_eq!(z, *z0, "order {order:?}: Z moved with card order");
                assert_eq!(
                    &table, t0,
                    "order {order:?}: a current moved with card order"
                );
            }
        }
    }
}

/// The chain-ordered answer is the antenna's: within the basis difference of the
/// Hallén solve of the same deck (1.2 % here, the same as in deck order, where
/// nothing is permuted). A permutation applied to only part of the solve misses
/// this by far more.
#[test]
fn the_chain_ordered_answer_agrees_with_hallen() {
    let tail = LOADED_TAIL;
    for order in ORDERS {
        let d = deck(&SPLIT_DIPOLE, &order, tail);
        let (ok_s, out_s, err_s) = run("sinusoidal", &d);
        let (ok_h, out_h, err_h) = run("hallen", &d);
        assert!(ok_s && ok_h, "order {order:?}:\n{err_s}\n{err_h}");
        let (s, h) = (z_at(&out_s, 2, 4), z_at(&out_h, 2, 4));
        let rel = (s.0 - h.0).hypot(s.1 - h.1) / h.0.hypot(h.1);
        assert!(
            rel < 0.02,
            "order {order:?}: sinusoidal {s:?} vs hallen {h:?} ({rel:.4})"
        );
    }
}

/// The refusal still holds where the plain basis has nothing to solve, in any
/// order: a T junction, and a chain with its middle wire written backwards (a
/// reversed split, which only the conductor-path basis solves).
#[test]
fn a_scrambled_junction_or_reversed_split_is_still_refused() {
    const REFUSAL: &str = "solves straight wires and collinear chains only";
    let tee = [
        "GW 1 11 0 0 0 0 0 2 0.001",
        "GW 2 7 -1.5 0 2 0 0 2 0.001",
        "GW 3 7 0 0 2 1.5 0 2 0.001",
    ];
    let reversed = [
        SPLIT_DIPOLE[0],
        "GW 2 9 0 0 0.9 0 0 -0.7 0.001",
        SPLIT_DIPOLE[2],
    ];
    let cases: [(&str, &[&str; 3], &str); 2] = [
        ("tee", &tee, "GE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n"),
        (
            "reversed split",
            &reversed,
            "GE 0\nEX 0 2 4 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n",
        ),
    ];
    for (name, wires, tail) in cases {
        for order in ORDERS {
            let (ok, _, err) = run("sinusoidal", &deck(wires, &order, tail));
            assert!(!ok && err.contains(REFUSAL), "{name} {order:?}:\n{err}");
            // The control: the same deck is solvable, so the refusal is about the
            // basis, not a broken deck.
            let (ok_h, _, err_h) = run("hallen", &deck(wires, &order, tail));
            assert!(ok_h, "{name} {order:?} on hallen:\n{err_h}");
        }
    }
}
