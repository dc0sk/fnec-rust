use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

mod common;

use common::assert_diag_field;

fn make_dropin_alias_path(alias_name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!("fnec-dropin-alias-{alias_name}-{now}"))
}

fn create_dropin_alias(alias_name: &str) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_BIN_EXE_fnec"));
    let alias = make_dropin_alias_path(alias_name);

    if fs::hard_link(&source, &alias).is_ok() {
        return alias;
    }

    fs::copy(&source, &alias).unwrap_or_else(|e| {
        panic!(
            "failed to create drop-in alias by copy from '{}' to '{}': {e}",
            source.display(),
            alias.display()
        )
    });

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&alias)
            .unwrap_or_else(|e| panic!("failed to read alias metadata '{}': {e}", alias.display()))
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&alias, perms).unwrap_or_else(|e| {
            panic!("failed to mark alias executable '{}': {e}", alias.display())
        });
    }

    alias
}

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
        stderr.contains("warning: --exec hybrid scheduled"),
        "expected hybrid lane fallback warning in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("GPU-candidate lane"),
        "expected GPU-candidate lane warning details in stderr, got:\n{stderr}"
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

#[test]
fn hybrid_exec_mode_accepts_accelerator_stub_dispatch_path() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/frequency-sweep-dipole.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("hybrid")
        .env("FNEC_ACCEL_STUB_GPU", "1")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for hybrid exec stub test: {e}"));

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("GPU-candidate lane"),
        "did not expect GPU-candidate fallback warning in stub path, got:\n{stderr}"
    );
    assert!(
        stderr.contains("accelerator stub backend"),
        "expected accelerator stub warning in stderr, got:\n{stderr}"
    );
    assert_diag_field(&stderr, "exec", "hybrid");

    // Contract remains unchanged: one ordered report block per FR point.
    assert_eq!(stdout.matches("FNEC FEEDPOINT REPORT").count(), 5);
    assert_eq!(stdout.matches("FREQ_MHZ ").count(), 5);
}

#[test]
fn gpu_exec_mode_accepts_accelerator_stub_dispatch_path() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("gpu")
        .env("FNEC_ACCEL_STUB_GPU", "1")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for gpu exec stub test: {e}"));

    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: --exec gpu dispatched to accelerator stub backend"),
        "expected gpu stub dispatch warning in stderr, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("warning: --exec gpu requested"),
        "did not expect gpu fallback-request warning in stub path, got:\n{stderr}"
    );
    assert_diag_field(&stderr, "exec", "gpu(cpu-fallback)");
}

#[test]
fn filename_steering_sets_default_exec_for_dropin_alias() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");
    let alias = create_dropin_alias("nec2dxs500");

    let output = Command::new(&alias)
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "Failed to run drop-in alias '{}' for filename steering test: {e}",
                alias.display()
            )
        });

    let _ = fs::remove_file(&alias);

    assert!(
        output.status.success(),
        "fnec drop-in alias failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("drop-in compatibility profile detected by binary name"),
        "expected compatibility-profile warning in stderr, got:\n{stderr}"
    );
    assert_diag_field(&stderr, "exec", "hybrid");
}

#[test]
fn explicit_exec_overrides_dropin_filename_steering() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");
    let alias = create_dropin_alias("nec2dxs3k0");

    let output = Command::new(&alias)
        .arg("--solver")
        .arg("hallen")
        .arg("--exec")
        .arg("cpu")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "Failed to run drop-in alias '{}' for explicit exec override test: {e}",
                alias.display()
            )
        });

    let _ = fs::remove_file(&alias);

    assert!(
        output.status.success(),
        "fnec drop-in alias failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_diag_field(&stderr, "exec", "cpu");
}
