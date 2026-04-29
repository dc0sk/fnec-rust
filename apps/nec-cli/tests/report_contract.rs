use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_temp_deck(prefix: &str, body: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-{prefix}-{now}.nec"));
    fs::write(&path, body).expect("failed to write temporary deck");
    path
}

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
    assert!(stdout.contains("SOURCES\n"));
    assert!(stdout.contains("N_SOURCES 1\n"));
    assert!(stdout.contains("TYPE TAG SEG I4 V_RE V_IM\n"));
    assert!(stdout.contains("CURRENTS\n"));
    assert!(stdout.contains("TAG SEG I_RE I_IM I_MAG I_PHASE\n"));

    let feed_idx = stdout.find("FEEDPOINTS\n").expect("missing FEEDPOINTS");
    let source_idx = stdout.find("SOURCES\n").expect("missing SOURCES");
    let currents_idx = stdout.find("CURRENTS\n").expect("missing CURRENTS");
    assert!(
        feed_idx < source_idx && source_idx < currents_idx,
        "expected section order FEEDPOINTS -> SOURCES -> CURRENTS"
    );

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

    // Validate current table rows.
    let mut current_rows = 0usize;
    let mut in_currents = false;
    for line in stdout.lines() {
        if line == "CURRENTS" {
            in_currents = true;
            continue;
        }
        if !in_currents || line == "TAG SEG I_RE I_IM I_MAG I_PHASE" {
            continue;
        }
        // Stop at next section header or blank line.
        if line.is_empty()
            || (line.contains('_') && !line.starts_with(|c: char| c.is_ascii_digit()))
        {
            break;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 6 {
            continue;
        }
        if cols[0].parse::<usize>().is_err() || cols[1].parse::<usize>().is_err() {
            continue;
        }
        for value in cols.iter().skip(2) {
            assert!(
                value.parse::<f64>().is_ok(),
                "Expected numeric value in current row, got '{value}' in line '{line}'"
            );
        }
        current_rows += 1;
    }
    assert!(
        current_rows > 0,
        "Expected at least one current distribution row in stdout:\n{stdout}"
    );
}

#[test]
fn report_contract_includes_radiation_pattern_when_rp_present() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-rp-51seg.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for RP report contract test: {e}"));

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("RADIATION_PATTERN\n"));
    assert!(stdout.contains("N_POINTS 19\n"));
    assert!(stdout.contains("THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO\n"));
    assert!(stdout.contains("0.0000 0.0000 -999.9900"));
    assert!(stdout.contains("90.0000 0.0000"));
}

#[test]
fn report_contract_includes_sweep_points_table_for_multi_frequency_runs() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/frequency-sweep-dipole.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for sweep report contract test: {e}"));

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SWEEP_POINTS\n"));
    assert!(stdout.contains("N_POINTS 5\n"));
    assert!(stdout.contains("FREQ_MHZ TAG SEG Z_RE Z_IM\n"));

    let mut in_sweep = false;
    let mut sweep_rows = 0usize;
    let mut freqs: Vec<f64> = Vec::new();
    for line in stdout.lines() {
        if line == "SWEEP_POINTS" {
            in_sweep = true;
            continue;
        }
        if !in_sweep {
            continue;
        }
        if line.starts_with("N_POINTS") || line == "FREQ_MHZ TAG SEG Z_RE Z_IM" {
            continue;
        }
        if line.is_empty() {
            break;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 5 {
            continue;
        }
        let freq = cols[0]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("invalid sweep frequency '{}': {e}", cols[0]));
        cols[1]
            .parse::<usize>()
            .unwrap_or_else(|e| panic!("invalid sweep tag '{}': {e}", cols[1]));
        cols[2]
            .parse::<usize>()
            .unwrap_or_else(|e| panic!("invalid sweep segment '{}': {e}", cols[2]));
        cols[3]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("invalid sweep Z_RE '{}': {e}", cols[3]));
        cols[4]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("invalid sweep Z_IM '{}': {e}", cols[4]));
        freqs.push(freq);
        sweep_rows += 1;
    }

    assert_eq!(sweep_rows, 5, "expected 5 sweep rows, got {sweep_rows}");
    assert_eq!(freqs, vec![10.0, 12.0, 14.0, 16.0, 18.0]);
}

#[test]
fn report_contract_includes_load_table_when_ld_cards_exist() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-ld-series-rl-51seg.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for load-table contract test: {e}"));

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("LOADS\n"));
    assert!(stdout.contains("N_LOADS 1\n"));
    assert!(stdout.contains("TYPE TAG SEG_FIRST SEG_LAST F1 F2 F3\n"));

    let source_idx = stdout.find("SOURCES\n").expect("missing SOURCES");
    let load_idx = stdout.find("LOADS\n").expect("missing LOADS");
    let currents_idx = stdout.find("CURRENTS\n").expect("missing CURRENTS");
    assert!(
        source_idx < load_idx && load_idx < currents_idx,
        "expected section order SOURCES -> LOADS -> CURRENTS"
    );
}

#[test]
fn report_contract_keeps_operator_tables_ordered_before_sweep_summary() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck = "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nLD 2 1 26 26 5.0 1e-6 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 3 0 0 14.0 0.1\nEN\n";
    let deck_path = write_temp_deck("report-sweep-load-order", deck);

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for sweep/load report contract test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let block_starts: Vec<usize> = stdout
        .match_indices("FNEC FEEDPOINT REPORT\n")
        .map(|(idx, _)| idx)
        .collect();
    assert_eq!(
        block_starts.len(),
        3,
        "expected one full report block per frequency point, got:\n{stdout}"
    );
    assert_eq!(stdout.matches("SOURCES\n").count(), 3);
    assert_eq!(stdout.matches("LOADS\n").count(), 3);
    assert_eq!(stdout.matches("CURRENTS\n").count(), 3);

    let sweep_idx = stdout.find("SWEEP_POINTS\n").expect("missing SWEEP_POINTS");
    assert!(stdout.contains("N_POINTS 3\n"));
    assert!(
        sweep_idx > stdout.rfind("CURRENTS\n").expect("missing final CURRENTS"),
        "expected SWEEP_POINTS after the last per-frequency report block"
    );

    for (index, start) in block_starts.iter().enumerate() {
        let end = block_starts.get(index + 1).copied().unwrap_or(sweep_idx);
        let block = &stdout[*start..end];
        let feed_idx = block.find("FEEDPOINTS\n").expect("missing FEEDPOINTS");
        let source_idx = block.find("SOURCES\n").expect("missing SOURCES");
        let load_idx = block.find("LOADS\n").expect("missing LOADS");
        let currents_idx = block.find("CURRENTS\n").expect("missing CURRENTS");
        assert!(
            feed_idx < source_idx && source_idx < load_idx && load_idx < currents_idx,
            "expected per-frequency order FEEDPOINTS -> SOURCES -> LOADS -> CURRENTS in block:\n{block}"
        );
    }
}
