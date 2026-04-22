// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_solver::scale_excitation_for_pulse_rhs;
use num_complex::Complex64;

const C0: f64 = 299_792_458.0;

#[test]
fn pulse_rhs_scaling_applies_inverse_wavelength() {
    let freq_hz = 14.2e6;
    let v = vec![Complex64::new(2.0, -1.0), Complex64::new(0.0, 0.5)];
    let scaled = scale_excitation_for_pulse_rhs(&v, freq_hz);
    let lambda = C0 / freq_hz;

    assert!((scaled[0] - v[0] / lambda).norm() < 1e-12);
    assert!((scaled[1] - v[1] / lambda).norm() < 1e-12);
}
