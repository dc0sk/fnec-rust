use super::exec_profile::ExecutionMode;
use nec_model::card::Card;

pub(super) const C0: f64 = 299_792_458.0;
pub(super) const CONTINUITY_REL_RESIDUAL_MAX: f64 = 1e-3;
pub(super) const SINUSOIDAL_REL_RESIDUAL_MAX_DEFAULT: f64 = 1e-2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SolverMode {
    Hallen,
    Pulse,
    Continuity,
    Sinusoidal,
}

impl SolverMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            SolverMode::Hallen => "hallen",
            SolverMode::Pulse => "pulse",
            SolverMode::Continuity => "continuity",
            SolverMode::Sinusoidal => "sinusoidal",
        }
    }
}

/// Near-ground impedance model (PH9-CHK-006). `Rcm` is the default scalar-Γ
/// reflection-coefficient image; `Sommerfeld` adds the surface-wave correction for
/// straight horizontal wires (accurate below ~0.1 λ, = nec2c GN2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GroundSolver {
    Rcm,
    Sommerfeld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PulseRhsMode {
    Raw,
    Nec2,
}

impl PulseRhsMode {
    pub(super) fn as_contract_str(self) -> &'static str {
        match self {
            PulseRhsMode::Raw => "Raw",
            PulseRhsMode::Nec2 => "Nec2",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BenchRecord {
    pub(super) mode: String,
    pub(super) pulse_rhs: String,
    pub(super) exec: String,
    pub(super) freq_mhz: f64,
    pub(super) abs_res: f64,
    pub(super) rel_res: f64,
    pub(super) diag_spread: f64,
    pub(super) sin_rel_res: f64,
}
use nec_report::{
    render_text_report, CurrentRow, FeedpointRow, LoadRow, PatternRow, ReportInput, SourceRow,
};
use nec_solver::{
    assemble_pocklington_matrix, assemble_z_matrix_with_ground, build_conductor_paths,
    build_current_source_shape, build_current_source_shape_paths, build_hallen_rhs,
    build_hallen_rhs_paths, build_loads, build_nt_stamps, build_planewave_hallen,
    build_planewave_hallen_paths, build_tl_stamps, classify_unsupported_topology,
    compute_radiation_pattern, detect_wire_junctions, integrate_radiated_power,
    merge_collinear_wire_endpoints, radiation_efficiency, scale_excitation_for_pulse_rhs, solve,
    solve_hallen, solve_hallen_current_source, solve_hallen_current_source_paths,
    solve_hallen_paths, solve_hallen_planewave, solve_hallen_planewave_paths,
    solve_hallen_sinusoidal_basis, solve_with_continuity_basis_per_wire, FarFieldPoint,
    GroundModel, Segment, UnsupportedTopology, ZMatrix,
};
use num_complex::Complex64;

pub(super) struct PulseCurrentSourceConstraint {
    pub(super) seg_index: usize,
    pub(super) source_current: Complex64,
    pub(super) original_row: Vec<Complex64>,
}

pub(super) struct FrequencySolveResult {
    pub(super) report: String,
    pub(super) diag_line: String,
    pub(super) bench: BenchRecord,
    pub(super) sweep_summary: Option<SweepPointSummary>,
}

pub(super) struct SweepPointSummary {
    pub(super) freq_mhz: f64,
    pub(super) tag: usize,
    pub(super) seg: usize,
    pub(super) z_re: f64,
    pub(super) z_im: f64,
}

pub(super) struct HybridLanePlan {
    pub(super) cpu_indices: Vec<usize>,
    pub(super) gpu_candidate_indices: Vec<usize>,
}

pub(super) fn l2_norm(v: &[Complex64]) -> f64 {
    v.iter()
        .map(num_complex::Complex::norm_sqr)
        .sum::<f64>()
        .sqrt()
}

pub(super) fn matrix_diagonal_spread(z: &ZMatrix) -> f64 {
    if z.n == 0 {
        return 0.0;
    }

    let mut max_diag = 0.0f64;
    let mut min_diag = f64::INFINITY;
    for i in 0..z.n {
        let d = z.get(i, i).norm();
        max_diag = max_diag.max(d);
        min_diag = min_diag.min(d);
    }

    if !max_diag.is_finite() || !min_diag.is_finite() {
        return f64::NAN;
    }
    if max_diag == 0.0 {
        return 0.0;
    }

    max_diag / min_diag.max(1e-30)
}

pub(super) fn residual_zi_minus_v(
    z: &ZMatrix,
    i_vec: &[Complex64],
    v_vec: &[Complex64],
) -> (f64, f64) {
    let n = z.n;
    let mut r = vec![Complex64::new(0.0, 0.0); n];
    for row in 0..n {
        let mut zi = Complex64::new(0.0, 0.0);
        for (col, i_col) in i_vec.iter().enumerate().take(n) {
            zi += z.get(row, col) * *i_col;
        }
        r[row] = zi - v_vec[row];
    }

    let res = l2_norm(&r);
    let denom = l2_norm(v_vec);
    let rel = if denom > 0.0 { res / denom } else { res };
    (res, rel)
}

pub(super) fn residual_hallen(
    z: &ZMatrix,
    i_vec: &[Complex64],
    c_hom_per_wire: &[Complex64],
    cos_vec: &[f64],
    rhs: &[Complex64],
    wire_endpoints: &[(usize, usize)],
) -> (f64, f64) {
    let n = z.n;
    let mut r = vec![Complex64::new(0.0, 0.0); n];

    let endpoints: &[(usize, usize)];
    let fallback_endpoints;
    if wire_endpoints.is_empty() || n == 0 {
        fallback_endpoints = if n > 0 { vec![(0usize, n - 1)] } else { vec![] };
        endpoints = &fallback_endpoints;
    } else {
        endpoints = wire_endpoints;
    }

    let mut row_wire = vec![0usize; n];
    for (wi, &(first, last)) in endpoints.iter().enumerate() {
        for rw in row_wire.iter_mut().take(last + 1).skip(first) {
            *rw = wi;
        }
    }

    for row in 0..n {
        let mut zi = Complex64::new(0.0, 0.0);
        for (col, i_col) in i_vec.iter().enumerate().take(n) {
            zi += z.get(row, col) * *i_col;
        }
        let c_row = c_hom_per_wire
            .get(row_wire[row])
            .copied()
            .or_else(|| c_hom_per_wire.first().copied())
            .unwrap_or(Complex64::new(0.0, 0.0));
        let lhs = zi - c_row * cos_vec[row];
        r[row] = lhs - rhs[row];
    }

    let res = l2_norm(&r);
    let denom = l2_norm(rhs);
    let rel = if denom > 0.0 { res / denom } else { res };
    (res, rel)
}

/// Residual of the conductor-path Hallén system (PH9-CHK-002): the counterpart of
/// [`residual_hallen`] for the general-junction solve, where the homogeneous
/// constant is grouped by conductor path (`path_of_seg`) rather than by contiguous
/// wire range. `cos_vec` already carries the path sign.
pub(super) fn residual_hallen_paths(
    z: &ZMatrix,
    i_vec: &[Complex64],
    c_hom_per_path: &[Complex64],
    cos_vec: &[f64],
    rhs: &[Complex64],
    path_of_seg: &[usize],
) -> (f64, f64) {
    let n = z.n;
    let mut r = vec![Complex64::new(0.0, 0.0); n];
    for row in 0..n {
        let mut zi = Complex64::new(0.0, 0.0);
        for (col, i_col) in i_vec.iter().enumerate().take(n) {
            zi += z.get(row, col) * *i_col;
        }
        let c_row = c_hom_per_path
            .get(path_of_seg[row])
            .copied()
            .unwrap_or(Complex64::new(0.0, 0.0));
        r[row] = (zi - c_row * cos_vec[row]) - rhs[row];
    }
    let res = l2_norm(&r);
    let denom = l2_norm(rhs);
    let rel = if denom > 0.0 { res / denom } else { res };
    (res, rel)
}

/// True if the deck carries an incident-plane-wave EX card (NEC2 types 1/2/3).
pub(super) fn deck_has_plane_wave(deck: &nec_model::deck::NecDeck) -> bool {
    deck.cards.iter().any(|c| match c {
        Card::Ex(ex) => ex.kind().is_plane_wave(),
        _ => false,
    })
}

/// Solve a receiving antenna illuminated by an incident plane wave (PH8-CHK-002,
/// PH9-CHK-002).
///
/// Straight, non-junctioned wires (one or more) solve on the per-wire two-DOF
/// Hallén path. **Junctioned degree-2 geometry** (bends, start-to-start /
/// end-to-end splits, inverted-V) solves on continuous *conductor paths*
/// (PH9-CHK-002 receive side): `build_conductor_paths` walks the wire graph and
/// `solve_hallen_planewave_paths` carries two homogeneous constants (`cos`/`sin`)
/// per path with the signed-arc-length convention, so the induced current stays
/// continuous across the junction. Out-of-scope topologies (degree-3+ T/Y,
/// closed loops) return `None` from `build_conductor_paths` and fall back to the
/// per-wire builder, which fails fast with an accurate diagnostic. `z_mat` is the
/// assembled Hallén matrix (including any load / TL stamps); the plane-wave solve
/// adds its own cos/sin homogeneous columns.
fn solve_plane_wave_hallen(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    wire_endpoints: &[(usize, usize)],
    freq_hz: f64,
) -> Result<Vec<Complex64>, String> {
    // Route junctioned degree-2 geometry through the conductor-path receive solver.
    // Reducible decks (single wires, collinear chains, parallel arrays) keep the
    // validated per-wire path; only a non-trivial (bent / reversed) path diverts.
    if let Some(paths) = build_conductor_paths(segs) {
        if paths.iter().any(|p| !p.is_trivial()) {
            let pw = build_planewave_hallen_paths(deck, segs, freq_hz, &paths)
                .map_err(|e| e.to_string())?;
            let mut path_of = vec![0usize; segs.len()];
            let mut free_ends: Vec<usize> = Vec::with_capacity(paths.len() * 2);
            for (pi, p) in paths.iter().enumerate() {
                for &m in &p.segs {
                    path_of[m] = pi;
                }
                free_ends.push(p.free_ends.0);
                free_ends.push(p.free_ends.1);
            }
            return solve_hallen_planewave_paths(
                z_mat,
                &pw.rhs,
                &pw.cos_vec,
                &pw.sin_vec,
                &path_of,
                &free_ends,
            )
            .map_err(|e| e.to_string());
        }
    }

    // Linear (type 1) and elliptic (types 2/3) plane waves are all handled by
    // build_planewave_hallen via the complex polarization vector.
    let pw = build_planewave_hallen(deck, segs, freq_hz).map_err(|e| e.to_string())?;
    solve_hallen_planewave(z_mat, &pw.rhs, &pw.cos_vec, &pw.sin_vec, wire_endpoints)
        .map_err(|e| e.to_string())
}

/// Incident-plane-wave receive-pattern sweep (PH9-CHK-001).
///
/// The plane-wave EX card's `tag` = NTHETA, `segment` = NPHI, `voltage_real`/
/// `voltage_imag` = θ0/φ0, and `theta_inc`/`phi_inc` = Δθ/Δφ define a grid of
/// incidence directions. For each direction the receiving antenna is solved and
/// the peak induced current recorded; the peak current tracks the transmit gain
/// pattern by reciprocity, so the normalized response (dB, 0 at the sweep peak) is
/// the receive pattern. Returns an empty vector for a single incidence
/// (NTHETA·NPHI ≤ 1).
fn plane_wave_receive_sweep(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    wire_endpoints: &[(usize, usize)],
    freq_hz: f64,
) -> Result<Vec<nec_report::ReceivePatternRow>, String> {
    let ex = deck
        .cards
        .iter()
        .find_map(|c| match c {
            Card::Ex(e) if e.kind().is_plane_wave() => Some(e.clone()),
            _ => None,
        })
        .ok_or("no plane-wave EX card for receive sweep")?;
    let n_theta = ex.tag.max(1);
    let n_phi = ex.segment.max(1);
    if n_theta * n_phi <= 1 {
        return Ok(Vec::new());
    }

    let mut raw: Vec<(f64, f64, f64)> = Vec::new(); // (θ, φ, peak|I|)
    for it in 0..n_theta {
        for ip in 0..n_phi {
            let theta = ex.voltage_real + it as f64 * ex.theta_inc;
            let phi = ex.voltage_imag + ip as f64 * ex.phi_inc;
            // Single-incidence deck at this arrival direction.
            let mut d = deck.clone();
            for c in &mut d.cards {
                if let Card::Ex(e) = c {
                    if e.kind().is_plane_wave() {
                        e.tag = 1;
                        e.segment = 1;
                        e.voltage_real = theta;
                        e.voltage_imag = phi;
                        e.theta_inc = 0.0;
                        e.phi_inc = 0.0;
                    }
                }
            }
            let currents = solve_plane_wave_hallen(&d, segs, z_mat, wire_endpoints, freq_hz)?;
            let peak = currents.iter().map(|c| c.norm()).fold(0.0f64, f64::max);
            raw.push((theta, phi, peak));
        }
    }
    let max_peak = raw.iter().map(|r| r.2).fold(0.0f64, f64::max);
    Ok(raw
        .into_iter()
        .map(|(theta, phi, peak)| nec_report::ReceivePatternRow {
            theta_deg: theta,
            phi_deg: phi,
            response_db: if peak > 0.0 && max_peak > 0.0 {
                20.0 * (peak / max_peak).log10()
            } else {
                -999.99
            },
        })
        .collect())
}

/// True if the deck carries a current-source EX card (NEC2 type 4).
pub(super) fn deck_has_current_source(deck: &nec_model::deck::NecDeck) -> bool {
    deck.cards.iter().any(|c| match c {
        Card::Ex(ex) => ex.kind() == nec_model::card::ExcitationKind::CurrentSource,
        _ => false,
    })
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
fn solve_current_source_hallen(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    wire_endpoints: &[(usize, usize)],
    freq_hz: f64,
) -> Result<(Vec<Complex64>, Complex64), String> {
    let cs = deck
        .cards
        .iter()
        .find_map(|c| match c {
            Card::Ex(ex) if ex.kind() == nec_model::card::ExcitationKind::CurrentSource => Some(ex),
            _ => None,
        })
        .ok_or_else(|| "EX: no current-source card found".to_string())?;

    let i0 = Complex64::new(cs.voltage_real, cs.voltage_imag);

    // Route junctioned degree-2 geometry through the conductor-path current-source
    // solver. Reducible decks (single wires, collinear chains, parallel arrays) keep
    // the validated per-wire path; only a non-trivial (bent / reversed) path diverts.
    if let Some(paths) = build_conductor_paths(segs) {
        if paths.iter().any(|p| !p.is_trivial()) {
            let (shape, cos_vec, src_seg) =
                build_current_source_shape_paths(deck, segs, freq_hz, cs.tag, cs.segment, &paths)
                    .map_err(|e| e.to_string())?;
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
            .map_err(|e| e.to_string())?;
            return Ok((sol.currents, sol.port_voltage));
        }
    } else if !detect_wire_junctions(segs, wire_endpoints, 1e-6).is_empty() {
        // Out-of-scope junction topology (degree-3+ T/Y, closed loop).
        return Err(
            "EX: current source is supported on straight or degree-2 junctioned wires; \
             degree-3+ (T/Y) junctions and closed loops are not yet supported"
                .to_string(),
        );
    }

    let (shape, cos_vec, src_seg) =
        build_current_source_shape(deck, segs, freq_hz, cs.tag, cs.segment)
            .map_err(|e| e.to_string())?;
    let sol = solve_hallen_current_source(z_mat, &shape, &cos_vec, src_seg, i0, wire_endpoints)
        .map_err(|e| e.to_string())?;
    Ok((sol.currents, sol.port_voltage))
}

pub(super) fn collect_pulse_current_source_constraints(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
) -> Result<Vec<PulseCurrentSourceConstraint>, String> {
    let mut out = Vec::new();

    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        // NEC2 numbering: the current source is type 4. (This staged path is
        // reached only once the current-source runtime semantics land; today
        // build_excitation rejects non-0 types before the solve.)
        if ex.kind() != nec_model::card::ExcitationKind::CurrentSource {
            continue;
        }

        let seg_index = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
            .ok_or_else(|| format!("EX: no segment with tag {}, index {}", ex.tag, ex.segment))?;

        out.push(PulseCurrentSourceConstraint {
            seg_index,
            source_current: Complex64::new(ex.voltage_real, ex.voltage_imag),
            original_row: Vec::new(),
        });
    }

    Ok(out)
}

pub(super) fn apply_pulse_current_source_constraints(
    z_mat: &mut ZMatrix,
    rhs: &mut [Complex64],
    constraints: &mut [PulseCurrentSourceConstraint],
) {
    for constraint in constraints {
        constraint.original_row = (0..z_mat.n)
            .map(|col| z_mat.get(constraint.seg_index, col))
            .collect();

        let mut replacement_row = vec![Complex64::new(0.0, 0.0); z_mat.n];
        replacement_row[constraint.seg_index] = Complex64::new(1.0, 0.0);
        z_mat.replace_row(constraint.seg_index, &replacement_row);
        rhs[constraint.seg_index] = constraint.source_current;
    }
}

pub(super) fn pulse_current_source_voltage(
    constraint: &PulseCurrentSourceConstraint,
    i_vec: &[Complex64],
    seg_length: f64,
    freq_hz: f64,
) -> Complex64 {
    let impressed_field: Complex64 = constraint
        .original_row
        .iter()
        .zip(i_vec.iter())
        .map(|(z, i)| *z * *i)
        .sum();
    -(impressed_field * seg_length * (C0 / freq_hz))
}

#[allow(clippy::too_many_arguments)]
/// PH9-CHK-005: emit a warning when a voltage/current source drives a segment
/// that sits at a wire junction. fnec's `V/I` on the single driven segment is not
/// the true feedpoint impedance there — the feed current splits across the joined
/// wires — so the reported `Z` can be unphysical (negative resistance). Accurate
/// junction-fed impedance is deferred to PH9-CHK-002; this makes the limitation
/// visible instead of silently returning a wrong number.
fn warn_if_feedpoint_at_junction(deck: &nec_model::deck::NecDeck, segs: &[Segment]) {
    // PH9-CHK-002 general junction: when the whole deck decomposes into supported
    // degree-2 conductor paths, every junction feed (bends, start-to-start splits,
    // inverted-V apex) is now solved correctly on a continuous basis, so there is
    // nothing to warn about. Only decks with an out-of-scope junction (degree-3
    // T/Y, or a closed loop) reach the per-junction check below.
    if build_conductor_paths(segs).is_some() {
        return;
    }
    // Use the merged (collinear-conductor) grouping so a junction that PH9-CHK-002
    // now solves correctly — a straight conductor split across GW cards — is not
    // flagged. Only genuine, unmerged junctions (bends, T/Y, start-to-start) remain.
    let merged = merge_collinear_wire_endpoints(segs);
    let junctions = detect_wire_junctions(segs, &merged, 1e-6);
    if junctions.is_empty() {
        return;
    }
    let mut junction_segs = std::collections::HashSet::new();
    for j in &junctions {
        junction_segs.insert(j.seg_a);
        junction_segs.insert(j.seg_b);
    }
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if ex.kind().is_plane_wave() {
            continue; // receiving antenna, no feedpoint
        }
        if let Some((idx, _)) = segs
            .iter()
            .enumerate()
            .find(|(_, s)| s.tag == ex.tag && s.tag_index == ex.segment)
        {
            if junction_segs.contains(&idx) {
                eprintln!(
                    "warning: feedpoint at tag {} segment {} is on a wire junction; \
                     the feed current splits across the joined wires, so the reported \
                     impedance (V/I on one segment) is not accurate and may be unphysical \
                     (junction-fed impedance is deferred — see PH9-CHK-002)",
                    ex.tag, ex.segment
                );
            }
        }
    }
}

/// PH9-CHK-006 guardrail: warn when an antenna sits **very low over finite ground**,
/// where the feedpoint impedance is only approximate.
///
/// fnec models finite ground (GN0/GN2) with a reflection-coefficient image — after
/// the PH9-CHK-006 sign fix this matches nec2c's reflection-coefficient method (GN0)
/// and, for heights ≥ ~0.2 λ, the exact Sommerfeld solution (GN2) to ~10 %. Below
/// ~0.1 λ the two diverge sharply: the Sommerfeld **surface wave** dominates and the
/// reflection-coefficient approximation (which fnec and nec2c GN0 share) becomes
/// unreliable — e.g. for a horizontal λ/2 dipole at 0.025 λ the reflection-coefficient
/// ΔR is −24 Ω while the Sommerfeld truth is **+9 Ω** (a sign error). fnec does not
/// yet model the surface wave, so it warns rather than silently reporting an
/// unreliable low-antenna impedance. Threshold: the lowest conductor point below
/// 0.1 λ over `SimpleFiniteGround`.
fn warn_if_low_finite_ground(segs: &[Segment], ground: &GroundModel, freq_hz: f64) {
    if !matches!(ground, GroundModel::SimpleFiniteGround { .. }) || freq_hz <= 0.0 {
        return;
    }
    let lambda = C0 / freq_hz;
    let min_z = segs
        .iter()
        .flat_map(|s| [s.start[2], s.end[2]])
        .fold(f64::INFINITY, f64::min);
    if !min_z.is_finite() || min_z < 0.0 {
        return; // buried / below ground is handled by the geometry fail-fast path
    }
    if min_z < 0.1 * lambda {
        eprintln!(
            "warning: antenna is {:.3} λ ({:.3} m) above finite ground (below ~0.1 λ); the \
             near-ground feedpoint impedance uses a reflection-coefficient approximation and \
             does not model the Sommerfeld surface wave, so it is only approximate here \
             (finite-ground impedance is accurate to ~10% for heights ≥ ~0.2 λ — see PH9-CHK-006)",
            min_z / lambda,
            min_z
        );
    }
}

/// PH9-CHK-002 / PH9-CHK-005 guardrail: warn when the geometry contains a junction
/// topology the conductor-path Hallén solve does not yet handle — a **closed loop**
/// or a **degree-3+ (T/Y) junction**. For these classes fnec falls back to the
/// per-wire basis, which enforces neither the loop's periodic closure nor the
/// Kirchhoff current split at a branching node, so the reported impedance, currents,
/// and pattern are unreliable for the *whole* geometry (not only a junction-fed
/// segment). A 1λ square loop, for instance, reports ≈20 − j1210 Ω versus the true
/// ≈111 − j146 Ω. This surfaces the limitation instead of silently returning a wrong
/// number — the loop case in particular is missed by [`warn_if_feedpoint_at_junction`]
/// because the feed need not sit on the junction.
fn warn_if_unsupported_topology(segs: &[Segment]) {
    match classify_unsupported_topology(segs) {
        Some(UnsupportedTopology::ClosedLoop) => eprintln!(
            "warning: geometry contains a closed loop (a conductor with no free end); \
             the Hallén solve does not yet model the periodic loop closure, so the reported \
             impedance, currents, and pattern for this geometry are unreliable \
             (loop support is deferred — see PH9-CHK-002)"
        ),
        Some(UnsupportedTopology::HighDegreeJunction) => eprintln!(
            "warning: geometry contains a junction where three or more wires meet (a T/Y \
             junction); the Hallén solve does not yet model the Kirchhoff current split there, \
             so the reported impedance, currents, and pattern for this geometry are unreliable \
             (branching-junction support is deferred — see PH9-CHK-002)"
        ),
        None => {}
    }
}

/// PH9-CHK-004: compute the near electric field for every `NE` card in the deck.
///
/// Each rectangular `NE` card defines an `NX×NY×NZ` grid of observation points;
/// the near field is the Hertzian-element sum over the solved segment currents
/// (`nec_solver::near_e_field`), validated to match the far field at large range.
/// Spherical `NE` cards (`I1 ≠ 0`) are skipped with a warning.
fn build_near_field_rows(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    i_vec: &[Complex64],
    freq_hz: f64,
) -> Vec<nec_report::NearFieldRow> {
    let mut points: Vec<nec_solver::NearFieldPoint> = Vec::new();
    for card in &deck.cards {
        let Card::Ne(ne) = card else { continue };
        if ne.coord_type != 0 {
            eprintln!(
                "warning: NE coordinate type {} (spherical) is not supported; \
                 near-field card skipped (only rectangular, I1=0, is supported)",
                ne.coord_type
            );
            continue;
        }
        for ix in 0..ne.nx.max(1) {
            for iy in 0..ne.ny.max(1) {
                for iz in 0..ne.nz.max(1) {
                    points.push(nec_solver::NearFieldPoint {
                        x: ne.x0 + ix as f64 * ne.dx,
                        y: ne.y0 + iy as f64 * ne.dy,
                        z: ne.z0 + iz as f64 * ne.dz,
                    });
                }
            }
        }
    }
    if points.is_empty() {
        return Vec::new();
    }
    nec_solver::near_e_field(segs, i_vec, freq_hz, &points)
        .into_iter()
        .map(|f| nec_report::NearFieldRow {
            x: f.x,
            y: f.y,
            z: f.z,
            ex: f.e[0],
            ey: f.e[1],
            ez: f.e[2],
        })
        .collect()
}

/// PH9-CHK-004: compute the near magnetic field for every `NH` card in the deck
/// (the magnetic companion to [`build_near_field_rows`]).
fn build_near_h_field_rows(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    i_vec: &[Complex64],
    freq_hz: f64,
) -> Vec<nec_report::NearHFieldRow> {
    let mut points: Vec<nec_solver::NearFieldPoint> = Vec::new();
    for card in &deck.cards {
        let Card::Nh(nh) = card else { continue };
        if nh.coord_type != 0 {
            eprintln!(
                "warning: NH coordinate type {} (spherical) is not supported; \
                 near-field card skipped (only rectangular, I1=0, is supported)",
                nh.coord_type
            );
            continue;
        }
        for ix in 0..nh.nx.max(1) {
            for iy in 0..nh.ny.max(1) {
                for iz in 0..nh.nz.max(1) {
                    points.push(nec_solver::NearFieldPoint {
                        x: nh.x0 + ix as f64 * nh.dx,
                        y: nh.y0 + iy as f64 * nh.dy,
                        z: nh.z0 + iz as f64 * nh.dz,
                    });
                }
            }
        }
    }
    if points.is_empty() {
        return Vec::new();
    }
    nec_solver::near_h_field(segs, i_vec, freq_hz, &points)
        .into_iter()
        .map(|f| nec_report::NearHFieldRow {
            x: f.x,
            y: f.y,
            z: f.z,
            hx: f.h[0],
            hy: f.h[1],
            hz: f.h[2],
        })
        .collect()
}

/// PH9-CHK-004: apply the `PT` (print-control) card to the segment current table.
///
/// Supported subset (NEC-2 `PT I1 I2 I3 I4`): `I1 ≤ −1` suppresses the current
/// output entirely (fnec prints no charge densities, so all negative modes map to
/// "no currents"); `I1 = 0` prints all currents (the default); `I1 ≥ 1` restricts
/// the output to tag `I2` and, when given, the segment range `I3..=I4`. The last
/// `PT` card in the deck wins. Fields that are absent or unparsable default to 0.
fn apply_pt_current_filter(
    current_table: Vec<CurrentRow>,
    deck: &nec_model::deck::NecDeck,
) -> Vec<CurrentRow> {
    let Some(pt) = deck.cards.iter().rev().find_map(|c| match c {
        Card::Pt(p) => Some(p),
        _ => None,
    }) else {
        return current_table;
    };
    let field = |i: usize| -> i64 {
        pt.raw_fields
            .get(i)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|v| v as i64)
            .unwrap_or(0)
    };
    let mode = field(0);
    let tag = field(1);
    let seg_first = field(2);
    let seg_last = field(3);

