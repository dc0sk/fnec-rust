use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_deck(name: &str) -> PathBuf {
    workspace_root().join("corpus").join(name)
}

fn run_fnec(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"))
}

#[test]
fn missing_solver_value_reports_contract_error_and_usage() {
    let output = run_fnec(&["--solver"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: fnec [--solver <pulse|hallen|continuity|sinusoidal>]"),
        "missing usage contract in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("missing value after --solver"),
        "missing parse error detail in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_solver_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--solver",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --solver value 'bogus'"),
        "missing invalid solver detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: hallen|pulse|continuity|sinusoidal"),
        "missing expected solver values in stderr:\n{stderr}"
    );
}

#[test]
fn missing_pulse_rhs_value_reports_contract_error_and_usage() {
    let output = run_fnec(&["--pulse-rhs"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing value after --pulse-rhs"),
        "missing pulse-rhs parse error in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: raw|nec2"),
        "missing expected pulse-rhs values in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_pulse_rhs_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--pulse-rhs",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --pulse-rhs value 'bogus'"),
        "missing invalid pulse-rhs detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: raw|nec2"),
        "missing expected pulse-rhs values in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_exec_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--exec",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --exec value 'bogus'"),
        "missing invalid-value detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: cpu|hybrid|gpu"),
        "missing expected-value hint in stderr:\n{stderr}"
    );
}

#[test]
fn missing_bench_format_value_reports_contract_error_and_usage() {
    let output = run_fnec(&["--bench-format"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing value after --bench-format"),
        "missing bench-format parse error in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: human|csv|json"),
        "missing expected bench-format values in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_bench_format_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--bench-format",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --bench-format value 'bogus'"),
        "missing invalid bench-format detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: human|csv|json"),
        "missing expected bench-format values in stderr:\n{stderr}"
    );
}

#[test]
fn unknown_option_reports_contract_error() {
    let output = run_fnec(&["--definitely-not-a-flag"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown option: --definitely-not-a-flag"),
        "missing unknown-option error in stderr:\n{stderr}"
    );
}

#[test]
fn unexpected_extra_argument_reports_contract_error() {
    let deck = fixture_deck("dipole-freesp-51seg.nec");
    let output = run_fnec(&[
        "--solver",
        "hallen",
        deck.to_str().unwrap(),
        deck.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected extra argument:"),
        "missing extra-argument parse error in stderr:\n{stderr}"
    );
}

#[test]
fn missing_deck_path_reports_contract_error() {
    let output = run_fnec(&["--solver", "hallen", "--pulse-rhs", "nec2"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing deck path"),
        "missing deck-path parse error in stderr:\n{stderr}"
    );
}

#[test]
fn all_core_flags_combination_runs_successfully() {
    let deck = fixture_deck("dipole-freesp-51seg.nec");
    let output = run_fnec(&[
        "--solver",
        "hallen",
        "--pulse-rhs",
        "raw",
        "--exec",
        "cpu",
        "--bench-format",
        "json",
        "--gpu-fr",
        deck.to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "expected report header in stdout, got:\n{stdout}"
    );
}
