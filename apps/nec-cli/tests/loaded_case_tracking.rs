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
fn loaded_case_allow_noncollinear_flag_is_silently_ignored_and_succeeds() {
    // Phase-2: non-collinear Hallen is now supported by default.
    // --allow-noncollinear-hallen is silently ignored; the command should succeed.
    let hallen_exp = run_loaded_case(&["--solver", "hallen", "--allow-noncollinear-hallen"]);
    assert!(
        hallen_exp.status.success(),
        "Phase-2: loaded case should succeed with Hallen (non-collinear now supported)"
    );

    let stdout = String::from_utf8_lossy(&hallen_exp.stdout);
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "expected feedpoint report in stdout, got:\n{stdout}"
    );
}

#[test]
fn loaded_case_default_hallen_succeeds_with_non_collinear_topology() {
    let out = run_loaded_case(&["--solver", "hallen"]);

    assert!(
        out.status.success(),
        "Phase-2: default hallen on dipole-loaded should succeed; got stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "expected report in stdout, got:\n{stdout}"
    );

    let (z_re, z_im) = first_feedpoint_impedance(&stdout);
    assert!(
        z_re.is_finite() && z_re > 0.0,
        "expected positive finite R, got R={z_re}"
    );
    assert!(z_im.is_finite(), "expected finite X, got X={z_im}");
}
