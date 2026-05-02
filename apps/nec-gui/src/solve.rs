// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Single-frequency Hallen solve — thin wrapper around `nec_solver` for use
//! by the GUI.  Returns the first feedpoint impedance found in the deck.

use std::path::Path;

use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::{
    assemble_z_matrix_with_ground, build_excitation, build_geometry, build_hallen_rhs, build_loads,
    build_tl_stamps, compute_radiation_pattern, detect_wire_junctions, ground_model_from_deck,
    solve_hallen, wire_endpoints_from_segs, FarFieldPoint,
};
use num_complex::Complex64;

/// Result of a successful single-frequency solve.
#[derive(Debug, Clone, PartialEq)]
pub struct SolveResult {
    /// Frequency in MHz.
    pub freq_mhz: f64,
    /// Feedpoint resistance (Ω).
    pub z_re: f64,
    /// Feedpoint reactance (Ω).
    pub z_im: f64,
}

/// One row in the sweep result table.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepPoint {
    pub freq_mhz: f64,
    pub z_re: f64,
    pub z_im: f64,
}

/// Run a Hallen solve on the NEC deck at `path` and return the feedpoint
/// impedance at the first frequency found in the `FR` card.
///
/// Returns `Err` with a human-readable message if the file cannot be read,
/// parsed, or solved.
pub fn solve_deck_path(path: &Path) -> Result<SolveResult, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    solve_deck_str(&input)
}

/// Run a Hallen solve on `deck_text` (a raw NEC deck string).
pub fn solve_deck_str(deck_text: &str) -> Result<SolveResult, String> {
    let parsed = parse(deck_text).map_err(|e| e.to_string())?;
    let deck = &parsed.deck;

    // --- geometry & excitation -------------------------------------------
    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let ground = ground_model_from_deck(deck);
    let wire_endpoints = wire_endpoints_from_segs(&segs);

    // --- frequency -------------------------------------------------------
    let freq_hz = deck
        .cards
        .iter()
        .find_map(|c| {
            if let Card::Fr(fr) = c {
                Some(fr.frequency_mhz * 1_000_000.0)
            } else {
                None
            }
        })
        .ok_or_else(|| "deck has no FR card".to_string())?;

    // --- impedance matrix ------------------------------------------------
    let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);

    let (load_vec, _load_warnings) = build_loads(deck, &segs, freq_hz);
    z_mat.add_to_diagonal(&load_vec);

    let (tl_stamps, _tl_warnings) = build_tl_stamps(deck, &segs, freq_hz);
    for (row, col, delta) in &tl_stamps {
        z_mat.add_to_entry(*row, *col, *delta);
    }

    // --- Hallen solve ----------------------------------------------------
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

    // --- feedpoint impedance --------------------------------------------
    let z = feedpoint_impedance(deck, &segs, &v_vec, &sol.currents, freq_hz)?;

    Ok(SolveResult {
        freq_mhz: freq_hz / 1_000_000.0,
        z_re: z.re,
        z_im: z.im,
    })
}

/// Compute feedpoint impedance Z = V/I for the first EX card.
fn feedpoint_impedance(
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    v_vec: &[Complex64],
    i_vec: &[Complex64],
    _freq_hz: f64,
) -> Result<Complex64, String> {
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        let Some((idx, seg)) = segs
            .iter()
            .enumerate()
            .find(|(_, seg)| seg.tag == ex.tag && seg.tag_index == ex.segment)
        else {
            continue;
        };
        let current = i_vec[idx];
        let v_source = v_vec[idx] * seg.length;
        let z_in = if current.norm() > 1e-60 {
            v_source / current
        } else {
            v_source
        };
        return Ok(z_in);
    }
    Err("deck has no EX card — cannot determine feedpoint".to_string())
}

/// Run a Hallen sweep over a frequency range for the deck at `path`.
///
/// `start_mhz`, `end_mhz`, `step_mhz` define the linear sweep.  The geometry
/// and excitation vector are built once and reused for every frequency point.
pub fn sweep_deck_path(
    path: &std::path::Path,
    start_mhz: f64,
    end_mhz: f64,
    step_mhz: f64,
) -> Result<Vec<SweepPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    sweep_deck_str(&input, start_mhz, end_mhz, step_mhz)
}

