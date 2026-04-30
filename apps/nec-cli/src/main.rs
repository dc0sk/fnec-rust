// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

mod cli_args;
mod exec_profile;
mod geometry_validation;
mod solve_session;
mod warnings;

use cli_args::{parse_args, ParsedArgs, USAGE};
use exec_profile::{
    auto_select_execution_mode, detect_compatibility_profile, startup_execution_probe,
    steer_execution_mode_by_profile, warn_compatibility_profile, CompatibilityProfile,
};
use geometry_validation::{
    buried_wire_geometry_error, segment_intersection_error, sinusoidal_a4_topology_supported,
    source_risk_geometry_error,
};
use nec_accel::{
    dispatch_frequency_point, execute_frequency_point, AccelRequestKind, DispatchDecision,
    ExecutionPath,
};
use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::{
    build_excitation_with_options, build_geometry, ground_model_from_deck, rp_card_points,
    wire_endpoints_from_segs, Ex3NormalizationMode, FarFieldPoint,
};
use rayon::prelude::*;
use solve_session::{
    build_hybrid_lane_plan, frequencies_from_fr, solve_frequency_point, FrequencySolveResult,
    SweepPointSummary,
};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};
use warnings::{
    warn_deferred_ground_model, warn_ex_type1_portability_semantics,
    warn_ex_type2_portability_semantics, warn_ex_type3_normalization_semantics,
    warn_ex_type4_portability_semantics, warn_ex_type5_portability_semantics,
    warn_execution_mode_fallback, warn_ge_ground_reflection_flag, warn_nt_card_deferred_support,
    warn_pt_card_deferred_support, warn_pulse_mode_experimental,
};

const C0: f64 = 299_792_458.0;
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
enum BenchFormat {
    Human,
    Csv,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ex3I4Mode {
    Legacy,
    DivideByI4,
}

impl Ex3I4Mode {
    fn as_solver_mode(self) -> Ex3NormalizationMode {
        match self {
            Ex3I4Mode::Legacy => Ex3NormalizationMode::LegacyTreatAsType0,
            Ex3I4Mode::DivideByI4 => Ex3NormalizationMode::ProvisionalDivideByI4,
        }
    }
}

impl SolverMode {
    fn as_str(self) -> &'static str {
        match self {
            SolverMode::Hallen => "hallen",
            SolverMode::Pulse => "pulse",
            SolverMode::Continuity => "continuity",
            SolverMode::Sinusoidal => "sinusoidal",
        }
    }
}

impl ExecutionMode {
    pub(crate) fn as_cli_str(self) -> &'static str {
        match self {
            ExecutionMode::Cpu => "cpu",
            ExecutionMode::Hybrid => "hybrid",
            ExecutionMode::Gpu => "gpu",
        }
    }

    pub(crate) fn as_diag_str(self) -> &'static str {
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

#[derive(Debug, Clone)]
struct BenchRecord {
    mode: String,
    pulse_rhs: String,
    exec: String,
    freq_mhz: f64,
    abs_res: f64,
    rel_res: f64,
    diag_spread: f64,
    sin_rel_res: f64,
}

fn epoch_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn emit_bench_csv_header() {
    eprintln!(
        "bench_csv:timestamp_unix_ms,target,deck,solver,run,status,elapsed_ms,diag_mode,pulse_rhs,exec,freq_mhz,abs_res,rel_res,diag_spread,sin_rel_res"
    );
}

fn emit_bench_record_csv(
    target: &str,
    deck: &str,
    solver: &str,
    run: usize,
    elapsed_ms: u128,
    bench: &BenchRecord,
) {
    eprintln!(
        "bench_csv:{},{},{},{},{},ok,{},{},{},{},{:.6},{:.6e},{:.6e},{:.6e},{:.6e}",
        epoch_millis_now(),
        target,
        deck,
        solver,
        run,
        elapsed_ms,
        bench.mode,
        bench.pulse_rhs,
        bench.exec,
        bench.freq_mhz,
        bench.abs_res,
        bench.rel_res,
        bench.diag_spread,
        bench.sin_rel_res
    );
}

