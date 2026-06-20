// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Minimal Hallén solve path used by the distributed worker.
//!
//! Only `basis = "hallen"` is supported.  Other basis values return
//! [`SolveError::UnsupportedConfig`].  This is sufficient for PH6-CHK-006;
//! additional bases are added in subsequent milestones.

#[cfg(test)]
mod tests {
    use super::*;

    const DIPOLE: &str = include_str!("../../../corpus/dipole-freesp-51seg.nec");

    #[test]
    fn solve_dipole_at_resonance() {
        let result = solve_deck_at_frequency(DIPOLE, 14.175e6, "hallen").unwrap();
        // Free-space half-wave dipole at resonance: ~73 + j13 Ω
        assert!(
            result.impedance_re > 50.0 && result.impedance_re < 100.0,
            "R = {} Ω",
            result.impedance_re
        );
        assert!(
            result.impedance_im > -20.0 && result.impedance_im < 50.0,
            "X = {} Ω",
            result.impedance_im
        );
        assert!(result.current_mag > 0.0);
    }

    #[test]
    fn solve_rejects_unsupported_basis() {
        let err = solve_deck_at_frequency(DIPOLE, 14.0e6, "pulse").unwrap_err();
        assert!(matches!(err, SolveError::UnsupportedConfig(_)));
    }

    #[test]
    fn solve_rejects_empty_deck() {
        let err = solve_deck_at_frequency("", 14.0e6, "hallen").unwrap_err();
        assert!(
            matches!(err, SolveError::GeometryError(_)),
            "empty deck produced: {err}"
        );
    }

    #[test]
    fn solve_rejects_garbage_input() {
        let err = solve_deck_at_frequency("NOT A NEC DECK", 14.0e6, "hallen").unwrap_err();
        assert!(
            matches!(err, SolveError::GeometryError(_)),
            "garbage input produced: {err}"
        );
    }

    #[test]
    fn solve_no_feedpoint_returns_error() {
        // Deck with geometry but no EX card.
        let deck = "CM test\nGW 0 1 0 0 0 0 0 1 0.001\nGE 0\nFR 0 1 0 0 14.175 0\nEN\n";
        let err = solve_deck_at_frequency(deck, 14.175e6, "hallen").unwrap_err();
        assert!(matches!(err, SolveError::NoFeedpoint));
    }

    #[test]
    fn solve_error_display() {
        let err = SolveError::NoFeedpoint;
        assert_eq!(err.to_string(), "no EX type-0 card found in deck");

        let err = SolveError::SingularMatrix("det=0".into());
        assert_eq!(err.to_string(), "singular matrix: det=0");
    }

    #[test]
    fn feedpoint_result_is_deterministic() {
        let a = solve_deck_at_frequency(DIPOLE, 14.0e6, "hallen").unwrap();
        let b = solve_deck_at_frequency(DIPOLE, 14.0e6, "hallen").unwrap();
        assert!((a.impedance_re - b.impedance_re).abs() < 1e-12);
        assert!((a.impedance_im - b.impedance_im).abs() < 1e-12);
    }
}

use nec_model::card::Card;
use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, build_loads, build_tl_stamps,
    detect_wire_junctions, ground_model_from_deck, solve_hallen, wire_endpoints_from_segs,
};
use num_complex::Complex64;

/// Feedpoint impedance and current at the first `EX` voltage source.
#[derive(Debug, Clone)]
pub struct FeedpointResult {
    pub impedance_re: f64,
    pub impedance_im: f64,
    pub current_mag: f64,
    pub current_phase_deg: f64,
}

/// Errors from the worker solve path.
#[derive(Debug, Clone)]
pub enum SolveError {
    ParseError(String),
    GeometryError(String),
    SingularMatrix(String),
    UnsupportedConfig(String),
    NoFeedpoint,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::ParseError(m) => write!(f, "parse error: {m}"),
            SolveError::GeometryError(m) => write!(f, "geometry error: {m}"),
            SolveError::SingularMatrix(m) => write!(f, "singular matrix: {m}"),
            SolveError::UnsupportedConfig(m) => write!(f, "unsupported config: {m}"),
            SolveError::NoFeedpoint => write!(f, "no EX type-0 card found in deck"),
        }
    }
}

impl std::error::Error for SolveError {}

/// Run a Hallén solve on `deck_str` at `freq_hz` and return the feedpoint result.
///
/// The `basis` parameter must be `"hallen"`; any other value returns
/// [`SolveError::UnsupportedConfig`].
pub fn solve_deck_at_frequency(
    deck_str: &str,
    freq_hz: f64,
    basis: &str,
) -> Result<FeedpointResult, SolveError> {
    if basis != "hallen" {
        return Err(SolveError::UnsupportedConfig(format!(
            "basis '{basis}' not supported in worker; only 'hallen' is implemented"
        )));
    }

    // 1. Parse
    let parse_result =
        nec_parser::parse(deck_str).map_err(|e| SolveError::ParseError(e.to_string()))?;
    let deck = parse_result.deck;

    // 2. Build geometry
    let segs = build_geometry(&deck).map_err(|e| SolveError::GeometryError(e.to_string()))?;
    let wire_endpoints = wire_endpoints_from_segs(&segs);
    let ground = ground_model_from_deck(&deck);

    // 3. Build Hallén RHS
    let hallen_rhs = build_hallen_rhs(&deck, &segs, freq_hz).map_err(|e| {
        use nec_solver::ExcitationError;
        match e {
            ExcitationError::UnsupportedType { ex_type, .. } => SolveError::UnsupportedConfig(
                format!("EX type {ex_type} not supported in worker Hallén path"),
            ),
            other => SolveError::ParseError(other.to_string()),
        }
    })?;

    // 4. Assemble Z-matrix and apply loads / TL stamps
    let (load_vec, _load_warnings) = build_loads(&deck, &segs, freq_hz);
    let (tl_stamps, _tl_warnings) = build_tl_stamps(&deck, &segs, freq_hz);
    let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);
    z_mat.add_to_diagonal(&load_vec);
    for (row, col, delta) in &tl_stamps {
        z_mat.add_to_entry(*row, *col, *delta);
    }

    // 5. Wire-junction constraints
    let junctions = detect_wire_junctions(&segs, &wire_endpoints, 1e-6);
    let junc_constraints: Vec<(usize, usize, f64)> = junctions
        .iter()
        .map(|j| (j.seg_a, j.seg_b, j.sign))
        .collect();

    // 6. Solve
    let solution = solve_hallen(
        &z_mat,
        &hallen_rhs.rhs,
        &hallen_rhs.cos_vec,
        &wire_endpoints,
        &junc_constraints,
    )
    .map_err(|e| SolveError::SingularMatrix(e.to_string()))?;

    // 7. Extract feedpoint from first type-0 EX card
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if ex.excitation_type != 0 {
            continue;
        }
        let Some(idx) = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        else {
            continue;
        };
        let current = solution.currents[idx];
        let v_source = Complex64::new(ex.voltage_real, ex.voltage_imag);
        let z_in = if current.norm() > 1e-60 {
            v_source / current
        } else {
            v_source
        };
        return Ok(FeedpointResult {
            impedance_re: z_in.re,
            impedance_im: z_in.im,
            current_mag: current.norm(),
            current_phase_deg: current.im.atan2(current.re).to_degrees(),
        });
    }

    Err(SolveError::NoFeedpoint)
}
