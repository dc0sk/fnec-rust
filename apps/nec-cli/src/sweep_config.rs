// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! TOML parameter-sweep configuration reader.
//!
//! A sweep-config file specifies a frequency list either as a linear range
//! (start + step + count / end) or as an explicit list of frequency points.
//! When `--sweep-config <file>` is supplied, the resulting frequency list
//! replaces the one derived from the deck's `FR` card.
//!
//! # File format
//!
//! Range-based (linear step):
//! ```toml
//! [frequency]
//! start_mhz = 14.0
//! end_mhz   = 18.0
//! step_mhz  = 0.5
//! ```
//!
//! Explicit point list:
//! ```toml
//! [frequency]
//! points_mhz = [14.0, 14.5, 15.0, 16.0]
//! ```

use serde::Deserialize;

/// Raw TOML representation of the `[frequency]` table.
#[derive(Debug, Deserialize)]
struct FrequencySpec {
    /// Starting frequency in MHz (range mode).
    start_mhz: Option<f64>,
    /// Ending frequency in MHz, inclusive (range mode).
    end_mhz: Option<f64>,
    /// Step size in MHz (range mode).
    step_mhz: Option<f64>,
    /// Explicit frequency list in MHz (list mode).
    points_mhz: Option<Vec<f64>>,
}

/// Top-level TOML structure for a sweep-config file.
#[derive(Debug, Deserialize)]
struct SweepConfigToml {
    frequency: FrequencySpec,
}

/// A validated, resolved sweep configuration.
#[derive(Debug, Clone)]
pub struct SweepConfig {
    /// Frequency points in Hz, in solve order.
    pub frequencies_hz: Vec<f64>,
}

/// Error returned when a sweep-config file cannot be parsed or is invalid.
#[derive(Debug)]
pub struct SweepConfigError(pub String);

impl std::fmt::Display for SweepConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sweep-config error: {}", self.0)
    }
}

impl SweepConfig {
    /// Parse and validate a sweep-config from a TOML string.
    pub fn from_toml(s: &str) -> Result<Self, SweepConfigError> {
        let raw: SweepConfigToml =
            toml::from_str(s).map_err(|e| SweepConfigError(format!("TOML parse error: {e}")))?;

        let freq = raw.frequency;

        // Explicit list takes priority.
        if let Some(points) = freq.points_mhz {
            if points.is_empty() {
                return Err(SweepConfigError(
                    "frequency.points_mhz must not be empty".to_string(),
                ));
            }
            for &p in &points {
                // The shared predicate, not `p <= 0.0`: that comparison is false
                // for NaN, so a NaN point walked through validation and reached
                // the solver, which reported it as a convergence failure — or,
                // on a receive deck with no feedpoint to guard it, exited 0 with
                // a report of NaN currents (FND-109, FND-076).
                if !nec_solver::is_usable_frequency_mhz(p) {
                    return Err(SweepConfigError(format!(
                        "frequency point {p} MHz is not a usable frequency; \
                         frequencies must be finite and > 0"
                    )));
                }
            }
            return Ok(SweepConfig {
                frequencies_hz: points.iter().map(|&mhz| mhz * 1e6).collect(),
            });
        }

        // Range mode.
        let start = freq.start_mhz.ok_or_else(|| {
            SweepConfigError(
                "frequency.start_mhz is required when points_mhz is absent".to_string(),
            )
        })?;
        let end = freq.end_mhz.ok_or_else(|| {
            SweepConfigError("frequency.end_mhz is required when points_mhz is absent".to_string())
        })?;
        let step = freq.step_mhz.ok_or_else(|| {
            SweepConfigError("frequency.step_mhz is required when points_mhz is absent".to_string())
        })?;

        if !nec_solver::is_usable_frequency_mhz(start) {
            return Err(SweepConfigError(format!(
                "frequency.start_mhz ({start}) is not a usable frequency; \
                 frequencies must be finite and > 0"
            )));
        }
        if !step.is_finite() || step <= 0.0 {
            return Err(SweepConfigError(format!(
                "frequency.step_mhz ({step}) must be finite and > 0"
            )));
        }
        if !end.is_finite() {
            return Err(SweepConfigError(format!(
                "frequency.end_mhz ({end}) must be finite"
            )));
        }
        if end < start {
            return Err(SweepConfigError(format!(
                "frequency.end_mhz ({end}) must be >= start_mhz ({start})"
            )));
        }

        // Bound the count BEFORE expanding. The FR seam has carried this cap
        // since #417 and the GUI has its own; this third entry point had
        // neither, so `start=1, end=30000, step=1e-7` aborted the process on a
        // 2 GB allocation before any solve ran (FND-109). Computed rather than
        // discovered by pushing: `f += step` is a no-op for a large start and a
        // tiny step, so a loop that counts as it goes may never terminate.
        let span = (end - start) / step;
        if !span.is_finite() || span + 1.0 > nec_solver::MAX_FR_POINTS as f64 {
            return Err(SweepConfigError(format!(
                "frequency range {start}..{end} MHz in steps of {step} asks for \
                 {:.0} points; the limit is {}. Widen the step or narrow the range",
                span + 1.0,
                nec_solver::MAX_FR_POINTS
            )));
        }

        let mut points = Vec::new();
        let mut f = start;
        while f <= end + step * 1e-9 {
            points.push(f * 1e6);
            f += step;
        }

        if points.is_empty() {
            return Err(SweepConfigError(
                "frequency range produces no points".to_string(),
            ));
        }

        Ok(SweepConfig {
            frequencies_hz: points,
        })
    }

