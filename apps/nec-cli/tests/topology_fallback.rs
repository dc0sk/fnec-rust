use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

use common::{assert_diag_field, assert_diag_mode};

fn assert_non_single_chain_fallback(solver: &str, expected_diag_mode: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-topology-fallback-{solver}-{now}.nec"));

    // Two disjoint wires (different tags) to ensure the topology is not a single linear chain.
    let deck = "GW 1 11 0.0 0.0 -1.0 0.0 0.0 1.0 0.001\nGW 2 11 0.5 0.0 -1.0 0.5 0.0 1.0 0.001\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary topology-fallback deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg(solver)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for {solver} topology fallback test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "warning: {solver} solver currently supports only single linear chains; falling back to pulse on this topology"
        )),
        "expected topology fallback warning in stderr, got:\n{stderr}"
    );
    assert_diag_mode(&stderr, expected_diag_mode);
}

fn run_solver_on_reference_dipole(solver: &str) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");

    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg(solver)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for solver '{solver}': {e}"))
}

fn run_solver_on_reference_dipole_with_pulse_rhs(
    solver: &str,
    pulse_rhs: &str,
) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");

    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg(solver)
        .arg("--pulse-rhs")
        .arg(pulse_rhs)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| {
            panic!("Failed to run fnec for solver '{solver}' with pulse-rhs '{pulse_rhs}': {e}")
        })
}

#[test]
fn continuity_non_single_chain_falls_back_to_pulse() {
    assert_non_single_chain_fallback("continuity", "continuity->pulse");
}

#[test]
fn sinusoidal_non_single_chain_falls_back_to_pulse() {
    assert_non_single_chain_fallback("sinusoidal", "sinusoidal->pulse");
}

#[test]
fn sinusoidal_residual_falls_back_to_hallen_on_reference_dipole() {
    let output = run_solver_on_reference_dipole("sinusoidal");

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: sinusoidal residual "),
        "expected sinusoidal residual warning in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("falling back to hallen"),
        "expected sinusoidal fallback-to-hallen warning in stderr, got:\n{stderr}"
    );
    assert_diag_mode(&stderr, "sinusoidal->hallen(residual)");
}

#[test]
fn experimental_warning_is_mode_gated() {
    let hallen = run_solver_on_reference_dipole("hallen");
    assert!(
        hallen.status.success(),
        "fnec failed for hallen: {}",
        String::from_utf8_lossy(&hallen.stderr)
    );
    let hallen_stderr = String::from_utf8_lossy(&hallen.stderr);
    assert!(
        !hallen_stderr.contains("solver modes are EXPERIMENTAL"),
        "did not expect experimental warning for hallen, got:\n{hallen_stderr}"
    );

    for solver in ["pulse", "continuity", "sinusoidal"] {
        let output = run_solver_on_reference_dipole(solver);
        assert!(
            output.status.success(),
            "fnec failed for {solver}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("solver modes are EXPERIMENTAL"),
            "expected experimental warning for {solver}, got:\n{stderr}"
        );
    }
}

#[test]
fn pulse_rhs_flag_is_reflected_in_diag_field() {
    let raw = run_solver_on_reference_dipole_with_pulse_rhs("pulse", "raw");
    assert!(
        raw.status.success(),
        "fnec failed for pulse/raw: {}",
        String::from_utf8_lossy(&raw.stderr)
    );
    let raw_stderr = String::from_utf8_lossy(&raw.stderr);
    assert_diag_field(&raw_stderr, "pulse_rhs", "Raw");

    let nec2 = run_solver_on_reference_dipole_with_pulse_rhs("pulse", "nec2");
    assert!(
        nec2.status.success(),
        "fnec failed for pulse/nec2: {}",
        String::from_utf8_lossy(&nec2.stderr)
    );
    let nec2_stderr = String::from_utf8_lossy(&nec2.stderr);
    assert_diag_field(&nec2_stderr, "pulse_rhs", "Nec2");
}
