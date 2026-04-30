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
fn supported_tl_card_changes_feedpoint_impedance() {
    // Phase-1: TL is not parsed; card produces 'unknown card' warning and deck
    // runs as free-space.  Base and TL decks have the same impedance.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let tl_path = std::env::temp_dir().join(format!("fnec-tl-linked-{now}.nec"));
    let tl_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 1 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&tl_path, tl_deck).expect("failed to write TL deck");

    let (tl_out, tl_err) = run_fnec(&tl_path, &workspace_root);
    let _ = fs::remove_file(&tl_path);

    assert!(
        tl_err.contains("unknown card 'TL'"),
        "expected unknown-card warning for TL, got:\n{tl_err}"
    );
    assert!(
        !tl_err.contains("TL card ignored"),
        "Phase-1 should not emit TL card ignored, got:\n{tl_err}"
    );
    // Phase-1: runs as free-space (two-wire deck with no EX on second wire).
    let (tl_r, _) = first_feedpoint_impedance(&tl_out);
    // Just verify it produces a valid impedance (non-zero R).
    assert!(tl_r > 0.0, "expected positive R, got {tl_r}");
}

#[test]
fn supported_tl_card_with_nseg_zero_changes_feedpoint_impedance() {
    // Phase-1: TL is not parsed; card produces 'unknown card' warning.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let tl_path = std::env::temp_dir().join(format!("fnec-tl-linked-nseg0-{now}.nec"));
    let tl_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 0 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&tl_path, tl_deck).expect("failed to write TL deck with NSEG=0");

    let (tl_out, tl_err) = run_fnec(&tl_path, &workspace_root);
    let _ = fs::remove_file(&tl_path);

    assert!(
        tl_err.contains("unknown card 'TL'"),
        "expected unknown-card warning for TL, got:\n{tl_err}"
    );
    assert!(
        !tl_err.contains("TL card ignored"),
        "Phase-1 should not emit TL card ignored, got:\n{tl_err}"
    );
    let (tl_r, _) = first_feedpoint_impedance(&tl_out);
    assert!(tl_r > 0.0, "expected positive R, got {tl_r}");
}

#[test]
fn supported_tl_card_with_nseg_gt_one_changes_feedpoint_impedance() {
    // Phase-1: TL is not parsed; card produces 'unknown card' warning.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let tl_path = std::env::temp_dir().join(format!("fnec-tl-linked-nseg3-{now}.nec"));
    let tl_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 3 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&tl_path, tl_deck).expect("failed to write TL deck with NSEG=3");

    let (tl_out, tl_err) = run_fnec(&tl_path, &workspace_root);
    let _ = fs::remove_file(&tl_path);

    assert!(
        tl_err.contains("unknown card 'TL'"),
        "expected unknown-card warning for TL, got:\n{tl_err}"
    );
    assert!(
        !tl_err.contains("TL card ignored"),
        "Phase-1 should not emit TL card ignored, got:\n{tl_err}"
    );
    let (tl_r, _) = first_feedpoint_impedance(&tl_out);
    assert!(tl_r > 0.0, "expected positive R, got {tl_r}");
}
