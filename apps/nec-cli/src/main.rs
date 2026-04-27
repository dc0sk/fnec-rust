// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_accel::{
    dispatch_frequency_point, execute_frequency_point, AccelRequestKind, DispatchDecision,
    ExecutionPath,
};
use nec_model::card::Card;
use nec_parser::parse;
use nec_report::{render_text_report, CurrentRow, FeedpointRow, PatternRow, ReportInput};
use nec_solver::build_loads;
use nec_solver::{
    assemble_pocklington_matrix, assemble_z_matrix_with_ground, build_excitation, build_geometry,
    build_hallen_rhs_with_options, compute_radiation_pattern, ground_model_from_deck,
    rp_card_points, scale_excitation_for_pulse_rhs, solve, solve_hallen,
    solve_with_continuity_basis_per_wire, solve_with_sinusoidal_basis_per_wire,
    wire_endpoints_from_segs, FarFieldPoint, GroundModel, ZMatrix,
};
use num_complex::Complex64;
use rayon::prelude::*;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

const CONTINUITY_REL_RESIDUAL_MAX: f64 = 1e-3;
const SINUSOIDAL_REL_RESIDUAL_MAX: f64 = 1e-2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SolverMode {
    Hallen,
    Pulse,
    Continuity,
    Sinusoidal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PulseRhsMode {
    Raw,
    Nec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionMode {
    Cpu,
    Hybrid,
    Gpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompatibilityProfile {
    Native,
    FourNec2DropIn,
}

impl ExecutionMode {
    fn as_diag_str(self) -> &'static str {
        match self {
            ExecutionMode::Cpu => "cpu",
            ExecutionMode::Hybrid => "hybrid",
            ExecutionMode::Gpu => "gpu(cpu-fallback)",
        }
    }
}

impl PulseRhsMode {
    fn as_contract_str(self) -> &'static str {
        match self {
            PulseRhsMode::Raw => "Raw",
            PulseRhsMode::Nec2 => "Nec2",
        }
    }
}

fn detect_compatibility_profile(argv0: &str) -> CompatibilityProfile {
    let stem = Path::new(argv0)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if stem.contains("nec2dxs") || stem.contains("4nec2") {
        CompatibilityProfile::FourNec2DropIn
    } else {
        CompatibilityProfile::Native
    }
}

fn steer_execution_mode_by_profile(
    execution_mode: ExecutionMode,
    profile: CompatibilityProfile,
    exec_flag_explicitly_set: bool,
) -> ExecutionMode {
    if exec_flag_explicitly_set {
        return execution_mode;
    }

    match profile {
        CompatibilityProfile::Native => execution_mode,
        // In drop-in mode prefer throughput when caller did not force an exec mode.
        CompatibilityProfile::FourNec2DropIn => ExecutionMode::Hybrid,
    }
}

fn warn_compatibility_profile(
    profile: CompatibilityProfile,
    requested_execution_mode: ExecutionMode,
    effective_execution_mode: ExecutionMode,
    exec_flag_explicitly_set: bool,
) {
    if profile != CompatibilityProfile::FourNec2DropIn {
        return;
    }

    if exec_flag_explicitly_set {
        eprintln!(
            "warning: 4nec2 drop-in compatibility profile detected by binary name; preserving explicit --exec={}",
            requested_execution_mode.as_diag_str()
        );
    } else {
        eprintln!(
            "warning: 4nec2 drop-in compatibility profile detected by binary name; default execution path steered to exec={}",
            effective_execution_mode.as_diag_str()
        );
    }
}

fn parse_args(
    args: &[String],
) -> Result<
    (
        SolverMode,
        PulseRhsMode,
        ExecutionMode,
        bool,
        bool,
        bool,
        PathBuf,
    ),
    String,
> {
    let mut solver_mode = SolverMode::Hallen;
    let mut pulse_rhs_mode = PulseRhsMode::Nec2;
    let mut execution_mode = ExecutionMode::Cpu;
    let mut hallen_allow_non_collinear = false;
    let mut enable_benchmarking = false;
    let mut enable_gpu_fr = false;
    let mut deck_path: Option<PathBuf> = None;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--solver" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --solver (expected: hallen|pulse|continuity|sinusoidal)"
                            .to_string(),
                    );
                }
                solver_mode = match args[i].as_str() {
                    "hallen" => SolverMode::Hallen,
                    "pulse" => SolverMode::Pulse,
                    "continuity" => SolverMode::Continuity,
                    "sinusoidal" => SolverMode::Sinusoidal,
                    other => {
                        return Err(format!(
                            "invalid --solver value '{other}' (expected: hallen|pulse|continuity|sinusoidal)"
                        ))
                    }
                };
            }
            "--pulse-rhs" => {
                i += 1;
                if i >= args.len() {
                    return Err("missing value after --pulse-rhs (expected: raw|nec2)".to_string());
                }
                pulse_rhs_mode = match args[i].as_str() {
                    "raw" => PulseRhsMode::Raw,
                    "nec2" => PulseRhsMode::Nec2,
                    other => {
                        return Err(format!(
                            "invalid --pulse-rhs value '{other}' (expected: raw|nec2)"
                        ))
                    }
                };
            }
            "--exec" => {
                i += 1;
                if i >= args.len() {
                    return Err("missing value after --exec (expected: cpu|hybrid|gpu)".to_string());
                }
                execution_mode = match args[i].as_str() {
                    "cpu" => ExecutionMode::Cpu,
                    "hybrid" => ExecutionMode::Hybrid,
                    "gpu" => ExecutionMode::Gpu,
                    other => {
                        return Err(format!(
                            "invalid --exec value '{other}' (expected: cpu|hybrid|gpu)"
                        ))
                    }
                };
            }
            "--allow-noncollinear-hallen" => {
                hallen_allow_non_collinear = true;
            }
            "--bench" => {
                enable_benchmarking = true;
            }
            "--gpu-fr" => {
                enable_gpu_fr = true;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if deck_path.is_some() {
                    return Err(format!("unexpected extra argument: {path}"));
                }
                deck_path = Some(PathBuf::from(path));
            }
        }
        i += 1;
    }

    let path = deck_path.ok_or_else(|| "missing deck path".to_string())?;
    Ok((
        solver_mode,
        pulse_rhs_mode,
        execution_mode,
        hallen_allow_non_collinear,
        enable_benchmarking,
        enable_gpu_fr,
        path,
    ))
}

