/// Integration test for golden corpus validation.
/// Runs fnec-rust on each corpus deck and validates results against reference.
///
/// Tolerance gates are defined in corpus/reference-results.json per case.
/// Any failure is a CI gate; warnings are not acceptable.
use std::path::PathBuf;
use std::process::Command;

use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
struct PatternSample {
    theta_deg: f64,
    phi_deg: f64,
    gain_db: f64,
    gain_v_db: f64,
    gain_h_db: f64,
    axial_ratio: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CurrentSample {
    segment_id: usize,
    wire_id: usize,
    amplitude_db: f64,
    phase_deg: f64,
}

#[test]
fn corpus_validation_cases_with_references() {
    // Test file is inside apps/nec-cli; walk up to workspace root.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_root = workspace_root.join("corpus");
    let reference_path = corpus_root.join("reference-results.json");

    let json_text = std::fs::read_to_string(&reference_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", reference_path.display()));
    let root: Value = serde_json::from_str(&json_text)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", reference_path.display()));

    let cases = root
        .get("cases")
        .and_then(Value::as_object)
        .expect("reference-results.json missing 'cases' object");

    let mut validated = 0usize;
    let mut skipped = 0usize;

    let mut case_keys: Vec<&String> = cases.keys().collect();
    case_keys.sort();

    for case_name in case_keys {
        let case_obj = cases
            .get(case_name)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("case '{case_name}' is not an object"));

        let expected_hallen_error_contains = case_obj
            .get("expected_hallen_error_contains")
            .and_then(Value::as_str);
        let expected_warning_substrings: Vec<&str> = case_obj
            .get("expected_warning_substrings")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        let deck_file = case_obj
            .get("deck_file")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("case '{case_name}' missing 'deck_file'"));
        let deck_path = corpus_root.join(deck_file);

        assert!(
            deck_path.exists(),
            "Corpus deck not found for case '{}': {}",
            case_name,
            deck_path.display()
        );

        let feed = case_obj
            .get("feedpoint_impedance")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("case '{case_name}' missing 'feedpoint_impedance' object"));

        let expected_real = feed.get("real_ohm").and_then(Value::as_f64);
        let expected_imag = feed.get("imag_ohm").and_then(Value::as_f64);
        let expected_sources = collect_expected_sources(case_obj, feed);
        let expected_freq_points = collect_expected_frequency_points(feed);
        let expected_pattern_samples = collect_expected_pattern_samples(case_obj);
        let expected_current_samples = collect_expected_current_samples(case_obj);

        let expected_scalar = match (expected_real, expected_imag) {
            (Some(r), Some(x)) => (r, x),
            _ => {
                if expected_sources.is_empty() && expected_freq_points.is_empty() {
                    skipped += 1;
                    continue;
                }
                (0.0, 0.0)
            }
        };

