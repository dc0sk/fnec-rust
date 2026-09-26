// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-080 — the pulse and continuity bases run only on explicit opt-in, and
//! their caveat travels in the result itself, not only on stderr.
//!
//! They have never produced a correct dipole impedance (16+j47 Ω with the raw
//! RHS, −346−j988 Ω with the default scaling, against nec2c's 79+j46). The
//! maintainer kept them for experiment behind a hard gate; these tests are that
//! gate.

use std::process::{Command, Output};

const DIPOLE: &str =
    "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 2 0 0 14.0 0.2\nEN\n";

fn run(args: &[&str], tag: &str) -> Output {
    let path = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("exp-gate-{tag}-{}.nec", std::process::id()));
    std::fs::write(&path, DIPOLE).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    out
}

#[test]
fn the_pulse_bases_are_refused_without_the_opt_in() {
    for solver in ["pulse", "continuity"] {
        let out = run(&["--solver", solver], solver);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "{solver} must need the opt-in:\n{stderr}"
        );
        assert!(
            out.stdout.is_empty(),
            "{solver}: a refusal prints no report"
        );
        assert!(
            stderr.contains("--experimental-solver") && stderr.contains("FND-080"),
            "{solver}: the refusal must say how to opt in and why: {stderr}"
        );
    }
}

/// With the opt-in, every result carries the caveat: each text report's header
/// and each JSON record — a consumer that never reads stderr still sees it.
#[test]
fn every_result_of_a_pulse_basis_carries_the_caveat() {
    let text = run(&["--solver", "pulse", "--experimental-solver"], "text");
    assert!(
        text.status.success(),
        "{}",
        String::from_utf8_lossy(&text.stderr)
    );
    let stdout = String::from_utf8_lossy(&text.stdout);
    let reports = stdout.matches("FNEC FEEDPOINT REPORT").count();
    assert_eq!(reports, 2, "two frequency points: {stdout}");
    assert_eq!(
        stdout.matches("CAVEAT UNVALIDATED SOLVER").count(),
        reports,
        "every report must carry the caveat: {stdout}"
    );

    let json = run(
        &[
            "--solver",
            "continuity",
            "--experimental-solver",
            "--output-format",
            "json",
        ],
        "json",
    );
    let stdout = String::from_utf8_lossy(&json.stdout);
    assert_eq!(stdout.matches("\"freq_mhz\"").count(), 2, "{stdout}");
    assert_eq!(
        stdout.matches("\"caveat\":\"UNVALIDATED SOLVER").count(),
        2,
        "{stdout}"
    );
}

/// Negative control: validated solvers' output is unchanged — no header line, no
/// JSON field.
#[test]
fn validated_solvers_carry_no_caveat() {
    for solver in ["hallen", "sinusoidal"] {
        let text = run(&["--solver", solver], solver);
        assert!(
            !String::from_utf8_lossy(&text.stdout).contains("CAVEAT"),
            "{solver}"
        );
        let json = run(&["--solver", solver, "--output-format", "json"], solver);
        assert!(
            !String::from_utf8_lossy(&json.stdout).contains("caveat"),
            "{solver}"
        );
    }
}
