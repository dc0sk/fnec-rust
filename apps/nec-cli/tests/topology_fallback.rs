use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

use common::{assert_diag_field, assert_diag_field_is_finite_nonnegative, assert_diag_mode};

fn assert_non_single_chain_fallback(solver: &str, expected_diag_mode: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-topology-fallback-{solver}-{now}.nec"));

    // Topology that is invalid for per-wire basis solve: one wire has only 1 segment.
    // This must force continuity/sinusoidal to fall back to pulse.
    let deck = "GW 1 11 0.0 0.0 -1.0 0.0 0.0 1.0 0.001\nGW 2 1 0.5 0.0 0.0 0.5 0.0 0.1 0.001\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary topology-fallback deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg(solver)
        .env_remove("FNEC_ACCEL_STUB_GPU")
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
            "warning: {solver} solver requires >=2 segments per wire; falling back to pulse"
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
        .env_remove("FNEC_ACCEL_STUB_GPU")
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
        .env_remove("FNEC_ACCEL_STUB_GPU")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| {
            panic!("Failed to run fnec for solver '{solver}' with pulse-rhs '{pulse_rhs}': {e}")
        })
}

fn run_solver_on_reference_dipole_with_exec(solver: &str, exec_mode: &str) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");

    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg(solver)
        .arg("--exec")
        .arg(exec_mode)
        .env_remove("FNEC_ACCEL_STUB_GPU")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| {
            panic!("Failed to run fnec for solver '{solver}' with exec-mode '{exec_mode}': {e}")
        })
}

fn run_sinusoidal_with_threshold(
    threshold_cli: Option<&str>,
    threshold_env: Option<&str>,
) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.arg("--solver").arg("sinusoidal");
    if let Some(v) = threshold_cli {
        cmd.arg("--sin-fallback-rel-max").arg(v);
    }
    match threshold_env {
        Some(v) => {
            cmd.env("FNEC_SIN_FALLBACK_REL_MAX", v);
        }
        None => {
            cmd.env_remove("FNEC_SIN_FALLBACK_REL_MAX");
        }
    }
    cmd.env_remove("FNEC_ACCEL_STUB_GPU");
    cmd.arg(&deck_path);
    cmd.output()
        .unwrap_or_else(|e| panic!("Failed to run sinusoidal solver with threshold override: {e}"))
}

fn run_hallen_on_loaded_case(allow_noncollinear_hallen: bool) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-loaded.nec");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.arg("--solver").arg("hallen");
    cmd.env_remove("FNEC_ACCEL_STUB_GPU");
    if allow_noncollinear_hallen {
        cmd.arg("--allow-noncollinear-hallen");
    }
    cmd.arg(&deck_path);
    cmd.output().unwrap_or_else(|e| {
        panic!(
            "Failed to run fnec hallen for loaded case (allow_noncollinear_hallen={allow_noncollinear_hallen}): {e}"
        )
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
fn sinusoidal_solves_reference_dipole_without_fallback() {
    let output = run_solver_on_reference_dipole("sinusoidal");

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("falling back"),
        "sinusoidal should not fall back on a clean 51-segment dipole, got:\n{stderr}"
    );
    assert_diag_mode(&stderr, "sinusoidal");
}

#[test]
fn sinusoidal_threshold_cli_override_forces_hallen_fallback() {
    // Keep env permissive and force fallback via tiny CLI threshold.
    let output = run_sinusoidal_with_threshold(Some("1e-30"), Some("1e9"));
    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: sinusoidal residual"),
        "expected sinusoidal residual fallback warning, got:\n{stderr}"
    );
    assert_diag_mode(&stderr, "sinusoidal->hallen(residual)");
    assert_diag_field(&stderr, "sin_fallback_rel_max", "1.000000e-30");
}