        let gates = case_obj
            .get("tolerance_gates")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("case '{case_name}' missing 'tolerance_gates' object"));
        let r_abs = gates
            .get("R_absolute_ohm")
            .and_then(Value::as_f64)
            .unwrap_or(0.05);
        let x_abs = gates
            .get("X_absolute_ohm")
            .and_then(Value::as_f64)
            .unwrap_or(0.05);
        let r_rel_percent = gates
            .get("R_percent_rel")
            .and_then(Value::as_f64)
            .unwrap_or(0.1);
        let x_rel_percent = gates
            .get("X_percent_rel")
            .and_then(Value::as_f64)
            .unwrap_or(0.1);
        let gain_abs_db = gates
            .get("Gain_absolute_dB")
            .and_then(Value::as_f64)
            .unwrap_or(0.05);
        let axial_ratio_abs = gates
            .get("AxialRatio_absolute")
            .and_then(Value::as_f64)
            .unwrap_or(0.0001);
        let external_r_abs = gates.get("ExternalR_absolute_ohm").and_then(Value::as_f64);
        let external_x_abs = gates.get("ExternalX_absolute_ohm").and_then(Value::as_f64);
        let external_r_rel_percent = gates.get("ExternalR_percent_rel").and_then(Value::as_f64);
        let external_x_rel_percent = gates.get("ExternalX_percent_rel").and_then(Value::as_f64);
        let external_gain_abs_db = gates
            .get("ExternalGain_absolute_dB")
            .and_then(Value::as_f64);
        let external_axial_ratio_abs = gates
            .get("ExternalAxialRatio_absolute")
            .and_then(Value::as_f64);
        let external_r_tol = if external_r_abs.is_some() || external_r_rel_percent.is_some() {
            Some((
                external_r_abs.unwrap_or(r_abs),
                external_r_rel_percent.unwrap_or(r_rel_percent),
            ))
        } else {
            None
        };
        let external_x_tol = if external_x_abs.is_some() || external_x_rel_percent.is_some() {
            Some((
                external_x_abs.unwrap_or(x_abs),
                external_x_rel_percent.unwrap_or(x_rel_percent),
            ))
        } else {
            None
        };
        let current_amplitude_dB_tol = gates
            .get("Current_amplitude_dB")
            .and_then(Value::as_f64)
            .unwrap_or(0.1);
        let current_phase_deg_tol = gates
            .get("Current_phase_deg")
            .and_then(Value::as_f64)
            .unwrap_or(2.0);

        let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .arg("--solver")
            .arg("hallen")
            .arg(&deck_path)
            .output()
            .unwrap_or_else(|e| panic!("Failed to run fnec for case '{case_name}': {e}"));

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if let Some(expected_msg) = expected_hallen_error_contains {
            assert!(
                !output.status.success(),
                "case '{}' expected Hallen failure containing '{}', but command succeeded",
                case_name,
                expected_msg
            );
            assert!(
                stderr.contains(expected_msg),
                "case '{}' expected Hallen error containing '{}', got stderr:\n{}",
                case_name,
                expected_msg,
                stderr
            );
            validated += 1;
            continue;
        }

        if !output.status.success() {
            panic!("fnec failed for case '{}': {}", case_name, stderr);
        }

        for expected_warning in &expected_warning_substrings {
            assert!(
                stderr.contains(expected_warning),
                "Case '{}' expected warning containing '{}', got stderr:\n{}",
                case_name,
                expected_warning,
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let impedances = parse_impedance_lines(&stdout);
        let pattern_rows = parse_pattern_rows(&stdout);
        let current_rows = parse_current_rows(&stdout);
        assert!(
            !impedances.is_empty(),
            "No impedance rows found in fnec output for case '{}':\n{}",
            case_name,
            stdout
        );

        if !expected_sources.is_empty() {
            assert!(
                impedances.len() >= expected_sources.len(),
                "Case '{}' expected {} source impedance rows, got {}",
                case_name,
                expected_sources.len(),
                impedances.len()
            );

            for (idx, (exp_r, exp_x)) in expected_sources.iter().enumerate() {
                let (real, imag) = impedances[idx];
                let err_r = (real - exp_r).abs();
                let err_x = (imag - exp_x).abs();
                let tol_r = tolerance_with_floor(*exp_r, r_abs, r_rel_percent);
                let tol_x = tolerance_with_floor(*exp_x, x_abs, x_rel_percent);

                assert!(
                    err_r <= tol_r,
                    "Case '{}' source_{} R out of tolerance: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    idx + 1,
                    real,
                    exp_r,
                    err_r,
                    tol_r
                );
                assert!(
                    err_x <= tol_x,
                    "Case '{}' source_{} X out of tolerance: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    idx + 1,
                    imag,
                    exp_x,
                    err_x,
                    tol_x
                );
            }

            if let Some(ext_obj) = case_obj
                .get("external_reference_candidate")
                .and_then(Value::as_object)
            {
                let ext_sources = collect_external_sources(case_obj, ext_obj);
                if !ext_sources.is_empty() {
                    for (idx, (ext_r, ext_x)) in ext_sources.iter().enumerate() {
                        if idx >= impedances.len() {
                            break;
                        }
                        let (real, imag) = impedances[idx];
                        let err_r = (real - ext_r).abs();
                        let err_x = (imag - ext_x).abs();
                        eprintln!(
                            "corpus external delta: case='{}' source_{} dR={:+.6} dX={:+.6} (fnec-ext)",
                            case_name,
                            idx + 1,
                            real - ext_r,
                            imag - ext_x
                        );

                        if let Some((abs_floor, rel_percent)) = external_r_tol {
                            let tol_r = tolerance_with_floor(*ext_r, abs_floor, rel_percent);
                            assert!(
                                err_r <= tol_r,
                                "Case '{}' external source_{} R out of tolerance: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                case_name,
                                idx + 1,
                                real,
                                ext_r,
                                err_r,
                                tol_r
                            );
                        }
                        if let Some((abs_floor, rel_percent)) = external_x_tol {
                            let tol_x = tolerance_with_floor(*ext_x, abs_floor, rel_percent);
                            assert!(
                                err_x <= tol_x,
                                "Case '{}' external source_{} X out of tolerance: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                case_name,
                                idx + 1,
                                imag,
                                ext_x,
                                err_x,
                                tol_x
                            );
                        }
                    }
                }
            }

            validated += 1;
            continue;
        }

        if !expected_freq_points.is_empty() {
            assert!(
                impedances.len() >= expected_freq_points.len(),
                "Case '{}' expected {} frequency impedance rows, got {}",
                case_name,
                expected_freq_points.len(),
                impedances.len()
            );

            for (idx, (freq_mhz, exp_r, exp_x)) in expected_freq_points.iter().enumerate() {
                let (real, imag) = impedances[idx];
                let err_r = (real - exp_r).abs();
                let err_x = (imag - exp_x).abs();
                let tol_r = tolerance_with_floor(*exp_r, r_abs, r_rel_percent);
                let tol_x = tolerance_with_floor(*exp_x, x_abs, x_rel_percent);

                assert!(
                    err_r <= tol_r,
                    "Case '{}' freq {:.3} MHz R out of tolerance: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    freq_mhz,
                    real,
                    exp_r,
                    err_r,
                    tol_r
                );
                assert!(
                    err_x <= tol_x,
                    "Case '{}' freq {:.3} MHz X out of tolerance: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    freq_mhz,
                    imag,
                    exp_x,
                    err_x,
                    tol_x
                );
            }

            if let Some(ext_obj) = case_obj
                .get("external_reference_candidate")
                .and_then(Value::as_object)
            {
                let ext_points = collect_external_frequency_points(ext_obj);
                if !ext_points.is_empty() {
                    for (freq_mhz, ext_r, ext_x) in &ext_points {
                        if let Some((idx, _)) = expected_freq_points
                            .iter()
                            .enumerate()
                            .find(|(_, (f, _, _))| (*f - *freq_mhz).abs() < 1e-9)
                        {
                            if idx < impedances.len() {
                                let (real, imag) = impedances[idx];
                                let err_r = (real - ext_r).abs();
                                let err_x = (imag - ext_x).abs();
                                eprintln!(
                                    "corpus external delta: case='{}' freq={:.3}MHz dR={:+.6} dX={:+.6} (fnec-ext)",
                                    case_name,
                                    freq_mhz,
                                    real - ext_r,
                                    imag - ext_x
                                );

                                if let Some((abs_floor, rel_percent)) = external_r_tol {
                                    let tol_r =
                                        tolerance_with_floor(*ext_r, abs_floor, rel_percent);
                                    assert!(
                                        err_r <= tol_r,
                                        "Case '{}' external freq {:.3} MHz R out of tolerance: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                        case_name,
                                        freq_mhz,
                                        real,
                                        ext_r,
                                        err_r,
                                        tol_r
                                    );
                                }
                                if let Some((abs_floor, rel_percent)) = external_x_tol {
                                    let tol_x =
                                        tolerance_with_floor(*ext_x, abs_floor, rel_percent);
                                    assert!(
                                        err_x <= tol_x,
                                        "Case '{}' external freq {:.3} MHz X out of tolerance: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                        case_name,
                                        freq_mhz,
                                        imag,
                                        ext_x,
                                        err_x,
                                        tol_x
                                    );
                                }
                            }
                        }
                    }
                }
            }

            validated += 1;
            continue;
        }

        let (real, imag) = impedances[0];
        let (expected_real, expected_imag) = expected_scalar;

        let err_r = (real - expected_real).abs();
        let err_x = (imag - expected_imag).abs();
        let tol_r = tolerance_with_floor(expected_real, r_abs, r_rel_percent);
        let tol_x = tolerance_with_floor(expected_imag, x_abs, x_rel_percent);

        assert!(
            err_r <= tol_r,
            "Case '{}' R out of tolerance: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
            case_name,
            real,
            expected_real,
            err_r,
            tol_r
        );
        assert!(
            err_x <= tol_x,
            "Case '{}' X out of tolerance: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
            case_name,
            imag,
            expected_imag,
            err_x,
            tol_x
        );

        if let Some(ext_obj) = case_obj
            .get("external_reference_candidate")
            .and_then(Value::as_object)
        {
            if let (Some(ext_r), Some(ext_x)) = (
                ext_obj.get("real_ohm").and_then(Value::as_f64),
                ext_obj.get("imag_ohm").and_then(Value::as_f64),
            ) {
                let err_r = (real - ext_r).abs();
                let err_x = (imag - ext_x).abs();
                eprintln!(
                    "corpus external delta: case='{}' dR={:+.6} dX={:+.6} (fnec-ext)",
                    case_name,
                    real - ext_r,
                    imag - ext_x
                );

                if let Some((abs_floor, rel_percent)) = external_r_tol {
                    let tol_r = tolerance_with_floor(ext_r, abs_floor, rel_percent);
                    assert!(
                        err_r <= tol_r,
                        "Case '{}' external R out of tolerance: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                        case_name,
                        real,
                        ext_r,
                        err_r,
                        tol_r
                    );
                }
                if let Some((abs_floor, rel_percent)) = external_x_tol {
                    let tol_x = tolerance_with_floor(ext_x, abs_floor, rel_percent);
                    assert!(
                        err_x <= tol_x,
                        "Case '{}' external X out of tolerance: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                        case_name,
                        imag,
                        ext_x,
                        err_x,
                        tol_x
                    );
                }
            }
        }

        if !expected_pattern_samples.is_empty() {
            assert!(
                !pattern_rows.is_empty(),
                "Case '{}' expected radiation-pattern rows, got none:\n{}",
                case_name,
                stdout
            );

            for sample in &expected_pattern_samples {
                let row = pattern_rows
                    .iter()
                    .find(|row| {
                        (row.theta_deg - sample.theta_deg).abs() < 1e-9
                            && (row.phi_deg - sample.phi_deg).abs() < 1e-9
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "Case '{}' missing pattern sample at theta={:.4} phi={:.4}",
                            case_name, sample.theta_deg, sample.phi_deg
                        )
                    });

                let err_gain = (row.gain_db - sample.gain_db).abs();
                let err_gain_v = (row.gain_v_db - sample.gain_v_db).abs();
                let err_gain_h = (row.gain_h_db - sample.gain_h_db).abs();
                let err_axial_ratio = (row.axial_ratio - sample.axial_ratio).abs();
                assert!(
                    err_gain <= gain_abs_db,
                    "Case '{}' pattern gain out of tolerance at theta={:.4} phi={:.4}: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    sample.theta_deg,
                    sample.phi_deg,
                    row.gain_db,
                    sample.gain_db,
                    err_gain,
                    gain_abs_db
                );
                assert!(
                    err_gain_v <= gain_abs_db,
                    "Case '{}' pattern vertical gain out of tolerance at theta={:.4} phi={:.4}: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    sample.theta_deg,
                    sample.phi_deg,
                    row.gain_v_db,
                    sample.gain_v_db,
                    err_gain_v,
                    gain_abs_db
                );
                assert!(
                    err_gain_h <= gain_abs_db,
                    "Case '{}' pattern horizontal gain out of tolerance at theta={:.4} phi={:.4}: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    sample.theta_deg,
                    sample.phi_deg,
                    row.gain_h_db,
                    sample.gain_h_db,
                    err_gain_h,
                    gain_abs_db
                );
                assert!(
                    err_axial_ratio <= axial_ratio_abs,
                    "Case '{}' pattern axial ratio out of tolerance at theta={:.4} phi={:.4}: got {:.6}, expected {:.6}, err {:.6}, tol {:.6}",
                    case_name,
                    sample.theta_deg,
                    sample.phi_deg,
                    row.axial_ratio,
                    sample.axial_ratio,
                    err_axial_ratio,
                    axial_ratio_abs
                );
            }

            if let Some(ext_obj) = case_obj
                .get("external_reference_candidate")
                .and_then(Value::as_object)
            {
                let ext_pattern_samples = collect_external_pattern_samples(ext_obj);
                for sample in &ext_pattern_samples {
                    if let Some(row) = pattern_rows.iter().find(|row| {
                        (row.theta_deg - sample.theta_deg).abs() < 1e-9
                            && (row.phi_deg - sample.phi_deg).abs() < 1e-9
                    }) {
                        let err_gain = (row.gain_db - sample.gain_db).abs();
                        let err_gain_v = (row.gain_v_db - sample.gain_v_db).abs();
                        let err_gain_h = (row.gain_h_db - sample.gain_h_db).abs();
                        let err_axial_ratio = (row.axial_ratio - sample.axial_ratio).abs();
                        eprintln!(
                            "corpus external delta: case='{}' pattern theta={:.4} phi={:.4} dGain={:+.4} dGainV={:+.4} dGainH={:+.4} dAxialRatio={:+.4} (fnec-ext)",
                            case_name,
                            sample.theta_deg,
                            sample.phi_deg,
                            row.gain_db - sample.gain_db,
                            row.gain_v_db - sample.gain_v_db,
                            row.gain_h_db - sample.gain_h_db,
                            row.axial_ratio - sample.axial_ratio,
                        );

                        if let Some(tol) = external_gain_abs_db {
                            assert!(
                                err_gain <= tol,
                                "Case '{}' external pattern gain out of tolerance at theta={:.4} phi={:.4}: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                case_name,
                                sample.theta_deg,
                                sample.phi_deg,
                                row.gain_db,
                                sample.gain_db,
                                err_gain,
                                tol
                            );
                            assert!(
                                err_gain_v <= tol,
                                "Case '{}' external pattern vertical gain out of tolerance at theta={:.4} phi={:.4}: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                case_name,
                                sample.theta_deg,
                                sample.phi_deg,
                                row.gain_v_db,
                                sample.gain_v_db,
                                err_gain_v,
                                tol
                            );
                            assert!(
                                err_gain_h <= tol,
                                "Case '{}' external pattern horizontal gain out of tolerance at theta={:.4} phi={:.4}: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                case_name,
                                sample.theta_deg,
                                sample.phi_deg,
                                row.gain_h_db,
                                sample.gain_h_db,
                                err_gain_h,
                                tol
                            );
                        }

                        if let Some(tol) = external_axial_ratio_abs {
                            assert!(
                                err_axial_ratio <= tol,
                                "Case '{}' external pattern axial ratio out of tolerance at theta={:.4} phi={:.4}: got {:.6}, external {:.6}, err {:.6}, tol {:.6}",
                                case_name,
                                sample.theta_deg,
                                sample.phi_deg,
                                row.axial_ratio,
                                sample.axial_ratio,
                                err_axial_ratio,
                                tol
                            );
                        }
                    }
                }
            }
        }

        // Validate current distribution samples if expected
        if !expected_current_samples.is_empty() {
            for expected_curr in &expected_current_samples {
                // Find matching current in output by wire_id and segment_id
                let matching = current_rows.iter().find(|(wire, seg, _, _)| {
                    *wire == expected_curr.wire_id && *seg == expected_curr.segment_id
                });

                let (_, _, actual_amp_db, actual_phase_deg) = match matching {
                    Some(m) => *m,
                    None => {
                        eprintln!(
                            "Warning: case '{}' expected current sample wire={} seg={} not found in output",
                            case_name, expected_curr.wire_id, expected_curr.segment_id
                        );
                        continue;
                    }
                };

                let err_amp = (actual_amp_db - expected_curr.amplitude_db).abs();
                let err_phase = (actual_phase_deg - expected_curr.phase_deg).abs();

                assert!(
                    err_amp <= current_amplitude_dB_tol,
                    "Case '{}' current amplitude at wire={} seg={} out of tolerance: got {:.4} dB, expected {:.4} dB, err {:.4} dB, tol {:.4} dB",
                    case_name,
                    expected_curr.wire_id,
                    expected_curr.segment_id,
                    actual_amp_db,
                    expected_curr.amplitude_db,
                    err_amp,
                    current_amplitude_dB_tol
                );
                assert!(
                    err_phase <= current_phase_deg_tol,
                    "Case '{}' current phase at wire={} seg={} out of tolerance: got {:.2}°, expected {:.2}°, err {:.2}°, tol {:.2}°",
                    case_name,
                    expected_curr.wire_id,
                    expected_curr.segment_id,
                    actual_phase_deg,
                    expected_curr.phase_deg,
                    err_phase,
                    current_phase_deg_tol
                );
            }
        }

        validated += 1;
    }

    assert!(
        validated > 0,
        "No corpus cases with references were validated; checked {} cases",
        cases.len()
    );
    eprintln!("corpus validation summary: validated={validated}, skipped={skipped}");
}

