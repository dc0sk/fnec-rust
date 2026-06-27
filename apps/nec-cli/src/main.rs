// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

mod bench;
mod cli_args;
mod exec_profile;
mod geometry_validation;
mod resonance_search;
mod solve_session;
mod sweep_config;
mod vars_config;
mod warnings;

use bench::{emit_bench_csv_header, emit_bench_record_csv, emit_bench_record_json, BenchFormat};
use cli_args::{parse_args, OutputFormat, ParsedArgs, USAGE};
use exec_profile::{
    auto_select_execution_mode, detect_compatibility_profile, startup_execution_probe,
    steer_execution_mode_by_profile, warn_compatibility_profile, CompatibilityProfile,
    ExecutionMode,
};
use geometry_validation::{
    buried_wire_geometry_error, segment_intersection_error, source_risk_geometry_error,
};
use nec_model::card::Card;
use nec_model::{run_validators, DeckValidator, DiagnosticLevel, ValidationDiagnostic};
use nec_parser::parse;
use nec_solver::{
    build_excitation, build_geometry, ground_model_from_deck, rp_card_points,
    wire_endpoints_from_segs, FarFieldPoint,
};
use nec_worker::{
    encode_deck, HostsConfig, TaskMessage, TaskResult, WorkerPool, WorkerSolverConfig,
};
use solve_session::{
    execute_frequency_sweep, frequencies_from_fr, solve_frequency_point, BenchRecord,
    FrequencySolveResult, PulseRhsMode, SolverMode, SweepPointSummary,
    SINUSOIDAL_REL_RESIDUAL_MAX_DEFAULT,
};
use std::process::ExitCode;
use std::time::Instant;
use warnings::{
    warn_deferred_ground_model, warn_execution_mode_fallback, warn_ge_ground_reflection_flag,
    warn_nt_card_deferred_support, warn_pt_card_deferred_support, warn_pulse_mode_experimental,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let profile = detect_compatibility_profile(args.first().map(String::as_str).unwrap_or("fnec"));
    let exec_flag_explicitly_set = args.iter().any(|arg| arg == "--exec");

    if args.len() < 2 {
        eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    // --- sweep subcommand ---------------------------------------------------
    if args.get(1).map(String::as_str) == Some("sweep") {
        return run_sweep_subcommand(&args);
    }
    // ------------------------------------------------------------------------

    // --- worker subcommand --------------------------------------------------
    if args.get(1).map(String::as_str) == Some("worker") {
        return run_worker_subcommand();
    }
    // ------------------------------------------------------------------------

    let ParsedArgs {
        solver_mode,
        pulse_rhs_mode,
        mut execution_mode,
        enable_benchmarking,
        bench_format,
        output_format,
        sweep_config_path,
        vars_path,
        sin_fallback_rel_max_cli,
        hosts_path,
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

    let sin_fallback_rel_max = if let Some(v) = sin_fallback_rel_max_cli {
        v
    } else if let Ok(raw) = std::env::var("FNEC_SIN_FALLBACK_REL_MAX") {
        match raw.parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => v,
            _ => {
                eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
                eprintln!("{USAGE}");
                eprintln!(
                    "error: invalid FNEC_SIN_FALLBACK_REL_MAX='{raw}' (expected: positive number)"
                );
                return ExitCode::from(2);
            }
        }
    } else {
        SINUSOIDAL_REL_RESIDUAL_MAX_DEFAULT
    };

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

    let input = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let input = if let Some(ref vp) = vars_path {
        let vars = match vars_config::load_vars(vp) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        match nec_parser::template::substitute(&input, &vars) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        input
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
    warn_nt_card_deferred_support(deck);
    warn_pt_card_deferred_support(deck);

    // --- EP-4: run deck validators before geometry build ------------------
    struct NoExCardValidator;
    impl DeckValidator for NoExCardValidator {
        fn validate(&self, deck: &nec_model::deck::NecDeck) -> Vec<ValidationDiagnostic> {
            let has_ex = deck.cards.iter().any(|c| matches!(c, Card::Ex(_)));
            if has_ex {
                vec![]
            } else {
                vec![ValidationDiagnostic::warning(
                    "deck has no EX card — no feedpoint impedance will be computed",
                )]
            }
        }
    }
    let validators: Vec<&dyn DeckValidator> = vec![&NoExCardValidator];
    let validator_diags = run_validators(deck, &validators);
    let mut has_validator_error = false;
    for diag in &validator_diags {
        match diag.level {
            DiagnosticLevel::Error => {
                eprintln!("error: [validator] {}", diag.message);
                has_validator_error = true;
            }
            DiagnosticLevel::Warning => {
                eprintln!("warning: [validator] {}", diag.message);
            }
        }
    }
    if has_validator_error {
        return ExitCode::FAILURE;
    }
    // ----------------------------------------------------------------------

    let freqs_hz = if let Some(ref sc_path) = sweep_config_path {
        match sweep_config::SweepConfig::from_file(sc_path) {
            Ok(sc) => sc.frequencies_hz,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        frequencies_from_fr(deck)
    };
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

    // ------------------------------------------------------------------
    // Distributed solve via --hosts
    // ------------------------------------------------------------------
    if let Some(ref hosts_path) = hosts_path {
        return run_distributed_solve(
            &input,
            &freqs_hz,
            hosts_path,
            output_format,
            enable_benchmarking,
            bench_format,
            solver_mode,
            execution_mode,
            &path,
        );
    }
    // ------------------------------------------------------------------

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

    let v_vec = match build_excitation(deck, &segs) {
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
            sin_fallback_rel_max,
            freq_hz,
        )
    };

    let (mut solved, gpu_fallback_count) =
        execute_frequency_sweep(&freqs_hz, execution_mode, solve_one);
    solved.sort_by_key(|(idx, _, _)| *idx);

    if gpu_fallback_count > 0 {
        eprintln!(
            "warning: --exec hybrid scheduled {gpu_fallback_count} frequency point(s) for the GPU-candidate lane, but per-frequency GPU dispatch is not yet wired (PH7-CHK-004); running those points on CPU fallback"
        );
    }

    if enable_benchmarking && bench_format == BenchFormat::Csv {
        emit_bench_csv_header();
    }

    let bench_target = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let bench_deck = path.display().to_string();
    let bench_solver = solver_mode.as_str().to_string();
    let mut sweep_rows: Vec<SweepPointSummary> = Vec::new();
    let mut json_records: Vec<String> = Vec::new();

    for (fidx, result, elapsed_ms) in solved {
        let solved_point = match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };

        if output_format == OutputFormat::Text {
            if fidx > 0 {
                println!();
            }
            print!("{}", solved_point.report);
        }
        if let Some(summary) = solved_point.sweep_summary {
            if output_format == OutputFormat::Json {
                let z_abs = (summary.z_re * summary.z_re + summary.z_im * summary.z_im).sqrt();
                let z_arg_deg = summary.z_im.atan2(summary.z_re).to_degrees();
                json_records.push(format!(
                    "{{\"freq_mhz\":{freq_mhz},\"tag\":{tag},\"seg\":{seg},\"z_re\":{z_re},\"z_im\":{z_im},\"z_abs\":{z_abs},\"z_arg_deg\":{z_arg_deg}}}",
                    freq_mhz = summary.freq_mhz,
                    tag = summary.tag,
                    seg = summary.seg,
                    z_re = summary.z_re,
                    z_im = summary.z_im,
                    z_abs = z_abs,
                    z_arg_deg = z_arg_deg,
                ));
            }
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

    if sweep_rows.len() > 1 && output_format == OutputFormat::Text {
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

    if output_format == OutputFormat::Json {
        println!("[{records}]", records = json_records.join(","));
    }

    ExitCode::SUCCESS
}

/// Distributed solve via `--hosts`.
///
/// Loads the hosts config, creates a worker pool, base64-encodes the deck, and
/// dispatches one task per frequency point.  Results are collected and emitted
/// in the same output format as the local solve path.
#[allow(clippy::too_many_arguments)]
fn run_distributed_solve(
    input: &str,
    freqs_hz: &[f64],
    hosts_path: &std::path::Path,
    output_format: OutputFormat,
    enable_benchmarking: bool,
    bench_format: BenchFormat,
    solver_mode: SolverMode,
    execution_mode: ExecutionMode,
    path: &std::path::Path,
) -> ExitCode {
    let cfg = match HostsConfig::from_file(hosts_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if cfg.worker.is_empty() {
        eprintln!(
            "error: --hosts file '{}' contains no [[worker]] entries",
            hosts_path.display()
        );
        return ExitCode::FAILURE;
    }

    let mut pool = WorkerPool::new_ssh_skip_failures(&cfg.worker);
    if pool.is_empty() {
        eprintln!(
            "error: no workers could be reached from '{}'",
            hosts_path.display()
        );
        return ExitCode::FAILURE;
    }

    let deck_b64 = encode_deck(input);
    let deck_hash = "na".to_string(); // informational; worker does not verify
    let basis = solver_mode.as_str().to_string();
    // PH7-CHK-004: ask workers to use the GPU when the run is --exec gpu; each
    // worker falls back to CPU if it has no adapter or the deck is out of class.
    let exec = if execution_mode == ExecutionMode::Gpu {
        "gpu".to_string()
    } else {
        "cpu".to_string()
    };
    let solver_config = WorkerSolverConfig {
        basis,
        exec,
        ..WorkerSolverConfig::default()
    };

    let n = freqs_hz.len();
    let mut solved: Vec<(usize, Result<FrequencySolveResult, String>, u128)> =
        Vec::with_capacity(n);

    for (fidx, &freq_hz) in freqs_hz.iter().enumerate() {
        let task_id = format!("{deck_hash}-{fidx}");
        let task = TaskMessage {
            task_id,
            deck_hash: deck_hash.clone(),
            deck_b64: deck_b64.clone(),
            solver_config: solver_config.clone(),
            frequency_hz: freq_hz,
        };

        let start = Instant::now();
        let result = match pool.dispatch(&task) {
            Ok((
                TaskResult::Ok {
                    impedance,
                    vswr_50,
                    feedpoint_current_mag,
                    feedpoint_current_phase_deg,
                    ..
                },
                label,
            )) => {
                let freq_mhz = freq_hz / 1e6;
                let report = format!(
                    "FEEDPOINTS\nFREQ {freq_mhz}\nZ {re} {im}\nVSWR 50 {vswr}\nFEEDPOINT CURRENT {mag} {phase}\n",
                    freq_mhz = freq_mhz,
                    re = impedance.re_ohm,
                    im = impedance.im_ohm,
                    vswr = vswr_50,
                    mag = feedpoint_current_mag,
                    phase = feedpoint_current_phase_deg,
                );
                let diag_line = format!(
                    "diag: mode=distributed freq_mhz={freq_mhz:.6} z_abs={:.6e} vswr={:.6} worker={label}",
                    (impedance.re_ohm * impedance.re_ohm + impedance.im_ohm * impedance.im_ohm).sqrt(),
                    vswr_50,
                );
                let bench = BenchRecord {
                    mode: "distributed".to_string(),
                    pulse_rhs: "unknown".to_string(),
                    exec: "ssh".to_string(),
                    freq_mhz,
                    abs_res: 0.0,
                    rel_res: 0.0,
                    diag_spread: 0.0,
                    sin_rel_res: 0.0,
                };
                let sweep_summary = Some(SweepPointSummary {
                    freq_mhz,
                    tag: 0,
                    seg: 0,
                    z_re: impedance.re_ohm,
                    z_im: impedance.im_ohm,
                });
                Ok(FrequencySolveResult {
                    report,
                    diag_line,
                    bench,
                    sweep_summary,
                })
            }
            Ok((
                TaskResult::Error {
                    frequency_hz,
                    error_code,
                    error_message,
                    ..
                },
                label,
            )) => Err(format!(
                "worker '{label}' failed at {frequency_hz} Hz: {error_code:?} — {error_message}"
            )),
            Err(e) => Err(e),
        };
        let elapsed_ms = start.elapsed().as_millis();
        solved.push((fidx, result, elapsed_ms));
    }

    // Drop pool explicitly to shut down workers before output
    pool.shutdown_all();

    // --- output (mirrors local solve path) ---
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
    let mut json_records: Vec<String> = Vec::new();

    for (fidx, result, elapsed_ms) in solved {
        let solved_point = match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };

        if output_format == OutputFormat::Text {
            if fidx > 0 {
                println!();
            }
            print!("{}", solved_point.report);
        }
        if let Some(summary) = solved_point.sweep_summary {
            if output_format == OutputFormat::Json {
                let z_abs = (summary.z_re * summary.z_re + summary.z_im * summary.z_im).sqrt();
                let z_arg_deg = summary.z_im.atan2(summary.z_re).to_degrees();
                json_records.push(format!(
                    "{{\"freq_mhz\":{freq_mhz},\"tag\":{tag},\"seg\":{seg},\"z_re\":{z_re},\"z_im\":{z_im},\"z_abs\":{z_abs},\"z_arg_deg\":{z_arg_deg}}}",
                    freq_mhz = summary.freq_mhz,
                    tag = summary.tag,
                    seg = summary.seg,
                    z_re = summary.z_re,
                    z_im = summary.z_im,
                    z_abs = z_abs,
                    z_arg_deg = z_arg_deg,
                ));
            }
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

    if sweep_rows.len() > 1 && output_format == OutputFormat::Text {
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

    if output_format == OutputFormat::Json {
        println!("[{records}]", records = json_records.join(","));
    }

    ExitCode::SUCCESS
}

/// Entry point for `fnec worker --stdio`.
///
/// Runs the distributed worker stdio event loop: reads newline-delimited JSON
/// task messages from stdin and writes result messages to stdout.  Exits when
/// stdin closes or a shutdown command is received.
fn run_worker_subcommand() -> ExitCode {
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout();
    nec_worker::run_worker_stdio(stdin, stdout);
    ExitCode::SUCCESS
}

/// Entry point for `fnec sweep --resonance <file.nec.toml>`.
fn run_sweep_subcommand(args: &[String]) -> ExitCode {
    const SWEEP_USAGE: &str = "Usage: fnec sweep --resonance <file.nec.toml>\n\
         The .nec.toml file must contain [search] and [deck] tables.";

    // Parse the sweep subcommand args (args[0] = binary, args[1] = "sweep").
    let mut resonance_path: Option<std::path::PathBuf> = None;
    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            "--resonance" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
                    eprintln!("{SWEEP_USAGE}");
                    eprintln!("error: missing value after --resonance");
                    return ExitCode::from(2);
                }
                resonance_path = Some(std::path::PathBuf::from(&args[i]));
            }
            flag if flag.starts_with('-') => {
                eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
                eprintln!("{SWEEP_USAGE}");
                eprintln!("error: unknown sweep option: {flag}");
                return ExitCode::from(2);
            }
            other => {
                eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
                eprintln!("{SWEEP_USAGE}");
                eprintln!("error: unexpected argument: {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let path = match resonance_path {
        Some(p) => p,
        None => {
            eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
            eprintln!("{SWEEP_USAGE}");
            eprintln!("error: --resonance <file> is required");
            return ExitCode::from(2);
        }
    };

    let rf = match resonance_search::ResonanceFile::from_file(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let template = rf.deck.template.clone();
    let cfg = rf.search;

    // Build a probe closure: substitutes the search variable into the template,
    // parses the deck, runs a single-frequency solve, and returns (z_re, z_im).
    let probe = |val: f64| -> Result<(f64, f64), String> {
        let mut vars = std::collections::HashMap::new();
        vars.insert(cfg.var.clone(), format!("{val:.9}"));
        let deck_str =
            nec_parser::template::substitute(&template, &vars).map_err(|e| e.to_string())?;
        let result = parse(&deck_str).map_err(|e| e.to_string())?;
        let deck = &result.deck;

        let segs = build_geometry(deck).map_err(|e| e.to_string())?;
        let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
        let ground = ground_model_from_deck(deck);
        let wire_endpoints = wire_endpoints_from_segs(&segs);
        let per_wire_basis_feasible = wire_endpoints.iter().all(|&(first, last)| last > first);

        // Find the single FR frequency from the deck.
        let freqs = frequencies_from_fr(deck);
        let freq_hz = freqs
            .first()
            .copied()
            .ok_or_else(|| "resonance search: deck must have an FR card".to_string())?;

        let solve_result = solve_frequency_point(
            deck,
            &segs,
            &wire_endpoints,
            per_wire_basis_feasible,
            &v_vec,
            &ground,
            &[],
            SolverMode::Hallen,
            PulseRhsMode::Nec2,
            ExecutionMode::Cpu,
            SINUSOIDAL_REL_RESIDUAL_MAX_DEFAULT,
            freq_hz,
        )?;

        let summary = solve_result.sweep_summary.ok_or_else(|| {
            "resonance search: solver did not produce a sweep summary".to_string()
        })?;

        Ok((summary.z_re, summary.z_im))
    };

    match resonance_search::bisect(
        cfg.lo,
        cfg.hi,
        cfg.target_reactance_ohm,
        cfg.tolerance_ohm,
        cfg.max_iter,
        probe,
    ) {
        Ok(result) => {
            resonance_search::print_result(&cfg.var, &result);
            if result.converged {
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "warning: resonance search did not converge within {} iterations \
                     (|z_im - target| = {:.3} Ω)",
                    result.iterations,
                    (result.final_z_im - cfg.target_reactance_ohm).abs()
                );
                ExitCode::SUCCESS // still emit result; caller decides
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
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
            detect_compatibility_profile("C:/4nec2/EXE/nec2dxs1K5.exe"),
            CompatibilityProfile::FourNec2DropIn
        );
        assert_eq!(
            detect_compatibility_profile("C:/4nec2/EXE/NEC2DXS3K0.EXE"),
            CompatibilityProfile::FourNec2DropIn
        );
        assert_eq!(
            detect_compatibility_profile("/opt/4nec2/nec2dxs5k0"),
            CompatibilityProfile::FourNec2DropIn
        );
        assert_eq!(
            detect_compatibility_profile("/opt/4nec2/nec2dxs8k0"),
            CompatibilityProfile::FourNec2DropIn
        );
        assert_eq!(
            detect_compatibility_profile("/opt/4nec2/nec2dxs11k"),
            CompatibilityProfile::FourNec2DropIn
        );
        assert_eq!(
            detect_compatibility_profile("C:/tools/4nec2-kernel"),
            CompatibilityProfile::FourNec2DropIn
        );
    }

    #[test]
    fn keeps_native_profile_for_unknown_nec2dxs_like_names() {
        assert_eq!(
            detect_compatibility_profile("/tmp/nec2dxs750"),
            CompatibilityProfile::Native
        );
        assert_eq!(
            detect_compatibility_profile("/tmp/custom-nec2dxs-wrapper"),
            CompatibilityProfile::Native
        );
    }

    #[test]
    fn detects_dropin_profile_when_known_kernel_name_is_embedded_as_token() {
        assert_eq!(
            detect_compatibility_profile("/tmp/fnec-dropin-alias-nec2dxs500-123"),
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
