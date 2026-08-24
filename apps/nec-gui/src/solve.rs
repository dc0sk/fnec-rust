// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Single-frequency Hallen solve — thin wrapper around `nec_solver` for use
//! by the GUI.  Returns the first feedpoint impedance found in the deck.

use std::collections::HashMap;
use std::path::Path;

use nec_model::card::Card;
use nec_model::deck::NecDeck;
use nec_parser::parse;
use nec_solver::validate;
use nec_solver::{
    assemble_z_matrix_with_ground, build_excitation, build_geometry, build_hallen_rhs,
    compute_radiation_pattern, detect_wire_junctions, ground_model_from_deck, solve_hallen,
    wire_endpoints_from_segs, FarFieldPoint, GroundModel, Segment,
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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SolveResult {
    /// Frequency in MHz.
    pub freq_mhz: f64,
    /// Feedpoint resistance (Ω).
    pub z_re: f64,
    /// Feedpoint reactance (Ω).
    pub z_im: f64,
    /// Solver caveats for this deck (unreliable topology, deferred ground,
    /// unsupported loads) — the GUI runs the Hallén solver, so junctions/loops
    /// and finite-ground currents need the CLI's `--solver mpie`.
    pub warnings: Vec<String>,
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
/// Reject a deck the solver cannot honestly take, and collect the caveats for one
/// it can.
///
/// The GUI used to do neither: it went straight from `build_geometry` to
/// `solve_hallen`, so a deck the CLI refuses outright — wires crossing mid-span, a
/// source on a degenerate segment, a wire reaching into an active ground — solved
/// silently here and returned a wrong number (review-260719 FIND-006/007). Both
/// halves now come from `nec_solver::validate`, so the GUI and the CLI cannot drift
/// apart on what is accepted or on what a diagnostic says.
///
/// `Err` is a hard rejection; `Ok` carries the warnings to render.
fn validate_deck(
    deck: &NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
    freq_hz: f64,
    parse_warnings: &[nec_parser::ParseError],
) -> Result<Vec<String>, String> {
    let mut warnings: Vec<String> = parse_warnings.iter().map(ToString::to_string).collect();
    for d in validate::diagnose(deck, segs, ground, freq_hz) {
        match d.level {
            nec_model::DiagnosticLevel::Error => return Err(d.message),
            nec_model::DiagnosticLevel::Warning => warnings.push(d.message),
        }
    }
    // Warnings only — no matrix here. The same seam the solve paths use, so the
    // caveats shown and the stamps applied cannot describe different card sets;
    // this used to rebuild loads and TL by hand and miss NT entirely.
    warnings.extend(nec_solver::build_deck_stamps(deck, segs, freq_hz).warnings);
    Ok(warnings)
}

/// Deck-level diagnostics, independent of which tab the user is on.
///
/// The caveats `validate_deck` produces are about the *deck* — its geometry, its
/// ground model, its topology — not about whether you asked for an impedance, a
/// sweep or a pattern. Only the impedance panel rendered them, so a user who ran
/// only sweeps or only patterns saw none of it (review-260719, GUI follow-up).
///
/// Best-effort by design: a deck that cannot be parsed or built returns an empty
/// list, because the action the user actually ran reports that failure itself and
/// repeating it in a caveats strip would be noise.
pub fn deck_warnings(deck_text: &str) -> Vec<String> {
    let Ok(parsed) = parse(deck_text) else {
        return Vec::new();
    };
    let deck = &parsed.deck;
    let Ok(segs) = build_geometry(deck) else {
        return Vec::new();
    };
    let ground = ground_model_from_deck(deck);
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
        .unwrap_or(0.0);
    // A hard rejection is surfaced by the action itself; keep only the caveats.
    validate_deck(deck, &segs, &ground, freq_hz, &parsed.warnings).unwrap_or_default()
}

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

    // --- validation (before any solve) -----------------------------------
    let warnings = validate_deck(deck, &segs, &ground, freq_hz, &parsed.warnings)?;

    // --- impedance matrix ------------------------------------------------
    let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);

    nec_solver::build_deck_stamps(deck, &segs, freq_hz).apply(&mut z_mat);

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
    let (z, tag, seg) = feedpoint_impedance(deck, &segs, &v_vec, &sol.currents, freq_hz)?;

    // FND-014: physically impossible results were reported here without a caveat.
    // `warnings` is already rendered by `impedance_view`, so this needs no new
    // display path — only the check that was missing.
    let mut warnings = warnings;
    if let Some(w) = nec_solver::validate::negative_resistance_warning(z.re, tag, seg, deck, &segs)
    {
        warnings.push(w);
    }

    Ok(SolveResult {
        freq_mhz: freq_hz / 1_000_000.0,
        z_re: z.re,
        z_im: z.im,
        warnings,
    })
}