    if mode <= -1 {
        return Vec::new(); // suppress current output
    }
    if mode == 0 {
        return current_table; // print all
    }
    // mode >= 1: restrict to tag (and optional segment range).
    current_table
        .into_iter()
        .filter(|r| {
            let tag_ok = tag == 0 || r.tag as i64 == tag;
            let seg_ok = seg_first == 0
                || (r.seg as i64 >= seg_first && (seg_last == 0 || r.seg as i64 <= seg_last));
            tag_ok && seg_ok
        })
        .collect()
}

/// PH9-CHK-005: a passive antenna cannot have a negative input resistance. On the
/// Hallén path a negative `Re(Z)` is therefore a reliable post-solve signal that
/// the result is unphysical — in practice a junctioned-geometry limitation (a
/// bend, stepped-radius, or start-to-start split that the collinear fix does not
/// cover; see PH9-CHK-002). This catches cases the pre-solve junction-*fed* warning
/// misses, e.g. a bent antenna fed away from the bend. Scoped to `Hallen`: the
/// pulse current-source path has documented negative-`R` corpus values.
fn warn_if_negative_resistance(rows: &[FeedpointRow], solver_mode: SolverMode) {
    if !matches!(solver_mode, SolverMode::Hallen) {
        return;
    }
    for r in rows {
        if r.z_in.re < 0.0 {
            eprintln!(
                "warning: feedpoint tag {} segment {} has negative resistance \
                 (Re Z = {:.3} Ω), which is physically impossible for a passive antenna; \
                 the result is unreliable — commonly a junctioned-geometry limitation \
                 (see PH9-CHK-002)",
                r.tag, r.seg, r.z_in.re
            );
        }
    }
}

