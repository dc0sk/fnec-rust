// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Pure plotting math for the sweep chart (GUI-CHK-009).
//!
//! The `canvas` rendering lives in the binary, but everything that can be a pure
//! function of the data — SWR from impedance, "nice" axis ticks, data bounds,
//! and nearest-point selection for the frequency cursor — lives here so it is
//! covered by the headless test suite.

/// Which quantity the sweep chart plots against frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotMetric {
    /// Voltage standing-wave ratio against a reference impedance.
    Swr,
    /// Impedance magnitude |Z| (Ω).
    ZMag,
}

impl PlotMetric {
    /// Axis label for this metric.
    pub fn label(self) -> &'static str {
        match self {
            PlotMetric::Swr => "SWR",
            PlotMetric::ZMag => "|Z| (Ω)",
        }
    }
}

/// Voltage standing-wave ratio for a load `Z = z_re + j·z_im` on a line of
/// characteristic impedance `z0` (Ω).
///
/// `SWR = (1+|Γ|)/(1-|Γ|)` with `Γ = (Z − Z0)/(Z + Z0)`. Returns `f64::INFINITY`
/// for a fully reflective load (|Γ| ≥ 1, e.g. a purely reactive impedance).
pub fn swr(z_re: f64, z_im: f64, z0: f64) -> f64 {
    let num = ((z_re - z0).powi(2) + z_im * z_im).sqrt();
    let den = ((z_re + z0).powi(2) + z_im * z_im).sqrt();
    if den == 0.0 {
        return f64::INFINITY;
    }
    let gamma = num / den;
    if gamma >= 1.0 {
        f64::INFINITY
    } else {
        (1.0 + gamma) / (1.0 - gamma)
    }
}

/// Impedance magnitude |Z| (Ω).
pub fn z_mag(z_re: f64, z_im: f64) -> f64 {
    (z_re * z_re + z_im * z_im).sqrt()
}

/// Inclusive `(min, max)` of a finite value slice, or `None` if it is empty or
/// has no finite entries.
pub fn finite_bounds(values: &[f64]) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut any = false;
    for &v in values {
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
            any = true;
        }
    }
    any.then_some((lo, hi))
}

/// "Nice" round tick positions covering `[min, max]`, roughly `target` of them.
///
/// Steps are chosen from the 1-2-5 decade sequence so the labels are readable
/// (…, 0.5, 1, 2, 5, 10, …). Returns an empty vec for a degenerate range.
pub fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !(min.is_finite() && max.is_finite()) || max <= min || target == 0 {
        return Vec::new();
    }
    let raw_step = (max - min) / target as f64;
    let mag = 10f64.powf(raw_step.log10().floor());
    let norm = raw_step / mag;
    let nice = if norm < 1.5 {
        1.0
    } else if norm < 3.0 {
        2.0
    } else if norm < 7.0 {
        5.0
    } else {
        10.0
    };
    let step = nice * mag;
    let first = (min / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut v = first;
    // Guard against a pathological step; cap the count generously.
    while v <= max + step * 1e-9 && ticks.len() < 1000 {
        ticks.push(v);
        v += step;
    }
    ticks
}

/// Linear map of `value` in `[in_lo, in_hi]` onto `[out_lo, out_hi]`.
///
/// A degenerate input range maps everything to the midpoint of the output.
pub fn map_range(value: f64, in_lo: f64, in_hi: f64, out_lo: f64, out_hi: f64) -> f64 {
    if in_hi <= in_lo {
        return (out_lo + out_hi) * 0.5;
    }
    let t = (value - in_lo) / (in_hi - in_lo);
    out_lo + t * (out_hi - out_lo)
}

/// Index of the swept frequency nearest to `target`, or `None` if empty.
pub fn nearest_index(freqs: &[f64], target: f64) -> Option<usize> {
    freqs
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (**a - target)
                .abs()
                .partial_cmp(&(**b - target).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swr_is_one_at_match() {
        // Z = Z0 → perfect match → SWR = 1.
        assert!((swr(50.0, 0.0, 50.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn swr_two_to_one() {
        // A 100 Ω resistive load on a 50 Ω line → SWR = 2.
        assert!((swr(100.0, 0.0, 50.0) - 2.0).abs() < 1e-9);
        // And a 25 Ω load → also SWR = 2 (reciprocal).
        assert!((swr(25.0, 0.0, 50.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn swr_infinite_for_pure_reactance() {
        assert!(swr(0.0, 75.0, 50.0).is_infinite());
    }

    #[test]
    fn z_mag_pythagoras() {
        assert!((z_mag(3.0, 4.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn finite_bounds_skips_non_finite() {
        let vals = [f64::INFINITY, 2.0, 5.0, f64::NAN, 1.0];
        assert_eq!(finite_bounds(&vals), Some((1.0, 5.0)));
        assert_eq!(finite_bounds(&[]), None);
        assert_eq!(finite_bounds(&[f64::INFINITY]), None);
    }

    #[test]
    fn nice_ticks_are_round_and_in_range() {
        let ticks = nice_ticks(14.0, 18.0, 4);
        assert!(!ticks.is_empty());
        assert!(ticks
            .iter()
            .all(|&t| (14.0 - 1e-9..=18.0 + 1e-9).contains(&t)));
        // 1-2-5 stepping over a range of 4 with ~4 ticks → step 1.0.
        assert!(ticks.windows(2).all(|w| (w[1] - w[0] - 1.0).abs() < 1e-9));
    }

    #[test]
    fn nice_ticks_degenerate_is_empty() {
        assert!(nice_ticks(5.0, 5.0, 4).is_empty());
        assert!(nice_ticks(f64::NAN, 1.0, 4).is_empty());
        assert!(nice_ticks(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn map_range_endpoints_and_degenerate() {
        assert!((map_range(14.0, 14.0, 18.0, 0.0, 100.0) - 0.0).abs() < 1e-9);
        assert!((map_range(18.0, 14.0, 18.0, 0.0, 100.0) - 100.0).abs() < 1e-9);
        assert!((map_range(16.0, 14.0, 18.0, 0.0, 100.0) - 50.0).abs() < 1e-9);
        // Degenerate input range → output midpoint.
        assert!((map_range(5.0, 5.0, 5.0, 0.0, 100.0) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn nearest_index_picks_closest() {
        let freqs = [14.0, 14.5, 15.0, 15.5, 16.0];
        assert_eq!(nearest_index(&freqs, 14.6), Some(1));
        assert_eq!(nearest_index(&freqs, 15.9), Some(4));
        assert_eq!(nearest_index(&freqs, 0.0), Some(0));
        assert_eq!(nearest_index(&[], 1.0), None);
    }
}
