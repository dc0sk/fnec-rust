use std::path::PathBuf;
use std::process::Command;

fn run_loaded_case(args: &[&str]) -> std::process::Output {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-loaded.nec");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(args).arg(&deck_path);
    cmd.output().unwrap_or_else(|e| {
        panic!(
            "failed to run fnec for loaded case with args {:?}: {e}",
            args
        )
    })
}

fn first_feedpoint_impedance(stdout: &str) -> (f64, f64) {
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

        let z_re = cols[6]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse Z_RE from '{line}': {e}"));
        let z_im = cols[7]
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("failed to parse Z_IM from '{line}': {e}"));
        return (z_re, z_im);
    }
    panic!("no feedpoint rows found in stdout:\n{stdout}");
}

#[test]
fn loaded_case_experimental_hallen_reduces_reactance_error_vs_pulse() {
    // External candidate currently tracked in corpus/reference-results.json.
    let ext_r = 13.4632_f64;
    let ext_x = -896.032_f64;

    let pulse = run_loaded_case(&["--solver", "pulse"]);
    assert!(
        pulse.status.success(),
        "pulse solve failed: {}",
        String::from_utf8_lossy(&pulse.stderr)
    );
    let pulse_out = String::from_utf8_lossy(&pulse.stdout);
    let (pulse_r, pulse_x) = first_feedpoint_impedance(&pulse_out);

    let hallen_exp = run_loaded_case(&["--solver", "hallen", "--allow-noncollinear-hallen"]);
    assert!(
        hallen_exp.status.success(),
        "experimental hallen solve failed: {}",
        String::from_utf8_lossy(&hallen_exp.stderr)
    );
    let hallen_out = String::from_utf8_lossy(&hallen_exp.stdout);
    let (hallen_r, hallen_x) = first_feedpoint_impedance(&hallen_out);

    let pulse_dx = (pulse_x - ext_x).abs();
    let hallen_dx = (hallen_x - ext_x).abs();

    eprintln!(
        "loaded-case tracking: pulse=({:+.6},{:+.6}) hallen_exp=({:+.6},{:+.6}) ext=({:+.6},{:+.6}) dX_pulse={:.6} dX_hallen_exp={:.6}",
        pulse_r, pulse_x, hallen_r, hallen_x, ext_r, ext_x, pulse_dx, hallen_dx
    );

    assert!(
        hallen_dx < pulse_dx,
        "expected experimental hallen to reduce |dX| vs pulse; got dX_hallen_exp={hallen_dx:.6}, dX_pulse={pulse_dx:.6}"
    );
}

#[test]
fn loaded_case_default_hallen_fails_fast_with_noncollinear_error_contract() {
    let out = run_loaded_case(&["--solver", "hallen"]);

    assert_eq!(
        out.status.code(),
        Some(1),
        "default hallen on dipole-loaded should fail fast with exit code 1"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error: Hallén solver currently supports only collinear wire topologies"),
        "expected non-collinear Hallen contract error in stderr, got:\n{stderr}"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("FNEC FEEDPOINT REPORT"),
        "no report should be emitted on Hallen topology hard-fail path, got:\n{stdout}"
    );
}
