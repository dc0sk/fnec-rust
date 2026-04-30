// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Resonance-targeting helper (PH3-CHK-008).
//!
//! Performs a bisection search over one template variable (e.g. a wire
//! half-length) to find the value at which the feedpoint reactance matches a
//! target (typically 0 Ω for series resonance).
//!
//! # File format (`examples/resonance-search.nec.toml`)
//!
//! ```toml
//! [search]
//! var   = "HALF_LEN"
//! lo    = 4.5
//! hi    = 6.0
//! target_reactance_ohm = 0.0
//! tolerance_ohm        = 0.5   # optional, default 0.5
//! max_iter             = 50    # optional, default 50
//!
//! [deck]
//! template = """
//! GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN 0.001
//! GE
//! EX 0 1 26 0 1.0 0.0
//! FR 0 1 0 0 14.2 0.0
//! EN
//! """
//! ```
//!
//! Run with:
//! ```text
//! fnec sweep --resonance examples/resonance-search.nec.toml
//! ```
//!
//! # Output (stdout)
//!
//! ```text
//! RESONANCE_SEARCH_RESULT
//! VAR HALF_LEN
//! CONVERGED_VALUE 5.192382
//! Z_RE 73.112345
//! Z_IM -0.312456
//! ITERATIONS 14
//! CONVERGED true
//! ```

use serde::Deserialize;

/// The `[search]` table of a `.nec.toml` resonance-search file.
#[derive(Debug, Deserialize)]
pub struct ResonanceSearchConfig {
    /// Template variable name to search over.
    pub var: String,
    /// Lower bound of the search range.
    pub lo: f64,
    /// Upper bound of the search range.
    pub hi: f64,
    /// Target feedpoint reactance in ohms (0 = series resonance).
    pub target_reactance_ohm: f64,
    /// Convergence tolerance in ohms.  Default 0.5.
    #[serde(default = "default_tol")]
    pub tolerance_ohm: f64,
    /// Maximum bisection iterations.  Default 50.
    #[serde(default = "default_max_iter")]
    pub max_iter: usize,
}

fn default_tol() -> f64 {
    0.5
}
fn default_max_iter() -> usize {
    50
}

/// The `[deck]` table of a `.nec.toml` resonance-search file.
#[derive(Debug, Deserialize)]
pub struct ResonanceDeckConfig {
    /// NEC deck template with `$VARNAME` tokens.
    pub template: String,
}

/// Top-level structure of a `.nec.toml` resonance-search file.
#[derive(Debug, Deserialize)]
pub struct ResonanceFile {
    pub search: ResonanceSearchConfig,
    pub deck: ResonanceDeckConfig,
}

impl ResonanceFile {
    /// Parse a `.nec.toml` file from a string.
    pub fn from_toml(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| format!("resonance file parse error: {e}"))
    }

    /// Load a `.nec.toml` file from disk.
    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
        Self::from_toml(&s)
    }
}

/// Result of a completed resonance search.
#[derive(Debug)]
pub struct ResonanceResult {
    /// Final value of the search variable.
    pub converged_value: f64,
    /// Feedpoint resistance at the converged value.
    pub final_z_re: f64,
    /// Feedpoint reactance at the converged value.
    pub final_z_im: f64,
    /// Number of probe evaluations performed.
    pub iterations: usize,
    /// Whether the search converged within `tolerance_ohm`.
    pub converged: bool,
}

/// Bisection search: find the value of a parameter in `[lo, hi]` such that
/// `probe(value)` returns `(z_re, z_im)` with `|z_im - target| <= tol`.
///
/// `probe` must be callable many times and should return `(z_re, z_im)` for
/// the feedpoint impedance at the given parameter value.
///
/// Returns `Err` if the root is not bracketed (z_im does not cross target
/// between `lo` and `hi`), or if `probe` itself returns an error.
pub fn bisect<F>(
    lo: f64,
    hi: f64,
    target: f64,
    tol: f64,
    max_iter: usize,
    probe: F,
) -> Result<ResonanceResult, String>
where
    F: Fn(f64) -> Result<(f64, f64), String>,
{
    if lo >= hi {
        return Err(format!(
            "resonance search: lo ({lo}) must be less than hi ({hi})"
        ));
    }

    let (z_re_lo, z_im_lo) = probe(lo)?;
    let f_lo = z_im_lo - target;

    if f_lo.abs() <= tol {
        return Ok(ResonanceResult {
            converged_value: lo,
            final_z_re: z_re_lo,
            final_z_im: z_im_lo,
            iterations: 1,
            converged: true,
        });
    }

    let (z_re_hi, z_im_hi) = probe(hi)?;
    let f_hi = z_im_hi - target;

    if f_hi.abs() <= tol {
        return Ok(ResonanceResult {
            converged_value: hi,
            final_z_re: z_re_hi,
            final_z_im: z_im_hi,
            iterations: 2,
            converged: true,
        });
    }

    if f_lo * f_hi > 0.0 {
        return Err(format!(
            "resonance search: z_im does not bracket target={target:.3} over [{lo}, {hi}]; \
             z_im({lo:.4}) = {z_im_lo:.3} Ω, z_im({hi:.4}) = {z_im_hi:.3} Ω"
        ));
    }

    let mut a = lo;
    let mut b = hi;
    let mut fa = f_lo;
    let mut last_z_re = z_re_lo;
    let mut last_z_im = z_im_lo;
    let mut last_mid = lo;

    // 2 probes already done above.
    let mut iter = 2usize;

    while iter < max_iter + 2 {
        let mid = (a + b) / 2.0;
        let (z_re, z_im) = probe(mid)?;
        let fm = z_im - target;
        last_z_re = z_re;
        last_z_im = z_im;
        last_mid = mid;
        iter += 1;

        if fm.abs() <= tol || (b - a) / 2.0 < 1e-9 {
            return Ok(ResonanceResult {
                converged_value: mid,
                final_z_re: z_re,
                final_z_im: z_im,
                iterations: iter,
                converged: true,
            });
        }

        if fa * fm < 0.0 {
            b = mid;
        } else {
            a = mid;
            fa = fm;
        }
    }

    // Max iterations reached.
    Ok(ResonanceResult {
        converged_value: last_mid,
        final_z_re: last_z_re,
        final_z_im: last_z_im,
        iterations: iter,
        converged: (last_z_im - target).abs() <= tol,
    })
}

/// Emit the structured resonance result to stdout.
pub fn print_result(var_name: &str, result: &ResonanceResult) {
    println!("RESONANCE_SEARCH_RESULT");
    println!("VAR {var_name}");
    println!("CONVERGED_VALUE {:.6}", result.converged_value);
    println!("Z_RE {:.6}", result.final_z_re);
    println!("Z_IM {:.6}", result.final_z_im);
    println!("ITERATIONS {}", result.iterations);
    println!("CONVERGED {}", result.converged);
}
