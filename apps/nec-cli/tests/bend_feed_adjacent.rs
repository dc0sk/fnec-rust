// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-270: a feed on the segment beside a bend node. The node's continuity row
// extrapolated the current linearly to the node from each side's two nearest
// segments; a gap kinks the current at its segment's midpoint, between that
// side's two points, so the row was false by half a segment times the slope jump.
// A dipole drawn as two halves meeting at the feed's end read 80.057 + j43.088 Ω
// straight (one section, no node row) and 79.221 + j43.577 at a bend of 2e-4
// rad, a 1 % step no correct scheme makes. The row now takes the gap segment's
// current from the other section's quadratic extrapolation — on the conductor
// paths and on the section graph alike, for a feed, a port or a load step.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

/// Two halves of 10 × 0.264 m at 28.4 MHz, the upper tilted by `t` radians,
/// driven by `ex` (0 a voltage gap, 4 a current source) on tag 1 segment `feed`.
fn deck(t: f64, ex: u32, feed: u32) -> String {
    deck_with(t, &format!("EX {ex} 1 {feed} 0 1 0\n"), ".001")
}

/// [`deck`] with its cards after `GE` given, and the upper half's radius `r2`: at
/// .00101 the 1 % step sends the solve to the section graph (FND-252), whose
/// node sits at the join even when straight.
fn deck_with(t: f64, cards: &str, r2: &str) -> String {
    let (x, z) = (2.64 * t.sin(), 2.64 * t.cos());
    format!(
        "CE\nGW 1 10 0 0 -2.64 0 0 0 .001\nGW 2 10 0 0 0 {x:.12} 0 {z:.12} {r2}\nGE 0\n\
         {cards}FR 0 1 0 0 28.4 0\nEN\n"
    )
}

fn z_in(text: &str) -> (f64, f64) {
    let lines = run(text);
    let at = lines
        .iter()
        .position(|l| l == "FEEDPOINTS")
        .expect("FEEDPOINTS");
    let c: Vec<f64> = lines[at + 2]
        .split_whitespace()
        .map(|v| v.parse().expect("number"))
        .collect();
    (c[6], c[7])
}

/// `|I|` of tag 1 segment 10, beside the node, from the CURRENTS table.
fn current_beside_node(text: &str) -> f64 {
    let lines = run(text);
    let at = lines
        .iter()
        .position(|l| l == "CURRENTS")
        .expect("CURRENTS");
    lines[at..]
        .iter()
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .find(|c| c.len() == 6 && c[0] == "1" && c[1] == "10")
        .expect("tag 1 seg 10")[4]
        .parse()
        .expect("I_MAG")
}

fn rel((a, b): (f64, f64), (c, d): (f64, f64)) -> f64 {
    (a - c).hypot(b - d) / c.hypot(d)
}

fn run(text: &str) -> Vec<String> {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let name = format!(
        "fnec-270-{}-{}.nec",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    );
    let deck = common::TempDeck::new(&name, text);
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen"])
        .arg(&deck)
        .output()
        .expect("run fnec");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(String::from)
        .collect()
}

#[test]
fn a_vanishing_bend_beside_the_feed_is_the_straight_join() {
    for ex in [0, 4] {
        let straight = z_in(&deck(0.0, ex, 10));
        let bent = z_in(&deck(2e-4, ex, 10));
        let step = (bent.0 - straight.0).abs() / straight.0;
        // 1.04 % before; 0.11 % after, which a 2e-4 rad bend may move.
        assert!(
            step < 0.002,
            "EX {ex}: {bent:?} vs straight {straight:?}, {step:.4}"
        );
        // The control: two segments from the node, the row was always right.
        let (s8, b8) = (z_in(&deck(0.0, ex, 8)), z_in(&deck(2e-4, ex, 8)));
        assert!(
            (b8.0 - s8.0).abs() / s8.0 < 2e-4,
            "EX {ex}: {b8:?} vs {s8:?}"
        );
    }
}

