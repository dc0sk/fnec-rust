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
//! Errors are raised as `RuntimeError` with a descriptive message. Geometry the
//! solver cannot honestly take — wires crossing mid-span, a source on a degenerate
//! segment, a wire reaching into an active ground — is rejected the same way the
//! CLI rejects it, rather than silently producing a number. Non-fatal caveats
//! (parser warnings, an unreliable topology, a very low antenna over finite
//! ground) are emitted as Python `UserWarning`s, so they are visible by default and
//! can be filtered or escalated with the standard `warnings` module.

use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::validate;
use nec_solver::{
    assemble_z_matrix_with_ground, build_excitation, build_geometry, build_hallen_rhs,
    detect_wire_junctions, ground_model_from_deck, solve_hallen, wire_endpoints_from_segs,
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

/// Solve a NEC deck string at one frequency.
///
/// Returns the impedance record and the non-fatal caveats the caller should raise
/// as Python warnings. `Err` means the deck was rejected — either it could not be
/// solved at all, or `nec_solver::validate` found geometry outside the supported
/// class, which the CLI has always refused and these bindings used to solve
/// silently (review-260719 FIND-004).
fn solve_at_freq(
    deck: &nec_model::deck::NecDeck,
    freq_hz: f64,
) -> Result<(std::collections::HashMap<String, f64>, Vec<String>), String> {
    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    if segs.is_empty() {
        return Err("deck has no geometry (no GW cards)".to_string());
    }
    let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let ground = ground_model_from_deck(deck);
    let wire_endpoints = wire_endpoints_from_segs(&segs);

    // Same checks, in the same order, as the CLI and the GUI.
    let mut warnings = Vec::new();
    for d in validate::diagnose(deck, &segs, &ground, freq_hz) {
        match d.level {
            nec_model::DiagnosticLevel::Error => return Err(d.message),
            nec_model::DiagnosticLevel::Warning => warnings.push(d.message),
        }
    }

    let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);
    // The shared seam: LD loads, TL lines and NT networks. NT was previously
    // absent here, so the same deck solved to a different impedance than the CLI
    // (FND-015).
    let stamps = nec_solver::build_deck_stamps(deck, &segs, freq_hz);
    warnings.extend(stamps.warnings.iter().cloned());
    stamps.apply(&mut z_mat);

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
    // Through the shared seam (FND-031). This loop took the first `EX` of any
    // type, so a plane wave's NTHETA/NPHI could be reported as a feedpoint.
    if let Some(ex) = nec_solver::first_delta_gap_feedpoint(deck) {
        let Some((idx, seg)) = segs
            .iter()
            .enumerate()
            .find(|(_, s)| s.tag == ex.tag && s.tag_index == ex.segment)
        else {
            // Unreachable today: `build_hallen_rhs` rejects an EX naming an absent
            // segment before this runs. Kept defensive, but saying what would
            // actually be true — the deck HAS an EX; its segment is missing.
            return Err(format!(
                "EX on tag {} segment {} names a segment the geometry does not contain",
                ex.tag, ex.segment
            ));
        };
        let current: Complex64 = i_vec[idx];
        let v_source: Complex64 = v_vec[idx] * seg.length;
        let z_in: Complex64 = if current.norm() > 1e-60 {
            v_source / current
        } else {
            v_source
        };
        // FND-014: a negative Re(Z) is physically impossible for a passive antenna.
        // The CLI has warned about this since PH9-CHK-005; here it was silent, so a
        // junctioned deck returned an unreliable impedance as if it were sound.
        // Appended after the feedpoint is resolved because the message names the
        // tag and segment.
        if let Some(w) = nec_solver::validate::negative_resistance_warning(
            z_in.re,
            seg.tag as usize,
            seg.tag_index as usize,
            deck,
            &segs,
        ) {
            warnings.push(w);
        }
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
        return Ok((rec, warnings));
    }
    Err("deck has no EX card — cannot compute feedpoint impedance".to_string())
}

/// Raise each message as a Python `UserWarning`, so a caveat is visible by default
/// and can be filtered or turned into an error with the standard `warnings` module.
///
/// Duplicates are dropped: a sweep would otherwise repeat the same geometry caveat
/// once per frequency point.
fn emit_warnings(py: Python<'_>, messages: &[String], seen: &mut Vec<String>) -> PyResult<()> {
    let category = py.get_type::<pyo3::exceptions::PyUserWarning>();
    for m in messages {
        if seen.iter().any(|s| s == m) {
            continue;
        }
        seen.push(m.clone());
        let text = std::ffi::CString::new(m.as_str()).map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("warning contains a NUL byte")
        })?;
        PyErr::warn(py, &category, &text, 1)?;
    }
    Ok(())
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
    let (rec, mut warnings) =
        solve_at_freq(&result.deck, freq_hz).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
    let mut seen = Vec::new();
    let parse_warnings: Vec<String> = result.warnings.iter().map(ToString::to_string).collect();
    emit_warnings(py, &parse_warnings, &mut seen)?;
    warnings.sort();
    emit_warnings(py, &warnings, &mut seen)?;

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

    let mut seen = Vec::new();
    let parse_warnings: Vec<String> = result.warnings.iter().map(ToString::to_string).collect();
    emit_warnings(py, &parse_warnings, &mut seen)?;

    let mut records = Vec::with_capacity(freqs.len());
    for freq_hz in freqs {
        let (rec, mut warnings) = solve_at_freq(&result.deck, freq_hz)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        warnings.sort();
        emit_warnings(py, &warnings, &mut seen)?;
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
