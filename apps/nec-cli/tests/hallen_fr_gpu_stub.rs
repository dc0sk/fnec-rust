// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Integration tests for Hallen FR GPU kernel stub.

use nec_accel::gpu_kernels::{
    compute_hallen_fr_batch_stub, compute_hallen_fr_point_stub, GpuSegment, HallenFrGpuKernel,
};
use num_complex::Complex64;

/// Build a test dipole (single segment, z-axis).
fn dipole_test_kernel(length: f64, freq_hz: f64, norm: f64) -> HallenFrGpuKernel {
    let seg = GpuSegment {
        midpoint: [0.0, 0.0, 0.0],
        direction: [0.0, 0.0, 1.0],
        length,
    };
    let currents = vec![Complex64::new(1.0, 0.0)];
    HallenFrGpuKernel::new(vec![seg], currents, freq_hz, norm)
}

#[test]
fn hertzian_dipole_equator_vs_pole() {
    let kernel = dipole_test_kernel(0.01, 14.2e6, 1e-4);

    let equator = compute_hallen_fr_point_stub(&kernel, 90.0, 0.0);
    let pole = compute_hallen_fr_point_stub(&kernel, 0.0, 0.0);

    // Dipole has maximum gain at equator (θ=90°)
    assert!(
        equator.gain_total_dbi > pole.gain_total_dbi,
        "equator {:.2} should be > pole {:.2}",
        equator.gain_total_dbi,
        pole.gain_total_dbi
    );
}

#[test]
fn batch_computation_3_points() {
    let kernel = dipole_test_kernel(0.01, 14.2e6, 1e-4);
    let points = vec![(0.0, 0.0), (90.0, 0.0), (180.0, 0.0)];
    let batch = compute_hallen_fr_batch_stub(&kernel, &points);

    assert_eq!(batch.len(), 3);
    assert_eq!(batch[0].theta_deg, 0.0);
    assert_eq!(batch[1].theta_deg, 90.0);
    assert_eq!(batch[2].theta_deg, 180.0);

    // Verify polar symmetry: θ=0° and θ=180° should have same gain
    let gain_0 = batch[0].gain_total_dbi;
    let gain_180 = batch[2].gain_total_dbi;
    if gain_0.is_finite() && gain_180.is_finite() {
        assert!((gain_0 - gain_180).abs() < 0.1, "poles should be symmetric");
    }
}

#[test]
fn multi_segment_linear_array() {
    // Two collinear z-axis segments (linear array effect)
    let seg1 = GpuSegment {
        midpoint: [0.0, 0.0, -0.5],
        direction: [0.0, 0.0, 1.0],
        length: 1.0,
    };
    let seg2 = GpuSegment {
        midpoint: [0.0, 0.0, 0.5],
        direction: [0.0, 0.0, 1.0],
        length: 1.0,
    };
    let currents = vec![
        Complex64::new(1.0, 0.0),
        Complex64::new(1.0, 0.0), // co-phase
    ];
    let kernel = HallenFrGpuKernel::new(vec![seg1, seg2], currents, 14.2e6, 1e-4);

    let result = compute_hallen_fr_point_stub(&kernel, 90.0, 0.0);
    assert!(
        result.gain_total_dbi.is_finite(),
        "should compute valid gain"
    );
    assert!(result.gain_theta_dbi.is_finite());
    assert!(result.gain_phi_dbi.is_finite());
}

#[test]
fn azimuth_variation() {
    let kernel = dipole_test_kernel(0.01, 14.2e6, 1e-4);

    // For z-axis dipole, azimuth shouldn't matter at equator (rotational symmetry)
    let phi_0 = compute_hallen_fr_point_stub(&kernel, 90.0, 0.0);
    let phi_90 = compute_hallen_fr_point_stub(&kernel, 90.0, 90.0);
    let phi_180 = compute_hallen_fr_point_stub(&kernel, 90.0, 180.0);

    // Gains should be equal (within numerical precision)
    assert!(
        (phi_0.gain_total_dbi - phi_90.gain_total_dbi).abs() < 0.01,
        "φ=0° and φ=90° should have same gain at equator"
    );
    assert!(
        (phi_90.gain_total_dbi - phi_180.gain_total_dbi).abs() < 0.01,
        "φ=90° and φ=180° should have same gain at equator"
    );
}

#[test]
fn numerical_stability_edge_cases() {
    let kernel = dipole_test_kernel(0.01, 14.2e6, 1e-4);

    // Very small angles
    let result_small = compute_hallen_fr_point_stub(&kernel, 0.1, 0.1);
    assert!(result_small.gain_total_dbi.is_finite());

    // Large angles
    let result_large = compute_hallen_fr_point_stub(&kernel, 179.9, 359.9);
    assert!(result_large.gain_total_dbi.is_finite());
}

#[test]
fn polarization_components() {
    let kernel = dipole_test_kernel(0.01, 14.2e6, 1e-4);

    let result = compute_hallen_fr_point_stub(&kernel, 45.0, 45.0);

    // Both theta and phi polarization should be present for oblique angles
    assert!(result.gain_theta_dbi.is_finite());
    assert!(result.gain_phi_dbi.is_finite());
    assert!(
        result.axial_ratio >= 0.0,
        "axial ratio should be non-negative"
    );
}
