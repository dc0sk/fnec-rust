use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_fnec_with_args(
    deck_path: &Path,
    workspace_root: &Path,
    extra_args: &[&str],
) -> (String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0");

    for arg in extra_args {
        cmd.arg(arg);
    }

    let output = cmd
        .arg(deck_path)
        .current_dir(workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for {}: {e}", deck_path.display()));

    assert!(
        output.status.success(),
        "fnec failed for {}: {}",
        deck_path.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run_fnec(deck_path: &Path, workspace_root: &Path) -> (String, String) {
    run_fnec_with_args(deck_path, workspace_root, &[])
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

fn first_feedpoint_source_and_current(stdout: &str) -> (f64, f64, f64, f64) {
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

        let v_re = cols[2]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse V_RE from '{line}': {e}"));
        let v_im = cols[3]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse V_IM from '{line}': {e}"));
        let i_re = cols[4]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse I_RE from '{line}': {e}"));
        let i_im = cols[5]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse I_IM from '{line}': {e}"));
        return (v_re, v_im, i_re, i_im);
    }

    panic!("no feedpoint rows found in stdout:\n{stdout}");
}

#[test]
fn ex_type3_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex0_path = std::env::temp_dir().join(format!("fnec-ex0-{now}.nec"));
    let ex3_path = std::env::temp_dir().join(format!("fnec-ex3-{now}.nec"));

    let ex0_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let ex3_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 3 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fs::write(&ex0_path, ex0_deck).expect("failed to write EX type 0 deck");
    fs::write(&ex3_path, ex3_deck).expect("failed to write EX type 3 deck");

    let (ex0_out, ex0_err) = run_fnec(&ex0_path, &workspace_root);
    let (ex3_out, ex3_err) = run_fnec(&ex3_path, &workspace_root);

    let _ = fs::remove_file(&ex0_path);
    let _ = fs::remove_file(&ex3_path);

    assert!(
        !ex3_err.contains("excitation type 3") && !ex3_err.contains("not yet supported"),
        "EX type 3 should be accepted, got stderr:\n{ex3_err}"
    );

    let (ex0_r, ex0_x) = first_feedpoint_impedance(&ex0_out);
    let (ex3_r, ex3_x) = first_feedpoint_impedance(&ex3_out);

    let dr = (ex3_r - ex0_r).abs();
    let dx = (ex3_x - ex0_x).abs();
    assert!(
        dr <= 1e-6 && dx <= 1e-6,
        "expected EX type 3 to match EX type 0 impedance; dR={dr:.9}, dX={dx:.9}; ex0=({ex0_r:.9}, {ex0_x:.9}) ex3=({ex3_r:.9}, {ex3_x:.9}); ex0 stderr:\n{ex0_err}\nex3 stderr:\n{ex3_err}"
    );
}

#[test]
fn ex_type1_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex0_path = std::env::temp_dir().join(format!("fnec-ex0-{now}.nec"));
    let ex1_path = std::env::temp_dir().join(format!("fnec-ex1-{now}.nec"));

    let ex0_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let ex1_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 1 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fs::write(&ex0_path, ex0_deck).expect("failed to write EX type 0 deck");
    fs::write(&ex1_path, ex1_deck).expect("failed to write EX type 1 deck");

    let (ex0_out, ex0_err) = run_fnec(&ex0_path, &workspace_root);
    let (ex1_out, ex1_err) = run_fnec(&ex1_path, &workspace_root);

    let _ = fs::remove_file(&ex0_path);
    let _ = fs::remove_file(&ex1_path);

    assert!(
        ex1_err.contains("EX type 1 is currently treated like EX type 0")
            && !ex1_err.contains("not yet supported"),
        "EX type 1 should be accepted with portability warning, got stderr:\n{ex1_err}"
    );

    let (ex0_r, ex0_x) = first_feedpoint_impedance(&ex0_out);
    let (ex1_r, ex1_x) = first_feedpoint_impedance(&ex1_out);

    let dr = (ex1_r - ex0_r).abs();
    let dx = (ex1_x - ex0_x).abs();
    assert!(
        dr <= 1e-6 && dx <= 1e-6,
        "expected EX type 1 to match EX type 0 impedance; dR={dr:.9}, dX={dx:.9}; ex0=({ex0_r:.9}, {ex0_x:.9}) ex1=({ex1_r:.9}, {ex1_x:.9}); ex0 stderr:\n{ex0_err}\nex1 stderr:\n{ex1_err}"
    );
}

#[test]
fn ex_type2_matches_ex_type0_feedpoint_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex0_path = std::env::temp_dir().join(format!("fnec-ex0-{now}.nec"));
    let ex2_path = std::env::temp_dir().join(format!("fnec-ex2-{now}.nec"));

    let ex0_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let ex2_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 2 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fs::write(&ex0_path, ex0_deck).expect("failed to write EX type 0 deck");
    fs::write(&ex2_path, ex2_deck).expect("failed to write EX type 2 deck");

    let (ex0_out, ex0_err) = run_fnec(&ex0_path, &workspace_root);
    let (ex2_out, ex2_err) = run_fnec(&ex2_path, &workspace_root);

    let _ = fs::remove_file(&ex0_path);
    let _ = fs::remove_file(&ex2_path);

    assert!(
        ex2_err.contains("EX type 2 is currently treated like EX type 0")
            && !ex2_err.contains("not yet supported"),
        "EX type 2 should be accepted with portability warning, got stderr:\n{ex2_err}"
    );

    let (ex0_r, ex0_x) = first_feedpoint_impedance(&ex0_out);
    let (ex2_r, ex2_x) = first_feedpoint_impedance(&ex2_out);

    let dr = (ex2_r - ex0_r).abs();
    let dx = (ex2_x - ex0_x).abs();
    assert!(
        dr <= 1e-6 && dx <= 1e-6,
        "expected EX type 2 to match EX type 0 impedance; dR={dr:.9}, dX={dx:.9}; ex0=({ex0_r:.9}, {ex0_x:.9}) ex2=({ex2_r:.9}, {ex2_x:.9}); ex0 stderr:\n{ex0_err}\nex2 stderr:\n{ex2_err}"
    );
}

