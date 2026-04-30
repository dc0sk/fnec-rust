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
                if p <= 0.0 {
                    return Err(SweepConfigError(format!(
                        "frequency point {p} MHz is not positive"
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

        if start <= 0.0 {
            return Err(SweepConfigError(format!(
                "frequency.start_mhz ({start}) must be positive"
            )));
        }
        if step <= 0.0 {
            return Err(SweepConfigError(format!(
                "frequency.step_mhz ({step}) must be positive"
            )));
        }
        if end < start {
            return Err(SweepConfigError(format!(
                "frequency.end_mhz ({end}) must be >= start_mhz ({start})"
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