fn emit_bench_record_json(
    target: &str,
    deck: &str,
    solver: &str,
    run: usize,
    elapsed_ms: u128,
    bench: &BenchRecord,
) {
    eprintln!(
        "bench_json:{{\"timestamp_unix_ms\":{},\"target\":\"{}\",\"deck\":\"{}\",\"solver\":\"{}\",\"run\":{},\"status\":\"ok\",\"elapsed_ms\":{},\"diag_mode\":\"{}\",\"pulse_rhs\":\"{}\",\"exec\":\"{}\",\"freq_mhz\":{:.6},\"abs_res\":{:.6e},\"rel_res\":{:.6e},\"diag_spread\":{:.6e},\"sin_rel_res\":{:.6e}}}",
        epoch_millis_now(),
        json_escape(target),
        json_escape(deck),
        json_escape(solver),
        run,
        elapsed_ms,
        json_escape(&bench.mode),
        json_escape(&bench.pulse_rhs),
        json_escape(&bench.exec),
        bench.freq_mhz,
        bench.abs_res,
        bench.rel_res,
        bench.diag_spread,
        bench.sin_rel_res
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let profile = detect_compatibility_profile(args.first().map(String::as_str).unwrap_or("fnec"));
    let exec_flag_explicitly_set = args.iter().any(|arg| arg == "--exec");

    if args.len() < 2 {
        eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let ParsedArgs {
        solver_mode,
        pulse_rhs_mode,
        mut execution_mode,
        hallen_allow_non_collinear,
        enable_benchmarking,
        bench_format,
        ex3_i4_mode,
        enable_gpu_fr,
        path,
    } = match parse_args(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
            eprintln!("{USAGE}");
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
    warn_ex_type1_portability_semantics(deck, solver_mode);
    warn_ex_type2_portability_semantics(deck);
    warn_ex_type4_portability_semantics(deck, solver_mode);
    warn_ex_type5_portability_semantics(deck, solver_mode);
    warn_pt_card_deferred_support(deck);
    warn_nt_card_deferred_support(deck);
    warn_ex_type3_normalization_semantics(deck, ex3_i4_mode);

    let freqs_hz = frequencies_from_fr(deck);
    if freqs_hz.is_empty() {
        return ExitCode::SUCCESS;
    }

    if !exec_flag_explicitly_set && profile == CompatibilityProfile::Native {
        let probe = startup_execution_probe(freqs_hz.len());
        let auto_mode = auto_select_execution_mode(execution_mode, probe);
        eprintln!(
            "info: startup exec probe: cpu_threads={} freq_points={} gpu_available={} hybrid_gpu_lane_available={} selected_exec={}",
            probe.cpu_threads,
            probe.freq_points,
            probe.gpu_available,
            probe.hybrid_gpu_lane_available,
            auto_mode.as_cli_str(),
        );
        execution_mode = auto_mode;
    }

    warn_execution_mode_fallback(execution_mode);

    let segs = match build_geometry(deck) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(err) = segment_intersection_error(&segs) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
    if let Some(err) = source_risk_geometry_error(&deck.cards, &segs) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
    // Per-wire basis solve requires every wire to have >= 2 segments.
    let wire_endpoints = wire_endpoints_from_segs(&segs);
    let per_wire_basis_feasible = wire_endpoints.iter().all(|&(first, last)| last > first);

    let ex3_mode = ex3_i4_mode.as_solver_mode();
    let v_vec = match build_excitation_with_options(deck, &segs, ex3_mode) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let ground = ground_model_from_deck(deck);
    if let Some(err) = buried_wire_geometry_error(&segs, &ground) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
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

    let sinusoidal_topology_supported = sinusoidal_a4_topology_supported(&segs, &wire_endpoints);

    let solve_one = |freq_hz: f64| {
        solve_frequency_point(
            deck,
            &segs,
            &wire_endpoints,
            per_wire_basis_feasible,
            sinusoidal_topology_supported,
            &v_vec,
            &ground,
            &pattern_points,
            solver_mode,
            pulse_rhs_mode,
            execution_mode,
            hallen_allow_non_collinear,
            ex3_mode,
            enable_gpu_fr,
            freq_hz,
        )
    };

    let timed_solve_one = |freq_hz: f64| {
        let t0 = std::time::Instant::now();
        let result = solve_one(freq_hz);
        (result, t0.elapsed().as_millis())
    };

    let solved: Vec<(usize, Result<FrequencySolveResult, String>, u128)> = if matches!(
        execution_mode,
        ExecutionMode::Hybrid
    ) && freqs_hz.len()
        > 1
    {
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
                .map(|(idx, _, result, elapsed_ms)| (idx, result, elapsed_ms)),
        );

        solved
    } else {
        freqs_hz
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, freq_hz)| {
                let (result, elapsed_ms) = timed_solve_one(freq_hz);
                (idx, result, elapsed_ms)
            })
            .collect()
    };

    let mut solved = solved;
    solved.sort_by_key(|(idx, _, _)| *idx);

    if enable_benchmarking && bench_format == BenchFormat::Csv {
        emit_bench_csv_header();
    }

    let bench_target = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let bench_deck = path.display().to_string();
    let bench_solver = solver_mode.as_str().to_string();
    let mut sweep_rows: Vec<SweepPointSummary> = Vec::new();

    for (fidx, result, elapsed_ms) in solved {
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
        if let Some(summary) = solved_point.sweep_summary {
            sweep_rows.push(summary);
        }
        eprintln!("{}", solved_point.diag_line);

        if enable_benchmarking {
            let run = fidx + 1;
            match bench_format {
                BenchFormat::Human => {}
                BenchFormat::Csv => emit_bench_record_csv(
                    &bench_target,
                    &bench_deck,
                    &bench_solver,
                    run,
                    elapsed_ms,
                    &solved_point.bench,
                ),
                BenchFormat::Json => emit_bench_record_json(
                    &bench_target,
                    &bench_deck,
                    &bench_solver,
                    run,
                    elapsed_ms,
                    &solved_point.bench,
                ),
            }
        }
    }

    if sweep_rows.len() > 1 {
        println!();
        println!("SWEEP_POINTS");
        println!("N_POINTS {}", sweep_rows.len());
        println!("FREQ_MHZ TAG SEG Z_RE Z_IM");
        for row in sweep_rows {
            println!(
                "{:.6} {} {} {:.6} {:.6}",
                row.freq_mhz, row.tag, row.seg, row.z_re, row.z_im
            );
        }
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::exec_profile::StartupExecutionProbe;
    use super::{
        auto_select_execution_mode, detect_compatibility_profile, steer_execution_mode_by_profile,
        CompatibilityProfile, ExecutionMode,
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

    #[test]
    fn auto_probe_prefers_cpu_for_single_point_workloads() {
        let probe = StartupExecutionProbe {
            cpu_threads: 16,
            freq_points: 1,
            gpu_available: false,
            hybrid_gpu_lane_available: false,
        };
        assert_eq!(
            auto_select_execution_mode(ExecutionMode::Cpu, probe),
            ExecutionMode::Cpu
        );
    }

    #[test]
    fn auto_probe_prefers_hybrid_for_multifrequency_multicore_cpu() {
        let probe = StartupExecutionProbe {
            cpu_threads: 8,
            freq_points: 5,
            gpu_available: false,
            hybrid_gpu_lane_available: false,
        };
        assert_eq!(
            auto_select_execution_mode(ExecutionMode::Cpu, probe),
            ExecutionMode::Hybrid
        );
    }

    #[test]
    fn auto_probe_prefers_gpu_when_gpu_is_available_without_cpu_multithread_gain() {
        let probe = StartupExecutionProbe {
            cpu_threads: 1,
            freq_points: 1,
            gpu_available: true,
            hybrid_gpu_lane_available: true,
        };
        assert_eq!(
            auto_select_execution_mode(ExecutionMode::Cpu, probe),
            ExecutionMode::Gpu
        );
    }

    #[test]
    fn auto_probe_prefers_hybrid_when_gpu_and_cpu_multithread_are_available() {
        let probe = StartupExecutionProbe {
            cpu_threads: 8,
            freq_points: 9,
            gpu_available: true,
            hybrid_gpu_lane_available: true,
        };
        assert_eq!(
            auto_select_execution_mode(ExecutionMode::Cpu, probe),
            ExecutionMode::Hybrid
        );
    }
}
