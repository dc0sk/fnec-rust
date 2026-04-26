// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

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

impl ExecutionMode {
    fn as_diag_str(self) -> &'static str {
        match self {
            ExecutionMode::Cpu => "cpu",
            ExecutionMode::Hybrid => "hybrid(cpu-fallback)",
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

fn parse_args(
    args: &[String],
) -> Result<(SolverMode, PulseRhsMode, ExecutionMode, bool, PathBuf), String> {
    let mut solver_mode = SolverMode::Hallen;
    let mut pulse_rhs_mode = PulseRhsMode::Nec2;
    let mut execution_mode = ExecutionMode::Cpu;
    let mut hallen_allow_non_collinear = false;
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
        path,
    ))
}

fn warn_execution_mode_fallback(execution_mode: ExecutionMode) {
    match execution_mode {
        ExecutionMode::Cpu => {}
        ExecutionMode::Hybrid => eprintln!(
            "warning: --exec hybrid requested, but CPU+GPU split kernels are not yet wired; using CPU solve path"
        ),
        ExecutionMode::Gpu => eprintln!(
            "warning: --exec gpu requested, but GPU solve kernels are not yet wired; using CPU solve path"
        ),
    }
}

fn l2_norm(v: &[Complex64]) -> f64 {
    v.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt()
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
        eprintln!(
            "Usage: fnec [--solver <pulse|hallen|continuity|sinusoidal>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--allow-noncollinear-hallen] <deck.nec>"
        );
        return ExitCode::from(2);
    }

    let (solver_mode, pulse_rhs_mode, execution_mode, hallen_allow_non_collinear, path) =
        match parse_args(&args) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
                eprintln!("Usage: fnec [--solver <pulse|hallen|continuity|sinusoidal>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--allow-noncollinear-hallen] <deck.nec>");
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };

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

    for (fidx, freq_hz) in freqs_hz.iter().copied().enumerate() {
        let v_vec_pulse = match pulse_rhs_mode {
            PulseRhsMode::Raw => v_vec.clone(),
            PulseRhsMode::Nec2 => scale_excitation_for_pulse_rhs(&v_vec, freq_hz),
        };

        let mut z_mat = match solver_mode {
            SolverMode::Hallen => assemble_z_matrix_with_ground(&segs, freq_hz, &ground),
            SolverMode::Pulse | SolverMode::Continuity | SolverMode::Sinusoidal => {
                assemble_pocklington_matrix(&segs, freq_hz)
            }
        };

        let (load_vec, load_warnings) = build_loads(deck, &segs, freq_hz);
        for warning in &load_warnings {
            eprintln!("warning: {warning}");
        }
        z_mat.add_to_diagonal(&load_vec);

        let (i_vec, diag_abs, diag_rel, diag_label) = match solver_mode {
            SolverMode::Hallen => {
                let hallen_rhs = match build_hallen_rhs_with_options(
                    deck,
                    &segs,
                    freq_hz,
                    hallen_allow_non_collinear,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::FAILURE;
                    }
                };
                match solve_hallen(
                    &z_mat,
                    &hallen_rhs.rhs,
                    &hallen_rhs.cos_vec,
                    &hallen_rhs.wire_endpoints,
                ) {
                    Ok(sol) => {
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
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            SolverMode::Pulse => match solve(&z_mat, &v_vec_pulse) {
                Ok(i) => {
                    let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                    (i, a, r, "pulse")
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            },
            SolverMode::Continuity => {
                if !per_wire_basis_feasible {
                    eprintln!(
                        "warning: continuity solver requires >=2 segments per wire; falling back to pulse"
                    );
                    match solve(&z_mat, &v_vec_pulse) {
                        Ok(i) => {
                            let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                            (i, a, r, "continuity->pulse")
                        }
                        Err(e) => {
                            eprintln!("error: {e}");
                            return ExitCode::FAILURE;
                        }
                    }
                } else {
                    match solve_with_continuity_basis_per_wire(
                        &z_mat,
                        &v_vec_pulse,
                        &wire_endpoints,
                    ) {
                        Ok(i) => {
                            let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                            if r <= CONTINUITY_REL_RESIDUAL_MAX {
                                (i, a, r, "continuity")
                            } else {
                                eprintln!(
                                    "warning: continuity residual {:.3e} > {:.3e}; falling back to pulse",
                                    r, CONTINUITY_REL_RESIDUAL_MAX
                                );
                                match solve(&z_mat, &v_vec_pulse) {
                                    Ok(i2) => {
                                        let (a2, r2) =
                                            residual_zi_minus_v(&z_mat, &i2, &v_vec_pulse);
                                        (i2, a2, r2, "continuity->pulse(residual)")
                                    }
                                    Err(e) => {
                                        eprintln!("error: {e}");
                                        return ExitCode::FAILURE;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("error: {e}");
                            return ExitCode::FAILURE;
                        }
                    }
                }
            }
            SolverMode::Sinusoidal => {
                if !per_wire_basis_feasible {
                    eprintln!(
                        "warning: sinusoidal solver requires >=2 segments per wire; falling back to pulse"
                    );
                    match solve(&z_mat, &v_vec_pulse) {
                        Ok(i) => {
                            let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                            (i, a, r, "sinusoidal->pulse")
                        }
                        Err(e) => {
                            eprintln!("error: {e}");
                            return ExitCode::FAILURE;
                        }
                    }
                } else {
                    match solve_with_sinusoidal_basis_per_wire(
                        &z_mat,
                        &v_vec_pulse,
                        &wire_endpoints,
                    ) {
                        Ok(i) => {
                            let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                            if r <= SINUSOIDAL_REL_RESIDUAL_MAX {
                                (i, a, r, "sinusoidal")
                            } else {
                                eprintln!(
                                    "warning: sinusoidal residual {:.3e} > {:.3e}; falling back to hallen",
                                    r, SINUSOIDAL_REL_RESIDUAL_MAX
                                );
                                let hallen_rhs = match build_hallen_rhs_with_options(
                                    deck,
                                    &segs,
                                    freq_hz,
                                    hallen_allow_non_collinear,
                                ) {
                                    Ok(h) => h,
                                    Err(e) => {
                                        eprintln!("error: {e}");
                                        return ExitCode::FAILURE;
                                    }
                                };
                                let mut hallen_z =
                                    assemble_z_matrix_with_ground(&segs, freq_hz, &ground);
                                hallen_z.add_to_diagonal(&load_vec);
                                match solve_hallen(
                                    &hallen_z,
                                    &hallen_rhs.rhs,
                                    &hallen_rhs.cos_vec,
                                    &hallen_rhs.wire_endpoints,
                                ) {
                                    Ok(sol) => {
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
                                    Err(e) => {
                                        eprintln!("error: {e}");
                                        return ExitCode::FAILURE;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("error: {e}");
                            return ExitCode::FAILURE;
                        }
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

        if fidx > 0 {
            println!();
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

        // Collect RP cards and compute radiation pattern if any are present.
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

        let pattern_table: Vec<PatternRow> = if pattern_points.is_empty() {
            Vec::new()
        } else {
            let results = compute_radiation_pattern(&segs, &i_vec, freq_hz, &pattern_points);
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
            current_table: &current_table,
            pattern_table: &pattern_table,
        });
        print!("{report}");

        eprintln!(
            "diag: mode={diag_label} pulse_rhs={:?} exec={} freq_mhz={:.6} abs_res={:.6e} rel_res={:.6e}",
            pulse_rhs_mode,
            execution_mode.as_diag_str(),
            freq_hz / 1e6,
            diag_abs,
            diag_rel
        );
    }

    ExitCode::SUCCESS
}
