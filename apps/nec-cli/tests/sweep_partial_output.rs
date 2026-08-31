// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! A sweep prints the points it computed, even when one of them fails.
//!
//! It used to `return ExitCode::FAILURE` on the first `Err`, discarding every
//! point after it. On the distributed path that was worse than it sounds: when a
//! worker pool drained, the *first* result was already an error, so an
//! invocation that had solved most of the sweep printed **nothing at all**.
//!
//! The GUI has kept its points on a failed sweep since FND-033 — "the points are
//! as true as they were a moment earlier" — and this brings the CLI to the same
//! answer. The exit code still reports the run as unclean, so a script checking
//! the status is unaffected; what changed is that a human gets the points that
//! worked.
//!
//! The loop this policy lives in was **57 lines duplicated byte for byte**
//! between the local and distributed paths (verified by hash before the change).
//! It is now one function, so the policy cannot diverge between them — which is
//! why one test covers both.

use std::process::Command;

/// 1e-200 MHz solves to no current at the feedpoint, so the point fails while
/// its neighbours succeed. A whole-deck refusal would not exercise this: it has
/// to be a per-POINT failure inside an otherwise good sweep.
const MIXED: &str = "[frequency]\npoints_mhz = [14.0, 1e-200, 14.2]\n";

fn run(cfg: &str) -> (Option<i32>, String, String) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    // Per-TEST, not per-process: `cargo test` runs these as threads of one
    // process, so a directory keyed only on the pid is shared, and each test
    // deletes the other's config mid-run. Same isolation bug as the stub worker
    // in worker_task_fault.rs, one layer up.
    let dir = std::env::temp_dir().join(format!(
        "fnec-partial-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("sweep.toml");
    std::fs::write(&path, cfg).expect("write config");

    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--sweep-config")
        .arg(&path)
        .arg(root.join("corpus").join("dipole-freesp-51seg.nec"))
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_sweep_prints_the_points_that_worked_and_still_exits_nonzero() {
    let (code, stdout, stderr) = run(MIXED);

    let reports = stdout.matches("FNEC FEEDPOINT REPORT").count();
    assert_eq!(
        reports, 2,
        "both good points must be reported, not just the ones before the \
         failure:\n{stdout}"
    );
    // The point AFTER the failure is the one the old behaviour lost. Pin it by
    // frequency so "two reports" cannot be satisfied by printing the first twice.
    assert!(
        stdout.contains("FREQ_MHZ 14.200000"),
        "the point after the failed one must still be reported:\n{stdout}"
    );
    assert!(
        stderr.contains("no current flows at the feedpoint"),
        "the failure must still be reported, on stderr:\n{stderr}"
    );
    assert_eq!(
        code,
        Some(1),
        "a sweep with a failed point is not a clean run, whatever it printed"
    );
}

/// The control: a clean sweep still prints every point and exits zero. Without
/// it, a change that reported failure unconditionally would satisfy the test
/// above.
#[test]
fn a_clean_sweep_is_unaffected() {
    let (code, stdout, _) = run("[frequency]\npoints_mhz = [14.0, 14.1, 14.2]\n");
    assert_eq!(stdout.matches("FNEC FEEDPOINT REPORT").count(), 3);
    assert_eq!(code, Some(0), "a clean sweep must still exit 0");
}