/// A real bend, fed beside it, against nec2c's converged value at 1 rad: 81 ×
/// the mesh, extended kernel (`scripts/nec2c-converged-series.py --ek --to 81`,
/// captured 2026-10-23; R 64.519, 64.271, 64.411, 64.513, 64.565 at 1 / 3 / 9 /
/// 27 / 81 ×, the last step half the one before, a tail of 0.05 Ω): 64.565 +
/// j34.833 Ω. It was the thin kernel's 27 × point, 64.518, whose last two steps
/// (0.141, 0.106 Ω) left a 0.32 Ω (0.5 %) tail — wider than half this ±0.35 %
/// band (FND-296). The 81 × corner segment is 3.26 radii, FND-267's regime, so
/// it is taken with a thinner-wire control: at 0.2 mm the series keeps its shape
/// (63.055, 63.118, 63.17, 63.197 at 3 / 9 / 27 / 81 ×, steps 0.063, 0.052,
/// 0.027 against 0.140, 0.102, 0.052). Hallén sits about 1.25 % low in R, with
/// the straight dipole's 1.26 % (81.079 against 80.057); before the fix it was
/// 1.9 % low (63.316).
#[test]
fn a_real_bend_fed_beside_it_sits_with_the_straight_offset() {
    let (r, _) = z_in(&deck(1.0, 0, 10));
    let off = (r - 64.565) / 64.565;
    assert!(
        (-0.016..-0.009).contains(&off),
        "{r}: {off:.4} from converged nec2c"
    );
}

/// A network port beside the node is a gap too: the network arm solves a unit
/// gap there. Fed two segments away, a shunt admittance on the node-adjacent
/// segment stepped the input by 0.52 % at 2e-4 rad; 0.07 % with the port marked.
#[test]
fn a_network_port_beside_a_vanishing_bend_is_the_straight_join() {
    let with_port =
        |t: f64| deck(t, 0, 5).replace("EX 0", "NT 1 10 1 10 0.002 0.004 0 0 0.002 0.004\nEX 0");
    let (straight, bent) = (z_in(&with_port(0.0)), z_in(&with_port(2e-4)));
    let step = (bent.0 - straight.0).abs() / straight.0;
    assert!(step < 0.002, "{bent:?} vs straight {straight:?}, {step:.4}");
}

/// The section graph has the same node row (FND-270): a 1 % radius step on the
/// upper half takes the straight dipole to the graph, whose node beside the feed
/// read 79.226 + j43.63 Ω — the conductor paths' pre-fix figure — against the
/// single-section 80.057 + j43.09. With the gap marked it reads 79.973 + j43.13,
/// 0.11 %, for the voltage gap, the current source (the graph's unit-gap solve)
/// and a network port.
#[test]
fn the_section_graph_beside_a_feed_is_the_conductor_path() {
    for ex in [0, 4] {
        let path = z_in(&deck(0.0, ex, 10));
        let graph = z_in(&deck_with(0.0, &format!("EX {ex} 1 10 0 1 0\n"), ".00101"));
        assert!(rel(graph, path) < 0.003, "EX {ex}: {graph:?} vs {path:?}");
    }
    let port = "NT 1 10 1 10 0.002 0.004 0 0 0.002 0.004\nEX 0 1 5 0 1 0\n";
    let (path, graph) = (
        z_in(&deck_with(0.0, port, ".001")),
        z_in(&deck_with(0.0, port, ".00101")),
    );
    assert!(rel(graph, path) < 0.003, "port: {graph:?} vs {path:?}");
    // The control: two segments from the node, the row was always right.
    let path = z_in(&deck(0.0, 0, 8));
    let graph = z_in(&deck_with(0.0, "EX 0 1 8 0 1 0\n", ".00101"));
    assert!(rel(graph, path) < 3e-4, "{graph:?} vs {path:?}");
}

