use std::path::PathBuf;
use std::process::Command;

mod common;

use common::assert_diag_field;

#[test]
fn hybrid_exec_mode_runs_frequency_sweep_with_ordered_reports() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/frequency-sweep-dipole.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("hybrid")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for hybrid exec sweep test: {e}"));

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let report_blocks = stdout.matches("FNEC FEEDPOINT REPORT").count();
    let freq_headers = stdout.matches("FREQ_MHZ ").count();
    assert_eq!(
        report_blocks, 5,
        "expected 5 report blocks, got {report_blocks}"
    );
    assert_eq!(
        freq_headers, 5,
        "expected 5 FREQ_MHZ headers, got {freq_headers}"
    );

    assert!(
        !stderr.contains("warning: --exec hybrid requested"),
        "did not expect hybrid fallback warning in stderr, got:\n{stderr}"
    );
    assert_diag_field(&stderr, "exec", "hybrid");

    // Verify report ordering remains ascending by FR sweep points.
    let expected_order = [
        "FREQ_MHZ 10.000000",
        "FREQ_MHZ 12.000000",
        "FREQ_MHZ 14.000000",
        "FREQ_MHZ 16.000000",
        "FREQ_MHZ 18.000000",
    ];

    let mut cursor = 0usize;
    for marker in expected_order {
        let rel = stdout[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing frequency marker '{marker}' in stdout:\n{stdout}"));
        cursor += rel + marker.len();
    }
}
