// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Single-frequency Hallen solve — thin wrapper around `nec_solver` for use
//! by the GUI.  Returns the first feedpoint impedance found in the deck.

use std::collections::HashMap;
use std::path::Path;

use nec_model::deck::NecDeck;
use nec_parser::parse;
use nec_solver::validate;
use nec_solver::{
    assemble_z_matrix_with_ground, build_excitation, build_geometry, compute_radiation_pattern,
    ground_model_from_deck, FarFieldPoint, GroundModel, Segment,
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

/// How a GUI user reaches the MPIE: a picker, not a command-line flag.
///
/// The shared diagnostics take this from the caller precisely so neither frontend
/// quotes the other's interface — telling someone with a solver dropdown in front
/// of them to "re-run with `--solver mpie`" describes a program they are not using.
pub const GUI_MPIE_REMEDY: &str = "switch the solver to MPIE";

/// The solver the GUI will run. Re-exported so the rest of the app names one type.
pub use nec_solver::validate::SolverKind;

/// The diagnostic context for a GUI solve on `solver`.
fn gui_ctx(solver: SolverKind) -> nec_solver::validate::SolverContext<'static> {
    nec_solver::validate::SolverContext {
        kind: solver,
        mpie_remedy: GUI_MPIE_REMEDY,
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
    /// Caveats for this deck on the solver that produced it (unreliable
    /// topology, deferred ground, unsupported loads). They are solver-dependent:
    /// on Hallén a junction or loop earns a caveat naming the MPIE, and on the
    /// MPIE that caveat is absent because it models those correctly.
    pub warnings: Vec<String>,
    /// Wire tag the impedance was measured at.
    pub feed_tag: usize,
    /// Segment index within that tag.
    ///
    /// Reported so the panel can say *where* the impedance came from, and so a
    /// test can check that the GUI resolved the right `EX` card rather than
    /// asking the seam directly and proving nothing about the GUI (FND-031).
    pub feed_seg: usize,
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
pub fn solve_deck_path(
    path: &Path,
    vars_path: Option<&str>,
    solver: SolverKind,
) -> Result<SolveResult, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    solve_deck_str(&input, solver)
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
    solver: SolverKind,
) -> Result<crate::mesh::GeometryCurrents, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    load_currents_str(&input, solver)
}

/// Build geometry + current magnitudes from a raw NEC deck string.
pub fn load_currents_str(
    deck_text: &str,
    solver: SolverKind,
) -> Result<crate::mesh::GeometryCurrents, String> {
    let SolvedDeck {
        segs,
        currents,
        freq_hz: _freq_hz,
        ground,
        v_vec: _v_vec,
    } = solve_for_currents(deck_text, solver)?;
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
    solver: SolverKind,
) -> Result<crate::mesh::PatternSolve, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    pattern_grid_str(&input, solver)
}

