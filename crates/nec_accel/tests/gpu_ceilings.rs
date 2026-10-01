// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The GPU paths' size ceilings.
//!
//! Before: an N×N fill dispatched N²/64 workgroups in one dimension, past the
//! per-dimension limit (65535) at N = 2048 — wgpu panicked and the process exited
//! 101 on every `--exec gpu` deck that large. And the dense solve declined above a
//! fixed `MAX_S = 1024`, without a word, although its shader's only arrays are 64
//! wide. Now every linearly-indexed dispatch is a 2-D grid and the one size limit
//! is the device's storage binding, read from the device.
//!
//! The grid, the capacity arithmetic and the shaders' indexing run anywhere; the
//! device tests skip only where there is no hardware adapter.

use nec_accel::{
    dense_matrix_capacity, fill_zmatrix_wgpu, hardware_adapter_present, linear_dispatch_grid,
    shared_device_dense_capacity, solve_hallen_gpu_resident, GpuSolveDeclined, ZSegmentInput,
};

#[test]
fn the_grid_splits_exactly_past_the_per_dimension_limit() {
    let max = 65_535;
    assert_eq!(linear_dispatch_grid(max * 64, max), (max, 1));
    assert_eq!(linear_dispatch_grid(max * 64 + 1, max), (32_768, 2));
    assert_eq!(linear_dispatch_grid(0, max), (0, 1));
    // Every total is covered, and no dimension passes the limit. N = 2048 was
    // the first fill that panicked; 4096 is the downlevel binding's ceiling.
    for total in [
        1u32,
        63,
        64,
        65,
        2047 * 2047,
        2048 * 2048,
        4096 * 4096,
        16_384 * 16_384,
    ] {
        let (x, y) = linear_dispatch_grid(total, max);
        assert!(x <= max && y <= max, "{total}: ({x}, {y})");
        assert!(
            u64::from(x) * u64::from(y) * 64 >= u64::from(total),
            "{total}: ({x}, {y}) leaves invocations out"
        );
    }
}

#[test]
fn the_dense_capacity_is_what_one_storage_binding_holds() {
    // 128 MiB at the downlevel defaults: 8·4096² bytes exactly.
    assert_eq!(
        dense_matrix_capacity(&wgpu::Limits::downlevel_defaults()),
        4096
    );
    let two_gib = wgpu::Limits {
        max_storage_buffer_binding_size: 2 << 30,
        max_buffer_size: 4 << 30,
        ..wgpu::Limits::downlevel_defaults()
    };
    assert_eq!(dense_matrix_capacity(&two_gib), 16_384);
    // The smaller of the two limits governs.
    let small_buffer = wgpu::Limits {
        max_buffer_size: 8 * 1000 * 1000,
        ..two_gib
    };
    assert_eq!(dense_matrix_capacity(&small_buffer), 1000);
}

/// Every shader entry that reads its invocation id must index linearly across
/// the 2-D grid; one that read `gid.x` alone would repeat its work in every row
/// of a grid the host split, and write the same entries twice.
#[test]
fn every_indexed_shader_entry_reads_the_grid() {
    for (name, src) in [
        (
            "hallen_lu_solve.wgsl",
            include_str!("../src/shaders/hallen_lu_solve.wgsl"),
        ),
        (
            "zmatrix_fill.wgsl",
            include_str!("../src/shaders/zmatrix_fill.wgsl"),
        ),
        (
            "rp_farfield_batch.wgsl",
            include_str!("../src/shaders/rp_farfield_batch.wgsl"),
        ),
    ] {
        let mut entries = 0;
        for sig in src.lines().filter(|l| l.starts_with("fn cs_")) {
            if sig.contains("global_invocation_id") {
                entries += 1;
                assert!(sig.contains("num_workgroups"), "{name}: {sig}");
            }
        }
        assert!(
            entries > 0,
            "{name}: no indexed entry found — the scan is broken"
        );
        // `gid.x` only inside the one helper that builds the linear index.
        let helper = src
            .find("fn linear(")
            .unwrap_or_else(|| panic!("{name}: no linear() helper"));
        let helper_end = helper + src[helper..].find('}').expect("helper body");
        for (at, _) in src.match_indices("gid.x") {
            assert!(
                (helper..helper_end).contains(&at),
                "{name}: gid.x read outside linear() at byte {at}"
            );
        }
    }
}

fn straight_wire(n: usize) -> Vec<ZSegmentInput> {
    let len = 10.564 / n as f64;
    (0..n)
        .map(|i| ZSegmentInput {
            midpoint: [0.0, 0.0, -5.282 + (i as f64 + 0.5) * len],
            direction: [0.0, 0.0, 1.0],
            length: len,
            radius: 0.001,
        })
        .collect()
}

fn gpu_present() -> bool {
    pollster::block_on(hardware_adapter_present())
}

/// N = 2049: one past the first size that panicked.
#[test]
fn a_fill_past_the_old_dispatch_limit_runs_on_the_device() {
    if !gpu_present() {
        eprintln!("no hardware GPU adapter — skipped");
        return;
    }
    let n = 2049;
    let z = match pollster::block_on(fill_zmatrix_wgpu(&straight_wire(n), 14.2e6)) {
        Ok(z) => z,
        // The host driver can lose the device under any process (FND-190); that
        // decline names itself and is not this test's subject.
        Err(why) if why.contains("device is lost") => {
            eprintln!("NOTE: the driver lost the device (FND-190): {why}");
            return;
        }
        Err(why) => panic!("a GPU is present and the fill declined: {why}"),
    };
    assert_eq!(z.len(), n * n);
    // The last row was the part a 1-D grid could not reach; its self term (the
    // diagonal) is the largest entry of its row, so it cannot be zero or NaN.
    let last = &z[(n - 1) * n + (n - 1)];
    assert!(
        last.re.is_finite() && last.im.is_finite() && (last.re != 0.0 || last.im != 0.0),
        "Z[{0}][{0}] = {1} + j{2}",
        n - 1,
        last.re,
        last.im
    );
}

/// A system one past the device's capacity declines as out of class, naming the
/// storage binding — before building any buffer.
#[test]
fn a_system_past_the_device_capacity_declines_with_its_reason() {
    if !gpu_present() {
        eprintln!("no hardware GPU adapter — skipped");
        return;
    }
    // A straight wire with two free ends is square: S = N + 2.
    // `None` with a GPU present is the driver losing the device (FND-190).
    let Some(capacity) = shared_device_dense_capacity() else {
        eprintln!("NOTE: no shared device although an adapter is present (FND-190)");
        return;
    };
    let n = capacity - 1;
    let segs = straight_wire(n);
    let zeros = vec![num_complex::Complex64::new(0.0, 0.0); n];
    let cos = vec![0.0; n];
    let rows = [(0usize, None, 1.0, 0.0), (n - 1, None, 1.0, 0.0)];
    match pollster::block_on(solve_hallen_gpu_resident(
        &segs,
        &zeros,
        &cos,
        &cos,
        &[(0, n - 1)],
        &[true],
        &rows,
        14.2e6,
    )) {
        Err(GpuSolveDeclined::OutOfClass(why)) => {
            assert!(why.contains("storage binding"), "{why}");
        }
        other => panic!("expected an out-of-class decline, got {other:?}"),
    }
}
