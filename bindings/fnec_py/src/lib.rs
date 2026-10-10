// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Python bindings for fnec.
//!
//! Exposes three functions:
//! - `solve_deck_str(deck: str) -> dict`   — solve the first frequency point.
//! - `sweep_deck_str(deck: str) -> list[dict]` — solve all frequency points.
//! - `solve_currents_deck_str(deck: str) -> dict` — the segment currents at the
//!   first frequency point; the one entry point that answers a plane-wave
//!   receive deck, which has no feedpoint impedance (FND-108).
//!
//! The two impedance functions return dicts with keys:
//!   `freq_mhz`, `tag`, `seg`, `z_re`, `z_im`, `z_abs`, `z_arg_deg`
//!
//! Errors are raised as `RuntimeError` with a descriptive message. Geometry the
//! solver cannot honestly take — wires crossing mid-span, a source on a degenerate
//! segment, a wire reaching into an active ground — is rejected the same way the
//! CLI rejects it, rather than silently producing a number. Non-fatal caveats
//! (parser warnings, an unreliable topology, a very low antenna over finite
//! ground) are emitted as Python `UserWarning`s, so they are visible by default and
//! can be filtered or escalated with the standard `warnings` module.

use nec_parser::parse;
use nec_solver::validate;
use nec_solver::{
    assemble_z_matrix_with_ground, build_excitation, build_geometry, ground_model_from_deck,
};
use num_complex::Complex64;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Extract all frequencies (Hz) from the FR cards in a deck.
/// The frequencies this deck will be solved at.
///
/// Delegates to `nec_solver::frequency`, the one expansion. This read **every**
/// `FR` card; `nec2c` runs only the last one before execution (FND-057).
fn frequencies_from_deck(deck: &nec_model::deck::NecDeck) -> Vec<f64> {
    nec_solver::frequencies_hz(deck)
}

/// The solver context these bindings run under.
///
/// Hallén-only for now — `fnec_py` exposes no solver choice, so a diagnostic that
/// recommends the MPIE must tell a Python caller to reach for the CLI. Adopting
/// `solve_mpie_session` here is tracked separately; what matters is that the
/// choice is stated once rather than defaulted three times.
fn py_solver_context(
    kind: nec_solver::validate::SolverKind,
) -> nec_solver::validate::SolverContext<'static> {
    nec_solver::validate::SolverContext {
        kind,
        // The bindings have a solver argument now, so the remedy names it rather
        // than pointing a Python caller at a different program (FND-055).
        mpie_remedy: "pass solver=\"mpie\"",
        // The bindings have no Sommerfeld option, so their caveats name none (FND-217).
        sommerfeld_remedy: None,
        // The bindings' loads are the deck's own; the negative-resistance sites
        // describe them with `with_loads` (FND-209).
        loads: nec_solver::validate::RunLoads::NONE,
    }
}

/// A deck solved for its currents at one frequency.
struct SolvedStructure {
    segs: Vec<nec_solver::Segment>,
    /// The applied-field excitation vector, which prices a delta gap.
    v_vec: Vec<Complex64>,
    /// The wire currents — what radiates, and what the CLI's current table prints.
    wire: Vec<Complex64>,
    /// The currents the sources deliver: the wire current plus any TL/NT branch
    /// in parallel with a feed (FND-123). Equal to `wire` without networks.
    source: Vec<Complex64>,
    /// The solved port voltage of a current-source deck.
    port_voltage: Option<Complex64>,
    warnings: Vec<String>,
}

