// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `TL` end to end through the CLI: the NEC-2 card layout (FND-111) and the line
//! solved as a network across the port gaps (FND-123).

use std::process::{Command, Output};

const PAIR: &str =
    "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1 0 -5.282 1 0 5.282 0.001\nGE\n";
const TAIL: &str = "EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

fn run(cards: &str, args: &[&str], tag: &str) -> Output {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join(format!("tl-cards-{tag}-{}.nec", std::process::id()));
    std::fs::write(&path, format!("{PAIR}{cards}{TAIL}")).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    out
}

fn z(cards: &str, tag: &str) -> (f64, f64) {
    let out = run(cards, &[], tag);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
        .lines()
        .find_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() == 8 && c[0] == "1" && c[1] == "26")
                .then(|| (c[6].parse().unwrap(), c[7].parse().unwrap()))
        })
        .unwrap_or_else(|| panic!("no feedpoint row:\n{stdout}"))
}

/// The card nec2c reads, answered as nec2c answers it (84.826 + j31.131; the
/// residual is the unloaded pair's own offset, FND-156). The old fnec reading of
/// this card was a parse error, and its old model moved the feed by 0.37 Ω.
#[test]
fn a_standard_nec2_tl_card_moves_the_feed_as_nec2c_does() {
    let (r, x) = z("TL 1 26 2 26 50.0 0.1\n", "std");
    assert!(
        (r - 84.826).abs() < 2.0 && (x - 31.131).abs() < 5.0,
        "pair + TL: {r:.3} + j{x:.3}, nec2c 84.826 + j31.131"
    );
}

/// fnec's retired layout is refused, and the message carries the rewrite —
/// reading it as NEC-2 would have been a 1 Ω line with 50 S across one end.
#[test]
fn the_retired_fnec_layout_is_refused_with_its_nec2_rewrite() {
    for (card, rewrite) in [
        ("TL 1 26 2 26 1 0 50.0 0.1 1.0\n", "TL 1 26 2 26 50.0 0.1"),
        (
            "TL 1 26 2 26 1 0 50.0 0.1 0.66\n",
            "TL 1 26 2 26 50.0 0.1 0 0 0 0 0.66",
        ),
        (
            "TL 1 26 2 26 1 1 50.0 3.0 6.0\n",
            "TL 1 26 2 26 50.0 3.0 0 0 0 0 1 6.0",
        ),
    ] {
        let out = run(card, &[], "legacy");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{card} must be refused");
        assert!(
            stderr.contains(rewrite),
            "{card}: no rewrite '{rewrite}' in:\n{stderr}"
        );
    }
}

/// A NEC-2 card that writes Z0 as an integer is not mistaken for the old layout.
#[test]
fn an_integer_valued_nec2_card_is_not_mistaken_for_the_old_layout() {
    assert_eq!(
        z("TL 1 26 2 26 50 0.1\n", "int"),
        z("TL 1 26 2 26 50.0 0.1\n", "flt")
    );
}

/// NEC-2: a length of zero is the distance between the segment centres, 1 m here.
#[test]
fn a_zero_length_line_spans_the_centre_distance() {
    assert_eq!(
        z("TL 1 26 2 26 50.0 0\n", "zero"),
        z("TL 1 26 2 26 50.0 1.0\n", "one")
    );
}

#[test]
fn a_line_on_a_solver_without_a_network_solve_is_refused() {
    let out = run(
        "TL 1 26 2 26 50.0 0.1\n",
        &["--solver", "sinusoidal"],
        "sin",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "must refuse:\n{stderr}");
    assert!(stderr.contains("--solver hallen only"), "{stderr}");
}
