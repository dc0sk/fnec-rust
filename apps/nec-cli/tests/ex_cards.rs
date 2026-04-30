use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_fnec_output(deck_path: &Path, workspace_root: &Path, extra_args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0");

    for arg in extra_args {
        cmd.arg(arg);
    }

    cmd.arg(deck_path)
        .current_dir(workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for {}: {e}", deck_path.display()))
}

#[allow(dead_code)]
fn first_feedpoint_impedance(stdout: &str) -> (f64, f64) {
    for line in stdout.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 8 {
            continue;
        }
        if cols[0] == "TAG" {
            continue;
        }
        if cols[0].parse::<usize>().is_err() || cols[1].parse::<usize>().is_err() {
            continue;
        }

        let z_re = cols[6]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse Z_RE from '{line}': {e}"));
        let z_im = cols[7]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse Z_IM from '{line}': {e}"));
        return (z_re, z_im);
    }

    panic!("no feedpoint rows found in stdout:\n{stdout}");
}

/// Helper: write a temporary deck, run fnec, check it fails with the expected error message,
/// then clean up the temp file.
fn assert_ex_unsupported(ex_type: u8, workspace_root: &Path) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let deck_path = std::env::temp_dir().join(format!("fnec-ex{ex_type}-unsupported-{now}.nec"));
    let deck = format!(
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX {ex_type} 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n"
    );
    fs::write(&deck_path, deck).expect("failed to write deck");

    let output = run_fnec_output(&deck_path, workspace_root, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "EX type {ex_type} should fail with 'not yet supported', but command succeeded; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("is not yet supported"),
        "EX type {ex_type} stderr should contain 'is not yet supported', got:\n{stderr}"
    );
}

#[test]
fn ex_type3_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_ex_unsupported(3, &workspace_root);
}

#[test]
fn ex_type1_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_ex_unsupported(1, &workspace_root);
}

#[test]
fn ex_type1_pulse_imposes_requested_segment_current_without_portability_warning() {
    // Phase-1: EX type 1 is rejected (not yet supported), even in pulse-solver mode.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex1_path = std::env::temp_dir().join(format!("fnec-ex1-pulse-{now}.nec"));
    let ex1_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 1 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex1_path, ex1_deck).expect("failed to write EX type 1 pulse deck");

    let output = run_fnec_output(&ex1_path, &workspace_root, &["--solver", "pulse"]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex1_path);

    assert!(
        !output.status.success(),
        "EX type 1 (pulse mode) should fail with 'not yet supported', but succeeded; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("is not yet supported"),
        "EX type 1 (pulse mode) stderr should contain 'is not yet supported', got:\n{stderr}"
    );
}

#[test]
fn ex_type4_pulse_imposes_requested_segment_current_without_portability_warning() {
    // Phase-1: EX type 4 is rejected (not yet supported), even in pulse-solver mode.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex4_path = std::env::temp_dir().join(format!("fnec-ex4-pulse-{now}.nec"));
    let ex4_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 4 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex4_path, ex4_deck).expect("failed to write EX type 4 pulse deck");

    let output = run_fnec_output(&ex4_path, &workspace_root, &["--solver", "pulse"]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex4_path);

    assert!(
        !output.status.success(),
        "EX type 4 (pulse mode) should fail with 'not yet supported', but succeeded; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("is not yet supported"),
        "EX type 4 (pulse mode) stderr should contain 'is not yet supported', got:\n{stderr}"
    );
}

#[test]
fn ex_type5_pulse_imposes_requested_segment_current_without_portability_warning() {
    // Phase-1: EX type 5 is rejected (not yet supported), even in pulse-solver mode.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex5_path = std::env::temp_dir().join(format!("fnec-ex5-pulse-{now}.nec"));
    let ex5_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 5 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex5_path, ex5_deck).expect("failed to write EX type 5 pulse deck");

    let output = run_fnec_output(&ex5_path, &workspace_root, &["--solver", "pulse"]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex5_path);

    assert!(
        !output.status.success(),
        "EX type 5 (pulse mode) should fail with 'not yet supported', but succeeded; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("is not yet supported"),
        "EX type 5 (pulse mode) stderr should contain 'is not yet supported', got:\n{stderr}"
    );
}

#[test]
fn ex_type2_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_ex_unsupported(2, &workspace_root);
}

#[test]
fn ex_type4_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_ex_unsupported(4, &workspace_root);
}

#[test]
fn ex_type5_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_ex_unsupported(5, &workspace_root);
}

#[test]
fn ex_type3_i4_runtime_mode_divide_by_i4_scales_source_and_current() {
    // Phase-1: EX type 3 is rejected (not yet supported); --ex3-i4-mode is silently ignored.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex3_i4_path = std::env::temp_dir().join(format!("fnec-ex3-i4-{now}.nec"));
    let ex3_i4_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 3 1 26 2 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex3_i4_path, ex3_i4_deck).expect("failed to write EX type 3 I4 deck");

    let output = run_fnec_output(
        &ex3_i4_path,
        &workspace_root,
        &["--ex3-i4-mode", "divide-by-i4"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_file(&ex3_i4_path);

    assert!(
        !output.status.success(),
        "EX type 3 (--ex3-i4-mode) should fail with 'not yet supported', but succeeded; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("is not yet supported"),
        "EX type 3 stderr should contain 'is not yet supported', got:\n{stderr}"
    );
}
