// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-234: a failed device request is not cached for good. It is returned without
//! asking the driver until its backoff has passed, and then the next caller tries
//! again — before, one driver fault at `request_device` (FND-190) sent every later
//! point of a sweep, and every task of a worker, to the CPU. Its own test binary,
//! because the cache it pins is process-global; one test, so its steps run in order.

use nec_accel::gpu_test::gpu_expected;
use nec_accel::wgpu_device::{
    forget_shared_gpu_context, lose_shared_gpu_device_for_test, pin_shared_gpu_failure_for_test,
    shared_gpu_failures_for_test,
};
use nec_accel::{fill_zmatrix_wgpu, gpu_context_build_count, ZSegmentInput};
use std::time::Duration;

fn fill() -> Result<usize, String> {
    let segs: Vec<ZSegmentInput> = (0..11)
        .map(|i| ZSegmentInput {
            midpoint: [0.0, 0.0, -0.5 + (i as f64 + 0.5) / 11.0],
            direction: [0.0, 0.0, 1.0],
            length: 1.0 / 11.0,
            radius: 0.001,
        })
        .collect();
    pollster::block_on(fill_zmatrix_wgpu(&segs, 14.2e6)).map(|z| z.len())
}

#[test]
fn a_cached_device_failure_is_retried_after_its_backoff() {
    const PINNED: &str = "Parent device is lost (pinned by the test)";

    // A device lost under a dispatch is a counted failure, not a reset to Untried:
    // a reset rebuilt at once, so a device dying at every dispatch — each of which
    // can wait out the 120 s readback timeout — was rebuilt at every point.
    forget_shared_gpu_context();
    lose_shared_gpu_device_for_test("lost");
    assert_eq!(shared_gpu_failures_for_test(), Some(1));
    lose_shared_gpu_device_for_test("lost again");
    assert_eq!(shared_gpu_failures_for_test(), Some(2));
    assert!(
        fill().is_err(),
        "within its 2 s backoff the loss is returned"
    );

    // Within the backoff: the cached failure, with no device built. Any host.
    let builds = gpu_context_build_count();
    pin_shared_gpu_failure_for_test(PINNED, 3, Duration::from_secs(3));
    let err = fill().expect_err("within its 4 s backoff the failure is returned");
    assert!(err.contains(PINNED), "{err}");
    assert_eq!(gpu_context_build_count(), builds, "nothing was rebuilt");
    assert_eq!(
        shared_gpu_failures_for_test(),
        Some(3),
        "and nothing recounted"
    );

    // Past the 60 s cap of the 7th failure, but within its own: still cached.
    pin_shared_gpu_failure_for_test(PINNED, 7, Duration::from_secs(59));
    assert!(fill().is_err());
    assert_eq!(gpu_context_build_count(), builds);

    // Past the backoff the next caller asks the driver again.
    pin_shared_gpu_failure_for_test(PINNED, 7, Duration::from_secs(61));
    if gpu_expected() {
        assert_eq!(fill(), Ok(11 * 11), "a GPU host builds a fresh device");
        assert_eq!(gpu_context_build_count(), builds + 1);
        assert_eq!(shared_gpu_failures_for_test(), None, "the context is ready");
    } else {
        // No GPU: the retry fails afresh — counted, and with the driver's reason.
        let err = fill().expect_err("no GPU to build on");
        assert!(!err.contains(PINNED), "the retry asked the driver: {err}");
        assert_eq!(shared_gpu_failures_for_test(), Some(8));
    }
}
