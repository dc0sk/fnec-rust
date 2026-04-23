/// Integration test for golden corpus validation.
/// Runs fnec-rust on each corpus deck and validates results against reference.
/// 
/// Tolerance gates are defined in corpus/reference-results.json per case.
/// Any failure is a CI gate; warnings are not acceptable.

use std::path::Path;
use std::process::Command;

#[test]
#[ignore] // Skip by default; run with `cargo test -- --ignored` only after release build and manual reference capture
fn corpus_validation_dipole_freesp() {
    // Validate fnec-rust can parse and solve the free-space dipole.
    // Full validation against xnec2c reference is deferred until reference-results.json is populated with reference data.
    
    let corpus_root = Path::new("corpus");
    let deck_path = corpus_root.join("dipole-freesp-51seg.nec");
    
    assert!(
        deck_path.exists(),
        "Corpus deck not found: {}",
        deck_path.display()
    );
    
    // Run fnec with Hallén solver (binary must be built)
    let output = Command::new("./target/release/fnec")
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .expect("Failed to run fnec; ensure 'cargo build --release' is run first");
    
    if !output.status.success() {
        panic!(
            "fnec failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse impedance from output: expect line like "1      26      1.000000+0.000000j   0.013471-0.002503j   74.242874+13.899516j"
    let impedance_line = stdout
        .lines()
        .next()
        .expect("fnec produced no output");
    
    let parts: Vec<&str> = impedance_line.split_whitespace().collect();
    assert!(parts.len() >= 6, "Unexpected fnec output format: {}", impedance_line);
    
    let z_str = parts[5];
    let (real, imag) = parse_complex_impedance(z_str)
        .unwrap_or_else(|| panic!("Failed to parse impedance: {}", z_str));
    
    // Expected from reference: 74.23 + j13.90 (Python MoM validation in corpus/reference-results.json)
    let expected_real = 74.23;
    let expected_imag = 13.90;
    let tol_abs = 0.05; // From corpus/reference-results.json tolerance_gates
    
    let err_r = (real - expected_real).abs();
    let err_x = (imag - expected_imag).abs();
    
    assert!(
        err_r <= tol_abs,
        "Real part {:.2} outside tolerance ±{:.2}: error {:.3}",
        real,
        tol_abs,
        err_r
    );
    assert!(
        err_x <= tol_abs,
        "Imaginary part {:.2} outside tolerance ±{:.2}: error {:.3}",
        imag,
        tol_abs,
        err_x
    );
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
        (Some(p), None) => (p, true),              // only +
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
