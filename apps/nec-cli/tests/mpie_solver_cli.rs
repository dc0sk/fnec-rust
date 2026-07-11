// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-007 MPIE Phase E — `--solver mpie` CLI wiring.
//
// The opt-in mixed-potential EFIE is now reachable from the CLI. It reuses the
// whole feedpoint / far-field / report path (its per-segment currents are aligned
// to the deck segments), so these contract tests drive `fnec --solver mpie`
// end-to-end and check the reported impedance is physical — including the
// degree-3 Y-junction that the Hallén solver returns garbage for.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_fnec(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"))
}

/// Write a deck to a unique temp path and return it.
fn write_deck(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("fnec_mpie_{name}.nec"));
    std::fs::write(&path, body).unwrap();
    path
}

/// Parse `Z_RE Z_IM` from the FEEDPOINTS data line of a report.
fn feedpoint_z(stdout: &str) -> (f64, f64) {
    let mut lines = stdout.lines();
    while let Some(l) = lines.next() {
        if l.trim_start().starts_with("FEEDPOINTS") {
            let _header = lines.next();
            let data = lines.next().expect("feedpoint data line");
            let f: Vec<f64> = data
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            assert!(f.len() >= 8, "unexpected feedpoint line: {data}");
            return (f[6], f[7]);
        }
    }
    panic!("no FEEDPOINTS section in:\n{stdout}");
}

const DIPOLE: &str = "\
CM half-wave dipole 14.2 MHz
CE
GW 1 41 0.0 0.0 -5.2782 0.0 0.0 5.2782 0.001
GE 0
FR 0 1 0 0 14.2 0
EX 0 1 21 0 1.0 0.0
XQ
EN
";

/// A straight λ/2 dipole solves to a physical impedance whose reactance tracks
/// nec2c (~+42 Ω), not the Hallén ~32 Ω low offset — MPIE keeps the scalar
/// potential, so its absolute reactance is right.
#[test]
fn dipole_mpie_reports_physical_impedance() {
    let deck = write_deck("dipole", DIPOLE);
    let out = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    assert!(out.status.success(), "mpie dipole failed: {:?}", out);
    let (r, x) = feedpoint_z(&String::from_utf8_lossy(&out.stdout));
    assert!((70.0..80.0).contains(&r), "R={r} out of dipole range");
    assert!((35.0..50.0).contains(&x), "X={x} not near nec2c +44.7");
}

/// The degree-3 Y-junction — which the Hallén solver cannot feed at the junction
/// (it returns unphysical R≈8, X≈−960) — solves to a physical impedance on the
/// MPIE path.
#[test]
fn y_junction_mpie_solves() {
    let deck = write_deck(
        "yjunction",
        "\
CM Y-junction, feed mid arm 1
CE
GW 1 20 0.0 0.0 0.0 5.0 0.0 0.0 0.001
GW 2 20 0.0 0.0 0.0 -2.5 4.330127 0.0 0.001
GW 3 20 0.0 0.0 0.0 -2.5 -4.330127 0.0 0.001
GE 0
FR 0 1 0 0 14.2 0
EX 0 1 10 0 1.0 0.0
XQ
EN
",
    );
    let out = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    assert!(out.status.success(), "mpie Y-junction failed: {:?}", out);
    let (r, _x) = feedpoint_z(&String::from_utf8_lossy(&out.stdout));
    // Physical radiation resistance (nec2c ~71.5 Ω; feed slightly off-centre here),
    // decisively unlike the Hallén junction-fed garbage (R≈8).
    assert!((40.0..90.0).contains(&r), "Y-junction R={r} not physical");
}

/// The reported feedpoint impedance must be independent of the `EX` source
/// voltage (MoM is linear: Z = V/I with I ∝ V). Regression for the bug where the
/// MPIE always solved at 1 V, so a deck with EX voltage ≠ 1 V had its impedance
/// scaled by that voltage.
#[test]
fn mpie_impedance_is_independent_of_source_voltage() {
    let deck_1v = write_deck("dipole_1v", DIPOLE);
    let deck_2v = write_deck(
        "dipole_2v",
        &DIPOLE.replace("EX 0 1 21 0 1.0 0.0", "EX 0 1 21 0 2.0 0.0"),
    );
    let z1 = feedpoint_z(&String::from_utf8_lossy(
        &run_fnec(&["--solver", "mpie", deck_1v.to_str().unwrap()]).stdout,
    ));
    let z2 = feedpoint_z(&String::from_utf8_lossy(
        &run_fnec(&["--solver", "mpie", deck_2v.to_str().unwrap()]).stdout,
    ));
    assert!(
        (z1.0 - z2.0).abs() < 1e-3 && (z1.1 - z2.1).abs() < 1e-3,
        "impedance must not depend on source voltage: 1V={z1:?} vs 2V={z2:?}"
    );
}

