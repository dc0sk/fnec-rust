// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-227 — a source on the segment at a free wire end is refused, on every
//! Hallén route, rather than answered with regularisation noise.
//!
//! Hallén's delta gap enters as `sin(k·|s − s_f|)`, sampled at the segment
//! midpoints. On a free-end segment every other midpoint lies on one side of the
//! gap, so that vector IS a combination of the homogeneous `cos(k·s)` and
//! `sin(k·s)` columns: the constants absorb the whole source and the currents
//! solve to zero. Measured at 302c52a, a λ/2 wire fed on segment 1 read
//! 8.17×10¹⁰ + j2.20×10¹⁰ Ω at 21 segments, exit 0, no warning; nec2c reads
//! 4278 − j6312. The current source (`EX 4`) and the sinusoidal basis read the
//! same null solution, and a TL port there was an open circuit.
//!
//! The negative controls are the next segment in and the centre: those solve.

use std::process::Command;

fn run(args: &[&str], deck_text: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!(
        "fnec-end-segment-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck_text).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
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

/// A straight λ/2 wire at 29.98 MHz with `drive` on it.
fn wire(segs: usize, drive: &str) -> String {
    format!("CE\nGW 1 {segs} 0 0 -2.5 0 0 2.5 0.001\nGE\n{drive}\nFR 0 1 0 0 29.98 0\nEN\n")
}

/// A stem-fed Y: stem from z = 0 to the junction at z = 2, two arms to the tips.
/// It solves on the section graph (FND-162).
fn y_junction(tag: u32, seg: u32) -> String {
    format!(
        "CE\nGW 1 11 0 0 0 0 0 2 0.001\nGW 2 11 0 0 2 1.5 0 3.5 0.001\n\
         GW 3 11 0 0 2 -1.5 0 3.5 0.001\nGE\nEX 0 {tag} {seg} 0 1 0\nFR 0 1 0 0 29.98 0\nEN\n"
    )
}

const REFUSAL: &str = "lies wholly in the homogeneous solution";

#[test]
fn a_source_at_a_free_wire_end_is_refused_on_every_hallen_route() {
    let bent = "CE\nGW 1 11 -2 0 0 0 0 1.5 0.001\nGW 2 11 0 0 1.5 2 0 0 0.001\nGE\n\
                EX 0 1 1 0 1 0\nFR 0 1 0 0 29.98 0\nEN\n";
    let tl_port = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 1 0 -2.5 1 0 2.5 0.001\nGE\n\
                   EX 0 1 11 0 1 0\nTL 1 1 2 21 50 0 0 0 0 0\nFR 0 1 0 0 29.98 0\nEN\n";
    let cases: [(&str, &[&str], String); 9] = [
        ("first segment", &[], wire(21, "EX 0 1 1 0 1 0")),
        ("last segment", &[], wire(21, "EX 0 1 21 0 1 0")),
        ("finer mesh", &[], wire(81, "EX 0 1 1 0 1 0")),
        ("current source", &[], wire(21, "EX 4 1 1 0 1 0")),
        (
            "sinusoidal basis",
            &["--solver", "sinusoidal"],
            wire(21, "EX 0 1 1 0 1 0"),
        ),
        ("bent path end", &[], bent.to_string()),
        ("TL port at an end", &[], tl_port.to_string()),
        ("Y arm tip (section graph)", &[], y_junction(2, 11)),
        ("Y stem foot (section graph)", &[], y_junction(1, 1)),
    ];
    for (what, args, deck) in cases {
        let (ok, stdout, stderr) = run(args, &deck);
        assert!(!ok, "{what}: must not exit 0:\n{stdout}");
        assert!(stderr.contains(REFUSAL), "{what}: wrong refusal:\n{stderr}");
        assert!(
            stderr.contains("one segment in"),
            "{what}: the refusal must name the remedy:\n{stderr}"
        );
    }
}

/// The device solves the same system; it must decline it so the CPU refusal is
/// what the user reads, not a device answer of the null solution.
#[test]
fn a_source_at_a_free_wire_end_is_refused_under_exec_gpu() {
    for exec in ["gpu", "hybrid"] {
        let (ok, stdout, stderr) = run(&["--exec", exec], &wire(201, "EX 0 1 1 0 1 0"));
        assert!(!ok, "--exec {exec}: must not exit 0:\n{stdout}");
        assert!(
            stderr.contains(REFUSAL),
            "--exec {exec}: wrong refusal:\n{stderr}"
        );
    }
}

/// The negative control: one segment in, and the centre, have a solution. The
/// bound is loose on purpose — it separates an answer from the 10¹⁰ Ω null.
#[test]
fn a_source_one_segment_in_still_solves() {
    for (seg, args) in [
        (2, &[][..]),
        (11, &[][..]),
        (2, &["--solver", "sinusoidal"][..]),
    ] {
        let (ok, stdout, stderr) = run(args, &wire(21, &format!("EX 0 1 {seg} 0 1 0")));
        assert!(ok, "segment {seg} {args:?}: must solve:\n{stderr}");
        let row = stdout
            .lines()
            .find(|l| l.starts_with(&format!("1 {seg} ")))
            .unwrap_or_else(|| panic!("segment {seg}: no feedpoint row:\n{stdout}"));
        let f: Vec<f64> = row
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        let (r, x) = (f[f.len() - 2], f[f.len() - 1]);
        assert!(
            r.hypot(x) < 1e4,
            "segment {seg}: |Z| = {} is the null solution",
            r.hypot(x)
        );
    }
}

/// The worker (remote hosts, the GUI, fnec_py) refuses it too, on both lanes.
#[test]
fn the_worker_refuses_a_source_at_a_free_wire_end_on_both_lanes() {
    for (segs, exec) in [(21, "cpu"), (201, "gpu")] {
        let deck = wire(segs, "EX 0 1 1 0 1 0");
        match nec_worker::solve::solve_deck_at_frequency_with_exec(&deck, 29.98e6, "hallen", exec) {
            Err(e) => assert!(
                e.to_string().contains(REFUSAL),
                "exec {exec}: wrong error {e}"
            ),
            Ok(r) => panic!(
                "exec {exec}: answered {}+j{} instead of refusing",
                r.impedance_re, r.impedance_im
            ),
        }
    }
}

/// A gap beside a bend or a junction lies in its section's homogeneous span on
/// the segment rows too — but the rows that join the sections are not satisfied
/// by it, so it has a solution. Refusing it would be a false refusal: the first
/// version of this check asked the segment rows alone and refused the apex-fed
/// inverted-V, which solved (497.38 − j1377.66) before the check existed.
#[test]
fn a_source_beside_a_bend_or_junction_still_solves() {
    let apex_fed = "CE\nGW 1 21 -4 0 -2 0 0 2 0.001\nGW 2 21 0 0 2 4 0 -2 0.001\nGE\n\
                    EX 0 1 21 0 1 0\nFR 0 1 0 0 29.98 0\nEN\n";
    let cases = [
        ("apex-fed inverted-V", apex_fed.to_string(), "1 21 "),
        ("Y stem beside the junction", y_junction(1, 11), "1 11 "),
        ("Y arm beside the junction", y_junction(2, 1), "2 1 "),
    ];
    for (what, deck, row) in cases {
        let (ok, stdout, stderr) = run(&[], &deck);
        assert!(ok, "{what}: must solve:\n{stderr}");
        assert!(
            stdout.lines().any(|l| l.starts_with(row)),
            "{what}: no feedpoint row:\n{stdout}"
        );
    }
}
