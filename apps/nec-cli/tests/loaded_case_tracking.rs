use std::path::PathBuf;
use std::process::Command;

fn run_loaded_case(args: &[&str]) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-loaded.nec");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(args).arg(&deck_path);
    cmd.output().unwrap_or_else(|e| {
        panic!(
            "failed to run fnec for loaded case with args {:?}: {e}",
            args
        )
    })
}

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

#[test]
fn loaded_case_allow_noncollinear_flag_is_ignored_and_topology_error_remains() {
    // Phase-1: --allow-noncollinear-hallen is silently ignored; the loaded case
    // (multi-wire, non-collinear) fails with the topology error regardless.
    // This test now verifies that the flag is silently ignored and the topology
    // error is still emitted.
    let hallen_exp = run_loaded_case(&["--solver", "hallen", "--allow-noncollinear-hallen"]);
    assert!(
        !hallen_exp.status.success(),
        "Phase-1: --allow-noncollinear-hallen is silently ignored; loaded case should still fail with topology error"
    );

    let stderr = String::from_utf8_lossy(&hallen_exp.stderr);
    assert!(
        stderr.contains("error: Hallén solver currently supports only collinear wire topologies"),
        "expected non-collinear topology error even with --allow-noncollinear-hallen, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("--allow-noncollinear-hallen"),
        "Phase-1: --allow-noncollinear-hallen should be silently ignored (no warning), got:\n{stderr}"
    );
}

#[test]
fn loaded_case_default_hallen_fails_fast_with_noncollinear_error_contract() {
    let out = run_loaded_case(&["--solver", "hallen"]);

    assert_eq!(
        out.status.code(),
        Some(1),
        "default hallen on dipole-loaded should fail fast with exit code 1"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error: Hallén solver currently supports only collinear wire topologies"),
        "expected non-collinear Hallen contract error in stderr, got:\n{stderr}"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("FNEC FEEDPOINT REPORT"),
        "no report should be emitted on Hallen topology hard-fail path, got:\n{stdout}"
    );
}
