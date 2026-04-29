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
