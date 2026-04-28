use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn gn_deferred_type_emits_warning_and_falls_back_to_free_space() {
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
        stderr
            .contains("warning: GN type 0 is not yet supported; treating this deck as free-space"),
        "expected deferred GN warning in stderr, got:\n{stderr}"
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
fn gn_type2_deferred_emits_warning_and_falls_back_to_free_space() {
    // GN type 2 (Sommerfeld/Norton finite-conductivity) is deferred; the
    // solver must warn and fall back to free-space, not fail or produce PEC
    // results.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn2-{now}.nec"));

    // Average-ground parameters (EPSE=13, SIG=0.005 S/m); the parser now reads
    // and stores these fields, and the CLI warning appends them.
    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGN 2 0 0 0 13.0 0.005\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
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
        stderr
            .contains("warning: GN type 2 is not yet supported; treating this deck as free-space"),
        "expected deferred GN type 2 warning in stderr, got:\n{stderr}"
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
fn gn_type2_warning_includes_parsed_medium_params() {
    // When a GN card includes medium parameters (EPSE, SIG), the deferred
    // warning must append them so the user sees what was parsed.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn2-params-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGN 2 0 0 0 13.0 0.005\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
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
        stderr.contains("[parsed: EPSE=13, SIG=0.005 S/m]"),
        "expected medium params in deferred warning, got:\n{stderr}"
    );
}