/// Validate and solve a deck at one frequency, for every drive the shared routed
/// solve takes — delta gap, current source, and plane-wave receive.
///
/// `Err` means the deck was rejected — either it could not be solved at all, or
/// `nec_solver::validate` found geometry outside the supported class, which the
/// CLI has always refused and these bindings used to solve silently
/// (review-260719 FIND-004). The impedance and the currents entry points both
/// start here, so they cannot disagree about which decks are solvable.
fn solve_structure(
    deck: &nec_model::deck::NecDeck,
    freq_hz: f64,
    solver: nec_solver::validate::SolverKind,
) -> Result<SolvedStructure, String> {
    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    if segs.is_empty() {
        return Err("deck has no geometry (no GW cards)".to_string());
    }
    let ground = ground_model_from_deck(deck);

    // Same checks, in the same order, as the CLI and the GUI: the refusals before
    // the excitation is built, so a deck with two faults names the same one on
    // every frontend (FND-180).
    let mut warnings = Vec::new();
    for d in validate::diagnose(deck, &segs, &ground, freq_hz, py_solver_context(solver)) {
        match d.level {
            nec_model::DiagnosticLevel::Error => return Err(d.message),
            nec_model::DiagnosticLevel::Warning => warnings.push(d.message),
        }
    }
    let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;

    // The MPIE builds its own system from the geometry and reads neither the
    // assembled matrix nor the Hallén bookkeeping, so assembling them here would
    // be an O(N^2) fill computed and discarded (FND-055). Its refusals travel
    // inside `solve_mpie_session`, so this branch cannot hand it a deck it would
    // answer wrongly.
    let mpie = solver == nec_solver::validate::SolverKind::Mpie;

    let mut z_mat = if mpie {
        nec_solver::ZMatrix::new(0)
    } else {
        assemble_z_matrix_with_ground(&segs, freq_hz, &ground)
    };
    // The shared seam's notes: LD loads, TL lines and NT networks (FND-015). The
    // loads and networks themselves are applied inside the routed solve.
    if !mpie {
        let stamps = nec_solver::build_deck_stamps(deck, &segs, freq_hz);
        warnings.extend(stamps.warnings.iter().cloned());
    }

    // A current-driven deck needs a different solve, not a different pricing step:
    // its excitation vector is all zeros, so `V/I` has nothing to divide. The
    // machinery was always in `nec_solver`; the bindings just never called it, and
    // said "use the fnec CLI" instead (FND-045).
    //
    // Which member of the Hallén family a deck needs — plane-wave, current-source,
    // or delta-gap on the merged-conductor or the conductor-path basis — is now
    // one decision shared by every frontend (FND-121). This branch used to end in
    // a plain `solve_hallen` with no paths arm, so a bent or split geometry was
    // answered on the wrong basis, silently, exactly as in the worker and the GUI.
    let (wire, source, port_voltage) = if mpie {
        // Its refusals travel inside `solve_mpie_session` (#414), so this branch
        // cannot hand it a deck it would answer with a card silently ignored.
        let currents = nec_solver::solve_mpie_session(deck, &segs, &ground, freq_hz)
            .map_err(|e| e.to_string())?;
        (currents.clone(), currents, None)
    } else {
        let loads = nec_solver::build_deck_stamps(deck, &segs, freq_hz).diagonal;
        let routed = nec_solver::solve_hallen_routed(deck, &segs, &mut z_mat, freq_hz, &loads)
            .map_err(|e| e.to_string())?;
        // The source currents: a feed that is also a TL/NT port delivers the
        // network branch too, and the impedance is priced from that (FND-123).
        let source = routed.source_currents();
        (routed.currents, source, routed.port_voltage)
    };
    Ok(SolvedStructure {
        segs,
        v_vec,
        wire,
        source,
        port_voltage,
        warnings,
    })
}

