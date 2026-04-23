/// Integration test for golden corpus validation.
/// Runs fnec-rust on each corpus deck and validates results against reference.
///
/// Tolerance gates are defined in corpus/reference-results.json per case.
/// Any failure is a CI gate; warnings are not acceptable.
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

#[test]
#[ignore] // Skip by default; run with `cargo test -p nec-cli --test corpus_validation -- --ignored`
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

        // For now, validate only scalar feedpoint refs (real_ohm/imag_ohm at case level).
        // Nested per-frequency/per-source refs are tracked but validated in a later step.
        let feed = case_obj
            .get("feedpoint_impedance")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("case '{case_name}' missing 'feedpoint_impedance' object"));

        let expected_real = feed.get("real_ohm").and_then(Value::as_f64);
        let expected_imag = feed.get("imag_ohm").and_then(Value::as_f64);

        let (expected_real, expected_imag) = match (expected_real, expected_imag) {
            (Some(r), Some(x)) => (r, x),
            _ => {
                skipped += 1;
                continue;
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

        let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .arg("--solver")
            .arg("hallen")
            .arg(&deck_path)
            .output()
            .unwrap_or_else(|e| panic!("Failed to run fnec for case '{case_name}': {e}"));

        if !output.status.success() {
            panic!(
                "fnec failed for case '{}': {}",
                case_name,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let impedance_line = stdout
            .lines()
            .next()
            .unwrap_or_else(|| panic!("fnec produced no output for case '{case_name}'"));

        let parts: Vec<&str> = impedance_line.split_whitespace().collect();
        assert!(
            parts.len() >= 5,
            "Unexpected fnec output format for case '{}': {}",
            case_name,
            impedance_line
        );

        let z_str = parts[parts.len() - 1];
        let (real, imag) = parse_complex_impedance(z_str).unwrap_or_else(|| {
            panic!(
                "Failed to parse impedance for case '{}': {}",
                case_name, z_str
            )
        });

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

        validated += 1;
    }

    assert!(
        validated > 0,
        "No corpus cases with scalar references were validated; checked {} cases",
        cases.len()
    );
    eprintln!("corpus validation summary: validated={validated}, skipped={skipped}");
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
