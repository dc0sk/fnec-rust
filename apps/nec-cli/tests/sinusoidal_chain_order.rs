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
/// order: a T junction.
#[test]
fn a_scrambled_junction_is_still_refused() {
    const REFUSAL: &str = "solves straight wires and collinear chains only";
    let tee = [
        "GW 1 11 0 0 0 0 0 2 0.001",
        "GW 2 7 -1.5 0 2 0 0 2 0.001",
        "GW 3 7 0 0 2 1.5 0 2 0.001",
    ];
    let tail = "GE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
    for order in ORDERS {
        let (ok, _, err) = run("sinusoidal", &deck(&tee, &order, tail));
        assert!(!ok && err.contains(REFUSAL), "tee {order:?}:\n{err}");
        // The control: the same deck is solvable, so the refusal is about the
        // basis, not a broken deck.
        let (ok_h, _, err_h) = run("hallen", &deck(&tee, &order, tail));
        assert!(ok_h, "tee {order:?} on hallen:\n{err_h}");
    }
}

/// One CURRENTS row as (re, im).
fn current(stdout: &str, tag: u32, seg: u32) -> (f64, f64) {
    let rows = currents(stdout);
    let (_, line) = rows
        .iter()
        .find(|(k, _)| *k == (tag, seg))
        .unwrap_or_else(|| panic!("no current row for {tag}/{seg}:\n{stdout}"));
    let f: Vec<f64> = line
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    (f[2], f[3])
}

fn close(a: (f64, f64), b: (f64, f64), scale: f64) -> bool {
    (a.0 - b.0).hypot(a.1 - b.1) <= 1e-9 * scale
}

/// FND-249: a chain with its middle wire written backwards — a start-to-start
/// split — solves on the sinusoidal basis in any order. The arm flips the reversed
/// wire into the walk's direction, so it is the same antenna as the forward chain,
/// fed and loaded at the same points. The gap sits ON the reversed wire and drives
/// current along that wire's own direction, the other way from the forward deck's:
/// the feedpoint is equal, the reversed wire's currents — each reported in its own
/// start→end sense, as nec2c prints it, and as the Hallén path solve reports this
/// deck — equal the forward ones, and the other wires' are negated. A source not
/// negated for the flip, or currents not negated back, flips Z's sign.
#[test]
fn a_reversed_split_solves_as_the_forward_chain() {
    let reversed = [
        SPLIT_DIPOLE[0],
        "GW 2 9 0 0 0.9 0 0 -0.7 0.001",
        SPLIT_DIPOLE[2],
    ];
    // Reversed segment k of tag 2 is forward segment 10 − k: the feed at 4 is at 6.
    let rev_tail = LOADED_TAIL;
    let fwd_tail = LOADED_TAIL.replace("EX 0 2 4", "EX 0 2 6");
    let (ok_f, out_f, err_f) = run("sinusoidal", &deck(&SPLIT_DIPOLE, &[0, 1, 2], &fwd_tail));
    assert!(ok_f, "forward:\n{err_f}");
    let z_f = z_at(&out_f, 2, 6);
    for order in ORDERS {
        let (ok, out, err) = run("sinusoidal", &deck(&reversed, &order, rev_tail));
        assert!(ok, "reversed {order:?}:\n{err}");
        assert_eq!(diag_field(&err, "route"), "plain", "{order:?}");
        let scale = z_f.0.hypot(z_f.1);
        let z = z_at(&out, 2, 4);
        assert!(close(z, z_f, scale), "{order:?}: {z:?} vs forward {z_f:?}");
        let peak = current(&out_f, 2, 5).0.hypot(current(&out_f, 2, 5).1);
        for k in 1..=9 {
            let (r, f) = (current(&out, 2, k), current(&out_f, 2, 10 - k));
            assert!(close(r, f, peak), "{order:?} tag 2 seg {k}: {r:?} vs {f:?}");
        }
        for (tag, n) in [(1, 7), (3, 5)] {
            for k in 1..=n {
                let (r, f) = (current(&out, tag, k), current(&out_f, tag, k));
                assert!(
                    close(r, (-f.0, -f.1), peak),
                    "{order:?} tag {tag} seg {k}: {r:?} vs -{f:?}"
                );
            }
        }
    }
}

