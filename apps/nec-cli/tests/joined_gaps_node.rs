// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-278: a feed and a lumped load on the two segments either side of a node.
// The FND-270 dipole (two halves of 10 segments at 28.4 MHz) fed on the segment
// beside the node with `LD 4 2 1 1 0 1000` across it read 175.34 + j2031.3 Ω
// straight — one section, no node row — and 97.65 + j1052.2 at a bend of 2e-4
// rad, where a node row exists: FND-270's row took the feed's side and
// extrapolated across the load's kink, and the linear row before it read 79.22.
// With both node-adjacent segments gaps the current between the two kinks is one
// arc through the node, so no current row carries information. The node row is
// now A-continuity, `C_a cos ks₀ + D_a sin ks₀ = C_b cos ks₀ + D_b sin ks₀` (plus
// each side's corner and particular terms), on the conductor paths and on the
// section graph alike; at a vanishing bend that is the straight solve exactly.
// At a real bend it drops the jump of the tangential vector potential at the
// corner and the result carries a caveat naming the MPIE: no reference converges
// there (nec2c's series for a point load beside a gap drifts logarithmically).

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;

/// Two halves of `n` segments over 2.64 m each at 28.4 MHz, the upper tilted by
/// `t` radians and of radius `r2`, with `cards` after `GE`.
fn deck(t: f64, n: u32, cards: &str, r2: &str) -> String {
    let (x, z) = (2.64 * t.sin(), 2.64 * t.cos());
    format!(
        "CE\nGW 1 {n} 0 0 -2.64 0 0 0 .001\nGW 2 {n} 0 0 0 {x:.12} 0 {z:.12} {r2}\nGE 0\n\
         {cards}FR 0 1 0 0 28.4 0\nEN\n"
    )
}

/// The feed beside the node, the load across it: j1000 Ω (`L1`) or 50 Ω (`R50`).
const L1: &str = "LD 4 2 1 1 0 1000\nEX 0 1 10 0 1 0\n";
const R50: &str = "LD 4 2 1 1 50 0\nEX 0 1 10 0 1 0\n";

