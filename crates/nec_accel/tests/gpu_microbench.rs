// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH7-CHK-002: in-process GPU microbenchmark.
//!
//! Verifies that `microbench_zmatrix_dispatch` reports per-dispatch kernel time
//! **separately** from the one-time wgpu device-initialization cost. The
//! best-of-N (minimum) dispatch figure rejects positive-only wall-clock noise,
//! so the measurement is non-flaky (unlike the across-process G5 gate, where
//! every sample re-pays device-init).
//!
//! Skips vacuously when no wgpu adapter is available.

use nec_accel::{microbench_zmatrix_dispatch, ZSegmentInput};

/// Build a straight N-segment wire along Z for a deterministic fill workload.
fn line_segments(n: usize) -> Vec<ZSegmentInput> {
    let half = 5.0_f64;
    let seg_len = 2.0 * half / n as f64;
    (0..n)
        .map(|i| {
            let z = -half + (i as f64 + 0.5) * seg_len;
            ZSegmentInput {
                midpoint: [0.0, 0.0, z],
                direction: [0.0, 0.0, 1.0],
                length: seg_len,
                radius: 0.001,
            }
        })
        .collect()
}

#[test]
fn gpu_microbench_isolates_dispatch_from_device_init() {
    let segs = line_segments(160);
    let reps = 12;

    let Some(mb) = pollster::block_on(microbench_zmatrix_dispatch(&segs, 14.2e6, reps)) else {
        eprintln!("PH7-CHK-002: no wgpu adapter — microbenchmark skipped (software fallback)");
        return;
    };

    eprintln!(
        "PH7-CHK-002 microbench: n_segs={} dispatches={} device_init={}µs dispatch_min={}µs dispatch_median={}µs",
        mb.n_segments, mb.n_dispatches, mb.device_init_us, mb.dispatch_min_us, mb.dispatch_median_us
    );

    // Device-init was actually paid and measured.
    assert!(mb.device_init_us > 0, "device init time must be measured");
    assert_eq!(mb.n_dispatches, reps);
    assert_eq!(mb.n_segments, 160);

    // Dispatch timing is reported separately and is internally consistent.
    assert!(mb.dispatch_min_us > 0, "per-dispatch time must be measured");
    assert!(
        mb.dispatch_min_us <= mb.dispatch_median_us,
        "min ({}) must not exceed median ({})",
        mb.dispatch_min_us,
        mb.dispatch_median_us
    );

    // The whole point: the per-dispatch figure excludes device-init, so on any
    // real adapter a single reused dispatch is far cheaper than acquiring the
    // device. (Device-init is tens of ms; a small fill dispatch is well under
    // that.) This is what the across-process G5 gate cannot isolate.
    assert!(
        mb.dispatch_min_us < mb.device_init_us,
        "isolated dispatch ({}µs) should be cheaper than one-time device init ({}µs)",
        mb.dispatch_min_us,
        mb.device_init_us
    );
}
