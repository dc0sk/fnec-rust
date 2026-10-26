// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The physical constants fnec solves with — the one place they are defined.
//!
//! Fourteen modules across `nec_solver`, `nec_accel`, `nec_report` and the CLI
//! defined their own `C0`/`MU0`/`EPS0`/`ETA0`, and eight more sites wrote the
//! value inline (FND-297 b). The
//! `C0`, `MU0` and `ETA0` copies were bit-identical. `EPS0` was not: the
//! Sommerfeld ground took 8.854 187 817e-12 (1/(μ0 c²) truncated to ten digits) and
//! the MPIE took 8.854 187 812 8e-12 (CODATA 2018) — a value inconsistent with the
//! MPIE's own `MU0 = 4π·10⁻⁷`, since then `μ0 ε0 c² ≠ 1`. Both now read the value
//! derived from `MU0` and `C0`, so the three constants satisfy `μ0 ε0 c² = 1` by
//! construction.
//!
//! **μ0 is the pre-2019 exact `4π·10⁻⁷`**, the value every copy used. SI 2019 made
//! μ0 a measured quantity (1.000 000 000 55 × 4π·10⁻⁷); the 5.5·10⁻¹⁰ difference is
//! far below every gate, and keeping it means no `C0`/`MU0`/`ETA0` value moved.
//!
//! Defined here, in the crate every other one depends on, so the report crate can
//! read the same `C0` the solver uses without depending on the solver; the solver
//! re-exports them as `nec_solver::constants`, the path its callers use.
//!
//! A repo gate (`scripts/check-physical-constants.py`) refuses these constants'
//! literal values anywhere else in the Rust sources, so the next copy fails the
//! gate instead of drifting.

/// Speed of light in vacuum, m/s (exact by the SI definition of the metre).
pub const C0: f64 = 299_792_458.0;

/// Permeability of free space, H/m — NEC-2's `4π·10⁻⁷`.
pub const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7;

/// Permittivity of free space, F/m: `1/(μ0 c²)`, consistent with [`MU0`] and [`C0`].
pub const EPS0: f64 = 1.0 / (MU0 * C0 * C0);

/// Free-space wave impedance, Ω: `μ0 c` ≈ 376.73.
pub const ETA0: f64 = MU0 * C0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The three constants are mutually consistent: `μ0 ε0 c² = 1` and
    /// `η0 = √(μ0/ε0)`, to the last few ulps.
    #[test]
    fn the_constants_satisfy_maxwell() {
        assert!((MU0 * EPS0 * C0 * C0 - 1.0).abs() < 4.0 * f64::EPSILON);
        assert!((ETA0 / (MU0 / EPS0).sqrt() - 1.0).abs() < 4.0 * f64::EPSILON);
    }
}
