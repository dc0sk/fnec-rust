// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Python bindings for fnec.
//!
//! Exposes two functions:
//! - `solve_deck_str(deck: str) -> dict`   — solve the first frequency point.
//! - `sweep_deck_str(deck: str) -> list[dict]` — solve all frequency points.
//!
//! Both functions return dicts with keys:
//!   `freq_mhz`, `tag`, `seg`, `z_re`, `z_im`, `z_abs`, `z_arg_deg`
//!
//! Errors are raised as `RuntimeError` with a descriptive message.

use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::{
    assemble_z_matrix_with_ground, build_excitation, build_geometry, build_hallen_rhs, build_loads,
    build_tl_stamps, detect_wire_junctions, ground_model_from_deck, solve_hallen,
    wire_endpoints_from_segs,
};
use num_complex::Complex64;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Extract all frequencies (Hz) from the FR cards in a deck.
fn frequencies_from_deck(deck: &nec_model::deck::NecDeck) -> Vec<f64> {
    let mut freqs = Vec::new();
    for card in &deck.cards {
        let Card::Fr(fr) = card else { continue };
        let step_count = fr.steps.max(1) as usize;
        for i in 0..step_count {
            let f_mhz = if fr.step_type == 1 {
                // Multiplicative: freq(i) = start * step_mhz^i
                fr.frequency_mhz * fr.step_mhz.powi(i as i32)
            } else {
                // Linear
                fr.frequency_mhz + fr.step_mhz * (i as f64)
            };
            freqs.push(f_mhz * 1e6);
        }
    }
    freqs
}

/// Solve a NEC deck string at one frequency and return an impedance record.
fn solve_at_freq(
    deck: &nec_model::deck::NecDeck,
    freq_hz: f64,
) -> Result<std::collections::HashMap<String, f64>, String> {
    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    if segs.is_empty() {
        return Err("deck has no geometry (no GW cards)".to_string());
    }
    let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let ground = ground_model_from_deck(deck);
    let wire_endpoints = wire_endpoints_from_segs(&segs);

    let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);
    let (load_vec, _) = build_loads(deck, &segs, freq_hz);
    z_mat.add_to_diagonal(&load_vec);
    let (tl_stamps, _) = build_tl_stamps(deck, &segs, freq_hz);
    for (row, col, delta) in &tl_stamps {
        z_mat.add_to_entry(*row, *col, *delta);
    }

    let hallen_rhs = build_hallen_rhs(deck, &segs, freq_hz).map_err(|e| e.to_string())?;
    let wire_junctions = detect_wire_junctions(&segs, &wire_endpoints, 1e-6);
    let junction_tuples: Vec<(usize, usize, f64)> = wire_junctions
        .iter()
        .map(|j| (j.seg_a, j.seg_b, j.sign))
        .collect();
    let sol = solve_hallen(
        &z_mat,
        &hallen_rhs.rhs,
        &hallen_rhs.cos_vec,
        &wire_endpoints,
        &junction_tuples,
    )
    .map_err(|e| e.to_string())?;

    let i_vec = &sol.currents;

    // Find the first EX card and compute feedpoint impedance.
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        let Some((idx, seg)) = segs
            .iter()
            .enumerate()
            .find(|(_, s)| s.tag == ex.tag && s.tag_index == ex.segment)
        else {
            continue;
        };
        let current: Complex64 = i_vec[idx];
        let v_source: Complex64 = v_vec[idx] * seg.length;
        let z_in: Complex64 = if current.norm() > 1e-60 {
            v_source / current
        } else {
            v_source
        };
        let z_abs = z_in.norm();
        let z_arg_deg = z_in.im.atan2(z_in.re).to_degrees();
        let freq_mhz = freq_hz / 1e6;
        let mut rec = std::collections::HashMap::new();
        rec.insert("freq_mhz".to_string(), freq_mhz);
        rec.insert("tag".to_string(), seg.tag as f64);
        rec.insert("seg".to_string(), seg.tag_index as f64);
        rec.insert("z_re".to_string(), z_in.re);
        rec.insert("z_im".to_string(), z_in.im);
        rec.insert("z_abs".to_string(), z_abs);
        rec.insert("z_arg_deg".to_string(), z_arg_deg);
        return Ok(rec);
    }
    Err("deck has no EX card — cannot compute feedpoint impedance".to_string())
}

/// Solve a NEC deck string at the first frequency defined by its FR card.
///
/// Returns a dict with keys: ``freq_mhz``, ``tag``, ``seg``,
/// ``z_re``, ``z_im``, ``z_abs``, ``z_arg_deg``.
///
/// Raises ``RuntimeError`` on parse or solver errors.
#[pyfunction]
fn solve_deck_str(py: Python<'_>, deck: &str) -> PyResult<PyObject> {
    let result = parse(deck)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("parse error: {e}")))?;
    let freqs = frequencies_from_deck(&result.deck);
    let freq_hz = freqs
        .first()
        .copied()
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("deck has no FR card"))?;
    let rec =
        solve_at_freq(&result.deck, freq_hz).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;

    let d = PyDict::new(py);
    for (k, v) in &rec {
        d.set_item(k, v)?;
    }
    Ok(d.into())
}

/// Solve a NEC deck string at all frequency points defined by its FR card(s).
///
/// Returns a list of dicts, one per frequency point, each with keys:
/// ``freq_mhz``, ``tag``, ``seg``, ``z_re``, ``z_im``, ``z_abs``, ``z_arg_deg``.
///
/// Raises ``RuntimeError`` on parse or solver errors.
#[pyfunction]
fn sweep_deck_str(py: Python<'_>, deck: &str) -> PyResult<PyObject> {
    let result = parse(deck)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("parse error: {e}")))?;
    let freqs = frequencies_from_deck(&result.deck);
    if freqs.is_empty() {
        return Ok(pyo3::types::PyList::empty(py).into());
    }

    let mut records = Vec::with_capacity(freqs.len());
    for freq_hz in freqs {
        let rec = solve_at_freq(&result.deck, freq_hz)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        let d = PyDict::new(py);
        for (k, v) in &rec {
            d.set_item(k, v)?;
        }
        records.push(d.into_pyobject(py)?.into_any().unbind());
    }
    Ok(pyo3::types::PyList::new(py, records)?.into())
}

/// fnec Python bindings — NEC deck solver.
#[pymodule]
fn fnec_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(solve_deck_str, m)?)?;
    m.add_function(wrap_pyfunction!(sweep_deck_str, m)?)?;
    Ok(())
}
