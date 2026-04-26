use std::path::PathBuf;
use std::process::Command;

mod common;

use common::{assert_diag_field_is_finite_nonnegative, diag_mode};

fn run_solver(deck_rel: &str, solver: &str) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join(deck_rel);

    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg(solver)
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec ({solver}) on {deck_rel}: {e}"))
}

fn parse_impedance_lines(stdout: &str) -> Vec<(f64, f64)> {
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        if parts[0].parse::<usize>().is_err() {
            continue;
        }
        if let (Ok(z_re), Ok(z_im)) = (parts[6].parse::<f64>(), parts[7].parse::<f64>()) {
            rows.push((z_re, z_im));
        }
    }
    rows
}

fn compare_impedance_vectors(case_name: &str, hallen: &[(f64, f64)], sinusoidal: &[(f64, f64)]) {
    assert_eq!(
        hallen.len(),
        sinusoidal.len(),
        "case '{}' impedance row count mismatch: hallen={} sinusoidal={}",
        case_name,
        hallen.len(),
        sinusoidal.len()
    );

    for (idx, ((h_r, h_x), (s_r, s_x))) in hallen.iter().zip(sinusoidal.iter()).enumerate() {
        let err_r = (h_r - s_r).abs();
        let err_x = (h_x - s_x).abs();
        let tol_r = 0.5f64.max(h_r.abs() * 0.02);
        let tol_x = 0.5f64.max(h_x.abs() * 0.02);

        assert!(
            err_r <= tol_r,
            "case '{}' row {} R drift too high: hallen={:.6} sinusoidal={:.6} err={:.6} tol={:.6}",
            case_name,
            idx + 1,
            h_r,
            s_r,
            err_r,
            tol_r
        );
        assert!(
            err_x <= tol_x,
            "case '{}' row {} X drift too high: hallen={:.6} sinusoidal={:.6} err={:.6} tol={:.6}",
            case_name,
            idx + 1,
            h_x,
            s_x,
            err_x,
            tol_x
        );
    }
}

#[test]
fn sinusoidal_tracks_hallen_on_reference_dipole() {
    let hallen_out = run_solver("corpus/dipole-freesp-51seg.nec", "hallen");
    assert!(
        hallen_out.status.success(),
        "hallen failed: {}",
        String::from_utf8_lossy(&hallen_out.stderr)
    );

    let sinusoidal_out = run_solver("corpus/dipole-freesp-51seg.nec", "sinusoidal");
    assert!(
        sinusoidal_out.status.success(),
        "sinusoidal failed: {}",
        String::from_utf8_lossy(&sinusoidal_out.stderr)
    );

    let hallen_rows = parse_impedance_lines(&String::from_utf8_lossy(&hallen_out.stdout));
    let sinusoidal_rows = parse_impedance_lines(&String::from_utf8_lossy(&sinusoidal_out.stdout));
    compare_impedance_vectors("dipole-freesp-51seg", &hallen_rows, &sinusoidal_rows);

    let stderr = String::from_utf8_lossy(&sinusoidal_out.stderr);
    let mode = diag_mode(&stderr).unwrap_or("<missing>");
    assert!(
        mode.starts_with("sinusoidal"),
        "expected sinusoidal-mode diagnostic label, got '{}', stderr:\n{}",
        mode,
        stderr
    );
    assert_diag_field_is_finite_nonnegative(&stderr, "diag_spread");
}

#[test]
fn sinusoidal_tracks_hallen_on_frequency_sweep_dipole() {
    let hallen_out = run_solver("corpus/frequency-sweep-dipole.nec", "hallen");
    assert!(
        hallen_out.status.success(),
        "hallen failed: {}",
        String::from_utf8_lossy(&hallen_out.stderr)
    );

    let sinusoidal_out = run_solver("corpus/frequency-sweep-dipole.nec", "sinusoidal");
    assert!(
        sinusoidal_out.status.success(),
        "sinusoidal failed: {}",
        String::from_utf8_lossy(&sinusoidal_out.stderr)
    );

    let hallen_rows = parse_impedance_lines(&String::from_utf8_lossy(&hallen_out.stdout));
    let sinusoidal_rows = parse_impedance_lines(&String::from_utf8_lossy(&sinusoidal_out.stdout));
    compare_impedance_vectors("frequency-sweep-dipole", &hallen_rows, &sinusoidal_rows);

    let stderr = String::from_utf8_lossy(&sinusoidal_out.stderr);
    let mode = diag_mode(&stderr).unwrap_or("<missing>");
    assert!(
        mode.starts_with("sinusoidal"),
        "expected sinusoidal-mode diagnostic label, got '{}', stderr:\n{}",
        mode,
        stderr
    );
    assert_diag_field_is_finite_nonnegative(&stderr, "diag_spread");
}
