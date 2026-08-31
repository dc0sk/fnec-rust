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
// VERIFIES: FR-005 (4nec2-like text reports)
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
    // Phase-2: LD is parsed and applied; the LOADS section appears in the report.
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

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown card 'LD'"),
        "Phase-2: LD should be parsed, not produce unknown-card warning; got:\n{stderr}"
    );

    // Phase-2: LOADS section IS present since LD is parsed and applied.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("LOADS\n"),
        "Phase-2: LOADS section expected in report when LD is parsed, got:\n{stdout}"
    );
}

#[test]
fn report_contract_keeps_operator_tables_ordered_before_sweep_summary() {
    // Phase-2: LD is parsed; LOADS section appears once per frequency point.
    // GE is still unknown-card (not in the parser).
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
    // Phase-2: LOADS section appears once per frequency point (3 total).
    assert_eq!(
        stdout.matches("LOADS\n").count(),
        3,
        "Phase-2: expected one LOADS section per frequency point, got:\n{stdout}"
    );
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
        let currents_idx = block.find("CURRENTS\n").expect("missing CURRENTS");
        let loads_idx = block.find("LOADS\n").expect("missing LOADS in block");
        assert!(
            feed_idx < source_idx && source_idx < loads_idx && loads_idx < currents_idx,
            "expected per-frequency order FEEDPOINTS -> SOURCES -> LOADS -> CURRENTS in block:\n{block}"
        );
    }
}

/// Every report section the binary emits must appear in the CLI guide (FND-086).
///
/// The guide calls the report contract "stable, versioned" and enumerates its
/// sections — but `SWEEP_POINTS` was emitted on every multi-frequency text run
/// and appeared in the guide zero times, so the guide was the one diverged copy
/// of a contract three other places agreed on. A prose enumeration only stays
/// complete while someone remembers to extend it; this makes forgetting fail.
#[test]
fn the_cli_guide_documents_every_report_section_the_binary_emits() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // No single deck emits every section, so union across the classes: a sweep
    // (SWEEP_POINTS), a loaded deck (LOADS), an RP deck (RADIATION_PATTERN) and a
    // plane-wave deck (RECEIVE_PATTERN). Picking one deck and hoping is how the
    // first draft of this test asserted three sections and thought that was all
    // of them.
    let decks = [
        "corpus/frequency-sweep-dipole.nec",
        "corpus/dipole-ld-loaded-51seg.nec",
        "corpus/dipole-freesp-rp-51seg.nec",
        "corpus/dipole-ex1-freesp-51seg.nec",
    ];
    let mut emitted: Vec<String> = Vec::new();
    for deck in decks {
        let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .arg(workspace_root.join(deck))
            .current_dir(&workspace_root)
            .output()
            .expect("failed to run fnec");
        assert!(out.status.success(), "fixture deck {deck} must solve");
        // Section headers are the bare ALL-CAPS lines; data rows and column
        // headers carry spaces, and the banner is three words.
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let l = line.trim_end();
            if !l.is_empty()
                && !l.contains(' ')
                && l.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                && !emitted.iter().any(|e| e == l)
            {
                emitted.push(l.to_string());
            }
        }
    }
    assert!(
        emitted.len() >= 7,
        "expected every report section across these four decks, found {emitted:?} — \
         the extraction is wrong, not the guide"
    );

    let guide = std::fs::read_to_string(workspace_root.join("docs/cli-guide.md"))
        .expect("docs/cli-guide.md is readable");
    for section in &emitted {
        assert!(
            guide.contains(section),
            "the binary emits a `{section}` section that docs/cli-guide.md never mentions. \
             Either document it in the Output format section or stop emitting it."
        );
    }
}

/// Every subcommand in the binary's own usage must be documented (FND-086).
///
/// `fnec worker --stdio` was in neither the guide nor the usage text, despite
/// being one of the project's four shipped artifacts; `fnec project convert` was
/// in the usage and printed by the binary, but appeared in the guide zero times.
#[test]
fn the_cli_guide_documents_every_subcommand_the_usage_advertises() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .current_dir(&workspace_root)
        .output()
        .expect("failed to run fnec");
    let usage =
        String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout);

    // Continuation lines of the usage block name a subcommand: "fnec <word> ...".
    let subcommands: Vec<String> = usage
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("fnec "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase()))
        .map(str::to_string)
        .collect();
    assert!(
        subcommands.len() >= 3,
        "expected the usage text to advertise several subcommands, found {subcommands:?}"
    );

    let guide = std::fs::read_to_string(workspace_root.join("docs/cli-guide.md"))
        .expect("docs/cli-guide.md is readable");
    for sub in &subcommands {
        assert!(
            guide.contains(&format!("fnec {sub}")),
            "the binary advertises `fnec {sub}` in its usage, but docs/cli-guide.md \
             never mentions it"
        );
    }
}
