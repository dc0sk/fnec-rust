use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn unknown_card_emits_parser_warning_but_run_succeeds() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-unknown-card-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nZZ 123\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with unknown card");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for parser warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: line 2: unknown card 'ZZ'"),
        "expected parser warning in stderr, got:\n{stderr}"
    );
}

#[test]
fn supported_tl_card_runs_without_deferred_warning() {
    // Phase-2: TL type=0 is parsed and applied.  No unknown-card, no deferred,
    // no ignored warnings.  Deck runs successfully.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-tl-card-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 1 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with TL card");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for TL warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("TL card support is deferred"),
        "unexpected deferred TL warning in stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("TL card ignored"),
        "unexpected TL ignored warning for TL card:\n{stderr}"
    );
    // Phase-2: TL is parsed; no unknown-card warning.
    assert!(
        !stderr.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
}

#[test]
fn unsupported_tl_type_emits_warning_but_run_succeeds() {
    // Phase-2: TL is parsed.  TL type=1 is not yet implemented in the solver;
    // it emits "TL type 1 ... is not yet supported; TL card ignored".
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-tl-unsupported-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 1 1 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with unsupported TL card");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for unsupported TL warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains(
            "TL type 1 between (1, 26) and (2, 26) is not yet supported; TL card ignored"
        ),
        "expected solver-level warning for unsupported TL type 1, got:\n{stderr}"
    );
}

#[test]
fn tl_segment_zero_is_mapped_to_center_with_warning_and_runs() {
    // Phase-2: TL is parsed.  seg=0 on an odd-segment wire is mapped to the
    // center segment with a diagnostic; the TL stamp is applied.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-tl-seg0-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 0 2 0 1 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with TL segment 0");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for TL segment0 warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains("interpreting segment 0 as center segment"),
        "expected segment-0 center-mapping warning for TL, got:\n{stderr}"
    );
}

#[test]
fn tl_segment_zero_even_segment_count_warns_lower_center_selection() {
    // Phase-2: TL is parsed.  seg=0 on even-segment wires → lower-center
    // selection warning emitted by the solver.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-tl-seg0-even-{now}.nec"));

    let deck = "GW 1 52 0 0 -5.282 0 0 5.282 0.001\nGW 2 52 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 0 2 0 1 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck)
        .expect("failed to write temporary deck with even-segment TL segment 0");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| {
            panic!("Failed to run fnec for even-segment TL segment0 warning test: {e}")
        });

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains("tag has even segment count 52; using lower center segment 26"),
        "expected lower-center-segment warning for even-count TL seg=0, got:\n{stderr}"
    );
}

#[test]
fn tl_nseg_zero_runs_without_ignored_warning() {
    // Phase-2: TL is parsed.  NSEG=0 is a valid single-section shorthand;
    // no "TL card ignored" and no unknown-card warning.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-tl-nseg0-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1.0 0 -5.282 1.0 0 5.282 0.001\nTL 1 26 2 26 0 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with TL NSEG=0");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for TL NSEG=0 warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("TL card ignored"),
        "Phase-2: TL NSEG=0 should not emit TL card ignored:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'TL'"),
        "Phase-2: TL should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
}

#[test]
fn pt_card_emits_deferred_warning_but_run_succeeds() {
    // PT is now parsed; emits "PT card support is currently deferred" warning
    // from warn_pt_card_deferred_support; deck runs as free-space.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-pt-card-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nPT 0 1 26 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with PT card");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for PT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown card 'PT'"),
        "PT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains("PT card support is currently deferred"),
        "Phase-1 should not emit old deferred PT warning, got:\n{stderr}"
    );
}

#[test]
fn nt_card_emits_deferred_warning_but_run_succeeds() {
    // Phase-2: NT is parsed; emits "NT card support is currently deferred" warning
    // from warn_nt_card_deferred_support; deck runs as free-space.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-nt-card-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nNT 1 1 26 1 1 26 50.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with NT card");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for NT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NT card support is currently deferred"),
        "expected deferred-support warning for NT, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'NT'"),
        "Phase-2: NT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
}

#[test]
fn pt_and_nt_cards_emit_deferred_warnings_and_run_succeeds() {
    // PT+NT both parsed; both emit deferred-support warnings.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-pt-nt-card-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nPT 0 1 26 0 50.0 0.1 1.0\nNT 1 1 26 1 1 26 50.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with PT and NT cards");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for PT+NT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown card 'PT'"),
        "PT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains("NT card support is currently deferred"),
        "expected deferred-support warning for NT, got:\n{stderr}"
    );
    assert!(
        stderr.contains("PT card support is currently deferred"),
        "expected deferred-support warning for PT, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'NT'"),
        "Phase-2: NT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
}