/// FND-249: the dipole `GX` writes — one half, and its mirror as a second wire
/// meeting it start-to-start — is the one-wire dipole on the sinusoidal basis,
/// current for current. Here the feed is on the half that keeps its frame (the
/// first written), so only the reflected half's currents show whether they were
/// negated back: the mirror (tag 3, `GX` adds 2) segment k is the one-wire dipole's
/// segment 11 − k, negated.
#[test]
fn a_gx_dipole_is_the_one_wire_dipole() {
    let gx =
        "CE\nGW 1 10 0 0 0 0 0 2.64 .001\nGX 2 001\nGE 0\nEX 0 1 1 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n";
    let one = "CE\nGW 1 20 0 0 -2.64 0 0 2.64 .001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n";
    let (ok_g, out_g, err_g) = run("sinusoidal", gx);
    let (ok_o, out_o, err_o) = run("sinusoidal", one);
    assert!(ok_g && ok_o, "{err_g}\n{err_o}");
    let (z_g, z_o) = (z_at(&out_g, 1, 1), z_at(&out_o, 1, 11));
    assert!(close(z_g, z_o, z_o.0.hypot(z_o.1)), "{z_g:?} vs {z_o:?}");
    let peak = current(&out_o, 1, 11).0.hypot(current(&out_o, 1, 11).1);
    for k in 1..=10 {
        let (a, b) = (current(&out_g, 1, k), current(&out_o, 1, 10 + k));
        assert!(close(a, b, peak), "tag 1 seg {k}: {a:?} vs {b:?}");
        let (a, b) = (current(&out_g, 3, k), current(&out_o, 1, 11 - k));
        assert!(
            close(a, (-b.0, -b.1), peak),
            "tag 3 seg {k}: {a:?} vs -{b:?}"
        );
    }
}

/// FND-273: a one-segment wire inside a collinear chain is part of one longer
/// conductor, which is what the sinusoidal basis solves on. Judged per deck wire,
/// it fell back to the experimental pulse basis and answered −75.96 + j1.41 Ω
/// against nec2c's 78.81 + j40.99. Now it takes the sinusoidal route, whose own
/// residual gate hands this coarse chain to Hallén: the answer is `--solver
/// hallen`'s, either way round the middle wire (FND-249 signs its source).
#[test]
fn a_one_segment_wire_in_a_chain_takes_the_sinusoidal_route() {
    let wires = |mid: &str| {
        format!(
            "CE\nGW 1 4 0 0 -2.5 0 0 -0.3 0.001\n{mid}\nGW 3 4 0 0 0.3 0 0 2.5 0.001\n\
             GE 0\nEX 0 2 1 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n"
        )
    };
    let forward = wires("GW 2 1 0 0 -0.3 0 0 0.3 0.001");
    let (ok_f, out_f, err_f) = run("sinusoidal", &forward);
    let (ok_r, out_r, err_r) = run("sinusoidal", &wires("GW 2 1 0 0 0.3 0 0 -0.3 0.001"));
    let (ok_h, out_h, err_h) = run("hallen", &forward);
    assert!(ok_f && ok_r && ok_h, "{err_f}\n{err_r}\n{err_h}");
    for err in [&err_f, &err_r] {
        assert!(
            !err.contains("falling back to pulse") && !err.contains("->pulse"),
            "{err}"
        );
        assert!(err.contains("diag: mode=sinusoidal"), "{err}");
    }
    let (f, r, h) = (z_at(&out_f, 2, 1), z_at(&out_r, 2, 1), z_at(&out_h, 2, 1));
    assert!(close(r, f, f.0.hypot(f.1)), "{r:?} vs forward {f:?}");
    // The same Hallén solve in another segment order: equal to print precision.
    assert!(
        (f.0 - h.0).hypot(f.1 - h.1) <= 1e-5,
        "{f:?} vs hallen {h:?}"
    );
    // nec2c 1.3.1 on this deck: 78.806 + j40.991. Hallén on a 0.6 m middle segment
    // reads 77.923 + j35.535, 6 % of |Z| away; the pulse fallback was 177 % away.
    let nec2c = (78.806_f64, 40.991_f64);
    let off = (f.0 - nec2c.0).hypot(f.1 - nec2c.1) / nec2c.0.hypot(nec2c.1);
    assert!(off < 0.1, "{f:?} is {off:.3} of |Z| from nec2c {nec2c:?}");
}
