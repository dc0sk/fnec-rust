// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Turning a solved feedpoint into an impedance, or refusing to.
//!
//! `Z = V/I` needs a non-zero `I`. Six places computed it, and all six had the
//! same fallback:
//!
//! ```text
//! if current.norm() > 1e-60 { v / current } else { v }
//! ```
//!
//! The `else` branch prints the **source voltage** as an impedance. That is not
//! a degraded answer, it is a different quantity wearing the units of the one
//! that was asked for. Measured on `main` before this module, all at exit 0 with
//! no warning:
//!
//! | deck | `I` | reported `Z` |
//! |------|-----|--------------|
//! | plane wave beside a driven source | 0 + j0 | 1.000000 + j0.000000 |
//! | `EX 0 ... 0.0 0.0` (zero amplitude) | 0 + j0 | 0.000000 + j0.000000 |
//! | `FR ... 1e-300` | 0 + j0 | 1.000000 + j0.000000 |
//!
//! The first two are FND-050 and FND-058; the third is the residue #417 could
//! not reach, because `1e-300` is finite and positive and only *underflows* once
//! the matrix is built.
//!
//! **`NaN` is a separate answer from zero.** `current.norm() > 1e-60` is false
//! for `NaN` too, so a diverged solve took the same branch and got the same
//! sentence — "the current is zero" is the wrong thing to say about a solve that
//! did not converge. The two are distinguished here.

use num_complex::Complex64;

/// Below this magnitude a feedpoint current is treated as no current at all.
///
/// One named constant because there were six literals and a seventh convention
/// (`1e-30`) in the GPU tests. A driven feedpoint at 1 V would need |Z| > 10^60 Ω
/// to trip this falsely, so it separates an exact-zero right-hand side from any
/// current a real antenna carries.
pub const MIN_FEEDPOINT_CURRENT: f64 = 1e-60;

/// A solved current vector that is not a number.
///
/// Reported by [`check_currents_finite`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonFiniteCurrents {
    /// Index of the first segment whose current is not finite.
    pub first_bad: usize,
    /// How many segments were solved for.
    pub total: usize,
}

impl std::fmt::Display for NonFiniteCurrents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (i, n) = (self.first_bad, self.total);
        write!(
            f,
            "the solve did not converge: segment {i} of {n} has a current that is \
             not a finite number, so everything derived from it — impedance, \
             pattern, gain — would be meaningless"
        )
    }
}

impl std::error::Error for NonFiniteCurrents {}

/// Refuse a solved current vector that contains a non-finite entry.
///
/// [`FeedpointError::NonFiniteCurrent`] guards the same thing, but only at the
/// feedpoint, and therefore only on decks that HAVE one. A plane-wave receive
/// deck has none, so a fully diverged solve printed 51 rows of `NaN NaN NaN NaN`
/// and exited 0 — while the identical deck with `EX 0` instead of `EX 1` exited 1
/// with the right diagnostic. The guard existed and the sibling path had none
/// (FND-126).
///
/// This is the shared check, deliberately a free function rather than a hook
/// inside one solver: the CLI's GPU-resident arm and its receive-pattern sweep
/// exist precisely because they bypass the session entry points, so there is no
/// single choke point that covers everything. One implementation, called at every
/// site that produces currents, is the honest shape.
pub fn check_currents_finite(currents: &[Complex64]) -> Result<(), NonFiniteCurrents> {
    match currents
        .iter()
        .position(|c| !c.re.is_finite() || !c.im.is_finite())
    {
        Some(first_bad) => Err(NonFiniteCurrents {
            first_bad,
            total: currents.len(),
        }),
        None => Ok(()),
    }
}

/// Why a feedpoint has no impedance to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedpointError {
    /// The current is zero, so `V/I` is undefined. Not an approximation to
    /// recover from: there is no impedance, and the source voltage is not one.
    NoCurrent { tag: usize, seg: usize },
    /// The solve produced a non-finite current. Distinct from zero because the
    /// cause and the remedy differ — this is a failed solve, not a null port.
    NonFiniteCurrent { tag: usize, seg: usize },
}

impl std::fmt::Display for FeedpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCurrent { tag, seg } => write!(
                f,
                "no current flows at the feedpoint (tag {tag}, segment {seg}), so it has \
                 no impedance: Z = V/I is undefined at I = 0. Common causes are a \
                 zero-amplitude source and a frequency so small the solve underflows"
            ),
            Self::NonFiniteCurrent { tag, seg } => write!(
                f,
                "the solved current at the feedpoint (tag {tag}, segment {seg}) is not a \
                 finite number, so the solve did not converge and its impedance would be \
                 meaningless"
            ),
        }
    }
}

/// `Z = V/I`, or why there is none.
///
/// Used for both families: the delta-gap case divides the source voltage by the
/// solved current, and the current-source case divides the solved port voltage by
/// the impressed current. They are the same division and had the same defect, so
/// they get the same seam.
pub fn feedpoint_impedance(
    v: Complex64,
    i: Complex64,
    tag: usize,
    seg: usize,
) -> Result<Complex64, FeedpointError> {
    if !i.re.is_finite() || !i.im.is_finite() {
        return Err(FeedpointError::NonFiniteCurrent { tag, seg });
    }
    if i.norm() <= MIN_FEEDPOINT_CURRENT {
        return Err(FeedpointError::NoCurrent { tag, seg });
    }
    Ok(v / i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_feedpoint_divides() {
        let z = feedpoint_impedance(Complex64::new(1.0, 0.0), Complex64::new(0.01, 0.0), 1, 11)
            .expect("a real feedpoint");
        assert!((z.re - 100.0).abs() < 1e-9, "{z}");
    }

    #[test]
    fn a_zero_current_has_no_impedance_rather_than_the_source_voltage() {
        let e = feedpoint_impedance(Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0), 1, 26)
            .expect_err("zero current defines no impedance");
        assert_eq!(e, FeedpointError::NoCurrent { tag: 1, seg: 26 });
        // The message must name the port: a deck can have several.
        assert!(e.to_string().contains("tag 1") && e.to_string().contains("segment 26"));
    }

    /// `NaN` fails every comparison, so it took the zero branch and earned the
    /// zero sentence. The cause and the remedy are different.
    #[test]
    fn a_non_finite_current_is_not_reported_as_no_current() {
        let e = feedpoint_impedance(
            Complex64::new(1.0, 0.0),
            Complex64::new(f64::NAN, 0.0),
            1,
            11,
        )
        .expect_err("NaN is not an impedance");
        assert_eq!(e, FeedpointError::NonFiniteCurrent { tag: 1, seg: 11 });
        assert!(e.to_string().contains("did not converge"), "{e}");
    }

    /// The threshold must not refuse a current a real antenna could carry.
    #[test]
    fn a_tiny_but_physical_current_still_divides() {
        // 1 uA at 1 V is |Z| = 1e6 ohm — extreme, and nowhere near the threshold.
        assert!(
            feedpoint_impedance(Complex64::new(1.0, 0.0), Complex64::new(1e-6, 0.0), 1, 11).is_ok()
        );
    }
}
