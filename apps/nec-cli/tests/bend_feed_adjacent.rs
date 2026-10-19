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
// current from the other section's quadratic extrapolation.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

/// Two halves of 10 × 0.264 m at 28.4 MHz, the upper tilted by `t` radians,
/// driven by `ex` (0 a voltage gap, 4 a current source) on tag 1 segment `feed`.
fn deck(t: f64, ex: u32, feed: u32) -> String {
    let (x, z) = (2.64 * t.sin(), 2.64 * t.cos());
    format!(
        "CE\nGW 1 10 0 0 -2.64 0 0 0 .001\nGW 2 10 0 0 0 {x:.12} 0 {z:.12} .001\nGE 0\n\
         EX {ex} 1 {feed} 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n"
    )
}

fn z_in(text: &str) -> (f64, f64) {
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
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = stdout.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == "FEEDPOINTS")
        .expect("FEEDPOINTS");
    let c: Vec<f64> = lines[at + 2]
        .split_whitespace()
        .map(|v| v.parse().expect("number"))
        .collect();
    (c[6], c[7])
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

/// A real bend, fed beside it, against nec2c converged at 81 × the mesh
/// (`scripts/nec2c-converged-series.py`): 64.565 + j34.83 Ω at 1 rad. Hallén sits
/// 1.26 % low in R, the same as on the straight dipole (81.079 against 80.057);
/// before the fix it was 1.94 % low (63.316).
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