/// Solve a NEC deck string at one frequency, for its feedpoint impedance.
///
/// Returns the impedance record and the non-fatal caveats the caller should raise
/// as Python warnings. `Err` means the deck was rejected (see [`solve_structure`])
/// or has no feedpoint to price.
fn solve_at_freq(
    deck: &nec_model::deck::NecDeck,
    freq_hz: f64,
    solver: nec_solver::validate::SolverKind,
) -> Result<(std::collections::HashMap<String, f64>, Vec<String>), String> {
    let SolvedStructure {
        segs,
        v_vec,
        source: currents,
        port_voltage,
        mut warnings,
        ..
    } = solve_structure(deck, freq_hz, solver)?;
    let i_vec = &currents;

    if let Some(v_port) = port_voltage {
        let (ex, _) = nec_solver::feedpoints(deck)
            .find(|(_, role)| *role == nec_model::card::FeedpointRole::CurrentSource)
            .ok_or("a current-source solve without a current source")?;
        let i0 = Complex64::new(ex.voltage_real, ex.voltage_imag);
        let z_in =
            nec_solver::feedpoint_impedance(v_port, i0, ex.tag as usize, ex.segment as usize)
                .map_err(|e| e.to_string())?;
        let mut rec = std::collections::HashMap::new();
        rec.insert("freq_mhz".to_string(), freq_hz / 1e6);
        rec.insert("tag".to_string(), f64::from(ex.tag));
        rec.insert("seg".to_string(), f64::from(ex.segment));
        rec.insert("z_re".to_string(), z_in.re);
        rec.insert("z_im".to_string(), z_in.im);
        rec.insert("z_abs".to_string(), z_in.norm());
        rec.insert("z_arg_deg".to_string(), z_in.im.atan2(z_in.re).to_degrees());
        if let Some(w) = nec_solver::validate::negative_resistance_warning(
            z_in.re,
            ex.tag as usize,
            ex.segment as usize,
            deck,
            &segs,
            py_solver_context(solver).with_loads(nec_solver::validate::RunLoads::of_deck(
                deck,
                &segs,
                &[freq_hz],
            )),
        ) {
            warnings.push(w);
        }
        return Ok((rec, warnings));
    }

    // Find the first EX card and compute feedpoint impedance.
    // Through the shared seam (FND-031). This loop took the first `EX` of any
    // type, so a plane wave's NTHETA/NPHI could be reported as a feedpoint.
    if let Some(ex) = nec_solver::first_delta_gap_feedpoint(deck) {
        let Some((idx, seg)) =
            nec_solver::find_deck_segment(&segs, ex.tag, ex.segment).map(|i| (i, &segs[i]))
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
        let z_in: Complex64 = nec_solver::feedpoint_impedance(
            v_source,
            current,
            ex.tag as usize,
            ex.segment as usize,
        )
        .map_err(|e| e.to_string())?;
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
            py_solver_context(solver).with_loads(nec_solver::validate::RunLoads::of_deck(
                deck,
                &segs,
                &[freq_hz],
            )),
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
    // A plane-wave receive deck: solvable, but with no feedpoint to price. The
    // refusal names the entry point that answers it (FND-108).
    Err(nec_solver::validate::unpriceable_feedpoint_error(
        deck,
        "call fnec_py.solve_currents_deck_str for its induced currents",
    ))
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

/// Map the Python `solver=` argument onto the shared solver kind.
///
/// `fnec_py` was the last frontend without a solver choice: the CLI has
/// `--solver mpie` and the GUI a picker, so a Python caller with a T/Y junction
/// or a closed loop was told to reach for a different program (FND-055). The
/// machinery is one library call — the same `solve_mpie_session` both others use.
///
/// Defaulted rather than required, so every existing caller keeps its behaviour.
fn solver_from_name(name: &str) -> PyResult<nec_solver::validate::SolverKind> {
    match name {
        "hallen" => Ok(nec_solver::validate::SolverKind::Hallen),
        "mpie" => Ok(nec_solver::validate::SolverKind::Mpie),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown solver '{other}': expected 'hallen' or 'mpie'"
        ))),
    }
}

/// The frequency a single-point entry point solves at: the deck's first.
fn first_frequency_hz(deck: &nec_model::deck::NecDeck) -> PyResult<f64> {
    let freqs = frequencies_from_deck(deck);
    freqs.first().copied().ok_or_else(|| {
        // The shared sentence (FND-070). No `--sweep-config` remedy: these
        // bindings take their frequencies from the deck only.
        pyo3::exceptions::PyRuntimeError::new_err(
            nec_solver::validate::no_frequency_error(&freqs, "Add an `FR` card to the deck.")
                .unwrap_or_else(|| "deck has no FR card".to_string()),
        )
    })
}

