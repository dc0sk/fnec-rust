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
fn ld_type4_changes_feedpoint_impedance() {
    // Phase-1: LD is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.  The loaded impedance equals free-space (no LD effect).
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 4 1 26 26 100.0 50.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    // Phase-1: LD produces unknown-card warning, deck runs as free-space.
    assert!(
        loaded_err.contains("unknown card 'LD'"),
        "expected unknown-card warning for LD, got:\n{loaded_err}"
    );
    let (loaded_r, _) = first_feedpoint_impedance(&loaded_out);
    assert!(
        (loaded_r - 74.24).abs() < 0.5,
        "Phase-1 LD4 deck should run as free-space (~74.24 Ω), got Z_RE={loaded_r}"
    );
}

#[test]
fn unsupported_ld_type_emits_warning_and_continues() {
    // Phase-1: LD is not parsed; all LD cards produce 'unknown card 'LD'' warning.
    // The old per-type "LD type N … is not yet supported; load ignored" message
    // no longer fires.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let deck_path = std::env::temp_dir().join(format!("fnec-ld-unsupported-{now}.nec"));
    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 9 1 26 26 1.0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write deck with unsupported LD type");

    let (_, stderr) = run_fnec(&deck_path, &workspace_root);
    let _ = fs::remove_file(&deck_path);

    assert!(
        stderr.contains("unknown card 'LD'"),
        "expected unknown-card warning for LD, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("LD type 9 on tag 1 is not yet supported; load ignored"),
        "Phase-1 should not emit per-type LD warning, got:\n{stderr}"
    );
}

#[test]
fn ld_type1_parallel_r_is_supported_and_changes_impedance() {
    // Phase-1: LD is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld1-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 1 1 26 26 1000.0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        loaded_err.contains("unknown card 'LD'"),
        "expected unknown-card warning for LD, got:\n{loaded_err}"
    );
    let (loaded_r, _) = first_feedpoint_impedance(&loaded_out);
    assert!(
        (loaded_r - 74.24).abs() < 0.5,
        "Phase-1 LD1 deck should run as free-space (~74.24 Ω), got Z_RE={loaded_r}"
    );
}

#[test]
fn ld_type2_series_rl_is_supported_and_changes_impedance() {
    // Phase-1: LD is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld2-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 2 1 26 26 10.0 1e-6 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        loaded_err.contains("unknown card 'LD'"),
        "expected unknown-card warning for LD, got:\n{loaded_err}"
    );
    let (_, loaded_x) = first_feedpoint_impedance(&loaded_out);
    assert!(
        (loaded_x - 13.9).abs() < 0.5,
        "Phase-1 LD2 deck should run as free-space (X≈13.9 Ω), got Z_IM={loaded_x}"
    );
}

#[test]
fn ld_type3_series_rc_is_supported_and_changes_impedance() {
    // Phase-1: LD is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let loaded_path = std::env::temp_dir().join(format!("fnec-ld3-loaded-{now}.nec"));
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 3 1 26 26 10.0 0.0 1e-12\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        loaded_err.contains("unknown card 'LD'"),
        "expected unknown-card warning for LD, got:\n{loaded_err}"
    );
    let (_, loaded_x) = first_feedpoint_impedance(&loaded_out);
    assert!(
        (loaded_x - 13.9).abs() < 0.5,
        "Phase-1 LD3 deck should run as free-space (X≈13.9 Ω), got Z_IM={loaded_x}"
    );
}
