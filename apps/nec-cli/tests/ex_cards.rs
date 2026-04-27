use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_fnec(deck_path: &Path, workspace_root: &Path) -> (String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
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
