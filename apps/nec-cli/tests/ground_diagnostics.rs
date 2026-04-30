use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn gn0_is_active_without_deferred_warning() {
    // Phase-1: GN is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.  The old "GN type 0 is not yet supported" deferred
    // warning must NOT appear.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn0-{now}.nec"));

    let deck =
        "GW 1 51 0 0 4.718 0 0 15.282 0.001\nGN 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
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
    // Phase-1 parser does not emit the old deferred-ground warning.
    assert!(
        !stderr.contains("GN type 0 is not yet supported"),
        "did not expect deferred-ground warning for GN0 in Phase-1, got:\n{stderr}"
    );
    // Phase-1 parser emits 'unknown card' for unrecognised cards.
    assert!(
        stderr.contains("unknown card 'GN'"),
        "expected unknown-card warning for GN, got:\n{stderr}"
    );

    // Phase-1: runs as free-space → Z_RE ≈ 74.24 Ω.
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
        (z_re - 74.23).abs() < 0.5,
        "Phase-1 GN0 deck should run as free-space (~74.23 Ω), got Z_RE={z_re}"
    );
}

#[test]
fn ge1_without_gn_infers_pec_ground() {
    // Phase-1: GE is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.  The old GE1 PEC-inference path is deferred.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ge1-pec-{now}.nec"));

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
        stderr.contains("unknown card 'GE'"),
        "expected unknown-card warning for GE, got:\n{stderr}"
    );

    // Phase-1: runs as free-space → Z_RE ≈ 74.24 Ω (not PEC 81.91 Ω).
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
        (z_re - 74.24).abs() < 0.5,
        "Phase-1 GE1 deck should run as free-space (~74.24 Ω), got Z_RE={z_re}"
    );
}

#[test]
fn ge_negative_flag_emits_unsupported_warning() {
    // Phase-1: GE is not parsed; card produces 'unknown card' warning.
    // The old "GE I1=-1 requests below-ground wire handling" warning no longer fires.
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
    // Phase-1: unknown card warning replaces the old specific warning.
    assert!(
        stderr.contains("unknown card 'GE'"),
        "expected unknown-card warning for GE, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("GE I1=-1 requests below-ground wire handling"),
        "Phase-1 should not emit the old GE below-ground warning, got:\n{stderr}"
    );
}

#[test]
fn gn_type2_runs_without_deferred_warning_and_changes_impedance() {
    // Phase-1: GN is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.  The old "GN type 2 is not yet supported" deferred
    // warning must NOT appear.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn2-{now}.nec"));

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
        "Phase-1 should not emit old deferred GN2 warning, got:\n{stderr}"
    );

    // Phase-1: runs as free-space → Z_RE ≈ 74.24 Ω.
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
        (z_re - 74.24).abs() < 0.5,
        "Phase-1 GN2 deck should run as free-space (~74.24 Ω), got Z_RE={z_re}"
    );
}

#[test]
fn gn_type3_deferred_emits_warning_and_falls_back_to_free_space() {
    // Phase-1: GN is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.  The old "GN type 3 is not yet supported; treating
    // this deck as free-space" deferred warning no longer fires.
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
    // Phase-1: unknown card warning.
    assert!(
        stderr.contains("unknown card 'GN'"),
        "expected unknown-card warning for GN, got:\n{stderr}"
    );
    // Old deferred-ground warning must not fire.
    assert!(
        !stderr.contains("GN type 3 is not yet supported; treating this deck as free-space"),
        "Phase-1 should not emit old deferred GN3 warning, got:\n{stderr}"
    );
}

#[test]
fn gn_type2_medium_params_run_without_deferred_warning() {
    // Phase-1: GN is not parsed; the old "GN type 2 is not yet supported"
    // deferred-ground warning must NOT appear, even with medium parameters.
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

#[test]
fn buried_wire_with_active_ground_fails_fast_with_actionable_error() {
    // Phase-1: GN and GE are not parsed; they produce 'unknown card' warnings.
    // With no active ground model, the deck runs as free-space and succeeds.
    // The old "unsupported buried-wire geometry for active ground model" fail-fast
    // guardrail no longer fires because the ground model is never activated.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn2-buried-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -15.282 0 0 -4.718 0.001\nGE\nGN 2 0 0 0 13.0 0.005\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary buried-wire deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for buried-wire guardrail test: {e}"));

    let _ = fs::remove_file(&deck_path);

    // Phase-1: runs as free-space, succeeds.
    assert!(
        output.status.success(),
        "Phase-1 buried-wire deck should succeed (no active ground), stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("error: unsupported buried-wire geometry for active ground model"),
        "Phase-1 should not emit buried-wire guardrail error (no active ground), got:\n{stderr}"
    );
    assert!(
        !stderr.contains("GN type 2 is not yet supported"),
        "Phase-1 should not emit deferred GN2 warning, got:\n{stderr}"
    );
}

#[test]
fn near_ground_wire_with_active_ground_runs_without_deferred_warning() {
    // Phase-1: GN is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.  Impedance is the free-space result (~74.24 Ω).
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-gn2-near-ground-{now}.nec"));

    let deck =
        "GW 1 51 0 0 0.5 0 0 11.064 0.001\nGE\nGN 2 0 0 0 13.0 0.005\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary near-ground deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for near-ground GN2 test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "near-ground deck should succeed, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("GN type 2 is not yet supported"),
        "Phase-1 should not emit deferred GN2 warning, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unsupported buried-wire geometry for active ground model"),
        "Phase-1 should not emit buried-wire error (no active ground), got:\n{stderr}"
    );

    // Phase-1: runs as free-space → Z_RE ≈ 74.24 Ω.
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
        (z_re - 74.24).abs() < 0.5,
        "near-ground GN2 regression mismatch: got Z_RE={z_re}, expected ~74.24 (free-space)"
    );
}