/// Run a Hallen sweep for a deck given as a string.
pub fn sweep_deck_str(
    deck_text: &str,
    start_mhz: f64,
    end_mhz: f64,
    step_mhz: f64,
) -> Result<Vec<SweepPoint>, String> {
    if step_mhz <= 0.0 {
        return Err(format!("step_mhz must be > 0, got {step_mhz}"));
    }
    if start_mhz >= end_mhz {
        return Err(format!(
            "start_mhz ({start_mhz}) must be less than end_mhz ({end_mhz})"
        ));
    }

    let parsed = parse(deck_text).map_err(|e| e.to_string())?;
    let deck = &parsed.deck;

    // Build geometry and excitation once — reused across frequencies.
    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let ground = ground_model_from_deck(deck);
    let wire_endpoints = wire_endpoints_from_segs(&segs);
    let wire_junctions = detect_wire_junctions(&segs, &wire_endpoints, 1e-6);
    let junction_tuples: Vec<(usize, usize, f64)> = wire_junctions
        .iter()
        .map(|j| (j.seg_a, j.seg_b, j.sign))
        .collect();

    // Build frequency list.
    let mut freqs_mhz = Vec::new();
    let mut f = start_mhz;
    while f <= end_mhz + step_mhz * 1e-9 {
        freqs_mhz.push(f);
        f += step_mhz;
    }

    let mut results = Vec::with_capacity(freqs_mhz.len());

    for freq_mhz in freqs_mhz {
        let freq_hz = freq_mhz * 1_000_000.0;

        let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);
        let (load_vec, _) = build_loads(deck, &segs, freq_hz);
        z_mat.add_to_diagonal(&load_vec);
        let (tl_stamps, _) = build_tl_stamps(deck, &segs, freq_hz);
        for (row, col, delta) in &tl_stamps {
            z_mat.add_to_entry(*row, *col, *delta);
        }

        let hallen_rhs = build_hallen_rhs(deck, &segs, freq_hz).map_err(|e| e.to_string())?;
        let sol = solve_hallen(
            &z_mat,
            &hallen_rhs.rhs,
            &hallen_rhs.cos_vec,
            &wire_endpoints,
            &junction_tuples,
        )
        .map_err(|e| e.to_string())?;

        let z = feedpoint_impedance(deck, &segs, &v_vec, &sol.currents, freq_hz)?;
        results.push(SweepPoint {
            freq_mhz,
            z_re: z.re,
            z_im: z.im,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Pattern slice (PH3-CHK-011)
// ---------------------------------------------------------------------------

/// One point in a 2D radiation-pattern slice (fixed φ, varying θ).
#[derive(Debug, Clone, PartialEq)]
pub struct PatternPoint {
    /// Zenith angle θ in degrees.
    pub theta_deg: f64,
    /// Fixed azimuth φ in degrees (same for all points in the slice).
    pub phi_deg: f64,
    /// Total directivity in dBi.
    pub gain_total_dbi: f64,
}

/// Compute an elevation-plane (fixed φ) radiation-pattern slice from a deck
/// at `path`.
///
/// `phi_deg` selects the azimuth plane.  θ is sampled in 5° steps from 0° to
/// 180° (37 points), giving a full elevation cut.
pub fn pattern_slice_deck_path(path: &Path, phi_deg: f64) -> Result<Vec<PatternPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    pattern_slice_deck_str(&input, phi_deg)
}

/// Compute an elevation-plane radiation-pattern slice from a raw deck string.
pub fn pattern_slice_deck_str(deck_text: &str, phi_deg: f64) -> Result<Vec<PatternPoint>, String> {
    let (segs, currents, freq_hz, ground) = solve_for_currents(deck_text)?;

    // Build 37-point theta grid: 0, 5, 10, … 180 deg.
    let points: Vec<FarFieldPoint> = (0..=36)
        .map(|i| FarFieldPoint {
            theta_deg: i as f64 * 5.0,
            phi_deg,
        })
        .collect();

    let results = compute_radiation_pattern(&segs, &currents, freq_hz, &points, &ground);

    Ok(results
        .into_iter()
        .map(|r| PatternPoint {
            theta_deg: r.theta_deg,
            phi_deg: r.phi_deg,
            gain_total_dbi: r.gain_total_dbi,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Current distribution (PH3-CHK-011)
// ---------------------------------------------------------------------------

/// Per-segment current magnitude for the current-distribution bar chart.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentPoint {
    /// Global segment index.
    pub seg_idx: usize,
    /// Segment midpoint distance from wire origin along the cumulative arc (m).
    pub position_m: f64,
    /// Current magnitude |I| in milliamperes.
    pub current_mag_ma: f64,
}

/// Compute the per-segment current distribution from the deck at `path`.
pub fn current_distribution_deck_path(path: &Path) -> Result<Vec<CurrentPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    current_distribution_deck_str(&input)
}

/// Compute the per-segment current distribution from a raw deck string.
pub fn current_distribution_deck_str(deck_text: &str) -> Result<Vec<CurrentPoint>, String> {
    let (segs, currents, _freq_hz, _ground) = solve_for_currents(deck_text)?;

    let mut pos: f64 = 0.0;
    let mut prev_mid: Option<[f64; 3]> = None;
    let points = segs
        .iter()
        .zip(currents.iter())
        .enumerate()
        .map(|(idx, (seg, &i))| {
            if let Some(p) = prev_mid {
                let dx = seg.midpoint[0] - p[0];
                let dy = seg.midpoint[1] - p[1];
                let dz = seg.midpoint[2] - p[2];
                pos += (dx * dx + dy * dy + dz * dz).sqrt();
            }
            prev_mid = Some(seg.midpoint);
            CurrentPoint {
                seg_idx: idx,
                position_m: pos,
                current_mag_ma: i.norm() * 1_000.0,
            }
        })
        .collect();

    Ok(points)
}

// ---------------------------------------------------------------------------
// Internal: shared Hallen solve returning (segs, currents, freq_hz, ground)
// ---------------------------------------------------------------------------

fn solve_for_currents(
    deck_text: &str,
) -> Result<
    (
        Vec<nec_solver::Segment>,
        Vec<Complex64>,
        f64,
        nec_solver::GroundModel,
    ),
    String,
> {
    let parsed = parse(deck_text).map_err(|e| e.to_string())?;
    let deck = &parsed.deck;

    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    let _v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let ground = ground_model_from_deck(deck);
    let wire_endpoints = wire_endpoints_from_segs(&segs);

    let freq_hz = deck
        .cards
        .iter()
        .find_map(|c| {
            if let Card::Fr(fr) = c {
                Some(fr.frequency_mhz * 1_000_000.0)
            } else {
                None
            }
        })
        .ok_or_else(|| "deck has no FR card".to_string())?;

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

    Ok((segs, sol.currents, freq_hz, ground))
}
