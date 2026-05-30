use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::symlink;

mod common;

use common::assert_diag_field;

fn test_tmp_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".tmp/nec-cli-tests");
    fs::create_dir_all(&dir).expect("failed to create repo-local test tmp dir");
    dir
}

fn make_dropin_alias_path(alias_name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    test_tmp_dir().join(format!("fnec-dropin-alias-{alias_name}-{now}"))
}

fn create_dropin_alias(alias_name: &str) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_BIN_EXE_fnec"));
    let alias = make_dropin_alias_path(alias_name);

    #[cfg(unix)]
    {
        if symlink(&source, &alias).is_ok() {
            return alias;
        }
    }

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

    alias
}

fn create_sandbox_dir(prefix: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let dir = test_tmp_dir().join(format!("{prefix}-{now}"));
    fs::create_dir_all(&dir).expect("failed to create test sandbox dir");
    dir
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
        .env_remove("FNEC_ACCEL_STUB_GPU")
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
    let freq_headers = stdout
        .lines()
        .filter(|line| line.starts_with("FREQ_MHZ ") && line.split_whitespace().count() == 2)
        .count();
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
    let freq_headers = stdout
        .lines()
        .filter(|line| line.starts_with("FREQ_MHZ ") && line.split_whitespace().count() == 2)
        .count();
    assert_eq!(freq_headers, 5);
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
        .env_remove("FNEC_ACCEL_STUB_GPU")
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
        .env_remove("FNEC_ACCEL_STUB_GPU")
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
    assert!(
        stderr.contains("preserving explicit --exec=cpu"),
        "expected explicit exec preservation warning in stderr, got:\n{stderr}"
    );
    assert_diag_field(&stderr, "exec", "cpu");
}

#[test]
fn filename_steering_also_detects_4nec2_alias_names() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");
    let alias = create_dropin_alias("4nec2-kernel");

    let output = Command::new(&alias)
        .arg("--solver")
        .arg("hallen")
        .env_remove("FNEC_ACCEL_STUB_GPU")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "Failed to run drop-in alias '{}' for 4nec2 detection test: {e}",
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
fn dropin_alias_keeps_report_on_stdout_and_warning_on_stderr() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");
    let alias = create_dropin_alias("nec2dxs500");

    let output = Command::new(&alias)
        .arg("--solver")
        .arg("hallen")
        .env_remove("FNEC_ACCEL_STUB_GPU")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "Failed to run drop-in alias '{}' for stream contract test: {e}",
                alias.display()
            )
        });

    let _ = fs::remove_file(&alias);

    assert!(
        output.status.success(),
        "fnec drop-in alias failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.starts_with("FNEC FEEDPOINT REPORT\nFORMAT_VERSION 1\n"),
        "expected stable report header on stdout, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("drop-in compatibility profile detected by binary name"),
        "drop-in compatibility warning must not appear on stdout, got:\n{stdout}"
    );
    assert!(
        stderr.contains("drop-in compatibility profile detected by binary name"),
        "expected compatibility-profile warning in stderr, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("FNEC FEEDPOINT REPORT"),
        "stderr must not contain report output, got:\n{stderr}"
    );
    assert_diag_field(&stderr, "exec", "hybrid");
}

#[test]
fn dropin_alias_missing_deck_keeps_exit_code_and_error_stream_contract() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let alias = create_dropin_alias("nec2dxs500");
    let bogus_path = test_tmp_dir().join("fnec-dropin-missing.nec");
    let _ = fs::remove_file(&bogus_path);

    let output = Command::new(&alias)
        .arg(&bogus_path)
        .current_dir(&workspace_root)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "Failed to run drop-in alias '{}' for missing-deck contract test: {e}",
                alias.display()
            )
        });

    let _ = fs::remove_file(&alias);

    assert_eq!(
        output.status.code(),
        Some(1),
        "missing deck under drop-in alias must exit with code 1; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.is_empty(),
        "expected no stdout on missing-deck error, got:\n{stdout}"
    );
    assert!(
        stderr.contains("error:"),
        "expected error message on stderr for missing deck, got:\n{stderr}"
    );
    assert!(
        stderr.contains("drop-in compatibility profile detected by binary name"),
        "expected compatibility-profile warning to remain on stderr, got:\n{stderr}"
    );
}
#[test]
fn dropin_alias_run_does_not_create_files_in_working_directory() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");
    let alias = create_dropin_alias("nec2dxs500");
    let sandbox = create_sandbox_dir("dropin-cwd-sandbox");

    let before_entries: Vec<PathBuf> = fs::read_dir(&sandbox)
        .expect("failed to read sandbox before run")
        .map(|entry| {
            entry
                .expect("failed to read sandbox entry before run")
                .path()
        })
        .collect();
    assert!(
        before_entries.is_empty(),
        "expected empty sandbox before run, got: {before_entries:?}"
    );

    let output = Command::new(&alias)
        .arg("--solver")
        .arg("hallen")
        .env_remove("FNEC_ACCEL_STUB_GPU")
        .arg(&deck_path)
        .current_dir(&sandbox)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "Failed to run drop-in alias '{}' for file-side-effect contract test: {e}",
                alias.display()
            )
        });

    let _ = fs::remove_file(&alias);

    assert!(
        output.status.success(),
        "drop-in alias run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after_entries: Vec<PathBuf> = fs::read_dir(&sandbox)
        .expect("failed to read sandbox after run")
        .map(|entry| {
            entry
                .expect("failed to read sandbox entry after run")
                .path()
        })
        .collect();

    assert!(
        after_entries.is_empty(),
        "drop-in alias run must not create files in working directory; got: {after_entries:?}"
    );

    fs::remove_dir_all(&sandbox).expect("failed to remove sandbox directory");
}