#[allow(clippy::too_many_arguments)] // cohesive feedpoint inputs; splitting would obscure
pub(super) fn build_feedpoint_rows(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    v_vec: &[Complex64],
    i_vec: &[Complex64],
    pulse_current_sources: &[PulseCurrentSourceConstraint],
    solver_mode: SolverMode,
    current_source_port: Option<Complex64>,
    freq_hz: f64,
    ground: &GroundModel,
    ground_solver: GroundSolver,
) -> Vec<FeedpointRow> {
    let mut rows = Vec::new();

    // PH9-CHK-006: precompute the Sommerfeld surface-wave ΔZ correction inputs once
    // (applied per feedpoint below) when the user selects the sommerfeld ground
    // solver over finite ground.
    let sommerfeld_ground = match (ground_solver, ground) {
        (GroundSolver::Sommerfeld, GroundModel::SimpleFiniteGround { eps_r, sigma }) => {
            Some((*eps_r, *sigma))
        }
        _ => None,
    };
    let midpoints: Vec<[f64; 3]> = segs.iter().map(|s| s.midpoint).collect();
    let directions: Vec<[f64; 3]> = segs.iter().map(|s| s.direction).collect();
    let lengths: Vec<f64> = segs.iter().map(|s| s.length).collect();

    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        // Incident plane waves have no feedpoint (receiving antenna); their
        // tag/segment fields carry NTHETA/NPHI, not a driven segment.
        if ex.kind().is_plane_wave() {
            continue;
        }
        let Some((idx, seg)) = segs
            .iter()
            .enumerate()
            .find(|(_, seg)| seg.tag == ex.tag && seg.tag_index == ex.segment)
        else {
            continue;
        };

        let current = i_vec[idx];
        let v_source = if ex.kind() == nec_model::card::ExcitationKind::CurrentSource {
            // Hallén current source: the solved port voltage V (feedpoint Z=V/i0).
            // The dormant pulse path is retained as a fallback but is unreachable
            // now that current sources require --solver hallen.
            current_source_port.unwrap_or_else(|| {
                if matches!(solver_mode, SolverMode::Pulse) {
                    pulse_current_sources
                        .iter()
                        .find(|constraint| constraint.seg_index == idx)
                        .map(|constraint| {
                            pulse_current_source_voltage(constraint, i_vec, seg.length, freq_hz)
                        })
                        .unwrap_or(v_vec[idx] * seg.length)
                } else {
                    v_vec[idx] * seg.length
                }
            })
        } else {
            v_vec[idx] * seg.length
        };
        let mut z_in = if current.norm() > 1e-60 {
            v_source / current
        } else {
            v_source
        };

        // PH9-CHK-006: add the Sommerfeld surface-wave correction to the near-ground
        // feedpoint impedance for a straight horizontal wire (declined otherwise).
        if let Some((eps_r, sigma)) = sommerfeld_ground {
            if let Some(dz) = nec_solver::sommerfeld::horizontal_ground_z_correction(
                &midpoints,
                &directions,
                &lengths,
                i_vec,
                idx,
                freq_hz,
                eps_r,
                sigma,
            ) {
                z_in += dz;
            }
        }

        rows.push(FeedpointRow {
            tag: seg.tag as usize,
            seg: seg.tag_index as usize,
            v_source,
            current,
            z_in,
        });
    }

    rows
}

