// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// PH9-CHK-006: the Sommerfeld reflected-field kernel must reproduce nec2c's exact
// GN2 near-ground impedance for a horizontal dipole — in particular the
// surface-wave SIGN FLIP below 0.1 λ that fnec's scalar-Γ (reflection-coefficient)
// model gets wrong. Validated end-to-end via an induced-EMF reaction integral with a
// sinusoidal current (matching studies/sommerfeld-ground/).
//
// nec2c references (14.2 MHz, horizontal λ/2 dipole, εr=13, σ=0.005), ΔZ vs the
// free-space Z = 78.85 + j44.70 Ω:
//   height   GN2 ΔR (truth)   GN0/RCM ΔR (≈ fnec today)
//   0.05 λ      -11.6            -32.4
//   0.025 λ     + 9.0            -24.3   <-- sign flip

use nec_solver::sommerfeld::reflected_ex_horizontal;
use num_complex::Complex64;

const C0: f64 = 299_792_458.0;
const FREQ: f64 = 14.2e6;

/// ΔZ (ground-induced) for a horizontal λ/2 dipole at height `h` over ground
/// (εr, σ), via an induced-EMF reaction integral with an assumed sinusoidal current.
/// This is the surface-wave-inclusive Sommerfeld result.
fn delta_z(h: f64, eps_r: f64, sigma: f64) -> Complex64 {
    let lam = C0 / FREQ;
    let k0 = 2.0 * std::f64::consts::PI * FREQ / C0;
    let l = lam / 4.0; // half length → λ/2 dipole
    let nw = 81usize;
    let xs: Vec<f64> = (0..nw)
        .map(|i| -l + 2.0 * l * i as f64 / (nw - 1) as f64)
        .collect();
    let iw: Vec<f64> = xs.iter().map(|&x| (k0 * (l - x.abs())).sin()).collect();
    let dxw = xs[1] - xs[0];

    // Precompute the reflected kernel on a ρ grid, then interpolate in the double sum.
    let ng = 200usize;
    let rgrid: Vec<f64> = (0..ng)
        .map(|i| 2.0 * l * i as f64 / (ng - 1) as f64)
        .collect();
    let d = 2.0 * h;
    let eg: Vec<Complex64> = rgrid
        .iter()
        .map(|&r| reflected_ex_horizontal(r, d, FREQ, eps_r, sigma, false))
        .collect();
    let interp = |rho: f64| -> Complex64 {
        let t = rho / (2.0 * l) * (ng - 1) as f64;
        let i = (t.floor() as usize).min(ng - 2);
        let frac = t - i as f64;
        eg[i] * (1.0 - frac) + eg[i + 1] * frac
    };

    let mut acc = Complex64::new(0.0, 0.0);
    for i in 0..nw {
        let mut inner = Complex64::new(0.0, 0.0);
        for j in 0..nw {
            inner += interp((xs[i] - xs[j]).abs()) * iw[j];
        }
        acc += inner * iw[i] * dxw * dxw;
    }
    acc // I0 = I(0) = sin(k0 L) = 1
}

#[test]
fn sommerfeld_reproduces_gn2_surface_wave_sign_flip() {
    let lam = C0 / FREQ;

    // 0.025 λ: nec2c GN2 ΔR = +9.0 (RCM gives a wrong-signed -24.3). The kernel MUST
    // give a positive ΔR here — the defining surface-wave signature.
    let dz_low = delta_z(0.025 * lam, 13.0, 0.005);
    assert!(
        dz_low.re > 0.0,
        "0.025λ ΔR must be positive (surface-wave sign flip); got {:.2}",
        dz_low.re
    );
    assert!(
        (dz_low.re - 9.0).abs() < 4.0,
        "0.025λ ΔR should be near nec2c GN2 +9.0; got {:.2}",
        dz_low.re
    );

    // 0.05 λ: nec2c GN2 ΔR = -11.6 (still negative, but far less than RCM's -32.4).
    let dz_mid = delta_z(0.05 * lam, 13.0, 0.005);
    assert!(
        dz_mid.re < 0.0 && (dz_mid.re - (-11.6)).abs() < 5.0,
        "0.05λ ΔR should be near nec2c GN2 -11.6; got {:.2}",
        dz_mid.re
    );

    // The kernel must resolve the two heights differently — sign actually flips.
    assert!(
        dz_low.re > dz_mid.re + 10.0,
        "ΔR must rise sharply from 0.05λ ({:.1}) to 0.025λ ({:.1})",
        dz_mid.re,
        dz_low.re
    );
}

#[test]
fn sommerfeld_matches_gn2_at_moderate_height() {
    let lam = C0 / FREQ;
    // 0.10 λ: nec2c GN2 ΔR = -19.2 (RCM -27.0). Sommerfeld should land between,
    // near the GN2 truth.
    let dz = delta_z(0.10 * lam, 13.0, 0.005);
    assert!(
        (dz.re - (-19.2)).abs() < 5.0,
        "0.10λ ΔR should be near nec2c GN2 -19.2; got {:.2}",
        dz.re
    );
}