/// Compute feedpoint impedance Z = V/I for the first EX card, with the tag and
/// segment it resolved to — a caveat about the result has to name where it is.
fn feedpoint_impedance(
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    v_vec: &[Complex64],
    i_vec: &[Complex64],
    _freq_hz: f64,
) -> Result<(Complex64, usize, usize), String> {
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
        return Ok((z_in, seg.tag as usize, seg.tag_index as usize));
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

/// Upper bound on the number of frequency points a single sweep may request,
/// so a mistyped step (e.g. `0.0001` over a 1000 MHz span) can't queue millions
/// of matrix solves and freeze the GUI.
pub const MAX_SWEEP_POINTS: usize = 100_000;

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
        // Guard against a runaway sweep (e.g. a slipped decimal in the step) that
        // would queue millions of full matrix solves and freeze/OOM the GUI.
        let point_count = ((end_mhz - start_mhz) / step_mhz).floor() + 1.0;
        if point_count > MAX_SWEEP_POINTS as f64 {
            return Err(format!(
                "sweep would compute {point_count:.0} points (max {MAX_SWEEP_POINTS}); \
                 widen the step or narrow the range"
            ));
        }

        let parsed = parse(deck_text).map_err(|e| e.to_string())?;
        let deck = parsed.deck;
        let segs = build_geometry(&deck).map_err(|e| e.to_string())?;
        let v_vec = build_excitation(&deck, &segs).map_err(|e| e.to_string())?;
        let ground = ground_model_from_deck(&deck);
        // Reject geometry the solver cannot honestly take, before queueing a whole
        // sweep of solves on it.
        if let Some(e) = validate::geometry_error(&deck, &segs, &ground) {
            return Err(e);
        }
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
        nec_solver::build_deck_stamps(&self.deck, &self.segs, freq_hz).apply(&mut z_mat);

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

        let (z, _tag, _seg) =
            feedpoint_impedance(&self.deck, &self.segs, &self.v_vec, &sol.currents, freq_hz)?;
        Ok(SweepPoint {
            freq_mhz,
            z_re: z.re,
            z_im: z.im,
        })
    }

    /// The one caveat a swept negative resistance deserves, or `None`.
    ///
    /// Deliberately **not** a `warnings` field on [`SweepPoint`]. The cause is a
    /// property of the geometry, which is fixed for the whole sweep — so a
    /// per-point string would repeat one diagnosis up to `MAX_SWEEP_POINTS` times,
    /// restating a `z_re` and a `freq_mhz` the point already carries. A junctioned
    /// deck sweeps negative nearly everywhere, so this is the normal case, not the
    /// pathological one. One aggregate line instead, counting the points.
    pub fn negative_resistance_caveat(&self, points: &[SweepPoint]) -> Option<String> {
        let n = points
            .iter()
            .filter(|p| nec_solver::validate::is_negative_resistance(p.z_re))
            .count();
        if n == 0 {
            return None;
        }
        let cause = nec_solver::validate::negative_resistance_cause(&self.deck, &self.segs);
        Some(format!(
            "{n} of {} sweep points report negative feedpoint resistance, which is \
             physically impossible for a passive antenna; those results are unreliable — {cause}",
            points.len()
        ))
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
    // The currents/pattern views share this path; they must refuse the same decks
    // the impedance view does rather than draw a plausible-looking wrong pattern.
    if let Some(e) = validate::geometry_error(deck, &segs, &ground) {
        return Err(e);
    }
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
    nec_solver::build_deck_stamps(deck, &segs, freq_hz).apply(&mut z_mat);

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

#[cfg(test)]
mod tests {
    use super::*;

    // An inverted-V fed away from the apex — a genuine junction. Solves to a
    // negative feedpoint resistance on the Hallén path, which is physically
    // impossible for a passive antenna. Before FND-014 the GUI reported that
    // number with no caveat at all.
    const BENT_NEGATIVE_R: &str = "CM inverted-V fed away from the apex\nCE\nGW 1 21 -5.0 0 0.0 0.0 0 3.0 0.001\nGW 2 21 0.0 0 3.0 5.0 0 0.0 0.001\nGE 0\nEX 0 1 5 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    const CLEAN_DIPOLE: &str = "CM plain dipole\nCE\nGW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

    #[test]
    fn a_negative_resistance_solve_carries_a_caveat() {
        let r = solve_deck_str(BENT_NEGATIVE_R).expect("solve");
        assert!(
            r.z_re < 0.0,
            "fixture must produce Re Z < 0, got {}",
            r.z_re
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("negative resistance")),
            "physically impossible result reported without a caveat: {:?}",
            r.warnings
        );
    }

    #[test]
    fn a_clean_solve_carries_no_negative_resistance_caveat() {
        let r = solve_deck_str(CLEAN_DIPOLE).expect("solve");
        assert!(r.z_re > 0.0);
        assert!(
            !r.warnings.iter().any(|w| w.contains("negative resistance")),
            "clean dipole must not be warned about: {:?}",
            r.warnings
        );
    }

    #[test]
    fn the_sweep_caveat_is_one_line_for_the_whole_sweep() {
        let job = SweepJob::prepare(BENT_NEGATIVE_R, 13.8, 14.6, 0.2).expect("prepare");
        let pts: Vec<SweepPoint> = job
            .freqs_mhz()
            .iter()
            .map(|&f| job.solve_at(f).expect("solve"))
            .collect();
        assert!(pts.len() >= 4);
        assert!(pts.iter().all(|p| p.z_re < 0.0), "fixture assumption");
        let caveat = job.negative_resistance_caveat(&pts).expect("caveat");
        // One string, naming how many points, not one string per point.
        assert!(
            caveat.contains(&format!("{} of {}", pts.len(), pts.len())),
            "{caveat}"
        );
        assert!(caveat.contains("PH9-CHK-002"), "{caveat}");
    }

    #[test]
    fn the_sweep_caveat_counts_only_the_negative_points() {
        // An all-negative fixture cannot tell a real count from `points.len()`:
        // both read "N of N". This mixes signs so the numerator has to be earned.
        let job = SweepJob::prepare(BENT_NEGATIVE_R, 13.8, 14.6, 0.2).expect("prepare");
        let mut pts: Vec<SweepPoint> = job
            .freqs_mhz()
            .iter()
            .map(|&f| job.solve_at(f).expect("solve"))
            .collect();
        assert!(pts.len() >= 4, "need room to flip some");
        let total = pts.len();
        // Flip all but two to a physically sound resistance.
        for p in pts.iter_mut().skip(2) {
            p.z_re = 50.0;
        }
        let caveat = job.negative_resistance_caveat(&pts).expect("caveat");
        assert!(
            caveat.contains(&format!("2 of {total}")),
            "must count the negative points, not the whole sweep: {caveat}"
        );
    }

    #[test]
    fn a_clean_sweep_earns_no_caveat() {
        let job = SweepJob::prepare(CLEAN_DIPOLE, 14.0, 14.4, 0.1).expect("prepare");
        let pts: Vec<SweepPoint> = job
            .freqs_mhz()
            .iter()
            .map(|&f| job.solve_at(f).expect("solve"))
            .collect();
        assert_eq!(job.negative_resistance_caveat(&pts), None);
    }

    fn tmp(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("fnec_gui_vars_{name}"));
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn load_vars_toml_strings_ints_floats() {
        let p = tmp("ok.toml", "A = \"1.5\"\nN = 51\nR = 0.001\n");
        let m = load_vars(&p).expect("toml vars");
        assert_eq!(m.get("A").map(String::as_str), Some("1.5"));
        assert_eq!(m.get("N").map(String::as_str), Some("51"));
        assert_eq!(m.get("R").map(String::as_str), Some("0.001"));
    }

    #[test]
    fn load_vars_toml_rejects_unsupported_type() {
        let p = tmp("bad.toml", "A = [1, 2, 3]\n");
        let err = load_vars(&p).unwrap_err();
        assert!(err.contains("unsupported type"), "unexpected: {err}");
    }

    #[test]
    fn load_vars_json_flat_object() {
        let p = tmp("ok.json", "{\"A\": \"1.5\", \"N\": \"51\"}");
        let m = load_vars(&p).expect("json vars");
        assert_eq!(m.get("A").map(String::as_str), Some("1.5"));
        assert_eq!(m.get("N").map(String::as_str), Some("51"));
    }

    #[test]
    fn load_vars_json_requires_object() {
        let p = tmp("bad.json", "[1, 2, 3]");
        assert!(load_vars(&p).unwrap_err().contains("top-level object"));
    }

    #[test]
    fn load_vars_missing_file_errors() {
        let err = load_vars(std::path::Path::new("/no/such/vars.toml")).unwrap_err();
        assert!(err.contains("cannot read"));
    }

    #[test]
    fn apply_vars_none_is_passthrough_and_some_substitutes() {
        assert_eq!(apply_vars("GW 1 $N", None).unwrap(), "GW 1 $N");
        let p = tmp("sub.toml", "N = 51\n");
        let out = apply_vars("GW 1 $N 0 0 0", Some(p.to_str().unwrap())).unwrap();
        assert!(out.contains("51"), "substituted: {out}");
    }

    // --- pre-solve validation parity with the CLI (review-260719 FIND-006/007) ---

    /// Two wires crossing at mid-span, neither meeting the other at an endpoint.
    /// The CLI has always refused this; the GUI used to solve it and show a number.
    const CROSSING_WIRES: &str = "GW 1 11 -5 0 0 5 0 0 0.001\nGW 2 11 0 -5 0 0 5 0 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    /// A vertical wire whose base sits on an active (PEC) ground plane.
    const BURIED_OVER_PEC: &str =
        "GW 1 21 0 0 0 0 0 10 0.001\nGE 1\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    /// A clean lambda/2 dipole — the negative control for all of the above.
    const GOOD_DIPOLE: &str =
        "GW 1 21 -5.278 0 0 5.278 0 0 0.001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    #[test]
    fn gui_refuses_the_geometry_the_cli_refuses() {
        let err = solve_deck_str(CROSSING_WIRES).expect_err("crossing wires must be refused");
        assert!(err.contains("intersecting-wire"), "unexpected: {err}");
        let err = solve_deck_str(BURIED_OVER_PEC)
            .expect_err("a wire on the ground plane must be refused");
        assert!(err.contains("buried-wire"), "unexpected: {err}");
        // Negative control: a clean deck still solves, with nothing to report.
        let ok = solve_deck_str(GOOD_DIPOLE).expect("a clean dipole must still solve");
        assert!(ok.z_re > 50.0 && ok.z_re < 100.0, "unexpected Z: {ok:?}");
        assert!(
            ok.warnings.is_empty(),
            "unexpected warnings: {:?}",
            ok.warnings
        );
    }

    /// The sweep and the currents/pattern views are separate entry points, and each
    /// used to skip validation independently. A deck refused by the impedance view
    /// must be refused by all of them, or the GUI draws a plausible-looking wrong
    /// pattern for geometry it should not have solved.
    #[test]
    fn every_gui_solve_path_applies_the_same_rejection() {
        let sweep = match SweepJob::prepare(CROSSING_WIRES, 14.0, 14.4, 0.1) {
            Err(e) => e,
            Ok(_) => panic!("the sweep path must refuse it too"),
        };
        assert!(sweep.contains("intersecting-wire"), "unexpected: {sweep}");
        let currents = match solve_for_currents(CROSSING_WIRES) {
            Err(e) => e,
            Ok(_) => panic!("the currents path must refuse it too"),
        };
        assert!(
            currents.contains("intersecting-wire"),
            "unexpected: {currents}"
        );
        // Negative control: the clean deck is accepted on both.
        assert!(SweepJob::prepare(GOOD_DIPOLE, 14.0, 14.4, 0.1).is_ok());
        assert!(solve_for_currents(GOOD_DIPOLE).is_ok());
    }

    /// The GUI omitted the CLI's low-finite-ground warning, so a user got an
    /// unreliable near-ground number with nothing said about it.
    #[test]
    fn gui_surfaces_the_warnings_the_cli_surfaces() {
        // 0.05 lambda over GN 2 — inside the band where the reflection-coefficient
        // ground model is only approximate.
        let low = solve_deck_str(
            "GW 1 21 -5.278 0 1.056 5.278 0 1.056 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        )
        .expect("a low dipole over finite ground still solves");
        assert!(
            low.warnings
                .iter()
                .any(|w| w.contains("above finite ground")),
            "missing the low-ground warning: {:?}",
            low.warnings
        );
        // A degree-3 junction must still be flagged, and still name the MPIE.
        let tee = solve_deck_str(
            "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5 0 0 0.001\nGW 3 11 0 0 0 0 0 5 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        )
        .expect("a T junction solves, unreliably");
        assert!(
            tee.warnings.iter().any(|w| w.contains("--solver mpie")),
            "missing the topology warning: {:?}",
            tee.warnings
        );
    }

    // --- deck-level caveats, shown on every tab (GUI follow-up to #369) -------

    #[test]
    fn deck_warnings_reports_the_same_caveats_the_solve_panel_shows() {
        // 0.05 lambda over GN 2 — solvable, but only approximately.
        let low = "GW 1 21 -5.278 0 1.056 5.278 0 1.056 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let from_panel = solve_deck_str(low).expect("solves").warnings;
        let from_deck = deck_warnings(low);
        assert!(
            !from_deck.is_empty(),
            "a low antenna over finite ground must produce caveats"
        );
        assert_eq!(
            from_deck, from_panel,
            "the strip and the Solve panel must not disagree about the same deck"
        );
    }

    #[test]
    fn a_clean_deck_has_no_caveats_so_the_strip_stays_hidden() {
        let clean = "GW 1 21 -5.278 0 0 5.278 0 0 0.001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        assert!(deck_warnings(clean).is_empty());
    }

    /// A deck that cannot be parsed or built reports nothing here: the action the
    /// user ran surfaces that failure itself, and repeating it would be noise.
    /// This must not panic — it runs on every solve.
    #[test]
    fn an_unusable_deck_yields_no_caveats_rather_than_panicking() {
        assert!(deck_warnings("NOT A DECK\n").is_empty());
        assert!(deck_warnings("").is_empty());
        // Parses, but the geometry cannot be built (no GW cards).
        assert!(deck_warnings("GE\nEN\n").is_empty());
    }

    /// The topology caveat is what a Sweep- or Pattern-only user most needs and
    /// never saw: it says the numbers on screen are unreliable.
    #[test]
    fn deck_warnings_carries_the_unreliable_topology_caveat() {
        let tee = "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5 0 0 0.001\nGW 3 11 0 0 0 0 0 5 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let w = deck_warnings(tee);
        assert!(
            w.iter().any(|m| m.contains("--solver mpie")),
            "expected the topology caveat: {w:?}"
        );
    }
}
