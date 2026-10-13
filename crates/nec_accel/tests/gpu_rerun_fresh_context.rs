// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-215: a re-run after a driver fault builds a fresh device. A failed device
//! request is cached for the process, so a re-run that did not drop it would be
//! handed the first attempt's failure and retry nothing. Its own test binary,
//! because the cache it pins is process-global.

use nec_accel::gpu_test::{fill_miss_is_transient, gpu_attempt_with, gpu_expected};
use nec_accel::wgpu_device::pin_shared_gpu_failure_for_test;
use nec_accel::{fill_zmatrix_wgpu, ZSegmentInput};

#[test]
fn a_re_run_is_not_handed_the_cached_device_failure() {
    if !gpu_expected() {
        eprintln!("no hardware GPU adapter — skipped");
        return;
    }
    let segs: Vec<ZSegmentInput> = (0..11)
        .map(|i| ZSegmentInput {
            midpoint: [0.0, 0.0, -0.5 + (i as f64 + 0.5) / 11.0],
            direction: [0.0, 0.0, 1.0],
            length: 1.0 / 11.0,
            radius: 0.001,
        })
        .collect();
    pin_shared_gpu_failure_for_test("Parent device is lost (pinned by the test)");
    let runs = std::cell::Cell::new(0);
    let z = gpu_attempt_with(
        || true,
        "a fill after a pinned device failure",
        || {
            runs.set(runs.get() + 1);
            match pollster::block_on(fill_zmatrix_wgpu(&segs, 14.2e6)) {
                Err(why) if fill_miss_is_transient(&why) => Err(why),
                Err(why) => panic!("the fill declined: {why}"),
                ok => ok,
            }
        },
    )
    .expect("a GPU is expected");
    assert_eq!(
        runs.get(),
        2,
        "the first attempt must meet the pinned failure"
    );
    assert_eq!(z.len(), 11 * 11);
}
