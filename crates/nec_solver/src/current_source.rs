// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Solving an antenna driven by an impressed current (`EX` type 4).
//!
//! Separate from `linear.rs`, which holds the numerics, because this is the
//! *routing*: which of the two current-source solvers a deck needs, and where its
//! conductor really ends. That decision lived in the CLI and made the CLI the only
//! frontend that could solve such a deck (FND-045).

use num_complex::Complex64;

use crate::excitation::{build_current_source_shape, build_current_source_shape_paths};
use crate::geometry::{
    build_conductor_paths, detect_wire_junctions, merge_collinear_wire_endpoints,
    wire_endpoints_from_segs, Segment,
};
use crate::linear::{solve_hallen_current_source, solve_hallen_current_source_paths};
use crate::matrix::ZMatrix;
use crate::{ExcitationError, SolveError};
use nec_model::card::Card;
use nec_model::deck::NecDeck;

/// What a current-source solve produced, and where.
#[derive(Debug, Clone)]
pub struct CurrentSourceFeedpoint {
    pub currents: Vec<Complex64>,
    /// The solved port voltage `V`. The feedpoint impedance is `V / i0`.
    pub port_voltage: Complex64,
    pub source_tag: u32,
    pub source_segment: u32,
}

/// Why a current-source deck could not be solved.
///
/// Typed rather than `String`, because three frontends now consume this and each
/// wants to phrase the refusal in its own terms — the CLI, the GUI panel, and a
/// Python exception are not the same audience.
#[derive(Debug)]
pub enum CurrentSourceError {
    NoCurrentSource,
    UnsupportedTopology,
    Excitation(ExcitationError),
    Solve(SolveError),
}

impl std::fmt::Display for CurrentSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCurrentSource => write!(f, "EX: no current-source card found"),
            Self::UnsupportedTopology => write!(
                f,
                "EX: current source is supported on straight or degree-2 junctioned \
                 wires; degree-3+ (T/Y) junctions and closed loops are not yet supported"
            ),
            Self::Excitation(e) => write!(f, "{e}"),
            Self::Solve(e) => write!(f, "{e}"),
        }
    }
}

/// Solve a current-source-driven antenna (PH8-CHK-001, PH9-CHK-002, NEC2 EX type
/// 4): force the specified current on the source segment and return the segment
/// currents plus the port voltage `V` (feedpoint impedance `Z = V/i0`).
///
/// Straight, non-junctioned wires (one or more) solve on the per-wire path.
/// **Junctioned degree-2 geometry** (bends, start-to-start / end-to-end splits,
/// inverted-V) solves on continuous *conductor paths* (PH9-CHK-002): one homogeneous
/// `cos(k·s)` constant per path plus the port voltage, `I = 0` at the free ends, and
/// the forced `I[src] = i0`. Out-of-scope topologies (degree-3+ T/Y, closed loops)
/// return `None` from `build_conductor_paths` and fail fast with a diagnostic.
/// `z_mat` is the assembled Hallén matrix (including any load / TL stamps).
///
/// This lived in the CLI, which made the CLI the only frontend that could solve a
/// current-source deck — the GUI and the Python bindings declined by name, and
/// their refusal documented unwired capability rather than a missing solver
/// (FND-045). Every callee below was already a `nec_solver` export; only the glue
/// was app-level.
///
/// It was **not** safe to share until two defects were fixed first. The per-wire
/// branch handed raw per-`GW` endpoints to a solver that pins `I = 0` at each
/// entry's ends, so a collinear split delivered half the requested current
/// (FND-048); and a deck carrying both drive kinds was answered rather than
/// refused (FND-036). Promoting it before those would have turned one wrong
/// frontend into four.
///
/// Returns the currents, the port voltage `V`, and the segment the source was
/// resolved to, so a caller can report `Z = V/i0` **and** say where.
pub fn solve_current_source_hallen(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    freq_hz: f64,
) -> Result<CurrentSourceFeedpoint, CurrentSourceError> {
    let cs = deck
        .cards
        .iter()
        .find_map(|c| match c {
            Card::Ex(ex) if ex.kind() == nec_model::card::ExcitationKind::CurrentSource => Some(ex),
            _ => None,
        })
        .ok_or(CurrentSourceError::NoCurrentSource)?;

    let i0 = Complex64::new(cs.voltage_real, cs.voltage_imag);

    // Route junctioned degree-2 geometry through the conductor-path current-source
    // solver. Reducible decks (single wires, collinear chains, parallel arrays) keep
    // the validated per-wire path; only a non-trivial (bent / reversed) path diverts.
    if let Some(paths) = build_conductor_paths(segs) {
        if paths.iter().any(|p| !p.is_trivial()) {
            let (shape, cos_vec, src_seg) =
                build_current_source_shape_paths(deck, segs, freq_hz, cs.tag, cs.segment, &paths)
                    .map_err(CurrentSourceError::Excitation)?;
            let mut path_of = vec![0usize; segs.len()];
            let mut free_ends: Vec<usize> = Vec::with_capacity(paths.len() * 2);
            for (pi, p) in paths.iter().enumerate() {
                for &m in &p.segs {
                    path_of[m] = pi;
                }
                free_ends.push(p.free_ends.0);
                free_ends.push(p.free_ends.1);
            }
            let sol = solve_hallen_current_source_paths(
                z_mat, &shape, &cos_vec, src_seg, i0, &path_of, &free_ends,
            )
            .map_err(CurrentSourceError::Solve)?;
            return Ok(CurrentSourceFeedpoint {
                currents: sol.currents,
                port_voltage: sol.port_voltage,
                source_tag: cs.tag,
                source_segment: cs.segment,
            });
        }
    } else if !detect_wire_junctions(segs, &wire_endpoints_from_segs(segs), 1e-6).is_empty() {
        // Out-of-scope junction topology (degree-3+ T/Y, closed loop).
        return Err(CurrentSourceError::UnsupportedTopology);
    }

    // Merged, not raw, endpoints — the defect this fixes (FND-048).
    //
    // `solve_hallen_current_source` pins `I = 0` at the first and last segment of
    // every entry it is given. Handed the raw per-`GW` list, a dipole written as
    // two collinear cards carries a spurious zero at the join — and when the
    // source sits there the solver is asked for `I[src] = i0` and `I[src] = 0` at
    // once, so least squares splits the difference. Measured: a 1 A source
    // delivered 0.5 A, at 36.953 + j7.013 Ω against the single-wire deck's
    // 74.228 + j13.897, exit 0, no warning.
    //
    // A collinear split is one conductor, not two wires with ends. The voltage
    // path has merged before solving since PH9-CHK-002; this one never did.
    //
    // Only the endpoints change here. The routing above is deliberately left
    // alone: it decides which *solver* runs, and moving a deck between solvers is
    // a different change from telling one solver where the conductor really ends.
    let merged_endpoints = merge_collinear_wire_endpoints(segs);

    let (shape, cos_vec, src_seg) =
        build_current_source_shape(deck, segs, freq_hz, cs.tag, cs.segment)
            .map_err(CurrentSourceError::Excitation)?;
    let sol = solve_hallen_current_source(z_mat, &shape, &cos_vec, src_seg, i0, &merged_endpoints)
        .map_err(CurrentSourceError::Solve)?;
    Ok(CurrentSourceFeedpoint {
        currents: sol.currents,
        port_voltage: sol.port_voltage,
        source_tag: cs.tag,
        source_segment: cs.segment,
    })
}
