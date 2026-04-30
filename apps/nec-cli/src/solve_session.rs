use super::exec_profile::ExecutionMode;
use nec_model::card::Card;

pub(super) const C0: f64 = 299_792_458.0;
pub(super) const CONTINUITY_REL_RESIDUAL_MAX: f64 = 1e-3;
pub(super) const SINUSOIDAL_REL_RESIDUAL_MAX: f64 = 1e-2;

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
    assemble_pocklington_matrix, assemble_z_matrix_with_ground, build_hallen_rhs, build_loads,
    build_tl_stamps, compute_radiation_pattern, scale_excitation_for_pulse_rhs, solve,
    solve_hallen, solve_with_continuity_basis_per_wire, solve_with_sinusoidal_basis_per_wire,
    FarFieldPoint, GroundModel, Segment, ZMatrix,
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
    v.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt()
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

pub(super) fn collect_pulse_current_source_constraints(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
) -> Result<Vec<PulseCurrentSourceConstraint>, String> {
    let mut out = Vec::new();

    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if ex.excitation_type != 1 && ex.excitation_type != 4 && ex.excitation_type != 5 {
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

pub(super) fn build_feedpoint_rows(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    v_vec: &[Complex64],
    i_vec: &[Complex64],
    pulse_current_sources: &[PulseCurrentSourceConstraint],
    solver_mode: SolverMode,
    freq_hz: f64,
) -> Vec<FeedpointRow> {
    let mut rows = Vec::new();

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
        let v_source =
            if (ex.excitation_type == 1 || ex.excitation_type == 4 || ex.excitation_type == 5)
                && matches!(solver_mode, SolverMode::Pulse)
            {
                pulse_current_sources
                    .iter()
                    .find(|constraint| constraint.seg_index == idx)
                    .map(|constraint| {
                        pulse_current_source_voltage(constraint, i_vec, seg.length, freq_hz)
                    })
                    .unwrap_or(v_vec[idx] * seg.length)
            } else {
                v_vec[idx] * seg.length
            };
        let z_in = if current.norm() > 1e-60 {
            v_source / current
        } else {
            v_source
        };

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

#[allow(clippy::too_many_arguments)]
pub(super) fn solve_frequency_point(
    deck: &nec_model::deck::NecDeck,
    segs: &[Segment],
    wire_endpoints: &[(usize, usize)],
    per_wire_basis_feasible: bool,
    sinusoidal_topology_supported: bool,
    v_vec: &[Complex64],
    ground: &GroundModel,
    pattern_points: &[FarFieldPoint],
    solver_mode: SolverMode,
    pulse_rhs_mode: PulseRhsMode,
    execution_mode: ExecutionMode,
    enable_gpu_fr: bool,
    freq_hz: f64,
) -> Result<FrequencySolveResult, String> {
    let mut v_vec_pulse = match pulse_rhs_mode {
        PulseRhsMode::Raw => v_vec.to_vec(),
        PulseRhsMode::Nec2 => scale_excitation_for_pulse_rhs(v_vec, freq_hz),
    };

    let mut z_mat = match solver_mode {
        SolverMode::Hallen => assemble_z_matrix_with_ground(segs, freq_hz, ground),
        SolverMode::Pulse | SolverMode::Continuity | SolverMode::Sinusoidal => {
            assemble_pocklington_matrix(segs, freq_hz)
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

    let (i_vec, diag_abs, diag_rel, diag_label) = match solver_mode {
        SolverMode::Hallen => {
            let hallen_rhs = build_hallen_rhs(deck, segs, freq_hz).map_err(|e| e.to_string())?;
            let sol = solve_hallen(&z_mat, &hallen_rhs.rhs, &hallen_rhs.cos_vec, wire_endpoints)
                .map_err(|e| e.to_string())?;
            let (a, r) = residual_hallen(
                &z_mat,
                &sol.currents,
                &sol.c_hom_per_wire,
                &hallen_rhs.cos_vec,
                &hallen_rhs.rhs,
                wire_endpoints,
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
            } else if !sinusoidal_topology_supported {
                eprintln!(
                    "warning: sinusoidal A4 currently supports only collinear wire-chain topologies; falling back to pulse"
                );
                let i = solve(&z_mat, &v_vec_pulse).map_err(|e| e.to_string())?;
                let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                (i, a, r, "sinusoidal->pulse(topology)")
            } else {
                let i = solve_with_sinusoidal_basis_per_wire(&z_mat, &v_vec_pulse, wire_endpoints)
                    .map_err(|e| e.to_string())?;
                let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                sin_rel_res = r;
                if r <= SINUSOIDAL_REL_RESIDUAL_MAX {
                    (i, a, r, "sinusoidal")
                } else {
                    eprintln!(
                        "warning: sinusoidal residual {:.3e} > {:.3e}; falling back to hallen",
                        r, SINUSOIDAL_REL_RESIDUAL_MAX
                    );
                    let hallen_rhs =
                        build_hallen_rhs(deck, segs, freq_hz).map_err(|e| e.to_string())?;
                    let mut hallen_z = assemble_z_matrix_with_ground(segs, freq_hz, ground);
                    hallen_z.add_to_diagonal(&load_vec);
                    for (row, col, delta) in &tl_stamps {
                        hallen_z.add_to_entry(*row, *col, *delta);
                    }
                    let sol = solve_hallen(
                        &hallen_z,
                        &hallen_rhs.rhs,
                        &hallen_rhs.cos_vec,
                        wire_endpoints,
                    )
                    .map_err(|e| e.to_string())?;
                    let (a2, r2) = residual_hallen(
                        &hallen_z,
                        &sol.currents,
                        &sol.c_hom_per_wire,
                        &hallen_rhs.cos_vec,
                        &hallen_rhs.rhs,
                        wire_endpoints,
                    );
                    (sol.currents, a2, r2, "sinusoidal->hallen(residual)")
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
        freq_hz,
    );

    let current_table: Vec<CurrentRow> = segs
        .iter()
        .enumerate()
        .map(|(idx, seg)| CurrentRow {
            tag: seg.tag as usize,
            seg: seg.tag_index as usize,
            current: i_vec[idx],
        })
        .collect();

    let source_table = build_source_rows(deck);
    let load_table = build_load_rows(deck);

    let pattern_table: Vec<PatternRow> = if pattern_points.is_empty() {
        Vec::new()
    } else if enable_gpu_fr {
        // Dispatch far-field to GPU kernel stub
        // Note: GPU stub uses CPU computation; normalization computed via simple heuristic
        let gpu_segments: Vec<_> = segs
            .iter()
            .map(|seg| nec_accel::gpu_kernels::GpuSegment {
                midpoint: seg.midpoint,
                direction: seg.direction,
                length: seg.length,
            })
            .collect();

        // Use standard CPU path once to get normalization, then switch to GPU stub for pattern eval
        // (In production, the GPU would compute this on-device)
        let _ = compute_radiation_pattern(segs, &i_vec, freq_hz, &[pattern_points[0]], ground);

        // For stub, use a simple normalized reference (total current squared)
        let norm_ref = i_vec.iter().map(|i| i.norm_sqr()).sum::<f64>();

        let kernel = nec_accel::HallenFrGpuKernel::new(
            gpu_segments,
            i_vec.clone(),
            freq_hz,
            norm_ref.max(1e-6),
        );
        let pattern_points_tuples: Vec<_> = pattern_points
            .iter()
            .map(|p| (p.theta_deg, p.phi_deg))
            .collect();

        nec_accel::compute_hallen_fr_batch_stub(&kernel, &pattern_points_tuples)
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

    let report = render_text_report(&ReportInput {
        solver_mode: diag_label,
        pulse_rhs: pulse_rhs_mode.as_contract_str(),
        frequency_hz: freq_hz,
        rows: &rows,
        source_table: &source_table,
        load_table: &load_table,
        current_table: &current_table,
        pattern_table: &pattern_table,
    });
    let sweep_summary = rows.first().map(|row| SweepPointSummary {
        freq_mhz: freq_hz / 1e6,
        tag: row.tag,
        seg: row.seg,
        z_re: row.z_in.re,
        z_im: row.z_in.im,
    });
    let diag_line = format!(
        "diag: mode={diag_label} pulse_rhs={:?} exec={} freq_mhz={:.6} abs_res={:.6e} rel_res={:.6e} diag_spread={:.6e} sin_rel_res={:.6e}",
        pulse_rhs_mode,
        execution_mode.as_diag_str(),
        freq_hz / 1e6,
        diag_abs,
        diag_rel,
        diag_spread,
        sin_rel_res
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
pub(super) fn execute_frequency_sweep<F>(
    freqs_hz: &[f64],
    execution_mode: ExecutionMode,
    solve_one: F,
) -> (
    Vec<(usize, Result<FrequencySolveResult, String>, u128)>,
    usize,
    usize,
)
where
    F: Fn(f64) -> Result<FrequencySolveResult, String> + Sync,
{
    use nec_accel::{
        dispatch_frequency_point, execute_frequency_point, AccelRequestKind, ExecutionPath,
    };
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

        let gpu_dispatch: Vec<(usize, nec_accel::DispatchDecision)> = lane_plan
            .gpu_candidate_indices
            .iter()
            .copied()
            .map(|idx| {
                (
                    idx,
                    dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, freqs_hz[idx]),
                )
            })
            .collect();

        let gpu_fallback_results: Vec<(
            usize,
            ExecutionPath,
            Result<FrequencySolveResult, String>,
            u128,
        )> = gpu_dispatch
            .into_iter()
            .map(|(idx, decision)| {
                let t0 = std::time::Instant::now();
                let (path, result) = execute_frequency_point(decision, || solve_one(freqs_hz[idx]));
                let elapsed_ms = t0.elapsed().as_millis();
                (idx, path, result, elapsed_ms)
            })
            .collect();

        let gpu_fallback_count = gpu_fallback_results
            .iter()
            .filter(|(_, path, _, _)| matches!(path, ExecutionPath::CpuFallback))
            .count();
        let gpu_stub_count = gpu_fallback_results
            .iter()
            .filter(|(_, path, _, _)| matches!(path, ExecutionPath::GpuStubEmulation))
            .count();

        solved.extend(
            gpu_fallback_results
                .into_iter()
                .map(|(idx, _, result, elapsed_ms)| (idx, result, elapsed_ms)),
        );

        (solved, gpu_fallback_count, gpu_stub_count)
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
        (solved, 0, 0)
    }
}
