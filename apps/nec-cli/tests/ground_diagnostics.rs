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
fn ge_reflection_flag_emits_warning() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ge1-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 1\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary GE deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for GE diagnostics test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "warning: GE ground-reflection flag 1 is not yet implemented; geometry reflection is ignored in Phase 1"
        ),
        "expected GE reflection warning in stderr, got:\n{stderr}"
    );
}
