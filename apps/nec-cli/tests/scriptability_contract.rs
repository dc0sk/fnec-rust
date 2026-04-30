use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_temp_deck(prefix: &str, body: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-{prefix}-{now}.nec"));
    fs::write(&path, body).expect("failed to write temporary deck");
    path
}

#[test]
fn report_headers_are_machine_parseable_on_stdout() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let deck_path = write_temp_deck("scriptable-headers", deck);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for scriptability test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert!(
        lines.len() >= 8,
        "expected report to include stable header rows, got:\n{stdout}"
    );
    assert_eq!(lines[0], "FNEC FEEDPOINT REPORT");
    assert_eq!(lines[1], "FORMAT_VERSION 1");
    assert_eq!(lines[3], "SOLVER_MODE hallen");
    assert_eq!(lines[4], "PULSE_RHS Nec2");
    assert_eq!(lines[6], "FEEDPOINTS");
    assert_eq!(lines[7], "TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM");

    let feed_idx = stdout.find("FEEDPOINTS\n").expect("missing FEEDPOINTS");
    let sources_idx = stdout.find("SOURCES\n").expect("missing SOURCES");
    let currents_idx = stdout.find("CURRENTS\n").expect("missing CURRENTS");
    assert!(
        feed_idx < sources_idx && sources_idx < currents_idx,
        "stdout section ordering must remain FEEDPOINTS -> SOURCES -> CURRENTS for machine parsers"
    );
}

#[test]
fn warnings_stay_on_stderr_not_stdout() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nXX 1 2 3\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let deck_path = write_temp_deck("scriptable-warn-stream", deck);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for stream test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("warning: line 2: unknown card 'XX'"),
        "expected parser warning in stderr, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("warning:"),
        "stdout must remain report-only for machine parsing, got:\n{stdout}"
    );
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "stdout report missing expected header, got:\n{stdout}"
    );
}

#[test]
fn benchmark_json_stays_on_stderr_not_stdout() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let deck_path = write_temp_deck("scriptable-bench-stream", deck);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--bench-format")
        .arg("json")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for bench stream test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("bench_json:{"),
        "expected benchmark json record in stderr, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("bench_json:{"),
        "stdout must stay report-only for parsers, got:\n{stdout}"
    );
    assert!(
        stdout.starts_with("FNEC FEEDPOINT REPORT\nFORMAT_VERSION 1\n"),
        "stdout should begin with stable report header, got:\n{stdout}"
    );
}

#[test]
fn bench_csv_stays_on_stderr_not_stdout() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let deck_path = write_temp_deck("scriptable-bench-csv", deck);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--bench-format")
        .arg("csv")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for bench csv stream test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("bench_csv:"),
        "expected bench csv records in stderr, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("bench_csv:"),
        "stdout must stay report-only for parsers, got:\n{stdout}"
    );
    assert!(
        stdout.starts_with("FNEC FEEDPOINT REPORT\nFORMAT_VERSION 1\n"),
        "stdout should begin with stable report header, got:\n{stdout}"
    );
}

#[test]
fn load_table_stays_on_stdout_while_warnings_stay_on_stderr() {
    // GE is parsed; LD is still deferred in this phase and should warn.
    // The LOADS section does not appear when LD is not parsed.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nLD 2 1 26 26 5.0 1e-6 0.0\nXX 1 2 3\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let deck_path = write_temp_deck("scriptable-load-table-stream", deck);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for load-table stream test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Phase-1: no LOADS section since LD is not parsed.
    assert!(
        !stdout.contains("LOADS\n"),
        "Phase-1: no LOADS section expected when LD is not parsed, got:\n{stdout}"
    );
    assert!(
        !stderr.contains("unknown card 'GE'"),
        "GE should be parsed and should not warn as unknown, got:\n{stderr}"
    );
    assert!(
        stderr.contains("warning: line 3: unknown card 'LD'"),
        "expected unknown-card warning for LD in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("warning: line 4: unknown card 'XX'"),
        "expected parser warning in stderr, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("warning:"),
        "stdout must remain report-only, got:\n{stdout}"
    );
}

#[test]
fn nonexistent_deck_exits_with_code_1_and_error_on_stderr() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bogus_path = std::env::temp_dir().join("fnec-definitely-does-not-exist.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&bogus_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for missing-file test: {e}"));

    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1 for missing deck file"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("cannot read"),
        "expected file-read error in stderr, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("FNEC FEEDPOINT REPORT"),
        "stdout must be empty on file-read error, got:\n{stdout}"
    );
}

#[test]
fn no_arg_invocation_exits_with_code_2_and_usage_on_stderr() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for no-arg test: {e}"));

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 for no-arg invocation"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("Usage: fnec"),
        "expected usage text in stderr, got:\n{stderr}"
    );
    assert!(
        !stdout.contains("FNEC FEEDPOINT REPORT"),
        "stdout must have no report output on no-arg invocation, got:\n{stdout}"
    );
}
