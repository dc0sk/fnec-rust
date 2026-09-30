// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-186: concurrent first use of wgpu crashed the process.
//!
//! Four threads creating instances and enumerating adapters at once segfaulted
//! in 5 of 5 runs on an NVIDIA GTX 1080 Ti (driver 580.178.04); one after another,
//! 0 of 5. Every such call now takes one lock. A regression is a crash of this
//! binary, not an assertion — which is also why the failures it explains were
//! nameless.
//!
//! Alone in its binary on purpose: the crash is in the driver's FIRST
//! initialisation. With the lock removed this test crashed 10 of 10 runs on its
//! own, and 0 of 10 beside a test that had already created a device.

use nec_accel::hardware_adapter_present;

#[test]
fn concurrent_first_adapter_enumeration_does_not_crash() {
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..5 {
                    let _ = pollster::block_on(hardware_adapter_present());
                }
            });
        }
    });
}
