// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Whether a mesh series has settled (FND-255, FND-256).
//!
//! A series is one quantity — a feedpoint resistance, a reactance, a current —
//! at meshes refined by the same factor between points, coarsest first. "Settled"
//! was once "the last step is shorter than the one before", which a series still
//! drifting at a constant rate passes; it is now the geometric tail of the last two
//! steps, the distance the series has still to move if it keeps contracting as it
//! did ([`remaining_drift`]). A series that is not contracting has no tail to
//! price, and [`classify`] says which way it fails: a near-constant step, the slow
//! convergence NEC-2 shows at a junction (the tee, the T through a bar, a point
//! load beside a delta gap, FND-278), or steps that grow.
//!
//! One rule for the corpus gate (`apps/nec-cli/tests/converged_reference.rs`,
//! which holds nec2c's own series to it) and for `fnec converge`, which holds the
//! user's deck to it; a copy in each would be the project's dominant defect.

/// How far `values` (one axis of a series, the same refinement factor between
/// points, coarsest first) has still to move past its last point: the geometric
/// tail of the last two steps, `d₂ r / (1 − r)` with `r = d₂ / d₁`. Infinite when
/// the series is not contracting (`r ≥ 1`): it drifts, and no tail can be priced.
///
/// `print_ulp` widens the last step by one unit in the last place the values were
/// printed to, for a series read from a program's rounded output (nec2c prints
/// five significant digits): the step is a difference of two rounded prints. The
/// ratio is not widened: widened both ways, a series that has settled to within a
/// print of its limit — the loaded dipole's R, 1129.3 / 1129.6 / 1129.7 — reads
/// `r = 1` and an infinite tail, which is the rounding speaking, not the series.
/// Pass `0.0` for values held in full precision.
///
/// Panics on fewer than three points: two points have no trend.
pub fn remaining_drift(values: &[f64], print_ulp: f64) -> f64 {
    let n = values.len();
    assert!(n >= 3, "a tail needs three points");
    let (a, b, c) = (values[n - 3], values[n - 2], values[n - 1]);
    let (d1, d2) = ((b - a).abs(), (c - b).abs());
    if d2 == 0.0 {
        return print_ulp;
    }
    let r = d2 / d1;
    if r >= 1.0 {
        f64::INFINITY
    } else {
        (d2 + print_ulp) * r / (1.0 - r)
    }
}

/// How a mesh series behaves at its last steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    /// Contracting: the last step is shorter than the one before, and a tail can
    /// be priced by [`remaining_drift`].
    Settling,
    /// A near-constant step — the last two step ratios both within 0.7–1.3 — the
    /// slow convergence NEC-2 shows at a junction, for which no tail exists and a
    /// gate must carry the step itself (FND-256). Needs four points to tell from
    /// a slow contraction.
    LogDrift,
    /// The steps grow.
    Diverging,
}

impl Trend {
    /// The word the CLI prints and the JSON carries.
    pub fn label(self) -> &'static str {
        match self {
            Trend::Settling => "settling",
            Trend::LogDrift => "log-drift",
            Trend::Diverging => "diverging",
        }
    }
}

/// The trend of a series of four points or more (the rule of FND-256: the two
/// last step ratios decide). Panics on fewer; use [`trend`] for any length.
pub fn classify(values: &[f64]) -> Trend {
    let n = values.len();
    assert!(n >= 4, "a class needs four points");
    let d: Vec<f64> = values.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    let (q1, q2) = (d[n - 3] / d[n - 4], d[n - 2] / d[n - 3]);
    let band = |q: f64| (0.7..=1.3).contains(&q);
    if band(q1) && band(q2) {
        Trend::LogDrift
    } else if q2 >= 1.0 {
        Trend::Diverging
    } else {
        Trend::Settling
    }
}

/// The trend of a series of any length: `None` below three points (two meshes
/// show a step, not a trend); with three, settling or diverging by the one step
/// ratio — a log drift cannot be told from a slow contraction on three points,
/// so it is never claimed there; with four or more, [`classify`].
pub fn trend(values: &[f64]) -> Option<Trend> {
    match values.len() {
        0..=2 => None,
        3 => {
            let (d1, d2) = ((values[1] - values[0]).abs(), (values[2] - values[1]).abs());
            Some(if d2 >= d1 && d2 > 0.0 {
                Trend::Diverging
            } else {
                Trend::Settling
            })
        }
        _ => Some(classify(values)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tail of a cleanly halving series is the geometric remainder, and a
    /// print ulp of zero leaves it alone.
    #[test]
    fn a_halving_series_has_the_geometric_tail() {
        let tail = remaining_drift(&[0.0, 1.0, 1.5], 0.0);
        assert!((tail - 0.5).abs() < 1e-12, "{tail}");
        // Widened by a print ulp: (d2 + ulp) r / (1 - r).
        let widened = remaining_drift(&[0.0, 1.0, 1.5], 0.1);
        assert!((widened - 0.6).abs() < 1e-12, "{widened}");
        // A series that stopped moving has only the print to price.
        assert_eq!(remaining_drift(&[1.0, 2.0, 2.0], 0.3), 0.3);
        assert_eq!(remaining_drift(&[1.0, 2.0, 2.0], 0.0), 0.0);
    }

    /// Not contracting: no tail.
    #[test]
    fn a_growing_or_constant_step_has_no_tail() {
        assert!(remaining_drift(&[0.0, 1.0, 2.0], 0.0).is_infinite());
        assert!(remaining_drift(&[0.0, 1.0, 2.5], 0.0).is_infinite());
    }

    /// Three points never read as a log drift; four can.
    #[test]
    fn the_trend_of_three_points_is_settling_or_diverging() {
        assert_eq!(trend(&[1.0, 2.0]), None);
        assert_eq!(trend(&[0.0, 1.0, 1.5]), Some(Trend::Settling));
        assert_eq!(trend(&[0.0, 1.0, 2.0]), Some(Trend::Diverging));
        // The same constant step on four points is the junction drift.
        assert_eq!(trend(&[0.0, 1.0, 2.0, 3.0]), Some(Trend::LogDrift));
        assert_eq!(classify(&[0.0, 1.0, 1.5, 1.75]), Trend::Settling);
        assert_eq!(classify(&[0.0, 1.0, 1.5, 2.5]), Trend::Diverging);
    }
}