fn warn_execution_mode_fallback(execution_mode: ExecutionMode) {
    match execution_mode {
        ExecutionMode::Cpu => {}
        ExecutionMode::Hybrid => {}
        ExecutionMode::Gpu => match dispatch_frequency_point(AccelRequestKind::GpuOnly, 0.0) {
            DispatchDecision::FallbackToCpu { reason } => {
                eprintln!("warning: --exec gpu requested, but {reason}; using CPU solve path");
            }
            DispatchDecision::RunOnGpu => {
                eprintln!(
                        "warning: --exec gpu dispatched to accelerator stub backend; solving with CPU emulation"
                    );
            }
        },
    }
}

fn l2_norm(v: &[Complex64]) -> f64 {
    v.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt()
}

fn matrix_diagonal_spread(z: &ZMatrix) -> f64 {
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

fn residual_zi_minus_v(z: &ZMatrix, i_vec: &[Complex64], v_vec: &[Complex64]) -> (f64, f64) {
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

fn residual_hallen(
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

fn warn_pulse_mode_experimental(solver_mode: SolverMode) {
    if !matches!(
        solver_mode,
        SolverMode::Pulse | SolverMode::Continuity | SolverMode::Sinusoidal
    ) {
        return;
    }
    eprintln!(
        "warning: pulse/continuity/sinusoidal solver modes are EXPERIMENTAL and known-inaccurate for \
thin-wire antennas. The pulse-basis Pocklington EFIE diverges from the physical solution \
as segment count increases. Use --solver hallen for accurate results. \
(Sinusoidal-basis EFIE fix tracked in backlog.)"
    );
}

fn sinusoidal_a4_topology_supported(
    segs: &[nec_solver::Segment],
    wire_endpoints: &[(usize, usize)],
) -> bool {
    if segs.is_empty() {
        return false;
    }

    let ref_dir = segs[0].direction;
    for seg in segs {
        let dot = seg.direction[0] * ref_dir[0]
            + seg.direction[1] * ref_dir[1]
            + seg.direction[2] * ref_dir[2];
        if dot.abs() < 1.0 - 1e-9 {
            return false;
        }
    }

    // A4 phase-2: wire-chain detection is orientation/order agnostic.
    // Build an undirected graph from wire endpoints and require a single
    // connected path (degrees <=2, exactly two degree-1 nodes unless there is
    // only one wire). This still rejects disconnected and branched topologies.
    const TOUCH_EPS: f64 = 1e-9;
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut degree: Vec<usize> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();

    for (first, last) in wire_endpoints.iter().copied() {
        if first > last || last >= segs.len() {
            return false;
        }
        let a = segs[first].start;
        let b = segs[last].end;
        let ia = find_or_insert_node(&mut nodes, &mut degree, a, TOUCH_EPS);
        let ib = find_or_insert_node(&mut nodes, &mut degree, b, TOUCH_EPS);
        if ia == ib {
            return false;
        }
        degree[ia] += 1;
        degree[ib] += 1;
        edges.push((ia, ib));
    }

    if degree.iter().any(|d| *d > 2) {
        return false;
    }
    let degree_one = degree.iter().filter(|d| **d == 1).count();
    if wire_endpoints.len() == 1 {
        if degree_one != 2 {
            return false;
        }
    } else if degree_one != 2 {
        return false;
    }

    if nodes.is_empty() {
        return false;
    }

    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (u, v) in edges {
        adjacency[u].push(v);
        adjacency[v].push(u);
    }

    let start = match degree.iter().position(|d| *d > 0) {
        Some(idx) => idx,
        None => return false,
    };
    let mut seen = vec![false; nodes.len()];
    let mut stack = vec![start];
    while let Some(u) = stack.pop() {
        if seen[u] {
            continue;
        }
        seen[u] = true;
        for &v in &adjacency[u] {
            if !seen[v] {
                stack.push(v);
            }
        }
    }
    if degree
        .iter()
        .enumerate()
        .any(|(idx, d)| *d > 0 && !seen[idx])
    {
        return false;
    }

    true
}

fn points_close(a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt() <= eps
}

fn find_or_insert_node(
    nodes: &mut Vec<[f64; 3]>,
    degree: &mut Vec<usize>,
    p: [f64; 3],
    eps: f64,
) -> usize {
    for (idx, node) in nodes.iter().enumerate() {
        if points_close(*node, p, eps) {
            return idx;
        }
    }
    nodes.push(p);
    degree.push(0);
    nodes.len() - 1
}

fn warn_deferred_ground_model(ground: &GroundModel) {
    let GroundModel::Deferred { gn_type } = ground else {
        return;
    };
    eprintln!("warning: GN type {gn_type} is not yet supported; treating this deck as free-space");
}

fn warn_ge_ground_reflection_flag(deck: &nec_model::deck::NecDeck) {
    let Some(flag) = deck.cards.iter().find_map(|c| {
        if let Card::Ge(ge) = c {
            Some(ge.ground_reflection_flag)
        } else {
            None
        }
    }) else {
        return;
    };

    // GE I1=0: no ground (default, no action needed).
    // GE I1=1: PEC image method — handled via ground_model_from_deck.
    match flag {
        0 | 1 => {}
        -1 => {
            eprintln!(
                "warning: GE I1=-1 requests below-ground wire handling \
                 (no image method); treating as free-space"
            );
        }
        _ => {
            eprintln!(
                "warning: GE I1={flag} is not a recognised ground-reflection flag \
                 (valid values: 0=free-space, 1=PEC image, -1=below-ground); \
                 treating as free-space"
            );
        }
    }
}

fn frequencies_from_fr(deck: &nec_model::deck::NecDeck) -> Vec<f64> {
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

struct HybridLanePlan {
    cpu_indices: Vec<usize>,
    gpu_candidate_indices: Vec<usize>,
}

fn build_hybrid_lane_plan(freq_count: usize) -> HybridLanePlan {
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

struct FrequencySolveResult {
    report: String,
    diag_line: String,
}

#[allow(clippy::too_many_arguments)]
fn solve_frequency_point(
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    wire_endpoints: &[(usize, usize)],
    per_wire_basis_feasible: bool,
    v_vec: &[Complex64],
    ground: &GroundModel,
    pattern_points: &[FarFieldPoint],
    solver_mode: SolverMode,
    pulse_rhs_mode: PulseRhsMode,
    execution_mode: ExecutionMode,
    hallen_allow_non_collinear: bool,
    enable_gpu_fr: bool,
    freq_hz: f64,
) -> Result<FrequencySolveResult, String> {
    let v_vec_pulse = match pulse_rhs_mode {
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
    let diag_spread = matrix_diagonal_spread(&z_mat);
    let mut sin_rel_res: f64 = 0.0;

    let (i_vec, diag_abs, diag_rel, diag_label) = match solver_mode {
        SolverMode::Hallen => {
            let hallen_rhs =
                build_hallen_rhs_with_options(deck, segs, freq_hz, hallen_allow_non_collinear)
                    .map_err(|e| e.to_string())?;
            let sol = solve_hallen(
                &z_mat,
                &hallen_rhs.rhs,
                &hallen_rhs.cos_vec,
                &hallen_rhs.wire_endpoints,
            )
            .map_err(|e| e.to_string())?;
            let (a, r) = residual_hallen(
                &z_mat,
                &sol.currents,
                &sol.c_hom_per_wire,
                &hallen_rhs.cos_vec,
                &hallen_rhs.rhs,
                &hallen_rhs.wire_endpoints,
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
            } else if !sinusoidal_a4_topology_supported(segs, wire_endpoints) {
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
                    let hallen_rhs = build_hallen_rhs_with_options(
                        deck,
                        segs,
                        freq_hz,
                        hallen_allow_non_collinear,
                    )
                    .map_err(|e| e.to_string())?;
                    let mut hallen_z = assemble_z_matrix_with_ground(segs, freq_hz, ground);
                    hallen_z.add_to_diagonal(&load_vec);
                    let sol = solve_hallen(
                        &hallen_z,
                        &hallen_rhs.rhs,
                        &hallen_rhs.cos_vec,
                        &hallen_rhs.wire_endpoints,
                    )
                    .map_err(|e| e.to_string())?;
                    let (a2, r2) = residual_hallen(
                        &hallen_z,
                        &sol.currents,
                        &sol.c_hom_per_wire,
                        &hallen_rhs.cos_vec,
                        &hallen_rhs.rhs,
                        &hallen_rhs.wire_endpoints,
                    );
                    (sol.currents, a2, r2, "sinusoidal->hallen(residual)")
                }
            }
        }
    };

    let mut rows: Vec<FeedpointRow> = Vec::new();
    for (idx, seg) in segs.iter().enumerate() {
        let v = v_vec[idx];
        if v.norm() < 1e-30 {
            continue;
        }
        let i = i_vec[idx];
        let v_source = v * seg.length;
        let z_in = if i.norm() > 1e-60 {
            v_source / i
        } else {
            v_source
        };
        rows.push(FeedpointRow {
            tag: seg.tag as usize,
            seg: seg.tag_index as usize,
            v_source,
            current: i,
            z_in,
        });
    }

    let current_table: Vec<CurrentRow> = segs
        .iter()
        .enumerate()
        .map(|(idx, seg)| CurrentRow {
            tag: seg.tag as usize,
            seg: seg.tag_index as usize,
            current: i_vec[idx],
        })
        .collect();

    let pattern_table: Vec<PatternRow> = if pattern_points.is_empty() {
        Vec::new()
    } else {
        if enable_gpu_fr {
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
            let _ = compute_radiation_pattern(segs, &i_vec, freq_hz, &[pattern_points[0]]);

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
            let results = compute_radiation_pattern(segs, &i_vec, freq_hz, pattern_points);
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
    };

    let report = render_text_report(&ReportInput {
        solver_mode: diag_label,
        pulse_rhs: pulse_rhs_mode.as_contract_str(),
        frequency_hz: freq_hz,
        rows: &rows,
        current_table: &current_table,
        pattern_table: &pattern_table,
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

    Ok(FrequencySolveResult { report, diag_line })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let profile = detect_compatibility_profile(args.first().map(String::as_str).unwrap_or("fnec"));
    let exec_flag_explicitly_set = args.iter().any(|arg| arg == "--exec");

    if args.len() < 2 {
        eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
        eprintln!(
            "Usage: fnec [--solver <pulse|hallen|continuity|sinusoidal>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--allow-noncollinear-hallen] [--bench] [--gpu-fr] <deck.nec>"
        );
        return ExitCode::from(2);
    }

    let (
        solver_mode,
        pulse_rhs_mode,
        mut execution_mode,
        hallen_allow_non_collinear,
        enable_benchmarking,
        enable_gpu_fr,
        path,
    ) = match parse_args(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
            eprintln!("Usage: fnec [--solver <pulse|hallen|continuity|sinusoidal>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--allow-noncollinear-hallen] [--bench] [--gpu-fr] <deck.nec>");
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    // Enable GPU benchmarking if --bench flag is set
    if enable_benchmarking {
        std::env::set_var("FNEC_GPU_BENCH", "1");
    }

    let requested_execution_mode = execution_mode;
    execution_mode = steer_execution_mode_by_profile(
        requested_execution_mode,
        profile,
        exec_flag_explicitly_set,
    );
    warn_compatibility_profile(
        profile,
        requested_execution_mode,
        execution_mode,
        exec_flag_explicitly_set,
    );

    if hallen_allow_non_collinear && solver_mode != SolverMode::Hallen {
        eprintln!(
            "warning: --allow-noncollinear-hallen is ignored unless --solver hallen is selected"
        );
    }

    if hallen_allow_non_collinear {
        eprintln!(
            "warning: --allow-noncollinear-hallen enables an EXPERIMENTAL Hallen RHS projection on non-collinear geometries; results may be inaccurate"
        );
    }

    warn_execution_mode_fallback(execution_mode);

    let input = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let result = match parse(&input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }

    let deck = &result.deck;

    warn_pulse_mode_experimental(solver_mode);
    warn_ge_ground_reflection_flag(deck);

    let freqs_hz = frequencies_from_fr(deck);
    if freqs_hz.is_empty() {
        return ExitCode::SUCCESS;
    }

    let segs = match build_geometry(deck) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Per-wire basis solve requires every wire to have >= 2 segments.
    let wire_endpoints = wire_endpoints_from_segs(&segs);
    let per_wire_basis_feasible = wire_endpoints.iter().all(|&(first, last)| last > first);

    let v_vec = match build_excitation(deck, &segs) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let ground = ground_model_from_deck(deck);
    warn_deferred_ground_model(&ground);

    let pattern_points: Vec<FarFieldPoint> = deck
        .cards
        .iter()
        .filter_map(|c| {
            if let Card::Rp(rp) = c {
                Some(rp_card_points(
                    rp.n_theta, rp.n_phi, rp.theta0, rp.phi0, rp.d_theta, rp.d_phi,
                ))
            } else {
                None
            }
        })
        .flatten()
        .collect();

    let solve_one = |freq_hz: f64| {
        solve_frequency_point(
            deck,
            &segs,
            &wire_endpoints,
            per_wire_basis_feasible,
            &v_vec,
            &ground,
            &pattern_points,
            solver_mode,
            pulse_rhs_mode,
            execution_mode,
            hallen_allow_non_collinear,
            enable_gpu_fr,
            freq_hz,
        )
    };

    let solved: Vec<(usize, Result<FrequencySolveResult, String>)> = if matches!(
        execution_mode,
        ExecutionMode::Hybrid
    ) && freqs_hz.len() > 1
    {
        let lane_plan = build_hybrid_lane_plan(freqs_hz.len());

        let mut solved = Vec::with_capacity(freqs_hz.len());

        let cpu_results: Vec<(usize, Result<FrequencySolveResult, String>)> = lane_plan
            .cpu_indices
            .par_iter()
            .copied()
            .map(|idx| (idx, solve_one(freqs_hz[idx])))
            .collect();
        solved.extend(cpu_results);

        let gpu_dispatch: Vec<(usize, DispatchDecision)> = lane_plan
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
        )> = gpu_dispatch
            .into_iter()
            .map(|(idx, decision)| {
                let (path, result) = execute_frequency_point(decision, || solve_one(freqs_hz[idx]));
                (idx, path, result)
            })
            .collect();

        let gpu_fallback_count = gpu_fallback_results
            .iter()
            .filter(|(_, path, _)| matches!(path, ExecutionPath::CpuFallback))
            .count();
        let gpu_stub_count = gpu_fallback_results
            .iter()
            .filter(|(_, path, _)| matches!(path, ExecutionPath::GpuStubEmulation))
            .count();

        if gpu_fallback_count > 0 {
            eprintln!(
                "warning: --exec hybrid scheduled {gpu_fallback_count} frequency point(s) for GPU-candidate lane, but GPU kernels are not yet wired; running those points on CPU fallback"
            );
        }
        if gpu_stub_count > 0 {
            eprintln!(
                "warning: --exec hybrid dispatched {gpu_stub_count} frequency point(s) to accelerator stub backend; solving with CPU emulation"
            );
        }

        solved.extend(
            gpu_fallback_results
                .into_iter()
                .map(|(idx, _, result)| (idx, result)),
        );

        solved
    } else {
        freqs_hz
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, freq_hz)| (idx, solve_one(freq_hz)))
            .collect()
    };

    let mut solved = solved;
    solved.sort_by_key(|(idx, _)| *idx);

    for (fidx, result) in solved {
        let solved_point = match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };

        if fidx > 0 {
            println!();
        }
        print!("{}", solved_point.report);
        eprintln!("{}", solved_point.diag_line);
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::{
        detect_compatibility_profile, steer_execution_mode_by_profile, CompatibilityProfile,
        ExecutionMode,
    };

    #[test]
    fn detects_fournec2_dropin_profile_by_kernel_name() {
        assert_eq!(
            detect_compatibility_profile("/tmp/nec2dxs500"),
            CompatibilityProfile::FourNec2DropIn
        );
        assert_eq!(
            detect_compatibility_profile("C:/tools/4nec2-kernel"),
            CompatibilityProfile::FourNec2DropIn
        );
    }

    #[test]
    fn keeps_native_profile_for_default_binary_name() {
        assert_eq!(
            detect_compatibility_profile("/usr/bin/fnec"),
            CompatibilityProfile::Native
        );
    }

    #[test]
    fn dropin_profile_steers_default_exec_to_hybrid() {
        assert_eq!(
            steer_execution_mode_by_profile(
                ExecutionMode::Cpu,
                CompatibilityProfile::FourNec2DropIn,
                false,
            ),
            ExecutionMode::Hybrid
        );
    }

    #[test]
    fn explicit_exec_flag_prevents_profile_steering() {
        assert_eq!(
            steer_execution_mode_by_profile(
                ExecutionMode::Gpu,
                CompatibilityProfile::FourNec2DropIn,
                true,
            ),
            ExecutionMode::Gpu
        );
    }
}