/// Solve a NEC deck string at the first frequency defined by its FR card.
///
/// Returns a dict with keys: ``freq_mhz``, ``tag``, ``seg``,
/// ``z_re``, ``z_im``, ``z_abs``, ``z_arg_deg``.
///
/// Raises ``RuntimeError`` on parse or solver errors.
#[pyfunction]
#[pyo3(signature = (deck, solver = "hallen"))]
fn solve_deck_str(py: Python<'_>, deck: &str, solver: &str) -> PyResult<PyObject> {
    let solver = solver_from_name(solver)?;
    let result = parse(deck)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("parse error: {e}")))?;
    let freq_hz = first_frequency_hz(&result.deck)?;
    let (rec, mut warnings) = solve_at_freq(&result.deck, freq_hz, solver)
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
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
#[pyo3(signature = (deck, solver = "hallen"))]
fn sweep_deck_str(py: Python<'_>, deck: &str, solver: &str) -> PyResult<PyObject> {
    let solver = solver_from_name(solver)?;
    let result = parse(deck)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("parse error: {e}")))?;
    let freqs = frequencies_from_deck(&result.deck);
    // This returned an empty list, at success, for a deck with no `FR` — while
    // `solve_deck_str` two functions up refused the same deck. So this module
    // disagreed with itself, and the sweep half returned the null-standing-for-an
    // -error shape that `docs/json-output-schema.md` now tells consumers not to
    // read that way (FND-070). Both raise now, with one sentence.
    if let Some(err) =
        nec_solver::validate::no_frequency_error(&freqs, "Add an `FR` card to the deck.")
    {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(err));
    }

    let mut seen = Vec::new();
    let parse_warnings: Vec<String> = result.warnings.iter().map(ToString::to_string).collect();
    emit_warnings(py, &parse_warnings, &mut seen)?;

    let mut records = Vec::with_capacity(freqs.len());
    let mut z_res: Vec<f64> = Vec::with_capacity(freqs.len());
    for freq_hz in freqs {
        let (rec, mut warnings) = solve_at_freq(&result.deck, freq_hz, solver)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        if let Some(z) = rec.get("z_re") {
            z_res.push(*z);
        }
        // The per-point negative-resistance sentence embeds `Re Z = {z_re:.3}`,
        // so every point's text differs and `seen` cannot dedup it: a 500-point
        // junctioned sweep raised 500 distinct `UserWarning`s (FND-032). Held
        // back here and reported once below, which is what the GUI does — through
        // the same producer, so the two cannot drift.
        warnings.retain(|w| !nec_solver::validate::is_negative_resistance_message(w));
        warnings.sort();
        emit_warnings(py, &warnings, &mut seen)?;
        let d = PyDict::new(py);
        for (k, v) in &rec {
            d.set_item(k, v)?;
        }
        records.push(d.into_pyobject(py)?.into_any().unbind());
    }

    // One line for the whole sweep. The cause is a property of the geometry,
    // fixed across the run, so repeating it per point restated a `z_re` the
    // record already carries.
    let segs = nec_solver::build_geometry(&result.deck)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    if let Some(w) = nec_solver::validate::swept_negative_resistance_caveat(
        &z_res,
        &result.deck,
        &segs,
        py_solver_context(solver).with_loads(nec_solver::validate::RunLoads::of_deck(
            &result.deck,
            &segs,
            &nec_solver::frequencies_hz(&result.deck),
        )),
    ) {
        emit_warnings(py, &[w], &mut seen)?;
    }
    Ok(pyo3::types::PyList::new(py, records)?.into())
}

/// Solve a NEC deck string for its segment currents at the first frequency
/// defined by its FR card.
///
/// Returns a dict with ``freq_mhz`` and ``currents``: a list with one dict per
/// segment, in geometry order, with keys ``tag``, ``seg``, ``re``, ``im``,
/// ``mag`` and ``phase_deg`` (amperes, degrees) — the CLI's ``CURRENTS`` table.
///
/// This is the entry point for a plane-wave (``EX 1``/``2``/``3``) receive deck,
/// which the impedance functions refuse because a receiving antenna has no
/// feedpoint (FND-108). It answers driven decks too, with the same solve.
///
/// The currents are the wire currents. At a feed that is also a ``TL``/``NT``
/// port the source additionally delivers the network branch, which is not a
/// wire current and is not listed — as in the CLI and nec2c.
///
/// Raises ``RuntimeError`` on parse or solver errors.
#[pyfunction]
#[pyo3(signature = (deck, solver = "hallen"))]
fn solve_currents_deck_str(py: Python<'_>, deck: &str, solver: &str) -> PyResult<PyObject> {
    let solver = solver_from_name(solver)?;
    let result = parse(deck)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("parse error: {e}")))?;
    let freq_hz = first_frequency_hz(&result.deck)?;
    let mut solved = solve_structure(&result.deck, freq_hz, solver)
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
    let mut seen = Vec::new();
    let parse_warnings: Vec<String> = result.warnings.iter().map(ToString::to_string).collect();
    emit_warnings(py, &parse_warnings, &mut seen)?;
    solved.warnings.sort();
    emit_warnings(py, &solved.warnings, &mut seen)?;

    let rows = pyo3::types::PyList::empty(py);
    for (seg, i) in solved.segs.iter().zip(&solved.wire) {
        let row = PyDict::new(py);
        row.set_item("tag", seg.tag)?;
        row.set_item("seg", seg.tag_index)?;
        row.set_item("re", i.re)?;
        row.set_item("im", i.im)?;
        row.set_item("mag", i.norm())?;
        row.set_item("phase_deg", i.im.atan2(i.re).to_degrees())?;
        rows.append(row)?;
    }
    let d = PyDict::new(py);
    d.set_item("freq_mhz", freq_hz / 1e6)?;
    d.set_item("currents", rows)?;
    Ok(d.into())
}

