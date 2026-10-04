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

/// The keys an entry may carry. Anything else is a typo the parser would
/// otherwise have read as "absent" (FND-207).
const ENTRY_KEYS: [&str; 5] = ["tag", "seg_first", "seg_last", "numerator", "denominator"];

/// Read and parse the loads-config file.
///
/// Strict, because every lenient reading was a silent wrong answer (FND-207): a
/// missing or misspelled index read as 0, which the LD convention means "all",
/// so `segment = 11` loaded every segment (866.41 − j431.27 Ω against the
/// intended 162.97 − j54.38); a float tag read as 0, loading every wire; and a
/// misspelled table read as "no loads", solving the antenna unloaded — and
/// slipping past every refusal keyed on "were loads given". So: the one table
/// name, the five keys, `tag` and `seg_first` required and non-negative
/// integers, and at least one load.
pub fn load_laplace_loads(path: &Path) -> Result<Vec<LaplaceLoad>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let file = path.display();

    let table = value
        .as_table()
        .ok_or_else(|| format!("{file}: expected a TOML table"))?;
    if let Some(k) = table.keys().find(|k| k.as_str() != "laplace_load") {
        return Err(format!(
            "{file}: unknown key `{k}`; a loads file holds `[[laplace_load]]` entries only"
        ));
    }
    let entries = match value.get("laplace_load") {
        Some(v) => v
            .as_array()
            .ok_or_else(|| "`laplace_load` must be an array of tables".to_string())?,
        None => {
            return Err(format!(
                "{file}: declares no `[[laplace_load]]` entries; --loads-config was given a \
                 file with no loads"
            ))
        }
    };
    if entries.is_empty() {
        return Err(format!("{file}: `laplace_load` is empty"));
    }

    let mut out = Vec::with_capacity(entries.len());
    for (idx, e) in entries.iter().enumerate() {
        let t = e
            .as_table()
            .ok_or_else(|| format!("laplace_load[{idx}]: must be a table"))?;
        if let Some(k) = t.keys().find(|k| !ENTRY_KEYS.contains(&k.as_str())) {
            return Err(format!(
                "laplace_load[{idx}]: unknown key `{k}` (expected one of {})",
                ENTRY_KEYS.join(", ")
            ));
        }
        let get_index = |k: &str, required: bool| -> Result<u32, String> {
            match e.get(k) {
                None if required => Err(format!(
                    "laplace_load[{idx}]: `{k}` is required (0 means all, as on an LD card)"
                )),
                None => Ok(0),
                Some(v) => v
                    .as_integer()
                    .and_then(|i| u32::try_from(i).ok())
                    .ok_or_else(|| {
                        format!("laplace_load[{idx}].{k}: must be a non-negative integer, got {v}")
                    }),
            }
        };
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
            tag: get_index("tag", true)?,
            seg_first: get_index("seg_first", true)?,
            seg_last: get_index("seg_last", false)?,
            numerator: get_vec("numerator")?,
            denominator: get_vec("denominator")?,
        });
    }
    Ok(out)
}
