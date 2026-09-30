// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-186: the shared GPU context is built once, however many threads ask for
//! it first. It was built outside its lock ("a race just builds twice and drops
//! the loser"), and a parallel sweep's points make concurrent first callers the
//! normal case. Its own test binary, because the build counter is process-global:
//! any other GPU test in the same process would move it.

use nec_accel::{
    fill_zmatrix_wgpu, gpu_context_build_count, hardware_adapter_present, ZSegmentInput,
};
use std::sync::Barrier;

#[test]
fn eight_first_callers_build_one_device() {
    let segs: Vec<ZSegmentInput> = (0..21)
        .map(|i| ZSegmentInput {
            midpoint: [0.0, 0.0, -5.0 + (i as f64 + 0.5) * 10.0 / 21.0],
            direction: [0.0, 0.0, 1.0],
            length: 10.0 / 21.0,
            radius: 0.001,
        })
        .collect();
    let start = Barrier::new(8);
    let results: Vec<bool> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    pollster::block_on(fill_zmatrix_wgpu(&segs, 14.2e6)).is_ok()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    if pollster::block_on(hardware_adapter_present()) {
        assert!(
            results.iter().all(|ok| *ok),
            "a GPU is present and a fill failed"
        );
        assert_eq!(
            gpu_context_build_count(),
            1,
            "the first callers raced to build"
        );
    } else {
        eprintln!("no hardware GPU adapter — the build count must stay 0");
        assert_eq!(gpu_context_build_count(), 0);
    }
}