pub(super) fn build_source_rows(deck: &nec_model::deck::NecDeck) -> Vec<SourceRow> {
    deck.cards
        .iter()
        .filter_map(|card| {
            let Card::Ex(ex) = card else { return None };
            Some(SourceRow {
                excitation_type: ex.excitation_type,
                tag: ex.tag,
                seg: ex.segment,
                i4: ex.i4,
                voltage_real: ex.voltage_real,
                voltage_imag: ex.voltage_imag,
            })
        })
        .collect()
}

pub(super) fn build_load_rows(deck: &nec_model::deck::NecDeck) -> Vec<LoadRow> {
    deck.cards
        .iter()
        .filter_map(|card| {
            let Card::Ld(ld) = card else { return None };
            Some(LoadRow {
                load_type: ld.load_type,
                tag: ld.tag,
                seg_first: ld.seg_first,
                seg_last: ld.seg_last,
                f1: ld.f1,
                f2: ld.f2,
                f3: ld.f3,
            })
        })
        .collect()
}

pub(super) fn frequencies_from_fr(deck: &nec_model::deck::NecDeck) -> Vec<f64> {
    let Some(fr) = deck
        .cards
        .iter()
        .find_map(|c| if let Card::Fr(fr) = c { Some(fr) } else { None })
    else {
        return Vec::new();
    };

    let steps = fr.steps.max(1) as usize;
    let mut out = Vec::with_capacity(steps);
    match fr.step_type {
        0 => {
            for idx in 0..steps {
                out.push((fr.frequency_mhz + (idx as f64) * fr.step_mhz) * 1e6);
            }
        }
        1 => {
            for idx in 0..steps {
                out.push(fr.frequency_mhz * fr.step_mhz.powi(idx as i32) * 1e6);
            }
        }
        _ => {
            // Unsupported FR stepping mode: use the first frequency only.
            out.push(fr.frequency_mhz * 1e6);
        }
    }
    out
}