/// A lumped load kinks the current as a gap does: its column is the same
/// `sin(k|s − s′|)`. A j1000 Ω load on the segment beyond the node, fed far from
/// it, stepped |Z| 10.8 % at a 2e-4 rad bend and sat 14.8 % from nec2c at 1 rad;
/// with the load marked it steps 0.72 %. On the graph (the radius step) the
/// straight node agrees with the bent path to 0.002 %. Two segments from the
/// node the load stepped 0.02 % before and after (the control).
#[test]
fn a_load_beside_a_vanishing_bend_is_the_straight_join() {
    let load = |at: u32| format!("LD 4 2 {at} {at} 0 1000\nEX 0 1 5 0 1 0\n");
    let (straight, bent) = (
        z_in(&deck_with(0.0, &load(1), ".001")),
        z_in(&deck_with(2e-4, &load(1), ".001")),
    );
    assert!(rel(bent, straight) < 0.01, "{bent:?} vs {straight:?}");
    let graph = z_in(&deck_with(0.0, &load(1), ".00101"));
    assert!(
        rel(graph, bent) < 1e-4,
        "graph {graph:?} vs bent path {bent:?}"
    );
    let (s3, b3) = (
        z_in(&deck_with(0.0, &load(3), ".001")),
        z_in(&deck_with(2e-4, &load(3), ".001")),
    );
    assert!(rel(b3, s3) < 5e-4, "{b3:?} vs {s3:?}");
}

/// At a real bend the loaded deck keeps the straight deck's offset from nec2c.
/// nec2c, thin kernel, 1 / 3 / 9 / 27 × the mesh: straight −1129.5, −1043.7,
/// −1009.6, −992.84 Ω; 1 rad −1136.8, −1045.9, −1011.2, −994.43 — not settled, and
/// the straight deck's own offset (Hallén −1029.2, 3.7 % past 27 ×) is the lumped
/// load's mesh error on a deck with no node, so the 1 rad offset is gated against
/// it, not against zero. Before the fix the 1 rad deck read −1141.2: 14.8 %.
#[test]
fn a_load_beside_a_real_bend_keeps_the_straight_offset() {
    let load = "LD 4 2 1 1 0 1000\nEX 0 1 5 0 1 0\n";
    let straight = z_in(&deck_with(0.0, load, ".001"));
    let bent = z_in(&deck_with(1.0, load, ".001"));
    let (off_s, off_b) = (
        rel(straight, (3.1132, -992.84)),
        rel(bent, (7.0342, -994.43)),
    );
    assert!(
        (off_b - off_s).abs() < 0.01,
        "straight {off_s:.4}, 1 rad {off_b:.4}"
    );
}

/// A uniform load is no kink: copper loss on every segment of both halves leaves
/// the feed's rule in force. Counting every loaded segment as a gap would put one
/// on both sides of every node, and the 1 % step came back.
#[test]
fn wire_loss_does_not_hide_the_feed_beside_a_bend() {
    let lossy = "LD 5 1 0 0 5.8e7\nLD 5 2 0 0 5.8e7\nEX 0 1 10 0 1 0\n";
    let (straight, bent) = (
        z_in(&deck_with(0.0, lossy, ".001")),
        z_in(&deck_with(2e-4, lossy, ".001")),
    );
    assert!(rel(bent, straight) < 0.002, "{bent:?} vs {straight:?}");
}

/// A plane wave has no feed, but a load step still kinks the receive current
/// (FND-270): the receive plans of both bases mark it. The node solves agree —
/// the bent conductor path and the straight graph (radius step) — at |I| =
/// 2.6206e-3 A beside the node.
#[test]
fn a_receive_load_beside_a_node_is_marked_on_both_bases() {
    let wave = "LD 4 2 1 1 0 1000\nEX 1 1 1 0 90 0 0 0 0\n";
    let path = current_beside_node(&deck_with(2e-4, wave, ".001"));
    let graph = current_beside_node(&deck_with(0.0, wave, ".00101"));
    assert!(
        (path - graph).abs() / graph < 1e-3,
        "path {path:e} vs graph {graph:e}"
    );
}