/// Build geometry + full-sphere pattern grid from a raw NEC deck string.
pub fn pattern_grid_str(
    deck_text: &str,
    solver: SolverKind,
) -> Result<crate::mesh::PatternSolve, String> {
    use crate::mesh::{LOBE_N_PHI, LOBE_N_THETA};
    let SolvedDeck {
        segs,
        currents,
        freq_hz,
        ground,
        v_vec,
    } = solve_for_currents(deck_text, solver)?;

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
    // Directivity becomes gain over lossy ground, as the CLI has done since
    // PH9-CHK-003. Without this the same deck's lobe read as gain in one frontend
    // and directivity in the other, with nothing saying which (FND-053).
    let delta_db = gui_gain_correction_db(deck_text, &segs, &currents, freq_hz, &ground, &v_vec);
    let gains_dbi = results
        .iter()
        .map(|r| (r.gain_total_dbi + delta_db) as f32)
        .collect();

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
    solver: SolverKind,
) -> Result<Vec<String>, String> {
    let mut warnings: Vec<String> = parse_warnings.iter().map(ToString::to_string).collect();
    for d in validate::diagnose(deck, segs, ground, freq_hz, gui_ctx(solver)) {
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
pub fn deck_warnings(deck_text: &str, solver: SolverKind) -> Vec<String> {
    let Ok(parsed) = parse(deck_text) else {
        return Vec::new();
    };
    let deck = &parsed.deck;
    let Ok(segs) = build_geometry(deck) else {
        return Vec::new();
    };
    let ground = ground_model_from_deck(deck);
    // The *governing* FR card — the last one — through the shared expansion, not
    // the first card read inline. Three sites in this file read the first card,
    // which is a divergent copy of a decision `nec_solver::frequency` now owns
    // (FND-057). The `0.0` placeholder for an FR-less deck is unchanged; the
    // caveat producers guard against it explicitly.
    let freq_hz = nec_solver::frequencies_hz(deck)
        .first()
        .copied()
        .unwrap_or(0.0);
    // A hard rejection is surfaced by the action itself; keep only the caveats.
    validate_deck(deck, &segs, &ground, freq_hz, &parsed.warnings, solver).unwrap_or_default()
}

pub fn solve_deck_str(deck_text: &str, solver: SolverKind) -> Result<SolveResult, String> {
    let parsed = parse(deck_text).map_err(|e| e.to_string())?;
    let deck = &parsed.deck;

    // --- geometry & excitation -------------------------------------------
    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let ground = ground_model_from_deck(deck);

    // --- frequency -------------------------------------------------------
    // The governing FR card, via the shared expansion (FND-057).
    let freq_hz = nec_solver::frequencies_hz(deck)
        .first()
        .copied()
        .ok_or_else(|| "deck has no FR card".to_string())?;

    // --- validation (before any solve) -----------------------------------
    let warnings = validate_deck(deck, &segs, &ground, freq_hz, &parsed.warnings, solver)?;

    // --- impedance matrix ------------------------------------------------
    // Only the Hallén path consumes it. The MPIE builds its own system from the
    // geometry and ignores `z_mat` entirely, so assembling it there was an O(N²)
    // fill computed and thrown away on every solve.
    let mut z_mat = hallen_z_matrix(deck, &segs, freq_hz, &ground, solver);

    // --- Hallen solve ----------------------------------------------------
    let (currents, port_voltage) =
        solve_currents(deck, &segs, &mut z_mat, freq_hz, &ground, solver)?;

    // --- feedpoint impedance --------------------------------------------
    let (z, tag, seg) = feedpoint_impedance(deck, &segs, &v_vec, &currents, freq_hz, port_voltage)?;

    // FND-014: physically impossible results were reported here without a caveat.
    // `warnings` is already rendered by `impedance_view`, so this needs no new
    // display path — only the check that was missing.
    let mut warnings = warnings;
    if let Some(w) = nec_solver::validate::negative_resistance_warning(
        z.re,
        tag,
        seg,
        deck,
        &segs,
        gui_ctx(solver),
    ) {
        warnings.push(w);
    }

    Ok(SolveResult {
        freq_mhz: freq_hz / 1_000_000.0,
        z_re: z.re,
        z_im: z.im,
        warnings,
        feed_tag: tag,
        feed_seg: seg,
    })
}

/// The currents for a deck at one frequency, on whichever solver and drive it carries.
///
/// One step, three callers. The single solve, the sweep and the currents/pattern
/// view each had their own copy of "build the RHS and call `solve_hallen`", and
/// adding a current-source branch to one of them would have made an `EX 4` deck
/// solvable on one tab and refused on three — the FND-038 shape. The solver
/// picker branches here for the same reason: a picker that changed only the Solve
/// tab would be that defect again, one solver over.
///
/// A current-source deck cannot be handled by branching at the *pricing* step:
/// its excitation vector is all zeros, so `V/I` has nothing to work with. It needs
/// a different solve, which is why the branch is here (FND-045).
///
/// Returns the port voltage for a current-source deck, and `None` for a delta gap.
/// A deck carrying both is refused before this by `validate::pre_solve_error`
/// (FND-036), so the two cases really are exclusive.
/// The stamped Hallén impedance matrix, or an empty one on the MPIE path.
///
/// The MPIE assembles its own system from the geometry and never reads this, so
/// filling it there is pure waste — an O(N²) matrix built and discarded, and on
/// a sweep once per frequency point. Returning an empty matrix rather than an
/// `Option` keeps the one call signature for `solve_currents`, whose MPIE branch
/// returns before touching it.
fn hallen_z_matrix(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    ground: &GroundModel,
    solver: SolverKind,
) -> nec_solver::ZMatrix {
    if solver == SolverKind::Mpie {
        return nec_solver::ZMatrix::new(0);
    }
    let mut z_mat = assemble_z_matrix_with_ground(segs, freq_hz, ground);
    nec_solver::build_deck_stamps(deck, segs, freq_hz).apply_couplings(&mut z_mat);
    z_mat
}

#[allow(clippy::too_many_arguments)]
fn solve_currents(
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    z_mat: &mut nec_solver::ZMatrix,
    freq_hz: f64,
    ground: &GroundModel,
    solver: SolverKind,
) -> Result<(Vec<Complex64>, Option<Complex64>), String> {
    // The MPIE builds its own system from the geometry, so it takes neither the
    // assembled `z_mat` nor the Hallén endpoint/junction bookkeeping. Its
    // refusals travel inside `solve_mpie_session` (#414), so this branch cannot
    // hand it a deck it would answer wrongly.
    if solver == SolverKind::Mpie {
        let currents = nec_solver::solve_mpie_session(deck, segs, ground, freq_hz)
            .map_err(|e| e.to_string())?;
        return Ok((currents, None));
    }

    // One call for every Hallén route (#FND-121). This used to be a
    // current-source branch followed by a plain `solve_hallen`, with no arm for
    // the conductor-path basis the CLI had — so the GUI answered a bent or split
    // geometry on the wrong basis and showed the result with no caveat, because
    // the shared caveat producer suppresses the junction warning for exactly
    // that class.
    // Loads are applied by the session as matrix columns in the basis that runs
    // (FND-122), so they arrive here as data and are stamped into the matrix that
    // is solved. Deltas, not assignments: this runs once per matrix.
    let loads = nec_solver::build_deck_stamps(deck, segs, freq_hz).diagonal;
    let routed = nec_solver::solve_hallen_routed(deck, segs, z_mat, freq_hz, &loads)
        .map_err(|e| e.to_string())?;
    Ok((routed.currents, routed.port_voltage))
}

/// Compute feedpoint impedance Z = V/I for the first EX card, with the tag and
/// segment it resolved to — a caveat about the result has to name where it is.
fn feedpoint_impedance(
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    v_vec: &[Complex64],
    i_vec: &[Complex64],
    _freq_hz: f64,
    // `Some` when the deck is current-driven: its excitation vector is all zeros,
    // so `V/I` has nothing to work with and the port voltage from the solve is the
    // only `V` there is (FND-045).
    port_voltage: Option<Complex64>,
) -> Result<(Complex64, usize, usize), String> {
    // A current-driven deck is priced from the solved port voltage, not V/I: its
    // excitation vector is all zeros, so there is no V to divide (FND-045). The
    // source current is the one the `EX 4` card impressed.
    if let Some(v_port) = port_voltage {
        let (ex, _) = nec_solver::feedpoints(deck)
            .find(|(_, role)| *role == nec_model::card::FeedpointRole::CurrentSource)
            .ok_or("a current-source solve without a current source")?;
        let i0 = Complex64::new(ex.voltage_real, ex.voltage_imag);
        let z_in =
            nec_solver::feedpoint_impedance(v_port, i0, ex.tag as usize, ex.segment as usize)
                .map_err(|e| e.to_string())?;
        return Ok((z_in, ex.tag as usize, ex.segment as usize));
    }

    // Through the shared seam (FND-031). This loop took the first `EX` of any
    // type, so a deck with a plane wave ahead of its voltage source reported the
    // plane wave's NTHETA/NPHI as a feedpoint tag and segment.
    if let Some(ex) = nec_solver::first_delta_gap_feedpoint(deck) {
        let Some((idx, seg)) = segs
            .iter()
            .enumerate()
            .find(|(_, seg)| seg.tag == ex.tag && seg.tag_index == ex.segment)
        else {
            // Unreachable today: `build_excitation` rejects an EX naming an absent
            // segment before this runs. Kept defensive, but saying what would
            // actually be true here — the deck HAS an EX; its segment is missing.
            return Err(format!(
                "EX on tag {} segment {} names a segment the geometry does not contain",
                ex.tag, ex.segment
            ));
        };
        let current = i_vec[idx];
        let v_source = v_vec[idx] * seg.length;
        let z_in = nec_solver::feedpoint_impedance(
            v_source,
            current,
            ex.tag as usize,
            ex.segment as usize,
        )
        .map_err(|e| e.to_string())?;
        return Ok((z_in, seg.tag as usize, seg.tag_index as usize));
    }
    // Reached only when the deck has no driven feedpoint at all — a plane-wave
    // receive deck. Current sources are priced above now (FND-045), so the
    // "use the fnec CLI" remedy no longer applies to them.
    Err(
        nec_solver::validate::unpriceable_feedpoint_error(deck, "use the fnec CLI for this deck")
            .unwrap_or_else(|| {
                // Not "no EX card" here either: a plane-wave receive deck has one,
                // and #397 deliberately lets such decks solve. What it lacks is a
                // *driven* feedpoint. Same wording the worker already used.
                "no driven feedpoint (EX voltage source) found in deck".to_string()
            }),
    )
}

/// Run a sweep on the selected solver over a frequency range for the deck at `path`.
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
    solver: SolverKind,
) -> Result<Vec<SweepPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    sweep_deck_str(&input, start_mhz, end_mhz, step_mhz, solver)
}