/// `(stdout lines, stderr)` of a Hallén run, with `toml` as its `--loads-config`.
fn run_with(text: &str, toml: Option<&str>) -> (Vec<String>, String) {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let stem = format!(
        "fnec-278-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    );
    let deck = common::TempDeck::new(&format!("{stem}.nec"), text);
    let config = toml.map(|t| common::TempDeck::new(&format!("{stem}.toml"), t));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(["--solver", "hallen"]);
    if let Some(config) = &config {
        cmd.arg("--loads-config").arg(config);
    }
    let out = cmd.arg(&deck).output().expect("run fnec");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{stderr}");
    (
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect(),
        stderr,
    )
}

fn z_in(text: &str) -> (f64, f64) {
    z_in_with(text, None)
}

fn z_in_with(text: &str, toml: Option<&str>) -> (f64, f64) {
    let (lines, _) = run_with(text, toml);
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

fn rel((a, b): (f64, f64), (c, d): (f64, f64)) -> f64 {
    (a - c).hypot(b - d) / c.hypot(d)
}

/// The caveat lines of a run.
fn caveats(text: &str) -> Vec<String> {
    caveats_with(text, None)
}

/// The caveat lines of a run with `toml` as its `--loads-config`.
fn caveats_with(text: &str, toml: Option<&str>) -> Vec<String> {
    run_with(text, toml)
        .1
        .lines()
        .filter(|l| l.contains("FND-278"))
        .map(String::from)
        .collect()
}

/// T1: at a vanishing bend the joined row is the straight solve, on the conductor
/// paths (a 2e-4 rad bend) and on the section graph (a radius step at the node,
/// FND-252), for the j1000 Ω load and the 50 Ω one. The reference is the straight
/// deck itself, which has no node row: an identity, not a figure of fnec's.
///
/// The graph's deck carries a radius step, whose own effect on Z is linear in the
/// step and not the row's to remove: measured 0.036 % of Z per 0.1 % of step on
/// the j1000 Ω deck, 0.0134 % on the 50 Ω one. A 0.2 % step keeps the graph within
/// 1e-3 of the straight path; a 1 % step is checked to move Z by five times what
/// the 0.2 % step does, which puts the intercept — the graph row's own difference
/// from the path's — at zero. (nec2c moves 0.145 % and the MPIE 0.05 % for the
/// 1 % step on the j1000 Ω deck: the step's own physics, where the three codes do
/// not agree, is outside this test.)
#[test]
fn gaps_either_side_of_a_vanishing_bend_solve_as_the_straight_join() {
    for (name, cards) in [("L1", L1), ("R50", R50)] {
        let straight = z_in(&deck(0.0, 10, cards, ".001"));
        let bent = z_in(&deck(2e-4, 10, cards, ".001"));
        assert!(
            rel(bent, straight) < 1e-3,
            "{name} path: {bent:?} vs straight {straight:?}"
        );
        let small = z_in(&deck(0.0, 10, cards, ".001002"));
        assert!(
            rel(small, straight) < 1e-3,
            "{name} graph (0.2 % step): {small:?} vs straight {straight:?}"
        );
        let large = z_in(&deck(0.0, 10, cards, ".00101"));
        let (d_small, d_large) = (small.0 - straight.0, large.0 - straight.0);
        assert!(
            (d_large / d_small - 5.0).abs() < 0.5,
            "{name} graph: the step's effect is not linear — 0.2 %: {d_small:.4}, \
             1 %: {d_large:.4} ({:.2}× instead of 5×)",
            d_large / d_small
        );
    }
    // The negative control: FND-270's j1000 Ω load fed far from the node is one
    // gap, and keeps its own rule (its tests in `bend_feed_adjacent.rs`).
    let far = "LD 4 2 1 1 0 1000\nEX 0 1 5 0 1 0\n";
    let (s, b) = (
        z_in(&deck(0.0, 10, far, ".001")),
        z_in(&deck(2e-4, 10, far, ".001")),
    );
    assert!(rel(b, s) < 0.01, "{b:?} vs {s:?}");
}

/// T2: the bend ratio `Z(1 rad)/Z(straight)` is mesh-robust where Z itself is
/// not. nec2c, extended kernel, on the decks `scripts/nec2c-fine-mesh.py` makes
/// at 9× (the feed and load on the centre sub-segments of their deck segments:
/// `EX 0 1 86`, `LD 4 2 5 5`): straight 186.29 + j2115.5, 1 rad 148.12 + j2066.5,
/// a ratio of 0.7951 / 0.9768 (0.808 / 0.982 at 1×, 0.794 / 0.976 at 81×). Hallén
/// at 9× sits within 1 % of it. On this mesh no gap touches the node, so the fix
/// leaves it as FND-270 left it (0.797 / 0.978) and no caveat is raised: the
/// negative control for the row and the caveat alike.
#[test]
fn the_fine_mesh_bend_ratio_stays_on_nec2c() {
    let fine = |t: f64| {
        let text = deck(t, 90, "LD 4 2 5 5 0 1000\nEX 0 1 86 0 1 0\n", ".001");
        assert!(caveats(&text).is_empty(), "no gap touches the node at 9×");
        z_in(&text)
    };
    let (s, b) = (fine(0.0), fine(1.0));
    let (r, x) = (b.0 / s.0, b.1 / s.1);
    assert!(
        (r / 0.7951 - 1.0).abs() < 0.01,
        "R ratio {r:.4} vs nec2c 0.7951"
    );
    assert!(
        (x / 0.9768 - 1.0).abs() < 0.01,
        "X ratio {x:.4} vs nec2c 0.9768"
    );
}

/// The configuration itself refined — the feed and load kept on the segments
/// beside the node as they shrink — has no converged reference, and the joined
/// row's bend-ratio error at a real bend is what the caveat is for. It heals with
/// the mesh: against nec2c (thin kernel, segments 29 radii long at 3×) on the same
/// decks, straight 130.99 + j1620.3 and 1 rad 105.83 + j1591.3 at 1× (ratio
/// 0.8079 / 0.9821), 94.627 + j1226.2 and 76.205 + j1209.4 at 3× (0.8053 /
/// 0.9863), Hallén's ratio is 41 % / 19 % low at 1× and 14.5 % / 6.3 % at 3× (7.1
/// / 2.9 % at 9×, 90 segments a half); the MPIE's within 1.4 % at every mesh.
/// Gated as a trend — the 3× error under half the 1× error, on R and X — not as
/// a figure, which would be fnec's own.
#[test]
fn the_node_adjacent_bend_ratio_error_heals_with_the_mesh() {
    let ratio = |n: u32| {
        let cards = format!("LD 4 2 1 1 0 1000\nEX 0 1 {n} 0 1 0\n");
        let (s, b) = (
            z_in(&deck(0.0, n, &cards, ".001")),
            z_in(&deck(1.0, n, &cards, ".001")),
        );
        (b.0 / s.0, b.1 / s.1)
    };
    let (h1, h3) = (ratio(10), ratio(30));
    let (n1, n3) = (
        (105.83 / 130.99, 1591.3 / 1620.3),
        (76.205 / 94.627, 1209.4 / 1226.2),
    );
    let err = |h: f64, n: f64| (h / n - 1.0).abs();
    let (e1r, e3r) = (err(h1.0, n1.0), err(h3.0, n3.0));
    let (e1x, e3x) = (err(h1.1, n1.1), err(h3.1, n3.1));
    assert!(
        e3r < 0.5 * e1r && e3r < 0.2,
        "R ratio error 1×: {e1r:.3}, 3×: {e3r:.3} (Hallén {h1:?}, {h3:?})"
    );
    assert!(
        e3x < 0.5 * e1x && e3x < 0.1,
        "X ratio error 1×: {e1x:.3}, 3×: {e3x:.3} (Hallén {h1:?}, {h3:?})"
    );
}

/// T3: at a real bend the deck carries the caveat, once, naming the bend and the
/// MPIE; the straight deck, the vanishing bend, the load beside the node fed far
/// (one gap, FND-270's case) and the unloaded dipole fed beside the bend carry
/// none.
#[test]
fn gaps_either_side_of_a_real_bend_carry_the_caveat_naming_the_mpie() {
    let bent = caveats(&deck(1.0, 10, L1, ".001"));
    assert_eq!(bent.len(), 1, "{bent:?}");
    assert!(bent[0].contains("57.3° bend"), "{}", bent[0]);
    assert!(bent[0].contains("unverified"), "{}", bent[0]);
    assert!(
        bent[0].ends_with("re-run with `--solver mpie`, which models the corner"),
        "{}",
        bent[0]
    );
    for (why, text) in [
        ("straight", deck(0.0, 10, L1, ".001")),
        ("vanishing bend", deck(2e-4, 10, L1, ".001")),
        ("graph, straight", deck(0.0, 10, L1, ".00101")),
        (
            "load beside the node, fed far",
            deck(1.0, 10, "LD 4 2 1 1 0 1000\nEX 0 1 5 0 1 0\n", ".001"),
        ),
        (
            "unloaded, fed beside the bend",
            deck(1.0, 10, "EX 0 1 10 0 1 0\n", ".001"),
        ),
    ] {
        let none = caveats(&text);
        assert!(none.is_empty(), "{why}: {none:?}");
    }
}

/// A `--loads-config` series inductor on tag `tag` segment `seg` whose reactance
/// at 28.4 MHz is `x` Ω exactly: `Z(s) = L·s` with `L = x/ω`.
fn laplace_l(tag: u32, seg: u32, x: f64) -> String {
    let l = x / (2.0 * std::f64::consts::PI * 28.4e6);
    format!(
        "[[laplace_load]]\ntag = {tag}\nseg_first = {seg}\nnumerator = [0.0, {l:e}]\n\
         denominator = [1.0]\n"
    )
}

/// FND-288: the j1000 Ω load beside the bend given as a `--loads-config` Laplace
/// load instead of `LD 4` carries the caveat, as the `LD` card does: the caveat
/// reads the loads the run solves with, not the deck's cards. The oracle that the
/// two runs ARE the same problem is the identity of their solves — the Laplace L
/// is 1000/ω exactly, so the two decks stamp the same diagonal and Z agrees to
/// rounding. Negative controls: the same load moved to tag 2 seg 5, away from the
/// node (one gap); a zero-valued Laplace load beside the node (the gap rule is by
/// value, `NodeGaps::load_step`); and the deck with no `--loads-config` at all.
#[test]
fn a_loads_config_load_beside_a_bend_carries_the_caveat() {
    let feed = "EX 0 1 10 0 1 0\n";
    let bent = deck(1.0, 10, feed, ".001");
    let config = laplace_l(2, 1, 1000.0);

    let shown = caveats_with(&bent, Some(&config));
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].contains("57.3° bend"), "{}", shown[0]);
    let by_card = caveats(&deck(1.0, 10, L1, ".001"));
    assert_eq!(shown, by_card, "the same caveat as the LD card's");

    let (laplace, card) = (
        z_in_with(&bent, Some(&config)),
        z_in(&deck(1.0, 10, L1, ".001")),
    );
    assert!(
        rel(laplace, card) < 1e-4,
        "Laplace {laplace:?} vs LD {card:?}: not the same problem"
    );

    for (why, toml) in [
        ("moved away from the node", Some(laplace_l(2, 5, 1000.0))),
        ("zero-valued beside the node", Some(laplace_l(2, 1, 0.0))),
        ("no --loads-config", None),
    ] {
        let none = caveats_with(&bent, toml.as_deref());
        assert!(none.is_empty(), "{why}: {none:?}");
    }
}

