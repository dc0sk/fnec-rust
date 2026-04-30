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
    // Phase-2: LD is parsed and applied.  LD type 4 (series impedance R=100, X=50)
    // stamps directly into the Z matrix at seg 26, shifting Z_RE well above free-space.
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

    // Phase-2: LD is parsed — no unknown-card warning.
    assert!(
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    // LD type 4 (R=100, X=50) shifts Z_RE to ~706.7 Ω (vs free-space ~74.2 Ω).
    let (loaded_r, loaded_x) = first_feedpoint_impedance(&loaded_out);
    assert!(
        (loaded_r - 706.724).abs() < 1.0,
        "Phase-2 LD4 Z_RE should be ~706.7 Ω, got {loaded_r}"
    );
    assert!(
        (loaded_x - 498.586).abs() < 1.0,
        "Phase-2 LD4 Z_IM should be ~498.6 Ω, got {loaded_x}"
    );
}

#[test]
fn unsupported_ld_type_emits_warning_and_continues() {
    // Phase-2: LD is parsed.  LD type 9 is still not implemented in the solver;
    // the solver emits "LD type 9 on tag 1 is not yet supported; load ignored"
    // and the deck runs without the load (free-space impedance).
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

    // Phase-2: no more generic "unknown card 'LD'" — the card is parsed.
    assert!(
        !stderr.contains("unknown card 'LD'"),
        "Phase-2: LD card should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    // The solver emits a specific warning for unsupported type 9.
    assert!(
        stderr.contains("LD type 9 on tag 1 is not yet supported; load ignored"),
        "expected solver warning for unsupported LD type 9, got:\n{stderr}"
    );
}

#[test]
fn ld_type1_parallel_r_is_supported_and_changes_impedance() {
    // Phase-2: LD type 1 (parallel RLC) is parsed and applied.
    // R=1000 Ω parallel raises Z_RE to ~7072 Ω (vs free-space ~74.2 Ω).
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
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    let (loaded_r, _) = first_feedpoint_impedance(&loaded_out);
    assert!(
        (loaded_r - 7072.840).abs() < 5.0,
        "Phase-2 LD1 (R=1000 Ω parallel) Z_RE should be ~7072.8 Ω, got {loaded_r}"
    );
}

#[test]
fn ld_type2_series_rl_is_supported_and_changes_impedance() {
    // Phase-2: LD type 2 (series RL) is parsed and applied.
    // R=10 Ω, L=1 µH at 14.2 MHz: X_L = ωL ≈ 89.1 Ω added to seg 26.
    // Z_IM shifts from ~13.9 Ω (free-space) to ~651.8 Ω.
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
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    let (_, loaded_x) = first_feedpoint_impedance(&loaded_out);
    assert!(
        (loaded_x - 651.799).abs() < 2.0,
        "Phase-2 LD2 (series RL) Z_IM should be ~651.8 Ω, got {loaded_x}"
    );
}

#[test]
fn ld_type3_series_rc_is_supported_and_changes_impedance() {
    // Phase-2: LD type 3 (series RC) is parsed and applied.
    // R=10 Ω, C=1 pF at 14.2 MHz: X_C = 1/(ωC) ≈ 11.2 kΩ — highly capacitive load.
    // Z_IM shifts to ~ -78413 Ω (strongly capacitive).
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
        !loaded_err.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{loaded_err}"
    );
    let (_, loaded_x) = first_feedpoint_impedance(&loaded_out);
    // Strongly capacitive: Z_IM << 0
    assert!(
        loaded_x < -1000.0,
        "Phase-2 LD3 (series RC, 1 pF) Z_IM should be strongly negative (got {loaded_x})"
    );
}
