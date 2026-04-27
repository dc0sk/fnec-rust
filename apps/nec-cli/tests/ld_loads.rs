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
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let base_path = std::env::temp_dir().join(format!("fnec-ld-base-{now}.nec"));
    let loaded_path = std::env::temp_dir().join(format!("fnec-ld-loaded-{now}.nec"));

    // Same dipole, once without LD and once with a series Z load on the feed segment.
    let base_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 4 1 26 26 100.0 50.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fs::write(&base_path, base_deck).expect("failed to write base deck");
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (base_out, _) = run_fnec(&base_path, &workspace_root);
    let (loaded_out, _) = run_fnec(&loaded_path, &workspace_root);

    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&loaded_path);

    let (base_r, base_x) = first_feedpoint_impedance(&base_out);
    let (loaded_r, loaded_x) = first_feedpoint_impedance(&loaded_out);

    assert!(
        loaded_r > base_r + 50.0,
        "expected LD type 4 load to increase real impedance significantly: base={base_r:.6}, loaded={loaded_r:.6}"
    );
    assert!(
        loaded_x > base_x + 20.0,
        "expected LD type 4 load to increase reactive impedance significantly: base={base_x:.6}, loaded={loaded_x:.6}"
    );
}

#[test]
fn unsupported_ld_type_emits_warning_and_continues() {
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
        stderr.contains("warning: LD type 9 on tag 1 is not yet supported; load ignored"),
        "expected unsupported LD warning in stderr, got:\n{stderr}"
    );
}

#[test]
fn ld_type1_parallel_r_is_supported_and_changes_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let base_path = std::env::temp_dir().join(format!("fnec-ld1-base-{now}.nec"));
    let loaded_path = std::env::temp_dir().join(format!("fnec-ld1-loaded-{now}.nec"));

    let base_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 1 1 26 26 1000.0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fs::write(&base_path, base_deck).expect("failed to write base deck");
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (base_out, base_err) = run_fnec(&base_path, &workspace_root);
    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);

    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        !loaded_err.contains("warning: LD type 1 on tag 1 is not yet supported; load ignored"),
        "LD type 1 should be supported, got stderr:\n{loaded_err}"
    );
    assert!(
        !base_err.contains("warning: LD type 1 on tag 1 is not yet supported; load ignored"),
        "unexpected unsupported LD1 warning in base run stderr:\n{base_err}"
    );

    let (base_r, _base_x) = first_feedpoint_impedance(&base_out);
    let (loaded_r, _loaded_x) = first_feedpoint_impedance(&loaded_out);

    assert!(
        loaded_r > base_r + 500.0,
        "expected LD type 1 parallel-R equivalent load to change impedance significantly: base={base_r:.6}, loaded={loaded_r:.6}"
    );
}

#[test]
fn ld_type2_series_rl_is_supported_and_changes_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let base_path = std::env::temp_dir().join(format!("fnec-ld2-base-{now}.nec"));
    let loaded_path = std::env::temp_dir().join(format!("fnec-ld2-loaded-{now}.nec"));

    let base_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 2 1 26 26 10.0 1e-6 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fs::write(&base_path, base_deck).expect("failed to write base deck");
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (base_out, _) = run_fnec(&base_path, &workspace_root);
    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);

    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        !loaded_err.contains("warning: LD type 2 on tag 1 is not yet supported; load ignored"),
        "LD type 2 should be supported, got stderr:\n{loaded_err}"
    );

    let (_base_r, base_x) = first_feedpoint_impedance(&base_out);
    let (_loaded_r, loaded_x) = first_feedpoint_impedance(&loaded_out);

    assert!(
        loaded_x > base_x + 20.0,
        "expected LD type 2 series RL load to increase reactive impedance significantly: base={base_x:.6}, loaded={loaded_x:.6}"
    );
}

#[test]
fn ld_type3_series_rc_is_supported_and_changes_impedance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let base_path = std::env::temp_dir().join(format!("fnec-ld3-base-{now}.nec"));
    let loaded_path = std::env::temp_dir().join(format!("fnec-ld3-loaded-{now}.nec"));

    let base_deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    let loaded_deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nLD 3 1 26 26 10.0 0.0 1e-12\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fs::write(&base_path, base_deck).expect("failed to write base deck");
    fs::write(&loaded_path, loaded_deck).expect("failed to write loaded deck");

    let (base_out, _) = run_fnec(&base_path, &workspace_root);
    let (loaded_out, loaded_err) = run_fnec(&loaded_path, &workspace_root);

    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&loaded_path);

    assert!(
        !loaded_err.contains("warning: LD type 3 on tag 1 is not yet supported; load ignored"),
        "LD type 3 should be supported, got stderr:\n{loaded_err}"
    );

    let (_base_r, base_x) = first_feedpoint_impedance(&base_out);
    let (_loaded_r, loaded_x) = first_feedpoint_impedance(&loaded_out);

    assert!(
        loaded_x < base_x - 20.0,
        "expected LD type 3 series RC load to decrease reactive impedance significantly: base={base_x:.6}, loaded={loaded_x:.6}"
    );
}