#[test]
fn repeated_pt_and_nt_cards_emit_deduplicated_warnings_per_family() {
    // PT+NT both parsed; both emit deferred-support warnings.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-pt-nt-repeated-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nPT 0 1 26 0 50.0 0.1 1.0\nPT 0 1 26 0 75.0 0.2 1.0\nNT 1 1 26 1 1 26 50.0 0.0\nNT 1 1 26 1 1 26 75.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck)
        .expect("failed to write temporary deck with repeated PT and NT cards");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for repeated PT+NT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("PT card support is currently deferred"),
        "expected deferred-support warning for PT, got:\n{stderr}"
    );
    assert!(
        stderr.contains("NT card support is currently deferred"),
        "expected deferred-support warning for NT, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'PT'"),
        "PT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'NT'"),
        "Phase-2: NT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
}

#[test]
fn nt_then_pt_cards_emit_deferred_warnings_and_run_succeeds() {
    // NT+PT both parsed; both emit deferred-support warnings.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-nt-pt-card-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nNT 1 1 26 1 1 26 50.0 0.0\nPT 0 1 26 0 50.0 0.1 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with NT then PT cards");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for NT+PT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NT card support is currently deferred"),
        "expected deferred-support warning for NT, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'PT'"),
        "PT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'NT'"),
        "Phase-2: NT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains("PT card support is currently deferred"),
        "expected deferred-support warning for PT, got:\n{stderr}"
    );
}

#[test]
fn repeated_nt_and_pt_cards_emit_deduplicated_warnings_per_family() {
    // NT+PT both parsed; both emit deferred-support warnings.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-nt-pt-repeated-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nNT 1 1 26 1 1 26 50.0 0.0\nNT 1 1 26 1 1 26 75.0 0.0\nPT 0 1 26 0 50.0 0.1 1.0\nPT 0 1 26 0 75.0 0.2 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck)
        .expect("failed to write temporary deck with repeated NT and PT cards");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for repeated NT+PT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NT card support is currently deferred"),
        "expected deferred-support warning for NT, got:\n{stderr}"
    );
    assert!(
        stderr.contains("PT card support is currently deferred"),
        "expected deferred-support warning for PT, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'NT'"),
        "Phase-2: NT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'PT'"),
        "PT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
}

#[test]
fn interleaved_pt_and_nt_cards_emit_deduplicated_warnings_per_family() {
    // Interleaved PT+NT; both parsed and both emit deferred-support warnings.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-pt-nt-interleaved-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nPT 0 1 26 0 50.0 0.1 1.0\nNT 1 1 26 1 1 26 50.0 0.0\nPT 0 1 26 0 75.0 0.2 1.0\nNT 1 1 26 1 1 26 75.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck)
        .expect("failed to write temporary deck with interleaved PT and NT cards");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for interleaved PT+NT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown card 'PT'"),
        "PT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains("NT card support is currently deferred"),
        "expected deferred-support warning for NT, got:\n{stderr}"
    );
    assert!(
        stderr.contains("PT card support is currently deferred"),
        "expected deferred-support warning for PT, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'NT'"),
        "Phase-2: NT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
}

#[test]
fn interleaved_nt_and_pt_cards_emit_deduplicated_warnings_per_family() {
    // Interleaved NT+PT; both parsed and both emit deferred-support warnings.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-nt-pt-interleaved-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nNT 1 1 26 1 1 26 50.0 0.0\nPT 0 1 26 0 50.0 0.1 1.0\nNT 1 1 26 1 1 26 75.0 0.0\nPT 0 1 26 0 75.0 0.2 1.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck)
        .expect("failed to write temporary deck with interleaved NT and PT cards");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for interleaved NT+PT warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NT card support is currently deferred"),
        "expected deferred-support warning for NT, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'PT'"),
        "PT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unknown card 'NT'"),
        "Phase-2: NT should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );
    assert!(
        stderr.contains("PT card support is currently deferred"),
        "expected deferred-support warning for PT, got:\n{stderr}"
    );
}

#[test]
fn ex_type3_runs_without_unsupported_error() {
    // Phase-1: EX type 3 is not yet supported; deck fails with "is not yet supported".
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type3-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 3 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with EX type 3");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type3 warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    // Phase-1: EX type 3 is rejected.
    assert!(
        !output.status.success(),
        "Phase-1: EX type 3 should be rejected as not yet supported"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("is not yet supported"),
        "expected unsupported error for EX type 3, got stderr:\n{stderr}"
    );
}