/// fnec Python bindings — NEC deck solver.
#[pymodule]
fn fnec_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(solve_deck_str, m)?)?;
    m.add_function(wrap_pyfunction!(sweep_deck_str, m)?)?;
    m.add_function(wrap_pyfunction!(solve_currents_deck_str, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The undriven-deck refusal, at the bindings' own entry point.
    ///
    /// Same reasoning as the test below: `solve_at_freq` reaches the shared gate
    /// through `validate::diagnose`, not by calling `pre_solve_error` directly,
    /// so the wiring is a second thing that can be missing here.
    ///
    /// Note what this test does NOT get you: `bindings/fnec_py` is excluded from
    /// the cargo workspace (`Cargo.toml`), and CI's bindings job runs fmt,
    /// clippy, maturin and pytest — never `cargo test`. So this and its two
    /// neighbours are CI-dormant; they run only in the local `check-all.sh` gate.
    /// The pytest twin is what actually guards this in CI.
    #[test]
    fn an_undriven_deck_is_refused() {
        let deck_src = "CE\nGW 1 21 0 0 -5.0 0 0 5.0 0.001\nGE\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let parsed = parse(deck_src).expect("deck parses");
        let err = solve_at_freq(
            &parsed.deck,
            14.2e6,
            nec_solver::validate::SolverKind::Hallen,
        )
        .expect_err("a deck nothing drives must be refused");
        assert!(err.contains("no EX card"), "{err}");
    }

    /// FND-129, at the bindings' own entry point.
    ///
    /// Every Python call funnels through [`solve_at_freq`], and it reaches the
    /// shared refusals through `validate::diagnose` rather than calling
    /// `pre_solve_error` itself — a second route to the same gate, and therefore
    /// a second place the wiring can be missing. The CLI, the GUI and the worker
    /// each have their own test that the refusal reaches them; this is that test
    /// for the bindings, and it needs no Python interpreter to run.
    #[test]
    fn a_deck_with_two_current_sources_is_refused() {
        let deck_src = "CE\nGW 1 21 0 0 -5.0 0 0 5.0 0.001\nGW 2 21 3.0 0 -5.0 3.0 0 5.0 0.001\nGE\nEX 4 1 11 0 1.0 0.0\nEX 4 2 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let parsed = parse(deck_src).expect("deck parses");
        let err = solve_at_freq(
            &parsed.deck,
            14.2e6,
            nec_solver::validate::SolverKind::Hallen,
        )
        .expect_err("two current sources must be refused");
        assert!(err.contains("2 current sources"), "{err}");
        assert!(err.contains("tag 2 segment 11"), "{err}");
    }

    /// FND-108: a plane-wave receive deck solves for currents through the shared
    /// structure solve, and the impedance path refuses it naming that route.
    #[test]
    fn a_receive_deck_solves_for_currents_and_is_refused_for_impedance() {
        let deck_src =
            "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 1 1 1 0 30 0 0\nFR 0 1 0 0 14.2 0\nEN\n";
        let parsed = parse(deck_src).expect("deck parses");
        let kind = nec_solver::validate::SolverKind::Hallen;
        let solved = solve_structure(&parsed.deck, 14.2e6, kind).expect("a receive deck solves");
        // nec2c's centre current for this deck, -2.6778e-2 + j1.6886e-2 A.
        let centre = solved.wire[25];
        let want = Complex64::new(-2.6778e-2, 1.6886e-2);
        assert!((centre - want).norm() < 0.06 * want.norm(), "{centre}");
        let err = solve_at_freq(&parsed.deck, 14.2e6, kind).expect_err("no feedpoint");
        assert!(err.contains("solve_currents_deck_str"), "{err}");
    }

    /// The control. Without it, a guard that refused every deck would pass above.
    #[test]
    fn one_current_source_still_solves_through_the_bindings() {
        let deck_src = "CE\nGW 1 21 0 0 -5.0 0 0 5.0 0.001\nGE\nEX 4 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let parsed = parse(deck_src).expect("deck parses");
        let (rec, _warnings) = solve_at_freq(
            &parsed.deck,
            14.2e6,
            nec_solver::validate::SolverKind::Hallen,
        )
        .expect("a single current source is the supported case");
        // A current source at the centre of a 21-segment half-wave element:
        // finite, non-zero, and priced from the solved port voltage.
        let z_re = rec.get("z_re").copied().expect("z_re in the record");
        assert!(
            z_re.is_finite() && z_re > 0.0,
            "expected a real positive resistance, got {rec:?}"
        );
    }
}