pub(super) fn build_hybrid_lane_plan(freq_count: usize) -> HybridLanePlan {
    let mut cpu_indices = Vec::new();
    let mut gpu_candidate_indices = Vec::new();

    for idx in 0..freq_count {
        // Interleaving preserves broad frequency spread in both lanes.
        if idx % 2 == 0 {
            cpu_indices.push(idx);
        } else {
            gpu_candidate_indices.push(idx);
        }
    }

    HybridLanePlan {
        cpu_indices,
        gpu_candidate_indices,
    }
}

/// Attempt the GPU-resident Hallén fill+solve (PH7-CHK-003) for the supported
/// deck class. Returns `None` (caller uses the CPU `solve_hallen`) unless:
/// `--exec gpu`, free-space/deferred ground, no LD/TL host matrix stamps, and at
/// least `MIN_GPU_RESIDENT_SEGS` segments. Also returns `None` when no wgpu
/// adapter is available.
#[allow(clippy::too_many_arguments)]
fn maybe_gpu_resident_hallen(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    hallen_rhs: &nec_solver::HallenRhs,
    wire_endpoints: &[(usize, usize)],
    junctions: &[(usize, usize, f64)],
    ground: &GroundModel,
    execution_mode: ExecutionMode,
    freq_hz: f64,
) -> Option<nec_solver::HallenSolution> {
    use nec_model::card::Card;

    const MIN_GPU_RESIDENT_SEGS: usize = 16;
    if execution_mode != ExecutionMode::Gpu || segs.len() < MIN_GPU_RESIDENT_SEGS {
        return None;
    }
    if !matches!(
        ground,
        GroundModel::FreeSpace | GroundModel::Deferred { .. }
    ) {
        return None;
    }
    // LD/TL cards stamp the host matrix in ways the GPU free-space fill does not
    // reproduce — keep those on the CPU path.
    if deck
        .cards
        .iter()
        .any(|c| matches!(c, Card::Ld(_) | Card::Tl(_)))
    {
        return None;
    }

    let z_inputs: Vec<nec_accel::ZSegmentInput> = segs
        .iter()
        .map(|s| nec_accel::ZSegmentInput {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
            radius: s.radius,
        })
        .collect();

    let x = pollster::block_on(nec_accel::solve_hallen_gpu_resident(
        &z_inputs,
        &hallen_rhs.rhs,
        &hallen_rhs.cos_vec,
        wire_endpoints,
        junctions,
        freq_hz,
    ))?;

    let n = segs.len();
    if x.len() < n {
        return None;
    }
    let currents = x[..n].to_vec();
    let c_hom_per_wire = x[n..].to_vec();
    let c_hom = c_hom_per_wire
        .first()
        .copied()
        .unwrap_or(Complex64::new(0.0, 0.0));
    Some(nec_solver::HallenSolution {
        currents,
        c_hom_per_wire,
        c_hom,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn solve_frequency_point(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    wire_endpoints: &[(usize, usize)],
    per_wire_basis_feasible: bool,
    v_vec: &[Complex64],
    ground: &GroundModel,
    pattern_points: &[FarFieldPoint],
    solver_mode: SolverMode,
    pulse_rhs_mode: PulseRhsMode,
    execution_mode: ExecutionMode,
    sin_fallback_rel_max: f64,
    freq_hz: f64,
    ground_solver: GroundSolver,
) -> Result<FrequencySolveResult, String> {
    // Incident plane waves and current sources are solved on the Hallén path
    // only (crate::planewave / solve_hallen_current_source).
    if deck_has_plane_wave(deck) && !matches!(solver_mode, SolverMode::Hallen) {
        return Err("EX: incident plane-wave excitation requires --solver hallen".to_string());
    }
    if deck_has_current_source(deck) && !matches!(solver_mode, SolverMode::Hallen) {
        return Err("EX: current-source excitation requires --solver hallen".to_string());
    }

    let mut v_vec_pulse = match pulse_rhs_mode {
        PulseRhsMode::Raw => v_vec.to_vec(),
        PulseRhsMode::Nec2 => scale_excitation_for_pulse_rhs(v_vec, freq_hz),
    };

    let mut z_mat = match solver_mode {
        SolverMode::Hallen => {
            // For free-space (or deferred) ground with --exec gpu, attempt GPU Z-matrix fill.
            // Ground-image-augmented models fall back to CPU; GPU fills free-space part only.
            // A minimum segment count guards against wgpu device-init overhead dominating for
            // small problems where CPU assembly is < 1 ms.
            const MIN_GPU_ZMATRIX_SEGS: usize = 128;
            let try_gpu = execution_mode == ExecutionMode::Gpu
                && segs.len() >= MIN_GPU_ZMATRIX_SEGS
                && matches!(
                    ground,
                    GroundModel::FreeSpace | GroundModel::Deferred { .. }
                );
            if try_gpu {
                let z_inputs: Vec<nec_accel::ZSegmentInput> = segs
                    .iter()
                    .map(|s| nec_accel::ZSegmentInput {
                        midpoint: s.midpoint,
                        direction: s.direction,
                        length: s.length,
                        radius: s.radius,
                    })
                    .collect();
                match pollster::block_on(nec_accel::fill_zmatrix_wgpu(&z_inputs, freq_hz)) {
                    Some(elems) => {
                        let n = segs.len();
                        let flat: Vec<Complex64> = elems
                            .iter()
                            .map(|e| Complex64::new(e.re as f64, e.im as f64))
                            .collect();
                        ZMatrix::from_flat(n, flat)
                    }
                    None => {
                        eprintln!("warning: --exec gpu: no wgpu adapter available, falling back to CPU Z-matrix fill");
                        assemble_z_matrix_with_ground(segs, freq_hz, ground)
                    }
                }
            } else {
                assemble_z_matrix_with_ground(segs, freq_hz, ground)
            }
        }
        SolverMode::Pulse | SolverMode::Continuity => assemble_pocklington_matrix(segs, freq_hz),
        SolverMode::Sinusoidal => {
            // Sinusoidal mode uses the Hallén thin-wire Z-matrix (same as Hallen),
            // not the Pocklington EFIE matrix. The accurate basis only matters for
            // the solve step, not the matrix assembly.
            assemble_z_matrix_with_ground(segs, freq_hz, ground)
        }
    };

    let (load_vec, load_warnings) = build_loads(deck, segs, freq_hz);
    for warning in &load_warnings {
        eprintln!("warning: {warning}");
    }
    z_mat.add_to_diagonal(&load_vec);
    let (tl_stamps, tl_warnings) = build_tl_stamps(deck, segs, freq_hz);
    for warning in &tl_warnings {
        eprintln!("warning: {warning}");
    }
    for (row, col, delta) in &tl_stamps {
        z_mat.add_to_entry(*row, *col, *delta);
    }
    // NT two-port networks: admittance-parameter stamp (PH8-CHK-004). Valid cards
    // stamp the Z matrix; malformed/unsupported cards warn and are skipped.
    // Warnings are deduplicated so repeated identical cards warn once.
    let (nt_stamps, nt_warnings) = build_nt_stamps(deck, segs);
    let mut seen_nt_warnings = std::collections::HashSet::new();
    for warning in &nt_warnings {
        if seen_nt_warnings.insert(warning.message.clone()) {
            eprintln!("warning: {warning}");
        }
    }
    for (row, col, delta) in &nt_stamps {
        z_mat.add_to_entry(*row, *col, *delta);
    }
    let mut pulse_current_sources = if matches!(solver_mode, SolverMode::Pulse) {
        collect_pulse_current_source_constraints(deck, segs)?
    } else {
        Vec::new()
    };
    if matches!(solver_mode, SolverMode::Pulse) {
        apply_pulse_current_source_constraints(
            &mut z_mat,
            &mut v_vec_pulse,
            &mut pulse_current_sources,
        );
    }
    let diag_spread = matrix_diagonal_spread(&z_mat);
    let mut sin_rel_res: f64 = 0.0;
    // Set by the current-source path: the solved port voltage V (feedpoint Z=V/i0).
    let mut current_source_port: Option<Complex64> = None;

    let (i_vec, diag_abs, diag_rel, diag_label) = match solver_mode {
        SolverMode::Hallen if deck_has_plane_wave(deck) => {
            let currents = solve_plane_wave_hallen(deck, segs, &z_mat, wire_endpoints, freq_hz)?;
            (currents, 0.0, 0.0, "hallen-planewave")
        }
        SolverMode::Hallen if deck_has_current_source(deck) => {
            let (currents, port_v) =
                solve_current_source_hallen(deck, segs, &z_mat, wire_endpoints, freq_hz)?;
            current_source_port = Some(port_v);
            (currents, 0.0, 0.0, "hallen-current-source")
        }
        SolverMode::Hallen
            if build_conductor_paths(segs).is_some_and(|ps| ps.iter().any(|p| !p.is_trivial())) =>
        {
            // PH9-CHK-002 general junction: the deck contains a degree-2 conductor
            // chain the collinear merge cannot handle (a bend, start-to-start /
            // end-to-end split, or inverted-V apex feed). Solve on continuous
            // conductor paths — one shared homogeneous constant per path, signed
            // arc-length `cos(k·s)`, and `I = 0` only at the true free ends — so the
            // Hallén basis stays continuous across the junction. Reducible decks
            // (single wires, collinear chains) and out-of-scope topologies (degree-3
            // T/Y, closed loops) fall through to the pre-existing path below.
            let paths = build_conductor_paths(segs).expect("checked in guard");
            let hallen_rhs =
                build_hallen_rhs_paths(deck, segs, freq_hz, &paths).map_err(|e| e.to_string())?;
            let mut path_of = vec![0usize; segs.len()];
            let mut free_ends: Vec<usize> = Vec::with_capacity(paths.len() * 2);
            for (pi, p) in paths.iter().enumerate() {
                for &m in &p.segs {
                    path_of[m] = pi;
                }
                free_ends.push(p.free_ends.0);
                free_ends.push(p.free_ends.1);
            }
            let sol = solve_hallen_paths(
                &z_mat,
                &hallen_rhs.rhs,
                &hallen_rhs.cos_vec,
                &path_of,
                &free_ends,
            )
            .map_err(|e| e.to_string())?;
            let (a, r) = residual_hallen_paths(
                &z_mat,
                &sol.currents,
                &sol.c_hom_per_wire,
                &hallen_rhs.cos_vec,
                &hallen_rhs.rhs,
                &path_of,
            );
            (sol.currents, a, r, "hallen")
        }
        SolverMode::Hallen => {
            let hallen_rhs = build_hallen_rhs(deck, segs, freq_hz).map_err(|e| e.to_string())?;
            // PH9-CHK-002: collinear-connected wires are merged into one logical
            // conductor for the Hallén homogeneous solution (matching build_hallen_rhs).
            // Junctions internal to a merged conductor are handled by the merged
            // basis, so only cross-conductor junctions keep an explicit continuity
            // constraint.
            let merged_endpoints = merge_collinear_wire_endpoints(segs);
            let mut comp_of = vec![0usize; segs.len()];
            for (ci, &(first, last)) in merged_endpoints.iter().enumerate() {
                for slot in comp_of.iter_mut().take(last + 1).skip(first) {
                    *slot = ci;
                }
            }
            // Detect wire junctions for continuity constraints.
            // Tolerance: 1e-6 m — well below any practical segment length.
            let wire_junctions = detect_wire_junctions(segs, &merged_endpoints, 1e-6);
            let junction_tuples: Vec<(usize, usize, f64)> = wire_junctions
                .iter()
                .filter(|j| comp_of[j.seg_a] != comp_of[j.seg_b])
                .map(|j| (j.seg_a, j.seg_b, j.sign))
                .collect();

            // GPU-resident fill+solve (PH7-CHK-003): for the supported class the
            // entire fill→solve runs on the device (no full-matrix copy-back).
            // Only valid when the host applies no matrix modifications the GPU
            // path does not reproduce: free-space/deferred ground, no load or TL
            // stamps, and enough segments to amortize device setup.
            let sol = maybe_gpu_resident_hallen(
                deck,
                segs,
                &hallen_rhs,
                &merged_endpoints,
                &junction_tuples,
                ground,
                execution_mode,
                freq_hz,
            )
            .map(Ok)
            .unwrap_or_else(|| {
                solve_hallen(
                    &z_mat,
                    &hallen_rhs.rhs,
                    &hallen_rhs.cos_vec,
                    &merged_endpoints,
                    &junction_tuples,
                )
            })
            .map_err(|e| e.to_string())?;
            let (a, r) = residual_hallen(
                &z_mat,
                &sol.currents,
                &sol.c_hom_per_wire,
                &hallen_rhs.cos_vec,
                &hallen_rhs.rhs,
                &merged_endpoints,
            );
            (sol.currents, a, r, "hallen")
        }
        SolverMode::Pulse => {
            let i = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
            let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
            (i, a, r, "pulse")
        }
        SolverMode::Continuity => {
            if !per_wire_basis_feasible {
                eprintln!(
                    "warning: continuity solver requires >=2 segments per wire; falling back to pulse"
                );
                let i = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
                let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                (i, a, r, "continuity->pulse")
            } else {
                let i = solve_with_continuity_basis_per_wire(&z_mat, &v_vec_pulse, wire_endpoints)
                    .map_err(|e| e.to_string())?;
                let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                if r <= CONTINUITY_REL_RESIDUAL_MAX {
                    (i, a, r, "continuity")
                } else {
                    eprintln!(
                        "warning: continuity residual {:.3e} > {:.3e}; falling back to pulse",
                        r, CONTINUITY_REL_RESIDUAL_MAX
                    );
                    let i2 = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
                    let (a2, r2) = residual_zi_minus_v(&z_mat, &i2, &v_vec_pulse);
                    (i2, a2, r2, "continuity->pulse(residual)")
                }
            }
        }
        SolverMode::Sinusoidal => {
            if !per_wire_basis_feasible {
                eprintln!(
                    "warning: sinusoidal solver requires >=2 segments per wire; falling back to pulse"
                );
                let i = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
                let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                (i, a, r, "sinusoidal->pulse")
            } else {
                // NEC2-style sinusoidal Galerkin basis on the Hallén integral equation.
                // Uses the Hallén thin-wire Z-matrix (assembled above) with piecewise-
                // sinusoidal expansion functions and pulse testing (projection).
                let hallen_rhs =
                    build_hallen_rhs(deck, segs, freq_hz).map_err(|e| e.to_string())?;
                let wire_junctions = detect_wire_junctions(segs, wire_endpoints, 1e-6);
                let junction_tuples: Vec<(usize, usize, f64)> = wire_junctions
                    .iter()
                    .map(|j| (j.seg_a, j.seg_b, j.sign))
                    .collect();
                let sol = solve_hallen_sinusoidal_basis(
                    &z_mat,
                    &hallen_rhs.rhs,
                    &hallen_rhs.cos_vec,
                    wire_endpoints,
                    &junction_tuples,
                )
                .map_err(|e| e.to_string())?;
                let (a, r) = residual_hallen(
                    &z_mat,
                    &sol.currents,
                    &sol.c_hom_per_wire,
                    &hallen_rhs.cos_vec,
                    &hallen_rhs.rhs,
                    wire_endpoints,
                );
                sin_rel_res = r;
                if r <= sin_fallback_rel_max {
                    (sol.currents, a, r, "sinusoidal")
                } else {
                    eprintln!(
                        "warning: sinusoidal residual {:.3e} > {:.3e}; falling back to hallen",
                        r, sin_fallback_rel_max
                    );
                    let hallen_sol = solve_hallen(
                        &z_mat,
                        &hallen_rhs.rhs,
                        &hallen_rhs.cos_vec,
                        wire_endpoints,
                        &junction_tuples,
                    )
                    .map_err(|e| e.to_string())?;
                    let (a2, r2) = residual_hallen(
                        &z_mat,
                        &hallen_sol.currents,
                        &hallen_sol.c_hom_per_wire,
                        &hallen_rhs.cos_vec,
                        &hallen_rhs.rhs,
                        wire_endpoints,
                    );
                    (hallen_sol.currents, a2, r2, "sinusoidal->hallen(residual)")
                }
            }
        }
    };

    let rows = build_feedpoint_rows(
        deck,
        segs,
        v_vec,
        &i_vec,
        &pulse_current_sources,
        solver_mode,
        current_source_port,
        freq_hz,
        ground,
        ground_solver,
    );

    // PH9-CHK-005: guard the junction-fed feedpoint limitation. When the driven
    // segment sits at a wire junction the feed current splits across the joined
    // wires, so the single-segment V/I is not the true feedpoint impedance and can
    // be unphysical (e.g. negative resistance). Warn rather than report it as
    // trustworthy; accurate junction-fed impedance is PH9-CHK-002.
    warn_if_unsupported_topology(segs);
    warn_if_feedpoint_at_junction(deck, segs);
    warn_if_low_finite_ground(segs, ground, freq_hz);
    warn_if_negative_resistance(&rows, solver_mode);

    let current_table: Vec<CurrentRow> = segs
        .iter()
        .enumerate()
        .map(|(idx, seg)| CurrentRow {
            tag: seg.tag as usize,
            seg: seg.tag_index as usize,
            current: i_vec[idx],
        })
        .collect();
    // PH9-CHK-004: apply PT (print-control) filtering to the segment current output.
    let current_table = apply_pt_current_filter(current_table, deck);

    let source_table = build_source_rows(deck);
    let load_table = build_load_rows(deck);

    let mut pattern_table: Vec<PatternRow> = if pattern_points.is_empty() {
        Vec::new()
    } else if execution_mode == ExecutionMode::Gpu {
        // Attempt wgpu RP kernel dispatch (gate G4).
        // Compute total radiated power on CPU for gain normalisation — the GPU
        // computes radiation intensity components, not normalised gain.
        let pec_ground = matches!(ground, GroundModel::PerfectConductor);
        let total_radiated = integrate_radiated_power(segs, &i_vec, freq_hz, pec_ground);
        let k = 2.0 * std::f64::consts::PI * freq_hz / 299_792_458.0;

        let gpu_segments: Vec<_> = segs
            .iter()
            .map(|seg| nec_accel::gpu_kernels::GpuSegment {
                midpoint: seg.midpoint,
                direction: seg.direction,
                length: seg.length,
            })
            .collect();

        let points_tuples: Vec<(f64, f64)> = pattern_points
            .iter()
            .map(|p| (p.theta_deg, p.phi_deg))
            .collect();

        let gpu_results = pollster::block_on(nec_accel::wgpu_device::run_rp_farfield_batch_wgpu(
            &gpu_segments,
            &i_vec,
            k,
            total_radiated,
            &points_tuples,
        ));

        match gpu_results {
            Some(rows) => rows
                .iter()
                .map(|r| PatternRow {
                    theta_deg: r.theta_deg,
                    phi_deg: r.phi_deg,
                    gain_total_dbi: r.gain_total_dbi,
                    gain_theta_dbi: r.gain_theta_dbi,
                    gain_phi_dbi: r.gain_phi_dbi,
                    axial_ratio: r.axial_ratio,
                })
                .collect(),
            None => {
                // No adapter available — fall back to CPU path silently.
                eprintln!("warning: --exec gpu: no wgpu adapter available, falling back to CPU RP");
                let results =
                    compute_radiation_pattern(segs, &i_vec, freq_hz, pattern_points, ground);
                results
                    .iter()
                    .map(|r| PatternRow {
                        theta_deg: r.theta_deg,
                        phi_deg: r.phi_deg,
                        gain_total_dbi: r.gain_total_dbi,
                        gain_theta_dbi: r.gain_theta_dbi,
                        gain_phi_dbi: r.gain_phi_dbi,
                        axial_ratio: r.axial_ratio,
                    })
                    .collect()
            }
        }
    } else {
        // Standard CPU path
        let results = compute_radiation_pattern(segs, &i_vec, freq_hz, pattern_points, ground);
        results
            .iter()
            .map(|r| PatternRow {
                theta_deg: r.theta_deg,
                phi_deg: r.phi_deg,
                gain_total_dbi: r.gain_total_dbi,
                gain_theta_dbi: r.gain_theta_dbi,
                gain_phi_dbi: r.gain_phi_dbi,
                axial_ratio: r.axial_ratio,
            })
            .collect()
    };

    // PH9-CHK-003: over a lossy finite ground the pattern gain is the directivity
    // reduced by the radiation efficiency η = P_radiated / P_input (ground-absorbed
    // power). compute_radiation_pattern returns directivity; convert to gain here so
    // the reported dBi matches nec2c's gain. (Free-space / PEC are lossless → η ≈ 1,
    // and are left as directivity so their corpus gates are unchanged.)
    if matches!(ground, GroundModel::SimpleFiniteGround { .. }) && !pattern_table.is_empty() {
        let p_in: f64 = rows
            .iter()
            .map(|r| 0.5 * (r.v_source * r.current.conj()).re)
            .sum();
        if p_in > 0.0 {
            let eta = radiation_efficiency(segs, &i_vec, freq_hz, ground, p_in);
            let delta_db = 10.0 * eta.log10();
            for row in &mut pattern_table {
                for g in [
                    &mut row.gain_total_dbi,
                    &mut row.gain_theta_dbi,
                    &mut row.gain_phi_dbi,
                ] {
                    if *g > -900.0 {
                        *g += delta_db;
                    }
                }
            }
        }
    }

    // PH9-CHK-001: incident-plane-wave receive-pattern sweep (NTHETA·NPHI > 1).
    let receive_pattern_table = if deck_has_plane_wave(deck) {
        plane_wave_receive_sweep(deck, segs, &z_mat, wire_endpoints, freq_hz)?
    } else {
        Vec::new()
    };

    // PH9-CHK-004: near electric field on the NE-card grid(s), magnetic on NH.
    let near_field_table = build_near_field_rows(deck, segs, &i_vec, freq_hz);
    let near_h_field_table = build_near_h_field_rows(deck, segs, &i_vec, freq_hz);

    let report = render_text_report(&ReportInput {
        solver_mode: diag_label,
        pulse_rhs: pulse_rhs_mode.as_contract_str(),
        frequency_hz: freq_hz,
        rows: &rows,
        source_table: &source_table,
        load_table: &load_table,
        current_table: &current_table,
        pattern_table: &pattern_table,
        receive_pattern_table: &receive_pattern_table,
        near_field_table: &near_field_table,
        near_h_field_table: &near_h_field_table,
        normalize_pattern: deck
            .cards
            .iter()
            .any(|c| matches!(c, Card::Rp(rp) if rp.normalize)),
    });
    let sweep_summary = rows.first().map(|row| SweepPointSummary {
        freq_mhz: freq_hz / 1e6,
        tag: row.tag,
        seg: row.seg,
        z_re: row.z_in.re,
        z_im: row.z_in.im,
    });
    let diag_line = format!(
        "diag: mode={diag_label} pulse_rhs={:?} exec={} freq_mhz={:.6} abs_res={:.6e} rel_res={:.6e} diag_spread={:.6e} sin_rel_res={:.6e} sin_fallback_rel_max={:.6e}",
        pulse_rhs_mode,
        execution_mode.as_diag_str(),
        freq_hz / 1e6,
        diag_abs,
        diag_rel,
        diag_spread,
        sin_rel_res,
        sin_fallback_rel_max
    );

    let bench = BenchRecord {
        mode: diag_label.to_string(),
        pulse_rhs: pulse_rhs_mode.as_contract_str().to_string(),
        exec: execution_mode.as_diag_str().to_string(),
        freq_mhz: freq_hz / 1e6,
        abs_res: diag_abs,
        rel_res: diag_rel,
        diag_spread,
        sin_rel_res,
    };

    Ok(FrequencySolveResult {
        report,
        diag_line,
        bench,
        sweep_summary,
    })
}

/// Execute a frequency sweep, handling both sequential and hybrid-parallel dispatch.
///
/// Returns results indexed by frequency position, unsorted. The caller is responsible
/// for sorting by index and emitting per-result warnings.
#[allow(clippy::type_complexity)]
pub(super) fn execute_frequency_sweep<F>(
    freqs_hz: &[f64],
    execution_mode: ExecutionMode,
    solve_one: F,
) -> (
    Vec<(usize, Result<FrequencySolveResult, String>, u128)>,
    usize,
)
where
    F: Fn(f64) -> Result<FrequencySolveResult, String> + Sync,
{
    use nec_accel::{dispatch_frequency_point, AccelRequestKind, DispatchDecision};
    use rayon::prelude::*;

    let timed_solve_one = |freq_hz: f64| {
        let t0 = std::time::Instant::now();
        let result = solve_one(freq_hz);
        (result, t0.elapsed().as_millis())
    };

    if matches!(execution_mode, ExecutionMode::Hybrid) && freqs_hz.len() > 1 {
        let lane_plan = build_hybrid_lane_plan(freqs_hz.len());

        let mut solved = Vec::with_capacity(freqs_hz.len());

        let cpu_results: Vec<(usize, Result<FrequencySolveResult, String>, u128)> = lane_plan
            .cpu_indices
            .par_iter()
            .copied()
            .map(|idx| {
                let (result, elapsed_ms) = timed_solve_one(freqs_hz[idx]);
                (idx, result, elapsed_ms)
            })
            .collect();
        solved.extend(cpu_results);

        // The GPU-candidate lane consults the scheduling seam. Until per-frequency
        // GPU dispatch is wired (PH7-CHK-004) every point falls back to CPU; we
        // count those so the CLI can warn honestly that no GPU work occurred.
        let mut gpu_fallback_count = 0usize;
        for idx in lane_plan.gpu_candidate_indices.iter().copied() {
            match dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, freqs_hz[idx]) {
                DispatchDecision::FallbackToCpu { .. } => gpu_fallback_count += 1,
                DispatchDecision::RunOnGpu => {}
            }
            let (result, elapsed_ms) = timed_solve_one(freqs_hz[idx]);
            solved.push((idx, result, elapsed_ms));
        }

        (solved, gpu_fallback_count)
    } else {
        let solved = freqs_hz
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, freq_hz)| {
                let (result, elapsed_ms) = timed_solve_one(freq_hz);
                (idx, result, elapsed_ms)
            })
            .collect();
        (solved, 0)
    }
}