#[test]
fn ex_type1_plane_wave_solves_without_legacy_warning() {
    // PH8-CHK-002: EX type 1 (incident plane wave) solves on --solver hallen and
    // must not emit the old "treated like EX type 0" portability warning.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type1-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 1 1 1 0 30.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with EX type 1");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type1 test: {e}"));

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "PH8-CHK-002: EX type 1 plane wave should solve on hallen; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("is not yet supported"),
        "EX type 1 plane wave must not be rejected, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("EX type 1 is currently treated like EX type 0"),
        "must not emit old portability warning, got stderr:\n{stderr}"
    );
}

#[test]
fn ex_type1_pulse_runs_without_portability_warning() {
    // Phase-1: EX type 1 is not yet supported under pulse solver either.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type1-pulse-{now}.nec"));

    let deck =
        "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 1 1 1 0 30.0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary pulse deck with EX type 1");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("pulse")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type1 pulse warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "PH8-CHK-002: EX type 1 plane wave under --solver pulse should fail fast"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires --solver hallen"),
        "expected 'requires --solver hallen' for EX type 1 pulse, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("EX type 1 is currently treated like EX type 0"),
        "must not emit old portability warning, got stderr:\n{stderr}"
    );
}

#[test]
fn ex_type2_runs_with_portability_warning_without_unsupported_error() {
    // Phase-1: EX type 2 is not yet supported; deck fails.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type2-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 2 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with EX type 2");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type2 warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "Phase-1: EX type 2 should be rejected as not yet supported"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("is not yet supported"),
        "expected unsupported error for EX type 2, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("EX type 2 is currently treated like EX type 0"),
        "Phase-1 should not emit old portability warning, got stderr:\n{stderr}"
    );
}

#[test]
fn ex_type4_runs_with_portability_warning_without_unsupported_error() {
    // Phase-1: EX type 4 is not yet supported; deck fails.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type4-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 4 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with EX type 4");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type4 warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "Phase-1: EX type 4 should be rejected as not yet supported"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("is not yet supported"),
        "expected unsupported error for EX type 4, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("EX type 4 is currently treated like EX type 0"),
        "Phase-1 should not emit old portability warning, got stderr:\n{stderr}"
    );
}

#[test]
fn ex_type5_runs_with_portability_warning_without_unsupported_error() {
    // Phase-1: EX type 5 is not yet supported; deck fails.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type5-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 5 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary deck with EX type 5");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type5 warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "Phase-1: EX type 5 should be rejected as not yet supported"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("is not yet supported"),
        "expected unsupported error for EX type 5, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("EX type 5 is currently treated like EX type 0"),
        "Phase-1 should not emit old portability warning, got stderr:\n{stderr}"
    );
}

#[test]
fn ex_type3_non_default_i4_emits_normalization_warning() {
    // Phase-1: EX type 3 is not yet supported regardless of I4 value; deck fails.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type3-i4-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 3 1 26 1 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck)
        .expect("failed to write temporary deck with EX type 3 non-default I4");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type3 I4 warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "Phase-1: EX type 3 non-default I4 should be rejected as not yet supported"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("is not yet supported"),
        "expected unsupported error for EX type 3, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("EX type 3 with non-default I4 is currently treated like EX type 0"),
        "Phase-1 should not emit old normalization warning, got stderr:\n{stderr}"
    );
}

#[test]
fn ex_type3_non_default_i4_divide_by_i4_mode_emits_experimental_warning() {
    // Phase-1: EX type 3 is not yet supported; --ex3-i4-mode is silently ignored.
    // Deck still fails with "is not yet supported".
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-ex-type3-i4-mode-{now}.nec"));

    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nEX 3 1 26 2 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck)
        .expect("failed to write temporary deck with EX type 3 non-default I4");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .arg("--ex3-i4-mode")
        .arg("divide-by-i4")
        .env("FNEC_ACCEL_STUB_GPU", "0")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for EX type3 I4 mode warning test: {e}"));

    let _ = fs::remove_file(&deck_path);

    // Phase-1: --ex3-i4-mode silently ignored; EX type 3 still rejected.
    assert!(
        !output.status.success(),
        "Phase-1: EX type 3 should be rejected even with --ex3-i4-mode"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("is not yet supported"),
        "expected unsupported error for EX type 3, got stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains(
            "--ex3-i4-mode=divide-by-i4 enables experimental EX type 3 normalization semantics"
        ),
        "Phase-1 should not emit old divide-by-i4 experimental warning, got stderr:\n{stderr}"
    );
}
