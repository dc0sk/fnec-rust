// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The one re-run policy for tests that run on the GPU (FND-215).
//!
//! This host's NVIDIA driver loses a process's device, or hides the adapter from
//! enumeration, about once in tens to hundreds of runs (FND-190). A GPU test that
//! fails outright on that fails as if the code under test were wrong, and one that
//! skips whenever the adapter is not seen passes having run nothing (FND-163).
//! Every GPU test decides both through this module:
//!
//! - [`gpu_expected`] — whether this run must have a GPU. A skip is only honest
//!   where it is `false`.
//! - [`gpu_attempt`] — one attempt on the device; where a GPU is expected, a miss
//!   earns exactly one re-run, after the shared context is dropped so the re-run
//!   builds a fresh device instead of being handed the first attempt's cached
//!   failure. A second miss in a row panics.
//!
//! The attempt reports a miss as `Err` only for a *transient* fault — no adapter
//! seen, the device lost or not created. A deterministic decline (an input out of
//! class, an f32 solve that failed its residual check) is the caller's to assert:
//! re-running it would only fail the same way twice.
//!
//! A test file may not call [`hardware_adapter_present`] itself; a structural test
//! (`apps/nec-cli/tests/gpu_rerun_policy.rs`) holds every workspace `tests/`
//! directory to that.

use crate::wgpu_device::{forget_shared_gpu_context, hardware_adapter_present};
use std::fmt::Debug;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set to `1` on a host that has a GPU: a test that sees none then fails instead
/// of skipping. `scripts/check-all.sh` sets it where `nvidia-smi -L` lists a GPU.
pub const GPU_REQUIRED_ENV: &str = "FNEC_GPU_REQUIRED";

/// A file each re-run appends one line to, so a gate can report how many it took
/// — a passing re-run is otherwise invisible, and FND-190 is re-opened on a new
/// fault. `scripts/check-all.sh` sets it and prints the count.
pub const RERUN_LOG_ENV: &str = "FNEC_GPU_RERUN_LOG";

/// The marker every re-run prints and logs.
pub const RERUN_MARKER: &str = "FND-190 re-run:";

static SEEN: AtomicBool = AtomicBool::new(false);

/// Whether this run must have a GPU: [`GPU_REQUIRED_ENV`] is `1`, or a hardware
/// adapter has been seen at any time in this process. The driver hides the
/// adapter only momentarily (FND-190), so once seen it stays expected — a later
/// miss is then a fault to re-run, never a reason to skip.
pub fn gpu_expected() -> bool {
    if std::env::var(GPU_REQUIRED_ENV).is_ok_and(|v| v == "1") || SEEN.load(Ordering::Relaxed) {
        return true;
    }
    let present = pollster::block_on(hardware_adapter_present());
    if present {
        SEEN.store(true, Ordering::Relaxed);
    }
    present
}

/// One attempt on the device under the shared policy; see the module docs.
/// `Err` is returned only where no GPU is expected — the caller's skip. Each
/// re-run is appended to [`RERUN_LOG_ENV`]'s file.
pub fn gpu_attempt<T, E: Debug>(what: &str, run: impl FnMut() -> Result<T, E>) -> Result<T, E> {
    attempt(gpu_expected, log_rerun, what, run)
}

/// [`gpu_attempt`] with the expectation given and nothing logged, so the policy
/// itself is tested on hosts without a GPU without counting as a gate's re-run.
pub fn gpu_attempt_with<T, E: Debug>(
    expected: impl Fn() -> bool,
    what: &str,
    run: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    attempt(expected, |_| {}, what, run)
}

fn log_rerun(line: &str) {
    if let Ok(path) = std::env::var(RERUN_LOG_ENV) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn attempt<T, E: Debug>(
    expected: impl Fn() -> bool,
    log: impl FnOnce(&str),
    what: &str,
    mut run: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let first = match run() {
        Ok(t) => return Ok(t),
        Err(e) => e,
    };
    if !expected() {
        return Err(first);
    }
    let line = format!("{RERUN_MARKER} {what}: {first:?}");
    eprintln!("{line}; re-running once");
    log(&line);
    forget_shared_gpu_context();
    match run() {
        Ok(t) => Ok(t),
        Err(second) => panic!(
            "{what}: a GPU is expected here and the attempt missed twice: {first:?}, then {second:?}"
        ),
    }
}

/// Whether a failed [`crate::fill_zmatrix_wgpu`] is a transient fault worth the
/// re-run: every miss is — no adapter, the device not created or lost, a readback
/// that failed — except a system larger than the device's storage binding, which
/// fails the same way every time.
pub fn fill_miss_is_transient(why: &str) -> bool {
    !why.contains("storage binding holds")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_success_is_not_re_run() {
        let runs = Cell::new(0);
        let r: Result<u8, ()> = gpu_attempt_with(
            || true,
            "t",
            || {
                runs.set(runs.get() + 1);
                Ok(1)
            },
        );
        assert_eq!((r, runs.get()), (Ok(1), 1));
    }

    #[test]
    fn a_miss_with_no_gpu_expected_is_returned_unretried() {
        let runs = Cell::new(0);
        let r: Result<u8, &str> = gpu_attempt_with(
            || false,
            "t",
            || {
                runs.set(runs.get() + 1);
                Err("no adapter")
            },
        );
        assert_eq!((r, runs.get()), (Err("no adapter"), 1));
    }

    #[test]
    fn a_miss_with_a_gpu_expected_earns_exactly_one_re_run() {
        let runs = Cell::new(0);
        let r: Result<u8, &str> = gpu_attempt_with(
            || true,
            "t",
            || {
                runs.set(runs.get() + 1);
                if runs.get() == 1 {
                    Err("device lost")
                } else {
                    Ok(2)
                }
            },
        );
        assert_eq!((r, runs.get()), (Ok(2), 2));
    }

    #[test]
    fn two_misses_in_a_row_fail() {
        let runs = Cell::new(0);
        let twice = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<u8, &str> = gpu_attempt_with(
                || true,
                "t",
                || {
                    runs.set(runs.get() + 1);
                    Err("device lost")
                },
            );
        }));
        assert!(twice.is_err(), "a second miss must fail the test");
        assert_eq!(runs.get(), 2, "and only one re-run is taken");
    }
}
