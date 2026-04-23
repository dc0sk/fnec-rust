use std::path::PathBuf;
use std::process::Command;

#[test]
fn report_contract_v1_headers_and_rows() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for report contract test: {e}"));

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("FNEC FEEDPOINT REPORT\n"));
    assert!(stdout.contains("FORMAT_VERSION 1\n"));
    assert!(stdout.contains("FREQ_MHZ "));
    assert!(stdout.contains("SOLVER_MODE hallen\n"));
    assert!(stdout.contains("PULSE_RHS Nec2\n"));
    assert!(stdout.contains("FEEDPOINTS\n"));
    assert!(stdout.contains("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM\n"));

    let mut data_rows = 0usize;
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
        for value in cols.iter().skip(2) {
            assert!(
                value.parse::<f64>().is_ok(),
                "Expected numeric value in report row, got '{value}' in line '{line}'"
            );
        }
        data_rows += 1;
    }

    assert!(
        data_rows > 0,
        "Expected at least one numeric feedpoint data row in stdout:\n{stdout}"
    );
}
