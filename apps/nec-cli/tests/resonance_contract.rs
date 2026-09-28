// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Contract tests for the `fnec sweep --resonance` subcommand (PH3-CHK-008).
//
// Test 1 — worked example converges:
//   Uses examples/resonance-search.nec.toml to search for the resonant
//   half-length of a 14.2 MHz dipole.  Accepts CONVERGED true and a final
//   Z_IM value within 0.5 Ω of zero.
//
// Test 2 — root-not-bracketed error path:
//   Provides a search range where z_im does not change sign (lo and hi both
//   produce positive reactance for a short dipole driven well below resonance).
//   Expects a non-zero exit code and a diagnostic on stderr.
//
// Test 3 — missing --resonance flag:
//   Runs `fnec sweep` with no flags; expects exit code 2 and usage hint.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn temp_nec_toml(prefix: &str, body: &str) -> common::TempDeck {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    common::TempDeck::new(&format!("fnec-res-{prefix}-{ts}.nec.toml"), body)
}

/// Extract the value of a named field from the structured output block.
///
/// Requires the field name to appear as a complete word (followed by a space).
/// For example, given stdout containing "CONVERGED true\n",
/// `field_value("CONVERGED", stdout)` returns `Some("true")` and does NOT
/// match the `"CONVERGED_VALUE ..."` line.
fn field_value<'a>(field: &str, stdout: &'a str) -> Option<&'a str> {
    let prefix = format!("{field} ");
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix.as_str()) {
            return Some(rest.trim());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Test 1: worked example converges within tolerance
// ---------------------------------------------------------------------------

#[test]
fn resonance_search_worked_example_converges() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let example_path = workspace_root.join("examples/resonance-search.nec.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("sweep")
        .arg("--resonance")
        .arg(&example_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run fnec sweep --resonance: {e}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "fnec sweep --resonance exited non-zero\nstderr: {stderr}\nstdout: {stdout}"
    );

    // Output must contain the structured result header.
    assert!(
        stdout.contains("RESONANCE_SEARCH_RESULT\n"),
        "missing RESONANCE_SEARCH_RESULT header\nstdout: {stdout}"
    );

    // Must report the searched variable.
    let var_field = field_value("VAR", &stdout);
    assert_eq!(
        var_field,
        Some("HALF_LEN"),
        "VAR field mismatch\nstdout: {stdout}"
    );

    // Must report convergence.
    let converged = field_value("CONVERGED", &stdout);
    assert_eq!(
        converged,
        Some("true"),
        "CONVERGED field should be 'true'\nstdout: {stdout}"
    );

    // Final reactance must be within 0.5 Ω of zero.
    let z_im_str = field_value("Z_IM", &stdout)
        .unwrap_or_else(|| panic!("Z_IM field missing\nstdout: {stdout}"));
    let z_im: f64 = z_im_str
        .parse()
        .unwrap_or_else(|_| panic!("Z_IM not a float: '{z_im_str}'\nstdout: {stdout}"));
    assert!(
        z_im.abs() <= 0.5,
        "Z_IM = {z_im:.4} Ω exceeds 0.5 Ω tolerance\nstdout: {stdout}"
    );

    // Converged value should be a plausible dipole half-length near 5.19 m.
    let val_str = field_value("CONVERGED_VALUE", &stdout)
        .unwrap_or_else(|| panic!("CONVERGED_VALUE field missing\nstdout: {stdout}"));
    let val: f64 = val_str
        .parse()
        .unwrap_or_else(|_| panic!("CONVERGED_VALUE not a float: '{val_str}'\nstdout: {stdout}"));
    assert!(
        (4.5..=6.0).contains(&val),
        "CONVERGED_VALUE {val:.4} m outside plausible [4.5, 6.0] range\nstdout: {stdout}"
    );

    // Iteration count should be reported and reasonable.
    let iters_str = field_value("ITERATIONS", &stdout)
        .unwrap_or_else(|| panic!("ITERATIONS field missing\nstdout: {stdout}"));
    let iters: usize = iters_str
        .parse()
        .unwrap_or_else(|_| panic!("ITERATIONS not an integer: '{iters_str}'\nstdout: {stdout}"));
    assert!(
        (1..=52).contains(&iters),
        "ITERATIONS {iters} outside plausible [1, 52] range\nstdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: root not bracketed → non-zero exit with diagnostic on stderr
// ---------------------------------------------------------------------------

#[test]
fn resonance_search_unbounded_range_fails_gracefully() {
    // Use a very short range where both endpoints produce the same sign of z_im.
    // A dipole driven at 100 MHz with half-lengths well below resonance will
    // both have strongly negative z_im — no sign change, so bisection fails.
    const TOML: &str = r#"
[search]
var                   = "HALF_LEN"
lo                    = 0.1
hi                    = 0.4
target_reactance_ohm  = 0.0
tolerance_ohm         = 0.5
max_iter              = 20

[deck]
template = """
GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 100.0 0.0
EN
"""
"#;

    let path = temp_nec_toml("unbounded", TOML);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("sweep")
        .arg("--resonance")
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run fnec sweep --resonance: {e}"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
        "expected non-zero exit for unbounded range\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Stderr must mention that z_im does not bracket or similar diagnostic.
    assert!(
        stderr.contains("error:"),
        "expected 'error:' in stderr\nstderr: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("z_im")
            || stderr.to_lowercase().contains("bracket")
            || stderr.to_lowercase().contains("resonance"),
        "expected bracket or z_im diagnostic in stderr\nstderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Test 3: missing --resonance flag → usage error (exit 2)
// ---------------------------------------------------------------------------

#[test]
fn resonance_search_missing_resonance_flag_exits_with_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("sweep")
        .output()
        .unwrap_or_else(|e| panic!("failed to run fnec sweep: {e}"));

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 for missing --resonance\nstderr: {stderr}"
    );

    assert!(
        stderr.contains("--resonance") || stderr.contains("Usage"),
        "expected usage hint in stderr\nstderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// FND-173: the probe refuses what every other frontend refuses
// ---------------------------------------------------------------------------

/// Run a resonance search over a dipole template with `extra` cards spliced in
/// before `EX`, and `fr` as its frequency card.
fn search(prefix: &str, extra: &str, fr: &str) -> std::process::Output {
    let body = format!(
        "[search]\nvar = \"HALF_LEN\"\nlo = 4.5\nhi = 6.0\ntarget_reactance_ohm = 0.0\n\
         tolerance_ohm = 0.5\nmax_iter = 50\n\n[deck]\ntemplate = \"\"\"\n\
         GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN 0.001\n{extra}GE\nEX 0 1 26 0 1.0 0.0\n{fr}\nEN\n\"\"\"\n"
    );
    let file = temp_nec_toml(prefix, &body);
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["sweep", "--resonance"])
        .arg(file.path())
        .output()
        .expect("runs fnec")
}

/// The route called none of `pre_solve_error`'s refusals: an unsupported load
/// solved WITHOUT the load and converged, a negative frequency converged, and
/// crossing wires solved (FND-173). Each must now be refused, with the same
/// sentence the main CLI gives. The unmodified template is the control.
#[test]
fn a_resonance_search_refuses_what_every_frontend_refuses() {
    let fr = "FR 0 1 0 0 14.2 0";
    let cases = [
        ("ld", "LD 9 1 26 26 50 0 0\n", fr, "load type 9"),
        ("negfr", "", "FR 0 1 0 0 -14.2 0", "not a usable frequency"),
        ("cross", "GW 2 11 -3 0 0 3 0 0 0.001\n", fr, "intersecting"),
    ];
    for (name, extra, fr_card, needle) in cases {
        let out = search(name, extra, fr_card);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "{name}: must be refused, got success"
        );
        assert!(stderr.contains(needle), "{name}: {stderr}");
        assert_eq!(
            field_value("CONVERGED", &String::from_utf8_lossy(&out.stdout)),
            None,
            "{name}: a refused search must not report convergence"
        );
    }
    let control = search("control", "", fr);
    assert!(
        control.status.success(),
        "the unmodified template must converge"
    );
}