/// The MPIE reduced kernel uses one wire radius for the whole geometry; a deck
/// mixing radii under `--solver mpie` must warn (rather than silently solve the
/// thin and fat wires with the same radius).
#[test]
fn mpie_warns_on_mixed_radii() {
    let deck = write_deck(
        "mixed_radii",
        "\
CM mixed-radius collinear dipole (fat + thin halves)
CE
GW 1 20 0.0 0.0 -2.5 0.0 0.0 0.0 0.002
GW 2 20 0.0 0.0 0.0 0.0 0.0 2.5 0.0005
GE 0
FR 0 1 0 0 14.2 0
EX 0 1 10 0 1.0 0.0
XQ
EN
",
    );
    let out = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("single wire radius") || stderr.contains("mixes radii"),
        "expected a mixed-radius warning on the MPIE path:\n{stderr}"
    );
}

/// A uniform-radius deck must NOT emit the mixed-radius warning (no false positive).
#[test]
fn mpie_uniform_radius_no_mixed_warning() {
    let deck = write_deck("uniform_radius", DIPOLE);
    let out = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("mixes radii"),
        "uniform-radius deck should not warn about mixed radii:\n{stderr}"
    );
}

/// Loads are not modelled by the MPIE triangle-basis system, so an `LD` deck is
/// rejected rather than silently ignored.
#[test]
fn loads_rejected_with_mpie() {
    let deck = write_deck(
        "loaded",
        "\
CM loaded dipole
CE
GW 1 41 0.0 0.0 -5.2782 0.0 0.0 5.2782 0.001
GE 0
LD 0 1 21 21 0.0 100.0 0.0
FR 0 1 0 0 14.2 0
EX 0 1 21 0 1.0 0.0
XQ
EN
",
    );
    let out = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    assert!(!out.status.success(), "expected LD rejection");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not supported with --solver mpie"),
        "missing LD rejection message:\n{stderr}"
    );
}

/// A degree-3 Y-junction on the DEFAULT (Hallén) solver keeps its guard warning
/// (results are not silently changed), and the warning now points the user to
/// `--solver mpie` as the fix.
#[test]
fn guarded_topology_warning_recommends_mpie() {
    let deck = write_deck(
        "yguard",
        "\
CM Y-junction on the default solver
CE
GW 1 20 0.0 0.0 0.0 5.0 0.0 0.0 0.001
GW 2 20 0.0 0.0 0.0 -2.5 4.330127 0.0 0.001
GW 3 20 0.0 0.0 0.0 -2.5 -4.330127 0.0 0.001
GE 0
FR 0 1 0 0 14.2 0
EX 0 1 10 0 1.0 0.0
EN
",
    );
    let out = run_fnec(&[deck.to_str().unwrap()]); // no --solver flag
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--solver mpie"),
        "guard warning should recommend --solver mpie:\n{stderr}"
    );
    // And using the MPIE explicitly clears the warning and solves it physically.
    let out2 = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    assert!(out2.status.success());
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        !stderr2.contains("unreliable"),
        "MPIE solve should not emit the unreliable-topology warning:\n{stderr2}"
    );
}

/// `--solver mpie` composes with `--output-format json` — the feedpoint impedance
/// is emitted in the machine-readable stream like any other solver.
#[test]
fn mpie_composes_with_json_output() {
    let deck = write_deck("dipole_json", DIPOLE);
    let out = run_fnec(&[
        "--solver",
        "mpie",
        "--output-format",
        "json",
        deck.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "mpie+json failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"z_re\":74.") && stdout.contains("\"z_im\":41."),
        "expected MPIE feedpoint Z in JSON:\n{stdout}"
    );
}

/// `--solver mpie` composes with a multi-point `FR` frequency sweep — one
/// feedpoint section per frequency.
#[test]
fn mpie_composes_with_frequency_sweep() {
    let deck = write_deck(
        "dipole_sweep",
        "\
CM dipole sweep
CE
GW 1 41 0 0 -5.2782 0 0 5.2782 0.001
GE 0
FR 0 3 0 0 14.0 0.2
EX 0 1 21 0 1.0 0.0
EN
",
    );
    let out = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    assert!(out.status.success(), "mpie+sweep failed: {out:?}");
    let n_feedpoints = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.trim_start().starts_with("FEEDPOINTS"))
        .count();
    assert_eq!(n_feedpoints, 3, "expected 3 swept feedpoint sections");
}

/// `--solver mpie` over finite ground produces a radiation pattern (MPIE currents
/// feed the existing far-field sum) and does NOT emit the reflection-coefficient
/// low-ground warning — the MPIE models the Sommerfeld surface wave in its Z-matrix.
#[test]
fn mpie_ground_pattern_and_no_rcm_warning() {
    let deck = write_deck(
        "dipole_ground_rp",
        "\
CM horizontal dipole over GN2 with pattern
CE
GW 1 41 -5.2782 0 2.1 5.2782 0 2.1 0.001
GE 0
GN 2 0 0 0 13.0 0.005
FR 0 1 0 0 14.2 0
EX 0 1 21 0 1.0 0.0
RP 0 19 1 1000 0 0 10 0
EN
",
    );
    let out = run_fnec(&["--solver", "mpie", deck.to_str().unwrap()]);
    assert!(out.status.success(), "mpie ground+RP failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("RADIATION_PATTERN"),
        "expected a radiation pattern:\n{stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("reflection-coefficient approximation"),
        "MPIE models the surface wave; the RCM low-ground warning must not fire:\n{stderr}"
    );
}
