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
    // Phase-2: TL is parsed and applied.  The lossless Z-stamp for Z0=50 Ω,
    // length=0.1 m between the two dipole center segments shifts Z_RE relative
    // to the no-TL two-wire deck.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let tl_path = std::env::temp_dir().join(format!("fnec-tl-linked-{now}.nec"));
    let tl_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 1 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&tl_path, tl_deck).expect("failed to write TL deck");

    let base_path = std::env::temp_dir().join(format!("fnec-tl-base-{now}.nec"));
    let base_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&base_path, base_deck).expect("failed to write base deck");

    let (tl_out, tl_err) = run_fnec(&tl_path, &workspace_root);
    let (base_out, _) = run_fnec(&base_path, &workspace_root);
    let _ = fs::remove_file(&tl_path);
    let _ = fs::remove_file(&base_path);

    // Phase-2: TL is parsed — no unknown-card warning.
    assert!(
        !tl_err.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{tl_err}"
    );
    // TL stamp changes Z_RE relative to no-TL base.
    let (tl_r, _) = first_feedpoint_impedance(&tl_out);
    let (base_r, _) = first_feedpoint_impedance(&base_out);
    assert!(
        (tl_r - base_r).abs() > 0.05,
        "Phase-2: TL card should alter Z_RE (tl={tl_r:.3} vs base={base_r:.3})"
    );
    assert!(tl_r > 0.0, "expected positive R with TL, got {tl_r}");
}

#[test]
fn supported_tl_card_with_nseg_zero_changes_feedpoint_impedance() {
    // Phase-2: TL is parsed.  NSEG=0 is a single-section shorthand and produces
    // the same impedance stamp as NSEG=1.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let tl_path = std::env::temp_dir().join(format!("fnec-tl-linked-nseg0-{now}.nec"));
    let tl_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 0 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&tl_path, tl_deck).expect("failed to write TL deck with NSEG=0");

    let base_path = std::env::temp_dir().join(format!("fnec-tl-base-nseg0-{now}.nec"));
    let base_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&base_path, base_deck).expect("failed to write base deck");

    let (tl_out, tl_err) = run_fnec(&tl_path, &workspace_root);
    let (base_out, _) = run_fnec(&base_path, &workspace_root);
    let _ = fs::remove_file(&tl_path);
    let _ = fs::remove_file(&base_path);

    assert!(
        !tl_err.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{tl_err}"
    );
    let (tl_r, _) = first_feedpoint_impedance(&tl_out);
    let (base_r, _) = first_feedpoint_impedance(&base_out);
    assert!(
        (tl_r - base_r).abs() > 0.05,
        "Phase-2: TL NSEG=0 should alter Z_RE (tl={tl_r:.3} vs base={base_r:.3})"
    );
    assert!(tl_r > 0.0, "expected positive R with TL, got {tl_r}");
}

#[test]
fn supported_tl_card_with_nseg_gt_one_changes_feedpoint_impedance() {
    // Phase-2: TL is parsed.  NSEG=3 uses uniform-line stamp semantics
    // identical to NSEG=1 for lossless lines (tl_type=0).
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();

    let tl_path = std::env::temp_dir().join(format!("fnec-tl-linked-nseg3-{now}.nec"));
    let tl_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 3 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&tl_path, tl_deck).expect("failed to write TL deck with NSEG=3");

    let base_path = std::env::temp_dir().join(format!("fnec-tl-base-nseg3-{now}.nec"));
    let base_deck = "GW 1 51 0.0 0 -5.282 0.0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&base_path, base_deck).expect("failed to write base deck");

    let (tl_out, tl_err) = run_fnec(&tl_path, &workspace_root);
    let (base_out, _) = run_fnec(&base_path, &workspace_root);
    let _ = fs::remove_file(&tl_path);
    let _ = fs::remove_file(&base_path);

    assert!(
        !tl_err.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{tl_err}"
    );
    let (tl_r, _) = first_feedpoint_impedance(&tl_out);
    let (base_r, _) = first_feedpoint_impedance(&base_out);
    assert!(
        (tl_r - base_r).abs() > 0.05,
        "Phase-2: TL NSEG=3 should alter Z_RE (tl={tl_r:.3} vs base={base_r:.3})"
    );
    assert!(tl_r > 0.0, "expected positive R with TL, got {tl_r}");
}