/// Run a sweep on the selected solver for a deck given as a string.
pub fn sweep_deck_str(
    deck_text: &str,
    start_mhz: f64,
    end_mhz: f64,
    step_mhz: f64,
    solver: SolverKind,
) -> Result<Vec<SweepPoint>, String> {
    let job = SweepJob::prepare(deck_text, start_mhz, end_mhz, step_mhz, solver)?;
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
    freqs_mhz: Vec<f64>,
    solver: SolverKind,
}

impl SweepJob {
    /// Parse the deck and build the frequency-independent pieces once.
    pub fn prepare(
        deck_text: &str,
        start_mhz: f64,
        end_mhz: f64,
        step_mhz: f64,
        solver: SolverKind,
    ) -> Result<Self, String> {
        // Finiteness first: every comparison against `NaN` is false, so an
        // ordering test alone lets one through (FND-056).
        for (name, v) in [
            ("start_mhz", start_mhz),
            ("end_mhz", end_mhz),
            ("step_mhz", step_mhz),
        ] {
            if !v.is_finite() {
                return Err(format!("{name} must be a finite number, got {v}"));
            }
        }
        if step_mhz <= 0.0 {
            return Err(format!("step_mhz must be > 0, got {step_mhz}"));
        }
        // This range comes from the UI, never from the deck's `FR` card, so the
        // shared `frequency_error` never sees it.
        if start_mhz <= 0.0 {
            return Err(format!(
                "start_mhz must be a positive frequency, got {start_mhz}"
            ));
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
        if let Some(e) = validate::pre_solve_error(&deck, &segs, &ground) {
            return Err(e);
        }
        // ...and what the *chosen solver* cannot take. Without this an MPIE sweep
        // of a loaded deck would queue every point and fail on the first one.
        if solver == SolverKind::Mpie {
            if let Some(u) = nec_solver::mpie_unsupported(&deck) {
                return Err(u.to_string());
            }
        }
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
            freqs_mhz,
            solver,
        })
    }

    /// The frequencies (MHz) this job will solve, in ascending order.
    pub fn freqs_mhz(&self) -> &[f64] {
        &self.freqs_mhz
    }

    /// Solve the feedpoint impedance at one frequency (MHz).
    pub fn solve_at(&self, freq_mhz: f64) -> Result<SweepPoint, String> {
        let freq_hz = freq_mhz * 1_000_000.0;

        // Per point, so the discarded fill cost the whole sweep, not one solve.
        let mut z_mat = hallen_z_matrix(&self.deck, &self.segs, freq_hz, &self.ground, self.solver);

        let (currents, port_voltage) = solve_currents(
            &self.deck,
            &self.segs,
            &mut z_mat,
            freq_hz,
            &self.ground,
            self.solver,
        )?;

        let (z, _tag, _seg) = feedpoint_impedance(
            &self.deck,
            &self.segs,
            &self.v_vec,
            &currents,
            freq_hz,
            port_voltage,
        )?;
        Ok(SweepPoint {
            freq_mhz,
            z_re: z.re,
            z_im: z.im,
        })
    }

    /// The deck's geometry and ground caveats, evaluated for the frequency range
    /// this sweep will actually run (FND-042).
    ///
    /// The caveat panel gets these from `deck_warnings`, which reads the deck's
    /// `FR` card — the right frequency for a single solve and the wrong one for a
    /// sweep, whose range the user types into the UI. A deck whose `FR` says
    /// 30 MHz, swept from 5 MHz, dipped far below 0.1 λ with nothing said.
    ///
    /// The low-ground check trips below 0.1 λ, so the **lowest** swept frequency
    /// is the worst case: if it does not trip there it trips nowhere. Same choice
    /// the distributed path makes, from the same shared producer, so the two
    /// cannot drift.
    pub fn geometry_caveats(&self) -> Vec<String> {
        // Only the frequency-dependent caveat. The deck-caveat strip above the tab
        // already renders the topology and junction ones from `diagnose`, and they
        // do not vary with frequency — including them here printed the same
        // sentence twice on one screen for a junction-fed deck, which is the normal
        // case for a sweep that earns caveats at all.
        // The MPIE carries the Sommerfeld surface wave in its Z-matrix, so on that
        // solver the low-ground caveat does not apply — it describes a Hallén
        // limitation. This used to pass a hardcoded `false` under a comment saying
        // "the GUI is Hallén-only", which stopped being true the moment the picker
        // landed.
        if self.solver == SolverKind::Mpie {
            return Vec::new();
        }
        nec_solver::validate::swept_low_ground_caveat(
            &self.segs,
            &self.ground,
            &self
                .freqs_mhz
                .iter()
                .map(|f| f * 1_000_000.0)
                .collect::<Vec<_>>(),
            false,
        )
        .into_iter()
        .collect()
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
        // Through the shared producer: `fnec_py` needs the same sentence, and a
        // second copy is how the GUI's fix stayed local (FND-032).
        let z_res: Vec<f64> = points.iter().map(|p| p.z_re).collect();
        nec_solver::validate::swept_negative_resistance_caveat(
            &z_res,
            &self.deck,
            &self.segs,
            gui_ctx(self.solver),
        )
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
    solver: SolverKind,
) -> Result<Vec<PatternPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    pattern_slice_deck_str(&input, phi_deg, solver)
}