    /// Load a sweep-config from a file path.
    pub fn from_file(path: &std::path::Path) -> Result<Self, SweepConfigError> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| SweepConfigError(format!("cannot read '{}': {e}", path.display())))?;
        Self::from_toml(&s)
    }
}

#[cfg(test)]
mod shared_frequency_gate_tests {
    use super::*;

    fn parse(toml: &str) -> Result<SweepConfig, SweepConfigError> {
        SweepConfig::from_toml(toml)
    }

    /// `p <= 0.0` is false for NaN, so the old check passed it. A full matrix was
    /// then assembled at NaN Hz; on a driven deck the feedpoint guard reported it
    /// as a convergence failure, and on a receive deck — which has no feedpoint to
    /// guard it — the run exited 0 with a report of NaN currents (FND-076).
    #[test]
    fn a_nan_frequency_point_is_refused_by_validation() {
        let e = parse("[frequency]\npoints_mhz = [nan, 14.2]\n").unwrap_err();
        assert!(e.0.contains("not a usable frequency"), "{}", e.0);
    }

    #[test]
    fn an_infinite_frequency_point_is_refused_by_validation() {
        let e = parse("[frequency]\npoints_mhz = [inf]\n").unwrap_err();
        assert!(e.0.contains("not a usable frequency"), "{}", e.0);
    }

    /// Range mode had no point cap at all, while both sibling expanders carry
    /// one: `start=1, end=30000, step=1e-7` aborted the process on a 2 GB
    /// allocation before any solve ran (FND-109).
    ///
    /// Deliberately asks for ~1e6 points rather than the 3e11 of the original
    /// reproduction. Both are over the limit, so both gate the same branch — but
    /// a test for an allocation cap must not itself become an unbounded
    /// allocation when someone removes the cap to check that this test fails.
    /// The 3e11 version spent over a minute growing a vector under sabotage and
    /// had to be killed; this one costs 8 MB if the guard is gone.
    #[test]
    fn an_oversized_range_is_refused_before_it_is_expanded() {
        let e = parse("[frequency]\nstart_mhz = 1.0\nend_mhz = 2.0\nstep_mhz = 0.000001\n")
            .unwrap_err();
        assert!(e.0.contains("the limit is"), "{}", e.0);
    }

    #[test]
    fn a_reasonable_range_still_expands() {
        let c = parse("[frequency]\nstart_mhz = 14.0\nend_mhz = 14.4\nstep_mhz = 0.1\n").unwrap();
        assert_eq!(c.frequencies_hz.len(), 5);
        assert!((c.frequencies_hz[0] - 14.0e6).abs() < 1.0);
    }

    #[test]
    fn a_nan_range_bound_is_refused_rather_than_compared() {
        let e =
            parse("[frequency]\nstart_mhz = nan\nend_mhz = 30.0\nstep_mhz = 0.1\n").unwrap_err();
        assert!(e.0.contains("not a usable frequency"), "{}", e.0);
    }
}
