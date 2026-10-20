// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-272: after FND-251 refused the loops and junctions the section graph cannot
// take, the Hallén topology caveat and the junction-feed warning looked dormant.
// They are not: the experimental bases build the deck geometry, which FND-251's
// refusal never sees, so a refused loop reaches both caveats there, and
// rightly. And they were reached wrongly by a junction standing on PEC ground
// under a plane wave, whose solve is the image problem's section graph.

use std::process::Command;

mod common;

fn run(args: &[&str], deck: &str, name: &str) -> (Option<i32>, String, String) {
    let deck = common::TempDeck::new(name, deck);
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&deck)
        .output()
        .expect("run fnec");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `|I|` of `tag`/`seg` from the CURRENTS table.
fn current_mag(stdout: &str, tag: u32, seg: u32) -> f64 {
    let mut in_currents = false;
    for line in stdout.lines() {
        if line.trim() == "CURRENTS" {
            in_currents = true;
            continue;
        }
        let c: Vec<&str> = line.split_whitespace().collect();
        if in_currents && c.len() == 6 && c[0] == tag.to_string() && c[1] == seg.to_string() {
            return c[4].parse().expect("I_MAG");
        }
    }
    panic!("no current for tag {tag} seg {seg}:\n{stdout}");
}

/// A vertical stem with a two-arm top hat, standing on PEC ground, receiving a
/// plane wave at 45°. nec2c 1.3.1 on this deck (`XQ` added) reads |I| at the
/// first segment of tags 1, 2 and 3 as 1.4913e-3, 2.4421e-3 and 2.0674e-3 A.
const RECEIVING_TOP_HAT: &str = "CE\nGW 1 5 0 0 0 0 0 1 0.001\nGW 2 5 0 0 1 0.5 0 1 0.001\n\
    GW 3 5 0 0 1 -0.5 0 1 0.001\nGE 1\nGN 1\nEX 1 1 1 0 45 0 0 0 0\nFR 0 1 0 0 100 0\n\
    RP 0 1 1 1000 45 0 0 0\nEN\n";

#[test]
fn a_receiving_junction_on_pec_ground_is_not_captioned_unsupported() {
    let (code, out, err) = run(
        &["--solver", "hallen"],
        RECEIVING_TOP_HAT,
        "fnec-272-tophat.nec",
    );
    assert_eq!(code, Some(0), "{err}");
    assert!(!err.contains("geometry contains"), "{err}");
    assert!(!err.contains("route=unsupported"), "{err}");
    for (tag, nec2c) in [(1, 1.4913e-3), (2, 2.4421e-3), (3, 2.0674e-3)] {
        let got = current_mag(&out, tag, 1);
        let off = (got - nec2c).abs() / nec2c;
        assert!(
            off < 0.025,
            "tag {tag}: |I| {got:e} is {off:.4} from nec2c {nec2c:e}"
        );
    }
}

/// A folded dipole with one-segment end jumpers, fed on a jumper: a closed loop
/// FND-251 refuses on Hallén. The experimental pulse basis takes it, on the
/// deck's own segments, and must carry both caveats, which say what it lacks.
const FOLDED_ONE_SEGMENT_JUMPERS: &str = "GW 1 21 -5.25 0 0 5.25 0 0 .001\n\
    GW 2 1 5.25 0 0 5.25 0 .1 .001\nGW 3 21 5.25 0 .1 -5.25 0 .1 .001\n\
    GW 4 1 -5.25 0 .1 -5.25 0 0 .001\nGE\nEX 0 2 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn a_loop_hallen_refuses_carries_its_caveats_on_the_pulse_basis() {
    let (code, _, err) = run(
        &["--solver", "pulse", "--experimental-solver"],
        FOLDED_ONE_SEGMENT_JUMPERS,
        "fnec-272-fold-pulse.nec",
    );
    assert_eq!(code, Some(0), "{err}");
    assert!(err.contains("geometry contains a closed loop"), "{err}");
    assert!(
        err.contains("feedpoint at tag 2 segment 1 is on a wire junction"),
        "{err}"
    );
    // The control: the Hallén geometry refuses the same deck (FND-251).
    let (code, _, err) = run(
        &["--solver", "hallen"],
        FOLDED_ONE_SEGMENT_JUMPERS,
        "fnec-272-fold-hallen.nec",
    );
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("straight run one segment long"), "{err}");
}
