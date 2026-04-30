use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn crossing_wires_fail_fast_with_actionable_error() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-geometry-crossing-{now}.nec"));

    // Two wires crossing at interior points (origin) are currently unsupported
    // and should fail before solve with an actionable geometry error.
    let deck = "GW 1 11 -1.0 0.0 0.0 1.0 0.0 0.0 0.001\nGW 2 11 0.0 -1.0 0.0 0.0 1.0 0.0 0.001\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary crossing-wires deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--allow-noncollinear-hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for crossing-wires test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "crossing-wire deck should fail, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: unsupported intersecting-wire geometry"),
        "expected intersection geometry error in stderr, got:\n{stderr}"
    );
}

#[test]
fn endpoint_wire_junction_is_not_rejected_as_intersection() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-geometry-endpoint-{now}.nec"));

    // Endpoint junction (shared wire endpoint) is allowed by current geometry
    // diagnostics (not an intersecting-wire error). However, since the two
    // wires are non-collinear and --allow-noncollinear-hallen is silently
    // ignored in Phase-1, the Hallen solver will still reject this geometry
    // with a non-collinear topology error.
    let deck = "GW 1 11 0.0 0.0 0.0 1.0 0.0 0.0 0.001\nGW 2 11 0.0 0.0 0.0 0.0 1.0 0.0 0.001\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary endpoint-junction deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--allow-noncollinear-hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for endpoint-junction test: {e}"));

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // The geometry-diagnostics intersection check should NOT flag this as an
    // intersecting-wire error (the wires meet only at a shared endpoint).
    assert!(
        !stderr.contains("unsupported intersecting-wire geometry"),
        "did not expect intersection geometry error for endpoint join, got:\n{stderr}"
    );
    // Phase-1: --allow-noncollinear-hallen is silently ignored, so this
    // non-collinear geometry is rejected with a collinear-topology error.
    assert!(
        !output.status.success(),
        "expected non-collinear Hallen rejection but command succeeded; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("collinear"),
        "expected collinear-topology error in stderr, got:\n{stderr}"
    );
}

#[test]
fn tiny_source_segment_fails_fast_with_actionable_error() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-geometry-tiny-source-{now}.nec"));

    // Very short source segment (length/radius < 2) is currently deferred and
    // should fail early with an actionable source-risk geometry diagnostic.
    let deck =
        "GW 1 1 0.0 0.0 0.0 0.000001 0.0 0.0 0.001\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary tiny-source deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for tiny-source test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "tiny-source deck should fail, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: unsupported source-risk geometry: EX on tiny segment"),
        "expected source-risk geometry error in stderr, got:\n{stderr}"
    );
}
