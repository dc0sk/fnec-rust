// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Single-frequency Hallen solve — thin wrapper around `nec_solver` for use
//! by the GUI.  Returns the first feedpoint impedance found in the deck.

use std::collections::HashMap;
use std::path::Path;

use nec_model::card::Card;
use nec_model::deck::NecDeck;
use nec_parser::parse;
use nec_solver::{
    assemble_z_matrix_with_ground, build_excitation, build_geometry, build_hallen_rhs, build_loads,
    build_tl_stamps, compute_radiation_pattern, detect_wire_junctions, ground_model_from_deck,
    solve_hallen, wire_endpoints_from_segs, FarFieldPoint, GroundModel, Segment,
};
use num_complex::Complex64;

// ---------------------------------------------------------------------------
// Variable-substitution helper
// ---------------------------------------------------------------------------

/// Load a flat string-to-string variable map from a `.toml` or `.json` file.
///
/// Accepts TOML (default) or JSON (detected by `.json` extension) flat
/// key-value maps.  Integer and float values are accepted and converted to
/// strings.  Returns `Err` with a human-readable message on any failure.
fn load_vars(path: &Path) -> Result<HashMap<String, String>, String> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read vars file '{}': {e}", path.display()))?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "json" {
        // Minimal flat-JSON-object parser (avoids serde_json in deps).
        let s = src.trim();
        if !s.starts_with('{') || !s.ends_with('}') {
            return Err(format!(
                "'{}': JSON vars file must be a top-level object",
                path.display()
            ));
        }
        let inner = s[1..s.len() - 1].trim();
        let mut map = HashMap::new();
        if inner.is_empty() {
            return Ok(map);
        }
        // Naive split on top-level commas (no nested objects supported).
        for raw_pair in inner.split(',') {
            let pair = raw_pair.trim();
            if pair.is_empty() {
                continue;
            }
            let colon = pair
                .find(':')
                .ok_or_else(|| format!("'{}': malformed JSON pair: {pair}", path.display()))?;
            let raw_key = pair[..colon].trim().trim_matches('"');
            let raw_val = pair[colon + 1..].trim().trim_matches('"');
            map.insert(raw_key.to_string(), raw_val.to_string());
        }
        Ok(map)
    } else {
        let table: toml::Table = toml::from_str(&src)
            .map_err(|e| format!("'{}': TOML parse error: {e}", path.display()))?;
        let mut map = HashMap::new();
        for (k, v) in table {
            match v {
                toml::Value::String(s) => {
                    map.insert(k, s);
                }
                toml::Value::Integer(i) => {
                    map.insert(k, i.to_string());
                }
                toml::Value::Float(f) => {
                    map.insert(k, format!("{f}"));
                }
                other => {
                    return Err(format!(
                        "'{}': variable '{k}' has unsupported type {} — use strings or numbers",
                        path.display(),
                        other.type_str()
                    ));
                }
            }
        }
        Ok(map)
    }
}

/// Apply variable substitution to `input` if `vars_path` is provided.
/// Returns the (possibly substituted) string or an error.
fn apply_vars(input: &str, vars_path: Option<&str>) -> Result<String, String> {
    if let Some(vp) = vars_path {
        let vars = load_vars(Path::new(vp))?;
        nec_parser::template::substitute(input, &vars).map_err(|e| e.to_string())
    } else {
        Ok(input.to_owned())
    }
}

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
/// If `vars_path` is `Some(path)`, the file is loaded as a variable map and
/// `$VAR` tokens in the deck are substituted before parsing.
///
/// Returns `Err` with a human-readable message if the file cannot be read,
/// parsed, or solved.
pub fn solve_deck_path(path: &Path, vars_path: Option<&str>) -> Result<SolveResult, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    solve_deck_str(&input)
}

/// Parse a deck (with optional `$VAR` substitution) and build **only** its
/// geometry — no solve — for the 3-D viewport (GUI-CHK-002). Cheap enough to run
/// on every valid edit for instant visual feedback.
pub fn load_geometry_path(
    path: &Path,
    vars_path: Option<&str>,
) -> Result<crate::mesh::SceneGeometry, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    load_geometry_str(&input)
}

/// Build the viewport geometry from a raw NEC deck string.
pub fn load_geometry_str(deck_text: &str) -> Result<crate::mesh::SceneGeometry, String> {
    let parsed = parse(deck_text).map_err(|e| e.to_string())?;
    let deck = &parsed.deck;
    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    if segs.is_empty() {
        return Err("deck has no wire geometry (no GW cards?)".to_string());
    }
    let has_ground = !matches!(
        ground_model_from_deck(deck),
        GroundModel::FreeSpace | GroundModel::Deferred { .. }
    );
    let f3 = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
    let wires = segs.iter().map(|s| (f3(s.start), f3(s.end))).collect();
    Ok(crate::mesh::SceneGeometry::from_segments(wires, has_ground))
}

