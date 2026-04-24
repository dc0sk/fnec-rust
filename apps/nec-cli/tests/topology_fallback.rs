use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn assert_non_single_chain_fallback(solver: &str, expected_diag_mode: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-topology-fallback-{solver}-{now}.nec"));

    // Two disjoint wires (different tags) to ensure the topology is not a single linear chain.
    let deck = "GW 1 11 0.0 0.0 -1.0 0.0 0.0 1.0 0.001\nGW 2 11 0.5 0.0 -1.0 0.5 0.0 1.0 0.001\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary topology-fallback deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg(solver)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for {solver} topology fallback test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "warning: {solver} solver currently supports only single linear chains; falling back to pulse on this topology"
        )),
        "expected topology fallback warning in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains(&format!("diag: mode={expected_diag_mode} ")),
        "expected fallback diag mode '{expected_diag_mode}' in stderr, got:\n{stderr}"
    );
}

#[test]
fn continuity_non_single_chain_falls_back_to_pulse() {
    assert_non_single_chain_fallback("continuity", "continuity->pulse");
}

#[test]
fn sinusoidal_non_single_chain_falls_back_to_pulse() {
    assert_non_single_chain_fallback("sinusoidal", "sinusoidal->pulse");
}