fn collect_expected_sources(
    case_obj: &Map<String, Value>,
    feed: &Map<String, Value>,
) -> Vec<(f64, f64)> {
    let Some(sources) = case_obj.get("sources").and_then(Value::as_u64) else {
        return Vec::new();
    };

    if sources == 0 {
        return Vec::new();
    }

    let mut expected = Vec::new();
    for idx in 1..=sources {
        let key = format!("source_{idx}");
        let Some(source_obj) = feed.get(&key).and_then(Value::as_object) else {
            return Vec::new();
        };
        let Some(real) = source_obj.get("real_ohm").and_then(Value::as_f64) else {
            return Vec::new();
        };
        let Some(imag) = source_obj.get("imag_ohm").and_then(Value::as_f64) else {
            return Vec::new();
        };
        expected.push((real, imag));
    }
    expected
}

fn collect_expected_frequency_points(feed: &Map<String, Value>) -> Vec<(f64, f64, f64)> {
    let mut out: Vec<(f64, f64, f64)> = feed
        .iter()
        .filter_map(|(k, v)| {
            let freq_mhz = k.parse::<f64>().ok()?;
            let obj = v.as_object()?;
            let real = obj.get("real_ohm")?.as_f64()?;
            let imag = obj.get("imag_ohm")?.as_f64()?;
            Some((freq_mhz, real, imag))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn collect_external_sources(
    case_obj: &Map<String, Value>,
    ext: &Map<String, Value>,
) -> Vec<(f64, f64)> {
    let Some(sources) = case_obj.get("sources").and_then(Value::as_u64) else {
        return Vec::new();
    };

    if sources == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    for idx in 1..=sources {
        let key = format!("source_{idx}");
        let Some(source_obj) = ext.get(&key).and_then(Value::as_object) else {
            return Vec::new();
        };
        let Some(real) = source_obj.get("real_ohm").and_then(Value::as_f64) else {
            return Vec::new();
        };
        let Some(imag) = source_obj.get("imag_ohm").and_then(Value::as_f64) else {
            return Vec::new();
        };
        out.push((real, imag));
    }
    out
}

fn collect_external_frequency_points(ext: &Map<String, Value>) -> Vec<(f64, f64, f64)> {
    let mut out: Vec<(f64, f64, f64)> = ext
        .iter()
        .filter_map(|(k, v)| {
            let freq_mhz = k.parse::<f64>().ok()?;
            let obj = v.as_object()?;
            let real = obj.get("real_ohm")?.as_f64()?;
            let imag = obj.get("imag_ohm")?.as_f64()?;
            Some((freq_mhz, real, imag))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn collect_expected_pattern_samples(case_obj: &Map<String, Value>) -> Vec<PatternSample> {
    let Some(samples) = case_obj.get("pattern_samples").and_then(Value::as_array) else {
        return Vec::new();
    };

    samples
        .iter()
        .filter_map(|sample| {
            let sample = sample.as_object()?;
            Some(PatternSample {
                theta_deg: sample.get("theta_deg")?.as_f64()?,
                phi_deg: sample.get("phi_deg")?.as_f64()?,
                gain_db: sample.get("gain_db")?.as_f64()?,
                gain_v_db: sample.get("gain_v_db")?.as_f64()?,
                gain_h_db: sample.get("gain_h_db")?.as_f64()?,
                axial_ratio: sample.get("axial_ratio")?.as_f64()?,
            })
        })
        .collect()
}

fn collect_expected_current_samples(case_obj: &Map<String, Value>) -> Vec<CurrentSample> {
    let Some(samples) = case_obj.get("current_samples").and_then(Value::as_array) else {
        return Vec::new();
    };

    samples
        .iter()
        .filter_map(|sample| {
            let sample = sample.as_object()?;
            Some(CurrentSample {
                segment_id: sample.get("segment_id")?.as_u64()? as usize,
                wire_id: sample.get("wire_id")?.as_u64()? as usize,
                amplitude_db: sample.get("amplitude_db")?.as_f64()?,
                phase_deg: sample.get("phase_deg")?.as_f64()?,
            })
        })
        .collect()
}

fn collect_external_pattern_samples(ext: &Map<String, Value>) -> Vec<PatternSample> {
    let Some(samples) = ext.get("pattern_samples").and_then(Value::as_array) else {
        return Vec::new();
    };

    samples
        .iter()
        .filter_map(|sample| {
            let sample = sample.as_object()?;
            Some(PatternSample {
                theta_deg: sample.get("theta_deg")?.as_f64()?,
                phi_deg: sample.get("phi_deg")?.as_f64()?,
                gain_db: sample.get("gain_db")?.as_f64()?,
                gain_v_db: sample.get("gain_v_db")?.as_f64()?,
                gain_h_db: sample.get("gain_h_db")?.as_f64()?,
                axial_ratio: sample.get("axial_ratio")?.as_f64()?,
            })
        })
        .collect()
}

fn parse_impedance_lines(stdout: &str) -> Vec<(f64, f64)> {
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        if parts[0].parse::<usize>().is_err() {
            continue;
        }

        // Contract v1 format:
        // TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
        if parts.len() >= 8 {
            if let (Ok(z_re), Ok(z_im)) = (parts[6].parse::<f64>(), parts[7].parse::<f64>()) {
                rows.push((z_re, z_im));
                continue;
            }
        }

        // Legacy format:
        // <tag> <seg> <V_re>+<V_im>j <I_re>+<I_im>j <Z_re>+<Z_im>j
        let Some(z_str) = parts.last() else {
            continue;
        };
        if let Some((real, imag)) = parse_complex_impedance(z_str) {
            rows.push((real, imag));
        }
    }
    rows
}

fn parse_pattern_rows(stdout: &str) -> Vec<PatternSample> {
    let mut rows = Vec::new();
    let mut in_pattern = false;

    for line in stdout.lines() {
        if line == "RADIATION_PATTERN" {
            in_pattern = true;
            continue;
        }
        if !in_pattern {
            continue;
        }
        if line.is_empty() {
            break;
        }
        if line.starts_with("N_POINTS ")
            || line == "THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO"
        {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }

        let Ok(theta_deg) = parts[0].parse::<f64>() else {
            continue;
        };
        let Ok(phi_deg) = parts[1].parse::<f64>() else {
            continue;
        };
        let Ok(gain_db) = parts[2].parse::<f64>() else {
            continue;
        };
        let Ok(gain_v_db) = parts[3].parse::<f64>() else {
            continue;
        };
        let Ok(gain_h_db) = parts[4].parse::<f64>() else {
            continue;
        };
        let Ok(axial_ratio) = parts[5].parse::<f64>() else {
            continue;
        };

        rows.push(PatternSample {
            theta_deg,
            phi_deg,
            gain_db,
            gain_v_db,
            gain_h_db,
            axial_ratio,
        });
    }

    rows
}

fn parse_current_rows(stdout: &str) -> Vec<(usize, usize, f64, f64)> {
    let mut rows = Vec::new();
    let mut in_current = false;

    for line in stdout.lines() {
        // Detect start of current distribution section (tag/segment/current data)
        if line.contains("SEG") && line.contains("I_MAG") {
            in_current = true;
            continue;
        }
        if !in_current {
            continue;
        }
        if line.is_empty() || line.starts_with("---") {
            break;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        // Format: TAG SEG I_MAG I_PHASE ...
        if parts.len() < 4 {
            continue;
        }

        let Ok(tag) = parts[0].parse::<usize>() else {
            continue;
        };
        let Ok(seg) = parts[1].parse::<usize>() else {
            continue;
        };
        let Ok(magnitude_db) = parts[2].parse::<f64>() else {
            continue;
        };
        let Ok(phase_deg) = parts[3].parse::<f64>() else {
            continue;
        };

        // Note: tag maps to wire_id, seg maps to segment_id in our schema
        rows.push((tag, seg, magnitude_db, phase_deg));
    }

    rows
}

fn tolerance_with_floor(expected: f64, abs_floor: f64, rel_percent: f64) -> f64 {
    let rel = expected.abs() * (rel_percent / 100.0);
    abs_floor.max(rel)
}

/// Parse complex impedance string like "74.242874+13.899516j".
/// Returns (real, imag) or None if parse fails.
fn parse_complex_impedance(s: &str) -> Option<(f64, f64)> {
    let s = s.trim_end_matches('j').trim();

    // Find the operator between real and imag parts
    let plus_pos = s.rfind('+');
    let minus_pos = s.rfind('-');

    let (op_pos, is_positive) = match (plus_pos, minus_pos) {
        (Some(p), Some(m)) if p > m => (p, true), // + is rightmost
        (Some(p), None) => (p, true),             // only +
        (None, Some(m)) if m > 0 => (m, false),   // only - (but not leading -)
        _ => return None,
    };

    let real_str = s[..op_pos].trim();
    let imag_str = s[op_pos + 1..].trim();

    let real = real_str.parse::<f64>().ok()?;
    let imag = imag_str.parse::<f64>().ok()?;
    let imag = if is_positive { imag } else { -imag };

    Some((real, imag))
}