/// Parse a deck file (with optional `$VAR` substitution) into an editable
/// [`ModelDoc`] for the visual wire editor (GUI-CHK-007). No solve.
pub fn load_model_doc_path(
    path: &Path,
    vars_path: Option<&str>,
) -> Result<crate::model_doc::ModelDoc, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    load_model_doc_str(&input)
}

/// Build an editable [`ModelDoc`] from a raw NEC deck string.
pub fn load_model_doc_str(deck_text: &str) -> Result<crate::model_doc::ModelDoc, String> {
    let parsed = parse(deck_text).map_err(|e| e.to_string())?;
    Ok(crate::model_doc::ModelDoc::from_deck(&parsed.deck))
}

/// Solve a deck and return its geometry **with** per-segment current magnitudes
/// (mA), aligned to the wire order, for current-colored 3-D display (GUI-CHK-004).
pub fn load_currents_path(
    path: &Path,
    vars_path: Option<&str>,
) -> Result<crate::mesh::GeometryCurrents, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    load_currents_str(&input)
}

/// Build geometry + current magnitudes from a raw NEC deck string.
pub fn load_currents_str(deck_text: &str) -> Result<crate::mesh::GeometryCurrents, String> {
    let (segs, currents, _freq_hz, ground) = solve_for_currents(deck_text)?;
    let has_ground = !matches!(
        ground,
        GroundModel::FreeSpace | GroundModel::Deferred { .. }
    );
    let f3 = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
    let wires = segs.iter().map(|s| (f3(s.start), f3(s.end))).collect();
    let currents_ma = currents
        .iter()
        .map(|i| (i.norm() * 1000.0) as f32)
        .collect();
    Ok(crate::mesh::GeometryCurrents {
        geometry: crate::mesh::SceneGeometry::from_segments(wires, has_ground),
        currents_ma,
    })
}

/// Solve a deck and return its geometry plus a full-sphere far-field gain grid
/// for the 3-D radiation-pattern lobe (GUI-CHK-005).
pub fn pattern_grid_path(
    path: &Path,
    vars_path: Option<&str>,
) -> Result<crate::mesh::PatternSolve, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    pattern_grid_str(&input)
}

/// Build geometry + full-sphere pattern grid from a raw NEC deck string.
pub fn pattern_grid_str(deck_text: &str) -> Result<crate::mesh::PatternSolve, String> {
    use crate::mesh::{LOBE_N_PHI, LOBE_N_THETA};
    let (segs, currents, freq_hz, ground) = solve_for_currents(deck_text)?;

    let (nt, np) = (LOBE_N_THETA, LOBE_N_PHI);
    let mut points = Vec::with_capacity(nt * np);
    for it in 0..nt {
        let theta = it as f64 * 180.0 / (nt - 1) as f64;
        for ip in 0..np {
            let phi = ip as f64 * 360.0 / (np - 1) as f64;
            points.push(FarFieldPoint {
                theta_deg: theta,
                phi_deg: phi,
            });
        }
    }
    let results = compute_radiation_pattern(&segs, &currents, freq_hz, &points, &ground);
    let gains_dbi = results.iter().map(|r| r.gain_total_dbi as f32).collect();

    let has_ground = !matches!(
        ground,
        GroundModel::FreeSpace | GroundModel::Deferred { .. }
    );
    let f3 = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
    let wires = segs.iter().map(|s| (f3(s.start), f3(s.end))).collect();
    Ok(crate::mesh::PatternSolve {
        geometry: crate::mesh::SceneGeometry::from_segments(wires, has_ground),
        grid: crate::mesh::PatternGrid {
            n_theta: nt,
            n_phi: np,
            gains_dbi,
        },
    })
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
/// If `vars_path` is `Some(path)`, `$VAR` tokens are substituted before parsing.
pub fn sweep_deck_path(
    path: &std::path::Path,
    vars_path: Option<&str>,
    start_mhz: f64,
    end_mhz: f64,
    step_mhz: f64,
) -> Result<Vec<SweepPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    sweep_deck_str(&input, start_mhz, end_mhz, step_mhz)
}

/// Run a Hallen sweep for a deck given as a string.
pub fn sweep_deck_str(
    deck_text: &str,
    start_mhz: f64,
    end_mhz: f64,
    step_mhz: f64,
) -> Result<Vec<SweepPoint>, String> {
    let job = SweepJob::prepare(deck_text, start_mhz, end_mhz, step_mhz)?;
    job.freqs_mhz().iter().map(|&f| job.solve_at(f)).collect()
}

