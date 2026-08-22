// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Parse a fnec `--loads-config` TOML file into Laplace-domain loads
//! ([`nec_solver::LaplaceLoad`]).
//!
//! There is no NEC-2 card for a rational `Z(s) = N(s)/D(s)` load, so fnec takes
//! it from a small TOML file:
//!
//! ```toml
//! # A series R+L load (Z = R + jωL) on tag 1, segment 5:
//! [[laplace_load]]
//! tag = 1
//! seg_first = 5
//! numerator   = [100.0, 1.0e-6]   # a0 + a1·s  ->  R + L·s
//! denominator = [1.0]
//! ```
//!
//! `tag`/`seg_first`/`seg_last` follow the LD-card convention (0 = all).

use nec_solver::LaplaceLoad;
use std::path::Path;

/// Read and parse the loads-config file. Returns an empty vector if the file
/// declares no `[[laplace_load]]` entries.
pub fn load_laplace_loads(path: &Path) -> Result<Vec<LaplaceLoad>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;

    let entries = match value.get("laplace_load") {
        Some(v) => v
            .as_array()
            .ok_or_else(|| "`laplace_load` must be an array of tables".to_string())?,
        None => return Ok(Vec::new()),
    };

    let mut out = Vec::with_capacity(entries.len());
    for (idx, e) in entries.iter().enumerate() {
        let get_u32 = |k: &str| e.get(k).and_then(toml::Value::as_integer).unwrap_or(0) as u32;
        let get_vec = |k: &str| -> Result<Vec<f64>, String> {
            let arr = e
                .get(k)
                .and_then(toml::Value::as_array)
                .ok_or_else(|| format!("laplace_load[{idx}]: `{k}` must be an array of numbers"))?;
            arr.iter()
                .map(|x| {
                    x.as_float()
                        .or_else(|| x.as_integer().map(|i| i as f64))
                        .ok_or_else(|| format!("laplace_load[{idx}].{k}: non-numeric coefficient"))
                })
                .collect()
        };
        out.push(LaplaceLoad {
            tag: get_u32("tag"),
            seg_first: get_u32("seg_first"),
            seg_last: get_u32("seg_last"),
            numerator: get_vec("numerator")?,
            denominator: get_vec("denominator")?,
        });
    }
    Ok(out)
}