/// FND-288 on the image route: a wire rising from PEC ground (`GN 1`), bent by
/// 1 rad at the top, fed below the bend with a `--loads-config` j1000 Ω load above
/// it. The solve and the caveat judge the image problem, whose loads are the
/// deck's doubled (`image_loads`); the image deck's cards cannot carry a Laplace
/// load, so a caveat that read them would see one gap. The `LD 4` deck is the
/// identity oracle for the solve, as above; the unloaded deck is the control.
#[test]
fn a_loads_config_load_beside_a_bend_over_pec_ground_carries_the_caveat() {
    let (x, z) = (2.64 * 1.0_f64.sin(), 2.64 + 2.64 * 1.0_f64.cos());
    let grounded = |cards: &str| {
        format!(
            "CE\nGW 1 10 0 0 0 0 0 2.64 .001\nGW 2 10 0 0 2.64 {x:.12} 0 {z:.12} .001\nGE 1\n\
             GN 1\n{cards}FR 0 1 0 0 28.4 0\nEN\n"
        )
    };
    let feed = "EX 0 1 10 0 1 0\n";
    let config = laplace_l(2, 1, 1000.0);

    let shown = caveats_with(&grounded(feed), Some(&config));
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].contains("57.3° bend"), "{}", shown[0]);
    assert_eq!(
        shown,
        caveats(&grounded(L1)),
        "the same caveat as the LD card's"
    );

    let (laplace, card) = (
        z_in_with(&grounded(feed), Some(&config)),
        z_in(&grounded(L1)),
    );
    assert!(
        rel(laplace, card) < 1e-4,
        "Laplace {laplace:?} vs LD {card:?}: not the same problem"
    );
    let none = caveats(&grounded(feed));
    assert!(none.is_empty(), "unloaded: {none:?}");
}
