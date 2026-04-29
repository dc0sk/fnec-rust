use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn gn0_is_active_without_deferred_warning() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn0-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGN 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GN deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GN diagnostics test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("warning: GN type 0 is not yet supported"),
        "GN0 should no longer use deferred-ground warning, got:\n{stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let z_re = stdout
        .lines()
        .find_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 8 && parts[0] == "1" && parts[1] == "26" {
                parts[6].parse::<f64>().ok()
            } else {
                None
            }
        })
        .expect("no feedpoint row in output");

    // GN0 finite ground should not collapse to free-space (~74.23 Ohm real).
    assert!(
        (z_re - 74.23).abs() > 0.5,
        "GN0 should alter impedance vs free-space, got Z_RE={z_re}"
    );
}

#[test]
fn ge1_without_gn_infers_pec_ground() {
    // GE 1 without an explicit GN card should infer PEC image-method ground.
    // The dipole from z=4.718m to z=15.282m over PEC ground (same geometry as
    // corpus/dipole-ground-51seg.nec which uses GE + GN 1) should produce the
    // same feedpoint impedance: ~81.91 + j16.42 Ω.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ge1-pec-{now}.nec"));

    // Same wire geometry as dipole-ground-51seg.nec, GE 1 instead of GE + GN 1.
    let deck =
        "GW 1 51 0 0 4.718 0 0 15.282 0.001\nGE 1\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GE1 deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GE1 PEC inference test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("warning: GE ground-reflection flag"),
        "unexpected GE warning for GE 1 (should be silently handled):\n{stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse Z_RE from the feedpoint row: "1 26 ... Z_RE Z_IM"
    let z_re = stdout
        .lines()
        .find_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 8 && parts[0] == "1" && parts[1] == "26" {
                parts[6].parse::<f64>().ok()
            } else {
                None
            }
        })
        .expect("no feedpoint row in output");

    let z_im = stdout
        .lines()
        .find_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 8 && parts[0] == "1" && parts[1] == "26" {
                parts[7].parse::<f64>().ok()
            } else {
                None
            }
        })
        .expect("no feedpoint row for imag");

    // Tolerance: same as corpus gate (0.05 Ω absolute).
    assert!(
        (z_re - 81.914743).abs() < 0.05,
        "Z_RE mismatch for GE1 PEC deck: got {z_re}, expected ~81.91"
    );
    assert!(
        (z_im - 16.416629).abs() < 0.05,
        "Z_IM mismatch for GE1 PEC deck: got {z_im}, expected ~16.42"
    );
}

#[test]
fn ge_negative_flag_emits_unsupported_warning() {
    // GE I1=-1 (half-space without image) is not supported; should warn.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ge-neg-{now}.nec"));

    let deck =
        "GW 1 51 0 0 4.718 0 0 15.282 0.001\nGE -1\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GE-1 deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GE negative flag test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: GE I1=-1 requests below-ground wire handling"),
        "expected below-ground warning in stderr, got:\n{stderr}"
    );
}

#[test]
fn gn_type2_runs_without_deferred_warning_and_changes_impedance() {
    // Phase-2 scoped behavior: GN type 2 uses the simple finite-ground path.
    // It should no longer emit the old deferred warning and should not
    // collapse to free-space impedance.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn2-{now}.nec"));

    // In-scope above-ground GN2 class with average-ground parameters.
    let deck =
        "GW 1 51 0 0 4.718 0 0 15.282 0.001\nGE\nGN 2 0 0 0 13.0 0.005\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GN-2 deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GN type 2 diagnostics test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("GN type 2 is not yet supported"),
        "GN2 should no longer use deferred-ground warning, got:\n{stderr}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let z_re = stdout
        .lines()
        .find_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 8 && parts[0] == "1" && parts[1] == "26" {
                parts[6].parse::<f64>().ok()
            } else {
                None
            }
        })
        .expect("no feedpoint row in output");

    assert!(
        (z_re - 78.170459).abs() < 0.05,
        "GN2 regression mismatch: got Z_RE={z_re}, expected ~78.17"
    );
}

#[test]
fn gn_type3_deferred_emits_warning_and_falls_back_to_free_space() {
    // GN type 3 is not a standard NEC-2 type but may appear in NEC-4 decks.
    // It must be treated as deferred with a warning, not silently accepted.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn3-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGN 3\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GN-3 deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GN type 3 diagnostics test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .contains("warning: GN type 3 is not yet supported; treating this deck as free-space"),
        "expected deferred GN type 3 warning in stderr, got:\n{stderr}"
    );
}

#[test]
fn gn_type2_medium_params_run_without_deferred_warning() {
    // GN2 with explicit medium params should run on the active finite-ground
    // path and must not emit the deferred-ground warning.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn2-params-{now}.nec"));

    let deck =
        "GW 1 51 0 0 4.718 0 0 15.282 0.001\nGE\nGN 2 0 0 0 13.0 0.005\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GN-2 params deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GN medium params test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("GN type 2 is not yet supported"),
        "GN2 with medium params should not use deferred warning, got:\n{stderr}"
    );
}

#[test]
fn gn_negative1_null_ground_is_silent_free_space() {
    // GN -1 cancels a previous GN statement (NEC spec §3.3); when it is the
    // only GN card the solver must treat it as free-space WITHOUT emitting the
    // deferred-ground warning.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn-neg1-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGN -1\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GN -1 deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GN -1 test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("warning: GN type -1 is not yet supported"),
        "GN -1 should NOT emit a deferred-ground warning, got:\n{stderr}"
    );

    // GN -1 should produce free-space results matching the free-space dipole.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let z_re = stdout
        .lines()
        .find_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 8 && parts[0] == "1" && parts[1] == "26" {
                parts[6].parse::<f64>().ok()
            } else {
                None
            }
        })
        .expect("no feedpoint row in output");

    assert!(
        (z_re - 74.23).abs() < 0.1,
        "Z_RE mismatch for GN -1 deck: got {z_re}, expected ~74.23 (free-space)"
    );
}
