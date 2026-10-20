// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-274: parallel wires whose surfaces touch solve without a caveat though
// every code is wrong there. The CLI prints the solver-neutral caveat on every
// solver; the dipole with a wire 2 mm along its upper half (1 mm radii) read
// Hallén 32.1 − j93.6 Ω, the MPIE 159.0 + j258.6 and nec2c 1271 + j2069.

use std::process::Command;

mod common;

const TOUCHING: &str = "GW 1 11 0 0 -2.64 0 0 2.64 .001\nGW 2 6 .002 0 0 .002 0 3.5 .001\nGE 0\n\
    EX 0 1 6 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n";

fn stderr(solver: &str, deck: &str, name: &str) -> String {
    let deck = common::TempDeck::new(name, deck);
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", solver])
        .arg(&deck)
        .output()
        .expect("run fnec");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn touching_parallel_wires_carry_the_caveat_on_every_solver() {
    for solver in ["hallen", "mpie"] {
        let err = stderr(solver, TOUCHING, &format!("fnec-274-{solver}.nec"));
        assert_eq!(err.matches("(FND-274)").count(), 1, "{solver}: {err}");
        assert!(err.contains("thin-wire kernel"), "{solver}: {err}");
    }
    // The control: the same wire 2 m away, segments shorter than the spacing.
    let far = TOUCHING.replace(".002 0 0 .002", "2 0 0 2");
    assert!(!stderr("hallen", &far, "fnec-274-far.nec").contains("FND-274"));
}