/// A prepared frequency sweep: geometry, excitation, ground and junctions built
/// once, so each frequency can be solved independently via [`SweepJob::solve_at`].
///
/// This is the streaming-friendly core behind [`sweep_deck_str`] — the GUI drives
/// it point-by-point so the sweep chart fills in live (GUI-CHK-009).
pub struct SweepJob {
    deck: NecDeck,
    segs: Vec<Segment>,
    v_vec: Vec<Complex64>,
    ground: GroundModel,
    wire_endpoints: Vec<(usize, usize)>,
    junction_tuples: Vec<(usize, usize, f64)>,
    freqs_mhz: Vec<f64>,
}

impl SweepJob {
    /// Parse the deck and build the frequency-independent pieces once.
    pub fn prepare(
        deck_text: &str,
        start_mhz: f64,
        end_mhz: f64,
        step_mhz: f64,
    ) -> Result<Self, String> {
        if step_mhz <= 0.0 {
            return Err(format!("step_mhz must be > 0, got {step_mhz}"));
        }
        if start_mhz >= end_mhz {
            return Err(format!(
                "start_mhz ({start_mhz}) must be less than end_mhz ({end_mhz})"
            ));
        }

        let parsed = parse(deck_text).map_err(|e| e.to_string())?;
        let deck = parsed.deck;
        let segs = build_geometry(&deck).map_err(|e| e.to_string())?;
        let v_vec = build_excitation(&deck, &segs).map_err(|e| e.to_string())?;
        let ground = ground_model_from_deck(&deck);
        let wire_endpoints = wire_endpoints_from_segs(&segs);
        let junction_tuples: Vec<(usize, usize, f64)> =
            detect_wire_junctions(&segs, &wire_endpoints, 1e-6)
                .iter()
                .map(|j| (j.seg_a, j.seg_b, j.sign))
                .collect();

        let mut freqs_mhz = Vec::new();
        let mut f = start_mhz;
        while f <= end_mhz + step_mhz * 1e-9 {
            freqs_mhz.push(f);
            f += step_mhz;
        }

        Ok(Self {
            deck,
            segs,
            v_vec,
            ground,
            wire_endpoints,
            junction_tuples,
            freqs_mhz,
        })
    }

    /// The frequencies (MHz) this job will solve, in ascending order.
    pub fn freqs_mhz(&self) -> &[f64] {
        &self.freqs_mhz
    }

    /// Solve the feedpoint impedance at one frequency (MHz).
    pub fn solve_at(&self, freq_mhz: f64) -> Result<SweepPoint, String> {
        let freq_hz = freq_mhz * 1_000_000.0;

        let mut z_mat = assemble_z_matrix_with_ground(&self.segs, freq_hz, &self.ground);
        let (load_vec, _) = build_loads(&self.deck, &self.segs, freq_hz);
        z_mat.add_to_diagonal(&load_vec);
        let (tl_stamps, _) = build_tl_stamps(&self.deck, &self.segs, freq_hz);
        for (row, col, delta) in &tl_stamps {
            z_mat.add_to_entry(*row, *col, *delta);
        }

        let hallen_rhs =
            build_hallen_rhs(&self.deck, &self.segs, freq_hz).map_err(|e| e.to_string())?;
        let sol = solve_hallen(
            &z_mat,
            &hallen_rhs.rhs,
            &hallen_rhs.cos_vec,
            &self.wire_endpoints,
            &self.junction_tuples,
        )
        .map_err(|e| e.to_string())?;

        let z = feedpoint_impedance(&self.deck, &self.segs, &self.v_vec, &sol.currents, freq_hz)?;
        Ok(SweepPoint {
            freq_mhz,
            z_re: z.re,
            z_im: z.im,
        })
    }
}

/// Read a deck file and apply `$VAR` substitution, returning the deck text.
/// Used by the GUI to prepare a streaming sweep off the UI thread.
pub fn read_deck_text(path: &Path, vars_path: Option<&str>) -> Result<String, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    apply_vars(&input, vars_path)
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
/// If `vars_path` is `Some(path)`, `$VAR` tokens are substituted before parsing.
pub fn pattern_slice_deck_path(
    path: &Path,
    vars_path: Option<&str>,
    phi_deg: f64,
) -> Result<Vec<PatternPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
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
/// If `vars_path` is `Some(path)`, `$VAR` tokens are substituted before parsing.
pub fn current_distribution_deck_path(
    path: &Path,
    vars_path: Option<&str>,
) -> Result<Vec<CurrentPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
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