#[test]
fn ex_type3_i4_runtime_mode_divide_by_i4_scales_source_and_current() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let ex3_i4_path = std::env::temp_dir().join(format!("fnec-ex3-i4-{now}.nec"));
    let ex3_i4_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 3 1 26 2 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&ex3_i4_path, ex3_i4_deck).expect("failed to write EX type 3 I4 deck");

    let (legacy_out, legacy_err) = run_fnec(&ex3_i4_path, &workspace_root);
    let (mode_out, mode_err) = run_fnec_with_args(
        &ex3_i4_path,
        &workspace_root,
        &["--ex3-i4-mode", "divide-by-i4"],
    );

    let _ = fs::remove_file(&ex3_i4_path);

    assert!(
        legacy_err.contains("EX type 3 with non-default I4 is currently treated like EX type 0"),
        "legacy run should emit pending-normalization warning, got stderr:\n{legacy_err}"
    );
    assert!(
        mode_err.contains(
            "--ex3-i4-mode=divide-by-i4 enables experimental EX type 3 normalization semantics"
        ),
        "divide-by-i4 run should emit experimental-mode warning, got stderr:\n{mode_err}"
    );

    let (legacy_v_re, legacy_v_im, legacy_i_re, legacy_i_im) =
        first_feedpoint_source_and_current(&legacy_out);
    let (mode_v_re, mode_v_im, mode_i_re, mode_i_im) =
        first_feedpoint_source_and_current(&mode_out);

    let source_ratio = (mode_v_re.hypot(mode_v_im)) / (legacy_v_re.hypot(legacy_v_im));
    let current_ratio = (mode_i_re.hypot(mode_i_im)) / (legacy_i_re.hypot(legacy_i_im));

    assert!(
        (source_ratio - 0.5).abs() <= 1e-6,
        "expected source ratio 0.5 for divide-by-i4 mode, got {source_ratio:.9}; legacy=({legacy_v_re:.9}, {legacy_v_im:.9}) mode=({mode_v_re:.9}, {mode_v_im:.9})"
    );
    assert!(
        (current_ratio - 0.5).abs() <= 5e-3,
        "expected current ratio near 0.5 for divide-by-i4 mode, got {current_ratio:.9}; legacy=({legacy_i_re:.9}, {legacy_i_im:.9}) mode=({mode_i_re:.9}, {mode_i_im:.9})"
    );
}