/// The lossy-ground gain correction for a GUI pattern, in dB (0.0 when none).
///
/// Re-parses the deck for its feedpoints rather than threading them through four
/// call sites: the parse is already done once per pattern request and costs
/// nothing beside the solve, and the alternative was a fifth parameter on a
/// function that already carries five.
fn gui_gain_correction_db(
    deck_text: &str,
    segs: &[nec_solver::Segment],
    currents: &[Complex64],
    freq_hz: f64,
    ground: &GroundModel,
    v_vec: &[Complex64],
) -> f64 {
    let Ok(parsed) = parse(deck_text) else {
        return 0.0;
    };
    let p_in = nec_solver::feedpoint_input_power(&parsed.deck, segs, v_vec, currents);
    nec_solver::gain_correction_db(segs, currents, freq_hz, ground, p_in).unwrap_or(0.0)
}

/// Compute an elevation-plane radiation-pattern slice from a raw deck string.
pub fn pattern_slice_deck_str(
    deck_text: &str,
    phi_deg: f64,
    solver: SolverKind,
) -> Result<Vec<PatternPoint>, String> {
    let SolvedDeck {
        segs,
        currents,
        freq_hz,
        ground,
        v_vec,
    } = solve_for_currents(deck_text, solver)?;

    // Build 37-point theta grid: 0, 5, 10, … 180 deg.
    let points: Vec<FarFieldPoint> = (0..=36)
        .map(|i| FarFieldPoint {
            theta_deg: i as f64 * 5.0,
            phi_deg,
        })
        .collect();

    let results = compute_radiation_pattern(&segs, &currents, freq_hz, &points, &ground);
    // Same correction as the full-sphere grid: the elevation slice is the same
    // quantity, and correcting one view and not the other would put two different
    // numbers for one deck on two tabs (FND-053).
    let delta_db = gui_gain_correction_db(deck_text, &segs, &currents, freq_hz, &ground, &v_vec);

    Ok(results
        .into_iter()
        .map(|r| PatternPoint {
            theta_deg: r.theta_deg,
            phi_deg: r.phi_deg,
            gain_total_dbi: r.gain_total_dbi + delta_db,
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
    solver: SolverKind,
) -> Result<Vec<CurrentPoint>, String> {
    let input = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    let input = apply_vars(&input, vars_path)?;
    current_distribution_deck_str(&input, solver)
}

/// Compute the per-segment current distribution from a raw deck string.
pub fn current_distribution_deck_str(
    deck_text: &str,
    solver: SolverKind,
) -> Result<Vec<CurrentPoint>, String> {
    let SolvedDeck {
        segs,
        currents,
        freq_hz: _freq_hz,
        ground: _ground,
        v_vec: _v_vec,
    } = solve_for_currents(deck_text, solver)?;

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

/// Everything a pattern or currents view needs from one solve.
///
/// A named struct rather than a five-tuple: it grew a fifth member when the gain
/// correction needed the excitation vector (FND-053), at which point the tuple
/// stopped being readable and clippy said so.
struct SolvedDeck {
    segs: Vec<nec_solver::Segment>,
    currents: Vec<Complex64>,
    freq_hz: f64,
    ground: nec_solver::GroundModel,
    /// Needed for the feedpoint input power the gain correction divides by.
    v_vec: Vec<Complex64>,
}

fn solve_for_currents(deck_text: &str, solver: SolverKind) -> Result<SolvedDeck, String> {
    let parsed = parse(deck_text).map_err(|e| e.to_string())?;
    let deck = &parsed.deck;

    let segs = build_geometry(deck).map_err(|e| e.to_string())?;
    let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
    let ground = ground_model_from_deck(deck);
    // The currents/pattern views share this path; they must refuse the same decks
    // the impedance view does rather than draw a plausible-looking wrong pattern.
    if let Some(e) = validate::pre_solve_error(deck, &segs, &ground) {
        return Err(e);
    }
    // No current-source refusal here any more: this path prices one now, through
    // the same `solve_currents` step the Solve tab uses (FND-045). The guard that
    // stood here existed because these views would otherwise have rendered zero
    // currents and a meaningless pattern — that hazard is gone with the capability.

    // The governing FR card, via the shared expansion (FND-057).
    let freq_hz = nec_solver::frequencies_hz(deck)
        .first()
        .copied()
        .ok_or_else(|| "deck has no FR card".to_string())?;

    let mut z_mat = hallen_z_matrix(deck, &segs, freq_hz, &ground, solver);

    // Currents and pattern get the current-source branch too. Solving an `EX 4`
    // deck on the Solve tab while these three refused it would be the FND-038
    // shape all over again.
    let (currents, _port_voltage) =
        solve_currents(deck, &segs, &mut z_mat, freq_hz, &ground, solver)?;

    Ok(SolvedDeck {
        segs,
        currents,
        freq_hz,
        ground,
        v_vec,
    })
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
        let r = solve_deck_str(BENT_NEGATIVE_R, SolverKind::Hallen).expect("solve");
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

    /// FND-031, still pinned through the GUI's own entry point — but the deck is
    /// now **refused** rather than solved (FND-050), so the assertion moved from
    /// the solved feedpoint to the refusal's text.
    ///
    /// It is the same fact either way: the message must name tag 1 segment 26,
    /// the *voltage source*, and not the plane wave's NTHETA/NPHI, which the old
    /// unfiltered loop read as a feedpoint. An earlier version of this test called
    /// `first_delta_gap_feedpoint` directly, which tested the seam and not the
    /// adoption — reverting the GUI to its old loop passed it, because no call
    /// edge to the GUI existed at all. That trap is why this still goes through
    /// `solve_deck_str`.
    #[test]
    fn a_plane_wave_is_not_read_as_the_feedpoint() {
        let deck_src = include_str!("../../../corpus/dipole-planewave-then-source-51seg.nec");
        let e = solve_deck_str(deck_src, SolverKind::Hallen)
            .expect_err("a plane wave beside a driven source is refused");
        assert!(
            e.contains("tag 1 segment 26"),
            "must name the voltage source, not the plane wave's NTHETA/NPHI: {e}"
        );
        assert!(e.contains("plane wave"), "{e}");
    }

    // The corpus reference for `dipole-ex4-freesp-51seg`, pinned under PH8-CHK-001:
    // the current-source feedpoint Z = V_port/i0 equals the voltage-source dipole
    // impedance, which is the internal consistency that path is validated against.
    const EX4_DECK: &str = "CM current-source feed\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 4 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

    #[test]
    fn a_current_source_deck_solves_and_agrees_with_the_cli() {
        // FND-045. This used to error with "use the fnec CLI for this deck" — the
        // machinery was in `nec_solver` all along, just unwired. The assertion is
        // the corpus value the CLI produces, so the two frontends cannot drift.
        let r = solve_deck_str(EX4_DECK, SolverKind::Hallen)
            .expect("the GUI can price a current source now");
        assert!(
            (r.z_re - 74.23).abs() < 0.05 && (r.z_im - 13.9).abs() < 0.05,
            "GUI disagrees with the CLI's corpus value: {} + j{}",
            r.z_re,
            r.z_im
        );
        // And it names the current source, not some other EX.
        assert_eq!((r.feed_tag, r.feed_seg), (1, 26));
    }

    #[test]
    fn a_current_source_sweep_agrees_with_the_single_solve() {
        // The sweep is a separate solve path; solving on one tab and refusing on
        // another is the FND-038 shape.
        let job =
            SweepJob::prepare(EX4_DECK, 14.2, 14.4, 0.1, SolverKind::Hallen).expect("prepare");
        let pt = job.solve_at(14.2).expect("sweep must price it too");
        let single = solve_deck_str(EX4_DECK, SolverKind::Hallen).expect("single solve");
        assert!(
            (pt.z_re - single.z_re).abs() < 1e-6,
            "sweep {} vs single {}",
            pt.z_re,
            single.z_re
        );
    }

    #[test]
    fn the_currents_and_pattern_views_solve_a_current_source_deck_too() {
        // They used to refuse it by name. Leaving them refusing while the Solve
        // tab priced it would be the same one-tab-over defect this arc keeps
        // finding — so all three paths go through `solve_currents`.
        let currents = load_currents_str(EX4_DECK, SolverKind::Hallen).expect("currents view");
        assert!(
            currents.currents_ma.iter().any(|c| *c > 1e-9),
            "a driven deck must carry current"
        );
        pattern_grid_str(EX4_DECK, SolverKind::Hallen).expect("pattern view");
    }

    #[test]
    fn a_clean_solve_carries_no_negative_resistance_caveat() {
        let r = solve_deck_str(CLEAN_DIPOLE, SolverKind::Hallen).expect("solve");
        assert!(r.z_re > 0.0);
        assert!(
            !r.warnings.iter().any(|w| w.contains("negative resistance")),
            "clean dipole must not be warned about: {:?}",
            r.warnings
        );
    }

    #[test]
    fn the_sweep_caveat_is_one_line_for_the_whole_sweep() {
        let job = SweepJob::prepare(BENT_NEGATIVE_R, 13.8, 14.6, 0.2, SolverKind::Hallen)
            .expect("prepare");
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

    // Antenna 0.634 m up over `GN 2`, with the deck's `FR` at 60 MHz — where
    // 0.634 m is 0.127 lambda, comfortably above the 0.1 lambda threshold. Sweeping
    // down to 14.2 MHz takes it to 0.030 lambda, deep into the caveat's range.
    const LOW_ONLY_WHEN_SWEPT: &str = "CM low over ground only at the bottom of the sweep\nCE\nGW 1 21 -5.282 0 0.634 5.282 0 0.634 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 60.0 0\nEN\n";

    #[test]
    fn a_sweep_earns_the_caveats_its_range_deserves_not_the_fr_cards() {
        // FND-042. `deck_warnings` reads the deck's `FR` card — right for a single
        // solve, wrong for a sweep, whose range the user types into the UI. This
        // deck is above the threshold at its `FR` frequency and far below it at the
        // bottom of the swept range, so the two answers genuinely differ.
        let at_fr = deck_warnings(LOW_ONLY_WHEN_SWEPT, SolverKind::Hallen);
        assert!(
            !at_fr.iter().any(|w| w.contains("above finite ground")),
            "fixture must be clean at its FR frequency or this proves nothing: {at_fr:?}"
        );

        let job = SweepJob::prepare(LOW_ONLY_WHEN_SWEPT, 14.2, 60.0, 5.0, SolverKind::Hallen)
            .expect("prepare");
        let caveats = job.geometry_caveats();
        assert!(
            caveats.iter().any(|w| w.contains("above finite ground")),
            "the swept range goes far below 0.1 lambda: {caveats:?}"
        );
        assert!(
            caveats.iter().any(|w| w.contains("worst case, at 14.2")),
            "must name the frequency the quoted height belongs to: {caveats:?}"
        );
    }

    #[test]
    fn the_sweep_panel_does_not_repeat_what_the_deck_strip_already_shows() {
        // A junction-fed deck is the normal case for a sweep that earns caveats at
        // all, and the topology and junction caveats do not vary with frequency —
        // so the strip above the tab already shows them. Emitting them again in the
        // sweep panel printed the same sentence twice on one screen.
        // A degree-3 T over GN 2 — a bend is merged into one conductor path and
        // earns no topology caveat (PH9-CHK-002), so a bent fixture here would have
        // an empty strip and prove nothing.
        const LOW_TEE: &str = "CM T junction low over ground\nCE\nGW 1 13 0 0 0.634 5.282 0 0.634 0.001\nGW 2 13 0 0 0.634 -5.282 0 0.634 0.001\nGW 3 13 0 0 0.634 0 0 5.916 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let job = SweepJob::prepare(LOW_TEE, 13.8, 14.6, 0.2, SolverKind::Hallen).expect("prepare");
        let strip = deck_warnings(LOW_TEE, SolverKind::Hallen);
        let panel = job.geometry_caveats();
        assert!(
            !strip.is_empty(),
            "fixture must earn strip caveats or this proves nothing"
        );
        assert!(
            strip.iter().any(|w| w.contains("junction")),
            "strip must carry the frequency-independent caveats: {strip:?}"
        );
        for w in &panel {
            assert!(
                !strip.contains(w),
                "sweep panel repeats a caveat the strip already shows: {w}"
            );
        }
    }

    #[test]
    fn a_sweep_that_stays_high_earns_no_low_ground_caveat() {
        // Positive control's mirror: the same deck swept only where it is high
        // must stay quiet, or the test above would pass for a check that always
        // fires.
        let job = SweepJob::prepare(LOW_ONLY_WHEN_SWEPT, 60.0, 80.0, 5.0, SolverKind::Hallen)
            .expect("prepare");
        let caveats = job.geometry_caveats();
        assert!(
            !caveats.iter().any(|w| w.contains("above finite ground")),
            "0.127 lambda and up is not low: {caveats:?}"
        );
    }

    #[test]
    fn the_sweep_caveat_counts_only_the_negative_points() {
        // An all-negative fixture cannot tell a real count from `points.len()`:
        // both read "N of N". This mixes signs so the numerator has to be earned.
        let job = SweepJob::prepare(BENT_NEGATIVE_R, 13.8, 14.6, 0.2, SolverKind::Hallen)
            .expect("prepare");
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
        let job =
            SweepJob::prepare(CLEAN_DIPOLE, 14.0, 14.4, 0.1, SolverKind::Hallen).expect("prepare");
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
        let err = solve_deck_str(CROSSING_WIRES, SolverKind::Hallen)
            .expect_err("crossing wires must be refused");
        assert!(err.contains("intersecting-wire"), "unexpected: {err}");
        let err = solve_deck_str(BURIED_OVER_PEC, SolverKind::Hallen)
            .expect_err("a wire on the ground plane must be refused");
        assert!(err.contains("buried-wire"), "unexpected: {err}");
        // Negative control: a clean deck still solves, with nothing to report.
        let ok = solve_deck_str(GOOD_DIPOLE, SolverKind::Hallen)
            .expect("a clean dipole must still solve");
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
        let sweep = match SweepJob::prepare(CROSSING_WIRES, 14.0, 14.4, 0.1, SolverKind::Hallen) {
            Err(e) => e,
            Ok(_) => panic!("the sweep path must refuse it too"),
        };
        assert!(sweep.contains("intersecting-wire"), "unexpected: {sweep}");
        let currents = match solve_for_currents(CROSSING_WIRES, SolverKind::Hallen) {
            Err(e) => e,
            Ok(_) => panic!("the currents path must refuse it too"),
        };
        assert!(
            currents.contains("intersecting-wire"),
            "unexpected: {currents}"
        );
        // Negative control: the clean deck is accepted on both.
        assert!(SweepJob::prepare(GOOD_DIPOLE, 14.0, 14.4, 0.1, SolverKind::Hallen).is_ok());
        assert!(solve_for_currents(GOOD_DIPOLE, SolverKind::Hallen).is_ok());
    }

    /// The GUI omitted the CLI's low-finite-ground warning, so a user got an
    /// unreliable near-ground number with nothing said about it.
    #[test]
    fn gui_surfaces_the_warnings_the_cli_surfaces() {
        // 0.05 lambda over GN 2 — inside the band where the reflection-coefficient
        // ground model is only approximate.
        let low = solve_deck_str(
            "GW 1 21 -5.278 0 1.056 5.278 0 1.056 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
            SolverKind::Hallen,
        )
        .expect("a low dipole over finite ground still solves");
        assert!(
            low.warnings
                .iter()
                .any(|w| w.contains("above finite ground")),
            "missing the low-ground warning: {:?}",
            low.warnings
        );
        // A degree-3 junction must still be flagged, and must point at the MPIE
        // in *this* frontend's terms — a GUI user has a picker, not a flag.
        let tee = solve_deck_str(
            "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5 0 0 0.001\nGW 3 11 0 0 0 0 0 5 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
            SolverKind::Hallen,
        )
        .expect("a T junction solves, unreliably");
        assert!(
            tee.warnings.iter().any(|w| w.contains(GUI_MPIE_REMEDY)),
            "missing the topology warning: {:?}",
            tee.warnings
        );
        assert!(
            !tee.warnings.iter().any(|w| w.contains("--solver")),
            "the GUI must not quote a CLI flag at a user who has a picker: {:?}",
            tee.warnings
        );
    }

    // --- deck-level caveats, shown on every tab (GUI follow-up to #369) -------

    #[test]
    fn deck_warnings_reports_the_same_caveats_the_solve_panel_shows() {
        // 0.05 lambda over GN 2 — solvable, but only approximately.
        let low = "GW 1 21 -5.278 0 1.056 5.278 0 1.056 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let from_panel = solve_deck_str(low, SolverKind::Hallen)
            .expect("solves")
            .warnings;
        let from_deck = deck_warnings(low, SolverKind::Hallen);
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
        assert!(deck_warnings(clean, SolverKind::Hallen).is_empty());
    }

    /// A deck that cannot be parsed or built reports nothing here: the action the
    /// user ran surfaces that failure itself, and repeating it would be noise.
    /// This must not panic — it runs on every solve.
    #[test]
    fn an_unusable_deck_yields_no_caveats_rather_than_panicking() {
        assert!(deck_warnings("NOT A DECK\n", SolverKind::Hallen).is_empty());
        assert!(deck_warnings("", SolverKind::Hallen).is_empty());
        // Parses, but the geometry cannot be built (no GW cards).
        assert!(deck_warnings("GE\nEN\n", SolverKind::Hallen).is_empty());
    }

    /// The topology caveat is what a Sweep- or Pattern-only user most needs and
    /// never saw: it says the numbers on screen are unreliable.
    #[test]
    fn deck_warnings_carries_the_unreliable_topology_caveat() {
        let tee = "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5 0 0 0.001\nGW 3 11 0 0 0 0 0 5 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let w = deck_warnings(tee, SolverKind::Hallen);
        assert!(
            w.iter().any(|m| m.contains(GUI_MPIE_REMEDY)),
            "expected the topology caveat: {w:?}"
        );

        // ...and on the MPIE the same deck earns no topology caveat at all: the
        // MPIE models the junction correctly, so repeating a Hallén limitation
        // there would be false — and its remedy would name the running solver.
        let w_mpie = deck_warnings(tee, SolverKind::Mpie);
        assert!(
            !w_mpie.iter().any(|m| m.contains("T/Y junction")),
            "the MPIE solves T/Y junctions; it must not carry the Hallén caveat: {w_mpie:?}"
        );
    }
}