#[test]
fn invalid_sinusoidal_threshold_env_reports_usage_error() {
    let output = run_sinusoidal_with_threshold(None, Some("bogus"));
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid FNEC_SIN_FALLBACK_REL_MAX='bogus'"),
        "expected invalid env-value error, got:\n{stderr}"
    );
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

    // Only pulse and continuity are EXPERIMENTAL; sinusoidal is now a stable solver.
    for solver in ["pulse", "continuity"] {
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
    // Sinusoidal must NOT emit the EXPERIMENTAL warning.
    let sin_output = run_solver_on_reference_dipole("sinusoidal");
    assert!(
        sin_output.status.success(),
        "fnec failed for sinusoidal: {}",
        String::from_utf8_lossy(&sin_output.stderr)
    );
    let sin_stderr = String::from_utf8_lossy(&sin_output.stderr);
    assert!(
        !sin_stderr.contains("solver modes are EXPERIMENTAL"),
        "sinusoidal must not emit EXPERIMENTAL warning, got:\n{sin_stderr}"
    );
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

#[test]
fn exec_mode_defaults_to_cpu_in_diag_field() {
    let output = run_solver_on_reference_dipole("hallen");
    assert!(
        output.status.success(),
        "fnec failed for hallen/default-exec: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_diag_field(&stderr, "exec", "cpu");
}

#[test]
fn hybrid_exec_is_reflected_without_fallback_warning_and_gpu_warns_cpu_fallback() {
    let hybrid = run_solver_on_reference_dipole_with_exec("hallen", "hybrid");
    assert!(
        hybrid.status.success(),
        "fnec failed for hallen/hybrid: {}",
        String::from_utf8_lossy(&hybrid.stderr)
    );
    let hybrid_stderr = String::from_utf8_lossy(&hybrid.stderr);
    assert!(
        !hybrid_stderr.contains("warning: --exec hybrid requested"),
        "did not expect hybrid fallback warning in stderr, got:\n{hybrid_stderr}"
    );
    assert_diag_field(&hybrid_stderr, "exec", "hybrid");

    let gpu = run_solver_on_reference_dipole_with_exec("hallen", "gpu");
    assert!(
        gpu.status.success(),
        "fnec failed for hallen/gpu: {}",
        String::from_utf8_lossy(&gpu.stderr)
    );
    let gpu_stderr = String::from_utf8_lossy(&gpu.stderr);
    assert!(
        gpu_stderr.contains("warning: --exec gpu requested"),
        "expected gpu fallback warning in stderr, got:\n{gpu_stderr}"
    );
    assert_diag_field(&gpu_stderr, "exec", "gpu(cpu-fallback)");
}

#[test]
fn freq_mhz_diag_field_has_fixed_six_decimal_format() {
    let hallen = run_solver_on_reference_dipole("hallen");
    assert!(
        hallen.status.success(),
        "fnec failed for hallen: {}",
        String::from_utf8_lossy(&hallen.stderr)
    );
    let hallen_stderr = String::from_utf8_lossy(&hallen.stderr);
    assert_diag_field(&hallen_stderr, "freq_mhz", "14.200000");

    let pulse = run_solver_on_reference_dipole("pulse");
    assert!(
        pulse.status.success(),
        "fnec failed for pulse: {}",
        String::from_utf8_lossy(&pulse.stderr)
    );
    let pulse_stderr = String::from_utf8_lossy(&pulse.stderr);
    assert_diag_field(&pulse_stderr, "freq_mhz", "14.200000");
}

#[test]
fn residual_diag_fields_are_finite_and_nonnegative() {
    let hallen = run_solver_on_reference_dipole("hallen");
    assert!(
        hallen.status.success(),
        "fnec failed for hallen: {}",
        String::from_utf8_lossy(&hallen.stderr)
    );
    let hallen_stderr = String::from_utf8_lossy(&hallen.stderr);
    assert_diag_field_is_finite_nonnegative(&hallen_stderr, "abs_res");
    assert_diag_field_is_finite_nonnegative(&hallen_stderr, "rel_res");
    assert_diag_field_is_finite_nonnegative(&hallen_stderr, "diag_spread");
    assert_diag_field_is_finite_nonnegative(&hallen_stderr, "sin_rel_res");

    let pulse = run_solver_on_reference_dipole("pulse");
    assert!(
        pulse.status.success(),
        "fnec failed for pulse: {}",
        String::from_utf8_lossy(&pulse.stderr)
    );
    let pulse_stderr = String::from_utf8_lossy(&pulse.stderr);
    assert_diag_field_is_finite_nonnegative(&pulse_stderr, "abs_res");
    assert_diag_field_is_finite_nonnegative(&pulse_stderr, "rel_res");
    assert_diag_field_is_finite_nonnegative(&pulse_stderr, "diag_spread");
    assert_diag_field_is_finite_nonnegative(&pulse_stderr, "sin_rel_res");

    let sinusoidal = run_solver_on_reference_dipole("sinusoidal");
    assert!(
        sinusoidal.status.success(),
        "fnec failed for sinusoidal: {}",
        String::from_utf8_lossy(&sinusoidal.stderr)
    );
    let sinusoidal_stderr = String::from_utf8_lossy(&sinusoidal.stderr);
    assert_diag_field_is_finite_nonnegative(&sinusoidal_stderr, "abs_res");
    assert_diag_field_is_finite_nonnegative(&sinusoidal_stderr, "rel_res");
    assert_diag_field_is_finite_nonnegative(&sinusoidal_stderr, "diag_spread");
    assert_diag_field_is_finite_nonnegative(&sinusoidal_stderr, "sin_rel_res");
}

#[test]
fn hallen_non_collinear_succeeds_without_opt_in_flag() {
    // Phase-2: non-collinear multi-wire topologies are now supported by default.
    let output = run_hallen_on_loaded_case(false);

    assert!(
        output.status.success(),
        "expected hallen to succeed on non-collinear loaded case; got stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "expected feedpoint report, got stdout:\n{stdout}"
    );
}

#[test]
fn hallen_non_collinear_opt_in_flag_is_silently_ignored() {
    // Phase-2: --allow-noncollinear-hallen is silently ignored; non-collinear
    // decks succeed regardless of whether the flag is passed.
    let output = run_hallen_on_loaded_case(true);

    assert!(
        output.status.success(),
        "expected non-collinear hallen to succeed even with --allow-noncollinear-hallen; got stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("collinear"),
        "unexpected collinear-topology message in stderr:\n{stderr}"
    );
}
