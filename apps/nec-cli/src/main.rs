// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

mod bench;
mod cli_args;
mod exec_profile;
mod laplace_config;
mod project_cmd;
mod resonance_search;
mod solve_session;
mod sweep_config;
mod vars_config;
mod warnings;

use bench::{emit_bench_csv_header, emit_bench_record_csv, emit_bench_record_json, BenchFormat};
use cli_args::{parse_args, OutputFormat, ParsedArgs, USAGE};
use exec_profile::{
    auto_select_execution_mode, detect_compatibility_profile, steer_execution_mode_by_profile,
    warn_compatibility_profile, CompatibilityProfile, ExecutionMode,
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
    FrequencySolveResult, GroundSolver, PulseRhsMode, SolverMode, SweepPointSummary,
    SINUSOIDAL_REL_RESIDUAL_MAX_DEFAULT,
};
use std::process::ExitCode;
use std::time::Instant;
use warnings::{
    warn_deferred_ground_model, warn_ge_ground_reflection_flag, warn_mpie_mixed_radius,
    warn_near_field_over_finite_ground, warn_pulse_mode_experimental,
};

/// Print every point a sweep computed, and report the ones that failed.
///
/// Returns `true` if any point failed, so the caller can exit non-zero after the
/// output rather than instead of it.
///
/// **The policy changed here (FND-101/FND-102 follow-up).** This loop used to
/// `return ExitCode::FAILURE` on the first `Err`, so a sweep that lost one point
/// threw away every point after it — and when a distributed run drained its
/// worker pool, the first result was already an error and the invocation printed
/// *nothing at all*, having solved most of the sweep. A point that computed is as
/// true as it was a moment earlier; the GUI has kept its points on a failed sweep
/// since FND-033, and this brings the CLI to the same answer.
///
/// Failures go to stderr as they are met, results to stdout, and the exit code
/// still says the run was not clean — so a script that checks the status is
/// unaffected, while a human gets the 499 points that worked.
#[allow(clippy::too_many_arguments)]
fn emit_sweep_points(
    solved: Vec<(usize, Result<FrequencySolveResult, String>, u128)>,
    output_format: OutputFormat,
    sweep_rows: &mut Vec<SweepPointSummary>,
    json_records: &mut Vec<String>,
    enable_benchmarking: bool,
    bench_format: BenchFormat,
    bench_target: &str,
    bench_deck: &str,
    bench_solver: &str,
) -> bool {
    let mut any_failed = false;
    for (fidx, result, elapsed_ms) in solved {
        let solved_point = match result {
            Ok(v) => v,
            Err(e) => {
                // Report and carry on. Returning here is what discarded the rest
                // of the sweep.
                eprintln!("error: {e}");
                any_failed = true;
                continue;
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
                // The unvalidated-solver caveat travels in the record itself, so a
                // consumer that never reads stderr still sees it (FND-080). Absent
                // for every validated solver: their records are unchanged.
                let caveat = summary
                    .caveat
                    .map(|c| format!(",\"caveat\":\"{c}\""))
                    .unwrap_or_default();
                json_records.push(format!(
                    "{{\"freq_mhz\":{freq_mhz},\"tag\":{tag},\"seg\":{seg},\"z_re\":{z_re},\"z_im\":{z_im},\"z_abs\":{z_abs},\"z_arg_deg\":{z_arg_deg}{caveat}}}",
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
                    bench_target,
                    bench_deck,
                    bench_solver,
                    run,
                    elapsed_ms,
                    &solved_point.bench,
                ),
                BenchFormat::Json => emit_bench_record_json(
                    bench_target,
                    bench_deck,
                    bench_solver,
                    run,
                    elapsed_ms,
                    &solved_point.bench,
                ),
            }
        }
    }
    any_failed
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

    // Asked-for version and help are answers, not errors: stdout, exit 0. Both
    // used to be "unknown option", exit 2, with the usage on stderr — whose first
    // line, `fnec <version>`, a release smoke test read as a version report (FND-169).
    match args[1].as_str() {
        "--version" | "-V" => {
            println!("fnec {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        "--help" | "-h" => {
            println!("fnec {}", env!("CARGO_PKG_VERSION"));
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    // --- sweep subcommand ---------------------------------------------------
    if args.get(1).map(String::as_str) == Some("sweep") {
        return run_sweep_subcommand(&args);
    }
    // ------------------------------------------------------------------------

    // --- project subcommand -------------------------------------------------
    // GAP-015's "explicit CLI entry points", which were never written (FND-006),
    // and the reason `nec_project` was a dependency nothing imported (FND-016).
    if args.get(1).map(String::as_str) == Some("project") {
        return project_cmd::run(&args);
    }
    // ------------------------------------------------------------------------

    // --- worker subcommand --------------------------------------------------
    if args.get(1).map(String::as_str) == Some("worker") {
        return run_worker_subcommand();
    }
    // ------------------------------------------------------------------------

    // --- taper subcommand (Leeson step-tapered-radius correction) -----------
    if args.get(1).map(String::as_str) == Some("taper") {
        return run_taper_subcommand(&args);
    }
    // ------------------------------------------------------------------------

    let ParsedArgs {
        solver_mode,
        ground_solver,
        pulse_rhs_mode,
        mut execution_mode,
        enable_benchmarking,
        bench_format,
        output_format,
        sweep_config_path,
        vars_path,
        loads_config_path,
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

    // Optional fnec-specific Laplace-domain loads (--loads-config <file.toml>).
    let laplace_loads = match loads_config_path {
        Some(ref p) => match laplace_config::load_laplace_loads(p) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        },
        None => Vec::new(),
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

    // A deck with several `FR` cards was usually written as `FR/XQ/FR/RP`, where
    // NEC-2 runs each. fnec drops `XQ` as an unknown card and runs one execution,
    // at the last `FR` — so an earlier card is dropped, and dropping it silently
    // discards a run the author asked for (FND-057). The GUI and the bindings get
    // this through `validate::diagnose`; this binary composes its own caveats, so
    // it has to ask.
    for w in nec_solver::validate::superseded_frequency_warnings(deck) {
        eprintln!("warning: {w}");
    }

    warn_pulse_mode_experimental(solver_mode);
    warn_ge_ground_reflection_flag(deck);
    // NT cards are now stamped in the solve path (PH8-CHK-004); malformed/
    // unsupported NT cards warn from there. PT cards are applied to the current
    // output in solve_session (PH9-CHK-004). No blanket deferred warnings.

    // --- EP-4: run deck validators before geometry build ------------------
    struct NoExCardValidator;
    impl DeckValidator for NoExCardValidator {
        fn validate(&self, deck: &nec_model::deck::NecDeck) -> Vec<ValidationDiagnostic> {
            // The sentence is `nec_solver`'s, not this file's. It used to be a
            // local `ValidationDiagnostic::warning("deck has no EX card — no
            // feedpoint impedance will be computed")`, and it was the ONLY
            // no-`EX` check in the tree — so the GUI, `fnec_py` and the worker
            // had none, and the GUI drew an all-zero current overlay with no
            // caveat whatsoever.
            //
            // Now an ERROR, and the same predicate the other three frontends
            // reach through `pre_solve_error`. This validator still earns its
            // place, though for less than an earlier version of this comment
            // claimed: running before the geometry build skips the FR and
            // sweep-config parsing, the exec probe, `build_geometry`, and
            // `geometry_error`'s pairwise crossing scan. It does NOT save an
            // O(N²) matrix fill — `pre_solve_error` below already refuses ahead
            // of any assembly. It also keeps EP-4's `DeckValidator` integration
            // demonstrated (PH4-CHK-005).
            //
            // The cost of keeping it is real, and is recorded where it bites:
            // the CLI now reaches this refusal by two independent routes, so the
            // CLI's own tests cannot discriminate a regression in the shared one.
            // See the note on the sweep test in `tests/deck_validator.rs`.
            nec_solver::validate::undriven_deck_error(deck)
                .map(|m| vec![ValidationDiagnostic::error(m)])
                .unwrap_or_default()
        }
    }
    // ----------------------------------------------------------------------

    // Every configuration file the run names is read and checked here, before
    // any refusal about the deck, as `--sweep-config` is just below: a bad
    // `--hosts` path used to go unreported whenever an earlier deck refusal
    // (a missing frequency) ended the run first (FND-150). Reading the file
    // contacts nothing; the pool is built only once the run gets that far.
    let hosts_cfg = match hosts_path.as_deref().map(HostsConfig::from_file) {
        None => None,
        Some(Ok(cfg)) => Some(cfg),
        Some(Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

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
    // After every configuration file is read (FND-150): a bad `--hosts` or
    // `--sweep-config` path is reported even on an undriven deck, as the comment
    // at the `--hosts` read promises (FND-181). Still ahead of the first deck
    // refusal, the exec probe and the geometry build.
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
    // This used to be `return ExitCode::SUCCESS` — a deck with no `FR` and no
    // `--sweep-config` exited 0 having written zero bytes to stdout AND stderr,
    // while the GUI and `fnec_py` refused the same deck (FND-070). A silent
    // success is the worst of the three answers: it is indistinguishable from a
    // run that worked.
    //
    // The check is over the RESOLVED list, so `--sweep-config` on a deck with no
    // `FR` still solves — frequencies do not have to come from the deck, which is
    // exactly why this cannot live in `pre_solve_error`.
    //
    // Reached only when both sources are absent: a `--sweep-config` that parsed
    // but yielded nothing is already refused by `SweepConfig::from_file`, and a
    // file that failed to parse exits above.
    if let Some(err) = nec_solver::validate::no_frequency_error(
        &freqs_hz,
        "Add an `FR` card to the deck, or pass `--sweep-config <file.toml>` to \
         supply the frequencies yourself.",
    ) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }

    // --- geometry + pre-solve validation, shared by BOTH solve paths --------
    //
    // This block sits ABOVE the `--hosts` branch deliberately. It used to sit
    // below it, so a distributed run skipped validation entirely and dispatched a
    // deck the local run refuses to every worker (FND-013). Putting the check
    // here rather than duplicating it inside `run_distributed_solve` also keeps
    // it ahead of `WorkerPool` construction, which spawns an SSH process per host
    // the moment it is built — validating after that would connect to every host
    // before noticing the deck was never solvable.
    //
    // `build_excitation` deliberately stays below the branch: hoisting it would
    // move EX-reference errors from the worker to the controller, a separate
    // behaviour change.
    //
    // The hoist does reorder the LOCAL path, and that is deliberate rather than
    // incidental. `buried_wire_geometry_error` and the deferred-ground warning now
    // run BEFORE `build_excitation` instead of after, so a deck with both a buried
    // wire and a bad `EX` reference reports the buried wire (it used to report the
    // `EX`), and a deferred `GN` type now warns even when the run then fails on the
    // `EX`. Same exit code either way. This is the order `validate::diagnose`
    // already uses, which the GUI and the Python bindings adopted in #369/#370, so
    // the three frontends now agree on it.
    let segs = match build_geometry(deck) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ground = ground_model_from_deck(deck);
    if let Some(err) = nec_solver::validate::pre_solve_error(deck, &segs, &ground) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
    // The loads file's twin of `pre_solve_error`'s LD check, which cannot see it.
    if let Some(err) = nec_solver::laplace_load_error(&laplace_loads, &segs) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
    warn_deferred_ground_model(&ground);
    warn_near_field_over_finite_ground(deck, &ground);

    // ------------------------------------------------------------------
    // Distributed solve via --hosts
    // ------------------------------------------------------------------
    if let (Some(ref hosts_path), Some(hosts_cfg)) = (hosts_path, hosts_cfg) {
        // Two flags change the answer locally and are dropped on the floor by the
        // distributed path: `run_distributed_solve` takes neither, and the worker
        // protocol carries no field for either. Left alone, both return a
        // plausible number for a deck the user did not describe — FND-023's
        // silent-wrong-answer signature one layer up (FND-025, FND-027). Reject
        // rather than solve the wrong problem.
        //
        // This sits ahead of `WorkerPool` construction for the FND-013 reason: the
        // pool spawns an SSH process per host the moment it is built, so a check
        // placed inside `run_distributed_solve` would dial every host before
        // noticing the run was never going to honour the flag.
        if !laplace_loads.is_empty() {
            eprintln!(
                "error: Laplace loads (--loads-config) are not supported with --hosts; \
                 the worker protocol carries no field for them. Run without --hosts."
            );
            return ExitCode::FAILURE;
        }
        // The worker accepts `basis == "hallen"` and nothing else, so every task
        // of a non-Hallén run fails `UnsupportedConfig` — but only after the pool
        // has dialled every host at a 5 s SSH timeout each, and only once per
        // frequency point. The user waits N x 5 s to be told something knowable
        // before the first connection (FND-018).
        if !matches!(solver_mode, SolverMode::Hallen) {
            eprintln!(
                "error: --solver {} is not supported with --hosts; the worker \
                 implements the Hallén basis only, so every task would be refused. \
                 Run without --hosts, or use --solver hallen.",
                solver_mode.as_flag()
            );
            return ExitCode::FAILURE;
        }
        if matches!(ground_solver, GroundSolver::Sommerfeld) {
            // The worker derives its ground model from the deck alone, so the
            // surface-wave correction never reaches it. Measured on
            // `corpus/dipole-gn2-near-ground-51seg.nec`: 95.524 + j12.166 Ω
            // locally with the correction against 92.266 + j13.617 Ω without it.
            eprintln!(
                "error: --ground-solver sommerfeld is not supported with --hosts; \
                 the worker derives its ground model from the deck and never applies \
                 the surface-wave correction. Run without --hosts."
            );
            return ExitCode::FAILURE;
        }
        // The worker returns a feedpoint impedance and nothing else, so it refuses
        // a receive deck — but only after the pool has dialled every host (N x 5 s)
        // and once per frequency. Knowable from the deck before any connection,
        // the same as the solver checks above (FND-018, FND-178).
        if nec_solver::deck_has_plane_wave(deck) {
            eprintln!(
                "error: an incident plane wave is not supported with --hosts; the \
                 distributed worker returns feedpoint impedances only and a receiving \
                 antenna has none. Run without --hosts to get its induced currents."
            );
            return ExitCode::FAILURE;
        }
        // Every pre-solve caveat the local run emits. This used to be the topology
        // one alone, so a distributed run of a low-over-ground or junction-fed deck
        // returned numbers with none of the qualifications the same deck earns
        // locally (FND-020).
        for w in distributed_pre_solve_caveats(deck, &segs, &ground, &freqs_hz, solver_mode) {
            eprintln!("warning: {w}");
        }
        return run_distributed_solve(
            &input,
            deck,
            &segs,
            &freqs_hz,
            hosts_path,
            hosts_cfg,
            output_format,
            enable_benchmarking,
            bench_format,
            solver_mode,
            execution_mode,
            exec_flag_explicitly_set,
            &path,
        );
    }

    // The deck class the device takes, and whether the system fits it: one
    // answer for the automatic pick and for hybrid's GPU lane, so they cannot
    // disagree. Local runs only, so it sits below the `--hosts` branch: a worker
    // is told `gpu` only when the user said so (FND-040), never by a pick made
    // from the controller's hardware.
    let stamped = deck.cards.iter().any(|c| {
        matches!(
            c,
            nec_model::card::Card::Ld(_)
                | nec_model::card::Card::Tl(_)
                | nec_model::card::Card::Nt(_)
        )
    });
    let gpu_deck_class = if !matches!(solver_mode, SolverMode::Hallen) {
        Err("the device solves the Hallén basis only")
    } else if stamped || !laplace_loads.is_empty() {
        Err("the deck stamps loads or networks into the matrix, which the device does not see")
    } else {
        nec_solver::gpu_resident_class(deck, &segs, &ground)
    };
    // Two homogeneous unknowns per straight wire with two free ends: the system
    // the device holds is S = N + 2W.
    let unknowns = segs.len() + 2 * nec_solver::merged_grouping(&segs).0.len();
    let device_capacity = || {
        // Hardware first: on a host with only a software adapter the capacity
        // query would build a device on it.
        if pollster::block_on(nec_accel::hardware_adapter_present()) {
            nec_accel::shared_device_dense_capacity()
        } else {
            None
        }
    };
    if !exec_flag_explicitly_set && profile == CompatibilityProfile::Native {
        let choice = auto_select_execution_mode(
            segs.len(),
            unknowns,
            freqs_hz.len(),
            gpu_deck_class,
            device_capacity,
        );
        eprintln!(
            "info: exec auto: selected_exec={} ({})",
            choice.mode.as_cli_str(),
            choice.reason
        );
        execution_mode = choice.mode;
    }
    // Hybrid's GPU lane opens on the same terms; without it hybrid is the CPU
    // pool, and says why.
    let gpu_lane = execution_mode == ExecutionMode::Hybrid && freqs_hz.len() > 1 && {
        let why = match gpu_deck_class {
            Err(why) => Some(format!("not a deck the GPU solves: {why}")),
            Ok(()) => match device_capacity() {
                None => Some("no hardware GPU".to_string()),
                Some(cap) if unknowns > cap => Some(format!(
                    "{unknowns} unknowns, more than the GPU holds ({cap})"
                )),
                Some(_) => None,
            },
        };
        if let Some(why) = &why {
            eprintln!("info: --exec hybrid runs on the CPU only: {why}");
        }
        why.is_none()
    };
    // ------------------------------------------------------------------

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

    warn_mpie_mixed_radius(solver_mode, &segs);

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

    let solve_one = |freq_hz: f64, point_mode: ExecutionMode| {
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
            point_mode,
            sin_fallback_rel_max,
            freq_hz,
            ground_solver,
            &laplace_loads,
        )
    };

    // FND-187: every CPU point holds its own matrices, so the points in flight are
    // bounded by memory, not only by cores. One point, or the device alone, needs
    // no budget.
    let max_parallel = if freqs_hz.len() > 1 && execution_mode != ExecutionMode::Gpu {
        let budget = solve_session::sweep_memory_budget().unwrap_or_else(|e| {
            eprintln!("warning: {e}; the sweep's memory is not budgeted");
            None
        });
        let pool = rayon::current_num_threads();
        let b = solve_session::sweep_parallelism(segs.len(), budget, pool);
        if let Some(budget) = budget {
            let mb = |bytes: u64| bytes / 1_000_000;
            if b.per_point > budget {
                eprintln!(
                    "warning: one sweep point needs about {} MB and the sweep's memory budget \
                     is {} MB; the points are solved one at a time, and may still not fit \
                     (FNEC_SWEEP_MEMORY_BUDGET_MB sets the budget; FND-187)",
                    mb(b.per_point),
                    mb(budget)
                );
            } else if b.threads < pool {
                eprintln!(
                    "info: the sweep solves {} of its points at a time, not {pool}: each holds \
                     about {} MB and the memory budget is {} MB (half the available memory; \
                     FNEC_SWEEP_MEMORY_BUDGET_MB sets it; FND-187)",
                    b.threads,
                    mb(b.per_point),
                    mb(budget)
                );
            }
        }
        b.threads
    } else {
        usize::MAX
    };
    let mut solved =
        execute_frequency_sweep(&freqs_hz, execution_mode, gpu_lane, max_parallel, solve_one);
    solved.sort_by_key(|(idx, _, _)| *idx);

    // A GPU or hybrid sweep says how many points the device actually solved:
    // each one can decline on its own (a stamp at that frequency, a rejected f32
    // answer, a lost device).
    if (execution_mode == ExecutionMode::Gpu || gpu_lane) && solved.len() > 1 {
        let on_device = solved
            .iter()
            .filter(|(_, r, _)| r.as_ref().is_ok_and(|p| p.ran_on_gpu))
            .count();
        eprintln!(
            "info: {on_device} of {} sweep points solved on the GPU",
            solved.len()
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

    // ONE copy of this loop, not two. It was 57 lines duplicated byte for byte
    // between the local and distributed sweep paths (verified: identical md5),
    // which is how a policy decision comes to be made twice and, eventually,
    // differently.
    warn_negative_resistance_for_run(&solved, deck, &segs, solver_mode);
    let any_failed = emit_sweep_points(
        solved,
        output_format,
        &mut sweep_rows,
        &mut json_records,
        enable_benchmarking,
        bench_format,
        &bench_target,
        &bench_deck,
        &bench_solver,
    );

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

    // The exit code still says the run was not clean, so a script checking the
    // status is unaffected; what changed is that a human now gets the points
    // that worked instead of nothing.
    if any_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Every pre-solve caveat a distributed run owes its user.
///
/// Produced by `validate::hallen_geometry_caveats`, the same function the local
/// path calls, so a caveat added there arrives here by construction rather than
/// by whoever remembers both call sites. That is the point: this gap existed
/// because the two paths listed the calls separately (FND-020).
///
/// Controller-side rather than on the wire, for the FND-014 reason: these are
/// pure functions of the deck, its geometry, the ground model and the frequency,
/// all of which the controller holds before it dispatches anything, and a caveat
/// computed worker-side goes silent against an older worker. Only what the
/// worker's own matrix fill actually did has to travel (FND-026).
///
/// Gated on the solver like its sibling `distributed_negative_resistance_warnings`,
/// and for the same reason: `--hosts --solver mpie` is reachable today (FND-018),
/// and the topology caveat says "re-run with `--solver mpie`" — advice that reads
/// as nonsense to someone already running it. The local path gates these three the
/// same way, so gating is also what "parity with the local path" means.
///
/// `surface_wave_modelled` is `false`: the worker derives its ground from the deck
/// and has no way to apply the Sommerfeld correction, which is the fact FND-027
/// records. That holds whether or not the `--ground-solver sommerfeld` rejection
/// stays in place, so the caveat cannot become a lie if someone removes it.
fn distributed_pre_solve_caveats(
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    ground: &nec_solver::GroundModel,
    freqs_hz: &[f64],
    solver_mode: SolverMode,
) -> Vec<String> {
    if !matches!(solver_mode, SolverMode::Hallen) {
        return Vec::new();
    }
    // The worst-case frequency choice and its annotation live in the producer, so
    // this path and the GUI sweep cannot describe the same range differently —
    // which they did, until one of them grew an affected-count the other lacked.
    nec_solver::validate::hallen_geometry_caveats_swept(
        deck,
        segs,
        ground,
        freqs_hz,
        false,
        crate::solve_session::CLI_MPIE_REMEDY,
    )
}

/// Whether a worker ran this point somewhere other than the user asked for.
///
/// The worker has always told us which path it took; the controller dropped it on
/// the floor (FND-040), so someone who passed `--exec gpu` and got a CPU solve had
/// no way to find out.
///
/// **It reports the fact and not a cause, deliberately.** The first version added
/// "that host has no usable adapter", which the controller cannot know and which
/// is often false: the worker also declines the device for a deck under
/// `nec_accel::MIN_GPU_RESIDENT_SEGS` segments, for anything but free-space or deferred ground, and for any live
/// `LD`/`TL`/`NT` stamp. PH7-CHK-004's own acceptance evidence is exactly that
/// case — a loaded deck falling back on a GPU-capable node. Asserting an adapter
/// fault there would print a wrong diagnosis, per point, on every worker of a
/// perfectly healthy GPU cluster, where the local CLI stays silent: a new
/// frontend disagreement of precisely the kind this finding was raised against.
///
/// Only on an explicit `--exec gpu`. Without the flag the startup probe inspects
/// the *controller's* adapter and can select `Gpu` by itself — irrelevant to a
/// remote host, and "the gpu you asked for" would then name a request nobody made.
///
/// Split out because the alternative is a line inside the result loop that no test
/// can reach: FND-034 was exactly that, three unreachable sends in a GUI closure,
/// and applying the lesson cost less than relearning it.
///
/// `exec_used` defaults to `"cpu"` for a worker too old to send it — accurate
/// rather than merely safe, since GPU execution and the field shipped together in
/// PH7-CHK-004, so a worker that omits it has no GPU path to report.
fn exec_fallback_warning(
    requested: ExecutionMode,
    exec_requested_explicitly: bool,
    exec_used: &str,
    label: &str,
) -> Option<String> {
    if !exec_requested_explicitly || !matches!(requested, ExecutionMode::Gpu) || exec_used == "gpu"
    {
        return None;
    }
    Some(format!(
        "worker '{label}' ran this point on {exec_used}, not the gpu you asked for"
    ))
}

/// What `exec` the controller asks each worker for (PH7-CHK-004).
///
/// A named function rather than a conditional inside `run_distributed_solve`,
/// because nothing can call that: FND-011 recorded that `--exec gpu` never
/// reached remote nodes, and by the time anyone looked it *did* — but the
/// derivation had no test, so neither the claim nor its refutation could be
/// checked. Each worker still falls back to CPU on its own if it has no adapter
/// or the deck is out of class, and reports which it used.
///
/// Only `Gpu` asks for a GPU: `Hybrid` splits work on the *controller*, which
/// says nothing about what a remote node should do with its own task.
fn worker_exec_preference(mode: ExecutionMode) -> String {
    if mode == ExecutionMode::Gpu {
        "gpu".to_string()
    } else {
        "cpu".to_string()
    }
}

/// The worker-warning lines to print, given what has already been printed.
///
/// A free function, and deduplicating, for two reasons that are the same reason.
/// A caveat that does not vary with frequency should be read once, not once per
/// point: the local CLI prints parse warnings exactly once, so echoing a worker's
/// per frequency turned one line into M+1 for a sweep. And the *deciding* lived
/// inline in the result loop, which nothing can call — the arrangement that let
/// the `Ok` arm's warnings go unread for a release (FND-026, FND-034).
///
/// Keyed on the rendered line, so the same text from two different workers is
/// still shown separately; a mixed-version pool is exactly when that matters.
fn worker_warning_lines(
    label: &str,
    warnings: &[String],
    seen: &mut std::collections::HashSet<String>,
) -> Vec<String> {
    warnings
        .iter()
        .map(|w| format!("warning: worker '{label}': {w}"))
        .filter(|line| seen.insert(line.clone()))
        .collect()
}

/// Print the run's negative-resistance caveat once — per point for a single
/// frequency, one aggregate line for a sweep (FND-069). One function for the
/// local and the distributed sweep, called just before their shared
/// `emit_sweep_points`.
fn warn_negative_resistance_for_run<T>(
    solved: &[(usize, Result<FrequencySolveResult, String>, T)],
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    solver_mode: SolverMode,
) {
    let ok: Vec<&FrequencySolveResult> = solved
        .iter()
        .filter_map(|(_, r, _)| r.as_ref().ok())
        .collect();
    let per_point = ok.iter().map(|r| r.negative_r.clone()).collect();
    let min_feed_re: Vec<Option<f64>> = ok.iter().map(|r| r.min_feed_re).collect();
    for w in solve_session::run_negative_resistance_warnings(
        per_point,
        &min_feed_re,
        deck,
        segs,
        solver_mode,
    ) {
        eprintln!("warning: {w}");
    }
}

/// The negative-resistance caveat for one distributed result, if it earns one.
///
/// Split out so it can be unit-tested without a worker: the distributed path is
/// the one frontend whose end-to-end gate needs SSH, and a check nothing can
/// exercise is how FND-014 survived in the first place.
///
/// The feedpoint tag/segment come from `nec_solver::first_delta_gap_feedpoint` —
/// the same call the worker uses to decide which segment it reported — because the
/// wire protocol does not carry them back. The controller already fabricates
/// `tag: 0, seg: 0` for `SweepPointSummary`, and a caveat naming segment 0 would
/// point at nothing.
///
/// Sharing the call is the point. This used to hand-roll the filter and a comment
/// asked the two files to be kept in step; they diverged twice inside one review
/// (FND-031).
///
/// Only `Hallen` reaches here in practice — the worker rejects any other basis —
/// but that invariant lives in another crate, so this matches on the mode rather
/// than assuming it. If a worker ever gains the MPIE, this must not go on
/// recommending `--solver mpie` to someone already running it.
fn distributed_negative_resistance_warnings(
    z_re: f64,
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    solver_mode: SolverMode,
) -> Vec<String> {
    if !matches!(solver_mode, SolverMode::Hallen)
        || !nec_solver::validate::is_negative_resistance(z_re)
    {
        return Vec::new();
    }
    // The same call the worker makes to decide which segment it reported, so the
    // caveat cannot name a different one. This was a hand-maintained mirror of the
    // worker's filter until the FND-031 seam landed; now it is the same function.
    let (tag, seg) = nec_solver::first_delta_gap_feedpoint(deck)
        .map(|ex| (ex.tag as usize, ex.segment as usize))
        .unwrap_or((0, 0));
    nec_solver::validate::negative_resistance_warning(
        z_re,
        tag,
        seg,
        deck,
        segs,
        nec_solver::validate::SolverContext::cli_hallen(),
    )
    .into_iter()
    .collect()
}

/// Distributed solve via `--hosts`.
///
/// Loads the hosts config, creates a worker pool, base64-encodes the deck, and
/// dispatches one task per frequency point.  Results are collected and emitted
/// in the same output format as the local solve path.
#[allow(clippy::too_many_arguments)]
fn run_distributed_solve(
    input: &str,
    deck: &nec_model::deck::NecDeck,
    segs: &[nec_solver::Segment],
    freqs_hz: &[f64],
    hosts_path: &std::path::Path,
    cfg: HostsConfig,
    output_format: OutputFormat,
    enable_benchmarking: bool,
    bench_format: BenchFormat,
    solver_mode: SolverMode,
    execution_mode: ExecutionMode,
    // Whether `--exec` was actually passed. Without it the startup probe can
    // select Gpu from the *controller's* adapter, which says nothing about a
    // remote host — so a fallback there is not a broken promise.
    exec_requested_explicitly: bool,
    path: &std::path::Path,
) -> ExitCode {
    // Before any connection, so a user who set a field that does nothing hears
    // about it even when the run then fails to reach a worker (FND-104).
    for line in cfg.ignored_field_warnings() {
        eprintln!("{line}");
    }
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
    let exec = worker_exec_preference(execution_mode);
    let solver_config = WorkerSolverConfig {
        basis,
        exec,
        ..WorkerSolverConfig::default()
    };

    let n = freqs_hz.len();
    let mut solved: Vec<(usize, Result<FrequencySolveResult, String>, u128)> =
        Vec::with_capacity(n);

    // Dispatch the whole sweep at once so every worker is busy: one task at a
    // time would leave N-1 workers idle and cost M x latency instead of
    // M/N x latency (review-260719 FIND-009).
    let tasks: Vec<TaskMessage> = freqs_hz
        .iter()
        .enumerate()
        .map(|(fidx, &freq_hz)| TaskMessage {
            task_id: format!("{deck_hash}-{fidx}"),
            deck_hash: deck_hash.clone(),
            deck_b64: deck_b64.clone(),
            solver_config: solver_config.clone(),
            frequency_hz: freq_hz,
        })
        .collect();

    let batch_start = Instant::now();
    let outcomes = pool.dispatch_batch(&tasks);
    // The batch overlaps, so a per-task wall time is not separable from it; charge
    // each point the mean rather than inventing a number per point.
    let elapsed_ms = (batch_start.elapsed().as_millis() / (tasks.len().max(1) as u128)).max(1);

    // A caveat that does not vary with frequency should be read once, not once
    // per point. Before FND-041 the worker sent only stamp warnings, which the
    // local CLI also re-prints per frequency, so repeating them was defensible
    // symmetry. Parse warnings broke that: the local CLI prints those exactly
    // once, so an M-point sweep of a deck with one unknown card printed M+1
    // lines where a local run printed 1 — noise this PR would have introduced.
    // Keyed on the rendered line, so the same text from *different* workers is
    // still shown separately; a mixed-version pool is exactly when that matters.
    let mut seen_worker_warnings: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for ((fidx, &freq_hz), outcome) in freqs_hz.iter().enumerate().zip(outcomes) {
        let result = match outcome {
            Ok((
                TaskResult::Ok {
                    impedance,
                    vswr_50,
                    feedpoint_current_mag,
                    feedpoint_current_phase_deg,
                    warnings,
                    exec_used,
                    ..
                },
                label,
            )) => {
                // Caveats the worker raised while filling the matrix — a skipped
                // `LD`, `TL` or `NT` card. The controller never parses the deck's
                // stamps, so these exist nowhere else (FND-026). An older worker
                // sends none, and this prints none.
                //
                for line in worker_warning_lines(&label, &warnings, &mut seen_worker_warnings) {
                    eprintln!("{line}");
                }
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
                // The worker already told us which path it took; the controller
                // used to drop it on the floor (FND-040). A user who passed
                // `--exec gpu` and got a CPU solve — because that worker has no
                // adapter — had no way to find out, while the local CLI says so
                // plainly. `exec_used` defaults to "cpu" for an old worker, so
                // this cannot invent a fallback that did not happen: the worst
                // case is an upgraded-worker run reported as CPU.
                if let Some(w) = exec_fallback_warning(
                    execution_mode,
                    exec_requested_explicitly,
                    &exec_used,
                    &label,
                ) {
                    eprintln!("warning: {w}");
                }
                let bench = BenchRecord {
                    mode: "distributed".to_string(),
                    pulse_rhs: "unknown".to_string(),
                    // Was hardcoded "ssh", which named the transport and hid the
                    // execution path — so every distributed benchmark record read
                    // the same whether the work ran on a GPU or a CPU.
                    exec: format!("ssh-{exec_used}"),
                    freq_mhz,
                    abs_res: 0.0,
                    rel_res: 0.0,
                    diag_spread: 0.0,
                    sin_rel_res: 0.0,
                };
                // FND-014, controller-side on purpose. Putting this in the worker
                // would reproduce the gap under version skew: a worker is a
                // separately installed binary, so an older one would send no
                // warning and the controller would stay silent — exactly the
                // silence this fixes. Here it covers every worker ever built, and
                // the controller already has the impedance and the deck. Printed
                // for the whole run, with the local path's (FND-069).
                let negative_r = distributed_negative_resistance_warnings(
                    impedance.re_ohm,
                    deck,
                    segs,
                    solver_mode,
                );
                let sweep_summary = Some(SweepPointSummary {
                    freq_mhz,
                    tag: 0,
                    seg: 0,
                    z_re: impedance.re_ohm,
                    z_im: impedance.im_ohm,
                    // The worker refuses the pulse bases, so its points never
                    // carry the unvalidated-solver caveat.
                    caveat: None,
                });
                Ok(FrequencySolveResult {
                    report,
                    diag_line,
                    bench,
                    sweep_summary,
                    negative_r,
                    min_feed_re: Some(impedance.re_ohm),
                    ran_on_gpu: exec_used == "gpu",
                })
            }
            Ok((
                TaskResult::Error {
                    frequency_hz,
                    error_code,
                    error_message,
                    warnings,
                    ..
                },
                label,
            )) => {
                // A refused deck can also be a flawed one, and the flaw is worth
                // reading even though the solve stopped — often it is the reason
                // (FND-059). Destructuring these with `..` is how the `Ok` arm's
                // warnings went unread for a whole release (FND-026).
                for line in worker_warning_lines(&label, &warnings, &mut seen_worker_warnings) {
                    eprintln!("{line}");
                }
                Err(format!(
                    "worker '{label}' failed at {frequency_hz} Hz: {error_code:?} — {error_message}"
                ))
            }
            Err(e) => Err(e),
        };
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

    // ONE copy of this loop, not two. It was 57 lines duplicated byte for byte
    // between the local and distributed sweep paths (verified: identical md5),
    // which is how a policy decision comes to be made twice and, eventually,
    // differently.
    warn_negative_resistance_for_run(&solved, deck, segs, solver_mode);
    let any_failed = emit_sweep_points(
        solved,
        output_format,
        &mut sweep_rows,
        &mut json_records,
        enable_benchmarking,
        bench_format,
        &bench_target,
        &bench_deck,
        &bench_solver,
    );

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

    // The exit code still says the run was not clean, so a script checking the
    // status is unaffected; what changed is that a human now gets the points
    // that worked instead of nothing.
    if any_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
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

/// Entry point for `fnec taper --sections "<dia>,<len> …"` — the Leeson
/// step-tapered-radius correction. Prints the equivalent uniform element.
fn run_taper_subcommand(args: &[String]) -> ExitCode {
    const TAPER_USAGE: &str = "Usage: fnec taper --sections \"<dia1>,<len1> <dia2>,<len2> ...\"\n\
         Sections run from the element centre outward (diameter,length pairs,\n\
         one consistent unit). Prints the Leeson equivalent uniform element.";

    let mut sections_arg: Option<String> = None;
    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            "--sections" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("fnec {}\n{TAPER_USAGE}", env!("CARGO_PKG_VERSION"));
                    eprintln!("error: missing value after --sections");
                    return ExitCode::from(2);
                }
                sections_arg = Some(args[i].clone());
            }
            other => {
                eprintln!("fnec {}\n{TAPER_USAGE}", env!("CARGO_PKG_VERSION"));
                eprintln!("error: unknown taper option: {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let Some(spec) = sections_arg else {
        eprintln!("fnec {}\n{TAPER_USAGE}", env!("CARGO_PKG_VERSION"));
        eprintln!("error: --sections is required");
        return ExitCode::from(2);
    };

    let mut sections = Vec::new();
    for tok in spec.split_whitespace() {
        let parts: Vec<&str> = tok.split(',').collect();
        if parts.len() != 2 {
            eprintln!("error: bad section '{tok}' (expected diameter,length)");
            return ExitCode::from(2);
        }
        let (Ok(d), Ok(l)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) else {
            eprintln!("error: non-numeric section '{tok}'");
            return ExitCode::from(2);
        };
        sections.push(nec_solver::TaperSection {
            radius: d / 2.0,
            length: l,
        });
    }

    match nec_solver::leeson_equivalent_element(&sections) {
        Ok(e) => {
            let phys: f64 = sections.iter().map(|s| s.length).sum();
            println!("TAPER_EQUIVALENT_ELEMENT");
            println!("SECTIONS {}", sections.len());
            println!("PHYS_HALF_LENGTH {phys:.6}");
            println!("EQUIV_HALF_LENGTH {:.6}", e.half_length);
            println!("EQUIV_FULL_LENGTH {:.6}", 2.0 * e.half_length);
            println!("EQUIV_RADIUS {:.6}", e.radius);
            println!("EQUIV_DIAMETER {:.6}", 2.0 * e.radius);
            println!("KA {:.3}", e.k_a);
            println!("Z0 {:.3}", e.z0);
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
    }
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
    // The caveats describe the geometry, so they are printed once, on the first
    // probe, not once per bisection step.
    let caveats_shown = std::cell::Cell::new(false);
    let probe = |val: f64| -> Result<(f64, f64), String> {
        let mut vars = std::collections::HashMap::new();
        vars.insert(cfg.var.clone(), format!("{val:.9}"));
        let deck_str =
            nec_parser::template::substitute(&template, &vars).map_err(|e| e.to_string())?;
        let result = parse(&deck_str).map_err(|e| e.to_string())?;
        let deck = &result.deck;

        let segs = build_geometry(deck).map_err(|e| e.to_string())?;
        let ground = ground_model_from_deck(deck);
        // Every refusal the other frontends make, in the order the main CLI makes
        // them. This route called none of them, so an unsupported load solved
        // without the load and a negative frequency converged (FND-173).
        if let Some(err) = nec_solver::validate::pre_solve_error(deck, &segs, &ground) {
            return Err(err);
        }
        let v_vec = build_excitation(deck, &segs).map_err(|e| e.to_string())?;
        let wire_endpoints = wire_endpoints_from_segs(&segs);
        let per_wire_basis_feasible = wire_endpoints.iter().all(|&(first, last)| last > first);

        // Find the single FR frequency from the deck.
        let freqs = frequencies_from_fr(deck);
        let freq_hz = freqs.first().copied().ok_or_else(|| {
            // Same sentence as every other frontend (FND-070). The remedy is
            // narrower here: a resonance search substitutes into a template
            // deck, so `--sweep-config` is not the answer.
            nec_solver::validate::no_frequency_error(
                &freqs,
                "Add an `FR` card to the template deck.",
            )
            .unwrap_or_else(|| "resonance search: no frequency".to_string())
        })?;

        if !caveats_shown.replace(true) {
            for w in nec_solver::validate::hallen_geometry_caveats(
                deck,
                &segs,
                &ground,
                freq_hz,
                false,
                solve_session::CLI_MPIE_REMEDY,
            ) {
                eprintln!("warning: {w}");
            }
        }

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
            GroundSolver::Rcm,
            &[], // Laplace loads apply to the normal solve path, not `sweep --resonance`.
        )?;

        // A probe is not a sweep point: each one still says so itself, as
        // before the sweep caveat was aggregated (FND-069).
        for w in &solve_result.negative_r {
            eprintln!("warning: {w}");
        }
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
    use super::solve_session::{
        negative_resistance_warnings, run_negative_resistance_warnings, SolverMode,
    };
    use super::{
        auto_select_execution_mode, detect_compatibility_profile,
        distributed_negative_resistance_warnings, distributed_pre_solve_caveats,
        exec_fallback_warning, steer_execution_mode_by_profile, worker_exec_preference,
        worker_warning_lines, CompatibilityProfile, ExecutionMode,
    };
    use nec_report::FeedpointRow;
    use num_complex::Complex64;

    // The distributed path is the one frontend whose end-to-end gate needs SSH.
    // Testing the mapping directly is what makes FND-014's fix verifiable here at
    // all — an unexercisable check is how the gap survived.
    fn deck_and_segs(src: &str) -> (nec_model::deck::NecDeck, Vec<nec_solver::Segment>) {
        let deck = nec_parser::parse(src).expect("parse").deck;
        let segs = nec_solver::build_geometry(&deck).expect("geometry");
        (deck, segs)
    }

    const BENT: &str = "GW 1 21 -5.0 0 0.0 0.0 0 3.0 0.001\nGW 2 21 0.0 0 3.0 5.0 0 0.0 0.001\nGE 0\nEX 0 1 5 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    fn row(z_re: f64) -> FeedpointRow {
        FeedpointRow {
            tag: 1,
            seg: 5,
            v_source: Complex64::new(1.0, 0.0),
            current: Complex64::new(1.0, 0.0),
            z_in: Complex64::new(z_re, -1122.0),
        }
    }

    /// FND-069: a sweep reports negative resistance once, counting the points;
    /// a single frequency keeps the per-point sentence that names the segment.
    #[test]
    fn a_sweep_reports_negative_resistance_once() {
        let (deck, segs) = deck_and_segs(BENT);
        let z = [-5.9, 12.0, -3.1];
        let per_point: Vec<Vec<String>> = z
            .iter()
            .map(|&r| negative_resistance_warnings(&[row(r)], &deck, &segs, SolverMode::Hallen))
            .collect();
        let mins: Vec<Option<f64>> = z.iter().map(|&r| Some(r)).collect();
        let w =
            run_negative_resistance_warnings(per_point, &mins, &deck, &segs, SolverMode::Hallen);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].starts_with("2 of 3 sweep points"), "{w:?}");

        let one = vec![negative_resistance_warnings(
            &[row(-5.9)],
            &deck,
            &segs,
            SolverMode::Hallen,
        )];
        let w =
            run_negative_resistance_warnings(one, &[Some(-5.9)], &deck, &segs, SolverMode::Hallen);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("tag 1 segment 5"), "{w:?}");

        // The pulse bases keep their own wording in the aggregate too.
        let per_point: Vec<Vec<String>> = z
            .iter()
            .map(|&r| negative_resistance_warnings(&[row(r)], &deck, &segs, SolverMode::Pulse))
            .collect();
        let w = run_negative_resistance_warnings(per_point, &mins, &deck, &segs, SolverMode::Pulse);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("unvalidated solver"), "{w:?}");

        // Nothing negative, nothing said.
        let clean: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
        let w = run_negative_resistance_warnings(
            clean,
            &[Some(70.0), Some(71.0)],
            &deck,
            &segs,
            SolverMode::Hallen,
        );
        assert!(w.is_empty(), "{w:?}");
    }

    /// FND-148: `SolverMode::ALL` holds every variant exactly once. The match
    /// is exhaustive, so a new variant is a compile error here until it has a
    /// position — and then this fails until `ALL` lists it in that position.
    #[test]
    fn every_solver_mode_is_listed_once() {
        fn position(m: SolverMode) -> usize {
            match m {
                SolverMode::Hallen => 0,
                SolverMode::Pulse => 1,
                SolverMode::Continuity => 2,
                SolverMode::Sinusoidal => 3,
                SolverMode::Mpie => 4,
            }
        }
        for (i, m) in SolverMode::ALL.iter().enumerate() {
            assert_eq!(position(*m), i, "{m:?} is out of place in ALL");
        }
    }

    /// The usage line is a literal; it must list exactly the modes `--solver`
    /// accepts, in the same order as the error messages (FND-148: it did not).
    #[test]
    fn the_usage_line_lists_every_solver_mode() {
        let want = format!("--solver <{}>", SolverMode::flag_alternation());
        assert!(super::cli_args::USAGE.contains(&want), "USAGE lacks {want}");
        for m in SolverMode::ALL {
            assert_eq!(SolverMode::from_flag(m.as_flag()), Some(m));
        }
        assert_eq!(SolverMode::from_flag("nec4"), None);
    }

    #[test]
    fn the_mpie_arm_blames_the_solver_rather_than_the_geometry() {
        // The MPIE models junctions correctly, so a junction is never the reason —
        // and this deck HAS one, which is what makes the assertion meaningful.
        // Nothing covered this arm before: deleting it failed no test, and the
        // shared-predicate sabotage cannot reach it.
        let (deck, segs) = deck_and_segs(BENT);
        let w = negative_resistance_warnings(&[row(-5.973)], &deck, &segs, SolverMode::Mpie);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("report it as a solver defect"), "{}", w[0]);
        assert!(
            !w[0].contains("PH9-CHK-002"),
            "must not offer the junction cause on the solver that handles junctions: {}",
            w[0]
        );
        // Same sentence as the Hallén arm, composed rather than hand-copied.
        assert!(
            w[0].contains("has negative resistance (Re Z = -5.973 Ω)"),
            "{}",
            w[0]
        );
    }

    /// FND-081: these modes were silent, sinusoidal included. A negative
    /// resistance is physically impossible whatever produced it.
    #[test]
    fn every_basis_reports_a_negative_resistance() {
        let (deck, segs) = deck_and_segs(BENT);
        let sin =
            negative_resistance_warnings(&[row(-5.973)], &deck, &segs, SolverMode::Sinusoidal);
        assert_eq!(sin.len(), 1, "{sin:?}");
        assert!(sin[0].contains("-5.973"), "{}", sin[0]);
        for mode in [SolverMode::Pulse, SolverMode::Continuity] {
            let w = negative_resistance_warnings(&[row(-5.973)], &deck, &segs, mode);
            assert_eq!(w.len(), 1, "{mode:?}: {w:?}");
            assert!(w[0].contains("unvalidated solver"), "{mode:?}: {}", w[0]);
        }
        // Negative control: a physical resistance earns nothing.
        for mode in [SolverMode::Sinusoidal, SolverMode::Pulse] {
            assert!(negative_resistance_warnings(&[row(73.0)], &deck, &segs, mode).is_empty());
        }
    }

    // A dipole 0.03 λ over GN 2 — low enough to trip the near-ground caveat — whose
    // two wires meet at a T, so the feed also sits on a junction. It earns three
    // separate pre-solve caveats, which is what makes it useful: a deck earning one
    // cannot tell a complete set from a lucky one. The stem is one segment long,
    // which keeps it off the section-graph solve (a one-segment run, measured and
    // kept refused in FND-162 stage 5); a T the graph takes earns neither the
    // topology nor the junction-feed caveat.
    const LOW_TEE: &str = "GW 1 13 0 0 0.634 5.282 0 0.634 0.001\nGW 2 13 0 0 0.634 -5.282 0 0.634 0.001\nGW 3 1 0 0 0.634 0 0 1.134 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    #[test]
    fn the_distributed_caveats_come_from_the_shared_producer() {
        // Asserts the distributed path's set IS the shared producer's, rather than
        // re-listing the three calls in the test body. That earlier oracle was
        // circular in the way that mattered: a fourth caveat added to
        // `hallen_geometry_caveats` would have left both sides unchanged and the
        // test green while parity broke. Comparing against the producer makes the
        // fourth caveat arrive on both sides by construction.
        let (deck, segs) = deck_and_segs(LOW_TEE);
        let ground = nec_solver::ground_model_from_deck(&deck);

        let produced = nec_solver::validate::hallen_geometry_caveats(
            &deck,
            &segs,
            &ground,
            14.2e6,
            false,
            crate::solve_session::CLI_MPIE_REMEDY,
        );
        assert!(
            produced.len() >= 3,
            "fixture must earn several caveats or this proves little: {produced:?}"
        );

        let distributed =
            distributed_pre_solve_caveats(&deck, &segs, &ground, &[14.2e6], SolverMode::Hallen);
        assert_eq!(
            distributed, produced,
            "a single-frequency distributed run must emit exactly the shared set"
        );
    }

    #[test]
    fn a_non_hallen_distributed_run_gets_no_hallen_caveats() {
        // The local path skips all three under MPIE, which models junctions, loops
        // and the surface wave correctly. `--hosts --solver mpie` is reachable
        // (FND-018), and the topology caveat says "re-run with `--solver mpie`" —
        // nonsense to someone already running it. Parity with the local path means
        // gating the same way, not emitting unconditionally.
        let (deck, segs) = deck_and_segs(LOW_TEE);
        let ground = nec_solver::ground_model_from_deck(&deck);
        assert!(
            !distributed_pre_solve_caveats(&deck, &segs, &ground, &[14.2e6], SolverMode::Hallen)
                .is_empty(),
            "fixture must earn caveats on the Hallén path"
        );
        assert!(
            distributed_pre_solve_caveats(&deck, &segs, &ground, &[14.2e6], SolverMode::Mpie)
                .is_empty(),
            "the MPIE solves all three correctly; the caveats do not apply"
        );
    }

    /// FND-011: `--exec gpu` must reach the workers, or GPU-capable remote nodes
    /// never use their GPU. It does — PH7-CHK-004 wired it — but the derivation
    /// sat inside `run_distributed_solve`, which no test can call, so the finding
    /// stood open long after the code it described had changed.
    #[test]
    fn a_gpu_run_asks_its_workers_for_the_gpu() {
        assert_eq!(worker_exec_preference(ExecutionMode::Gpu), "gpu");
    }

    /// ...and nothing else does. `Hybrid` splits work on the *controller*, which
    /// says nothing about what a remote node should do with its own task, so
    /// asking every worker for a GPU would be a claim the flag never made.
    #[test]
    fn only_a_gpu_run_asks_its_workers_for_the_gpu() {
        for mode in [ExecutionMode::Cpu, ExecutionMode::Hybrid] {
            assert_eq!(worker_exec_preference(mode), "cpu", "{mode:?}");
        }
    }

    /// FND-041's second-order defect: the local CLI prints a parse warning once,
    /// so echoing the worker's copy per frequency turned 1 line into M+1 for an
    /// M-point sweep.
    #[test]
    fn a_repeated_worker_caveat_is_printed_once_per_sweep() {
        let mut seen = std::collections::HashSet::new();
        let w = vec!["line 5: unknown card 'ZZ'".to_string()];
        let first = worker_warning_lines("ssh:hostA", &w, &mut seen);
        assert_eq!(first.len(), 1, "the first point must print it");
        assert!(first[0].contains("ssh:hostA") && first[0].contains("ZZ"));
        assert!(
            worker_warning_lines("ssh:hostA", &w, &mut seen).is_empty(),
            "the second frequency point must not repeat it"
        );
    }

    /// ...but the same text from a *different* worker is its own fact. A
    /// mixed-version pool is exactly when that distinction matters, so deduping
    /// on the message alone would hide which host disagreed.
    #[test]
    fn the_same_caveat_from_another_worker_is_still_shown() {
        let mut seen = std::collections::HashSet::new();
        let w = vec!["line 5: unknown card 'ZZ'".to_string()];
        assert_eq!(worker_warning_lines("ssh:hostA", &w, &mut seen).len(), 1);
        let other = worker_warning_lines("ssh:hostB", &w, &mut seen);
        assert_eq!(other.len(), 1, "hostB's copy is a separate fact");
        assert!(other[0].contains("ssh:hostB"));
    }

    #[test]
    fn a_worker_with_no_caveats_prints_nothing() {
        let mut seen = std::collections::HashSet::new();
        assert!(worker_warning_lines("ssh:hostA", &[], &mut seen).is_empty());
    }

    #[test]
    fn a_worker_that_fell_back_to_cpu_says_so() {
        // FND-040. `--exec gpu` against a host that did not use one produced a CPU
        // solve and total silence, while the local CLI warns.
        let w = exec_fallback_warning(ExecutionMode::Gpu, true, "cpu", "node-2").expect("warning");
        assert!(w.contains("node-2"), "{w}");
        assert!(w.contains("not the gpu you asked for"), "{w}");

        // It must not assert WHY. The worker also declines the device for a small
        // deck, for non-free-space ground, and for any live LD/TL/NT stamp — so
        // "that host has no usable adapter", which the first version said, is
        // false on a healthy GPU cluster running a loaded deck, and the local CLI
        // is silent in exactly that case.
        assert!(
            !w.contains("adapter"),
            "the controller cannot know the cause: {w}"
        );

        // Got what was asked for: nothing to say.
        assert_eq!(
            exec_fallback_warning(ExecutionMode::Gpu, true, "gpu", "node-2"),
            None
        );

        // Never asked for gpu: a cpu run is not a fallback. Without this the
        // warning fires on every ordinary distributed run.
        assert_eq!(
            exec_fallback_warning(ExecutionMode::Cpu, true, "cpu", "node-2"),
            None
        );
        assert_eq!(
            exec_fallback_warning(ExecutionMode::Hybrid, true, "cpu", "node-2"),
            None
        );

        // And `--exec` never passed: the startup probe can select Gpu from the
        // CONTROLLER's adapter, which says nothing about a remote host. Warning
        // there would name a request the user never made.
        assert_eq!(
            exec_fallback_warning(ExecutionMode::Gpu, false, "cpu", "node-2"),
            None
        );
    }

    #[test]
    fn a_clean_deck_earns_no_distributed_caveats() {
        let (deck, segs) = deck_and_segs(
            "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let ground = nec_solver::ground_model_from_deck(&deck);
        assert!(distributed_pre_solve_caveats(
            &deck,
            &segs,
            &ground,
            &[14.2e6],
            SolverMode::Hallen
        )
        .is_empty());
    }

    #[test]
    fn a_partly_low_sweep_says_how_many_points_it_affects() {
        // Raising the frequency shrinks lambda, so a fixed height stops being
        // "low". A sweep straddling the 0.1 lambda threshold must not imply the
        // caveat applies to every point.
        let (deck, segs) = deck_and_segs(LOW_TEE);
        let ground = nec_solver::ground_model_from_deck(&deck);
        let freqs = [14.2e6, 30.0e6, 60.0e6];
        let tripping = freqs
            .iter()
            .filter(|f| {
                nec_solver::validate::low_finite_ground_warning(&segs, &ground, **f, false)
                    .is_some()
            })
            .count();
        assert!(
            tripping > 0 && tripping < freqs.len(),
            "fixture must straddle the threshold, got {tripping}/{}",
            freqs.len()
        );
        let out = distributed_pre_solve_caveats(&deck, &segs, &ground, &freqs, SolverMode::Hallen);
        assert!(
            out.iter()
                .any(|w| w.contains(&format!("{tripping} of {} swept frequencies", freqs.len()))),
            "must say how many points are affected: {out:?}"
        );
    }

    #[test]
    fn the_low_ground_check_uses_the_worst_case_frequency_not_the_first() {
        // The caveat trips below 0.1 lambda, so the LOWEST frequency is the worst
        // case. A sweep whose lowest point is not its first would be missed
        // entirely by anything that just looked at `freqs_hz[0]` — and an
        // ascending fixture cannot tell the two apart, which my first attempt at
        // this test did not.
        let (deck, segs) = deck_and_segs(LOW_TEE);
        let ground = nec_solver::ground_model_from_deck(&deck);

        let descending = [60.0e6, 30.0e6, 14.2e6];
        assert_eq!(
            nec_solver::validate::low_finite_ground_warning(&segs, &ground, 60.0e6, false),
            None,
            "fixture must NOT trip at its first frequency"
        );
        assert!(
            nec_solver::validate::low_finite_ground_warning(&segs, &ground, 14.2e6, false)
                .is_some(),
            "fixture must trip at its lowest frequency"
        );

        let out =
            distributed_pre_solve_caveats(&deck, &segs, &ground, &descending, SolverMode::Hallen);
        assert!(
            out.iter().any(|w| w.contains("above finite ground")),
            "the low-ground caveat must survive a descending sweep: {out:?}"
        );
    }

    #[test]
    fn a_negative_distributed_result_earns_a_caveat_naming_the_real_feedpoint() {
        let (deck, segs) = deck_and_segs(BENT);
        let w = distributed_negative_resistance_warnings(-5.973, &deck, &segs, SolverMode::Hallen);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("negative resistance"), "{}", w[0]);
        assert!(w[0].contains("PH9-CHK-002"), "{}", w[0]);
        // The wire protocol does not return tag/seg, and the controller fabricates
        // 0/0 for the sweep summary. A caveat pointing at segment 0 would name
        // nothing, so it resolves the real feedpoint from the deck's EX card.
        assert!(
            w[0].contains("tag 1 segment 5"),
            "must name the real feedpoint, not the fabricated 0/0: {}",
            w[0]
        );
    }

    #[test]
    fn a_positive_distributed_result_earns_nothing() {
        let (deck, segs) = deck_and_segs(BENT);
        assert!(
            distributed_negative_resistance_warnings(74.24, &deck, &segs, SolverMode::Hallen)
                .is_empty()
        );
    }

    #[test]
    fn the_caveat_names_the_same_feedpoint_the_worker_reported() {
        // This deck has an `EX 5` before its `EX 0`. Before FND-031 the answer
        // depended on which file you asked: the worker skipped type 5 entirely
        // (and rejected a type-5-only deck outright), while the CLI's local path
        // took it. Both now call `first_delta_gap_feedpoint`, so the caveat names
        // the segment the reported impedance actually came from — the first
        // delta-gap source in deck order, here the `EX 5` on tag 2 segment 3.
        let (deck, segs) = deck_and_segs(
            "GW 1 21 -5.0 0 0.0 0.0 0 3.0 0.001\nGW 2 21 0.0 0 3.0 5.0 0 0.0 0.001\nGE 0\nEX 5 2 3 0 1.0 0.0\nEX 0 1 5 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let w = distributed_negative_resistance_warnings(-5.973, &deck, &segs, SolverMode::Hallen);
        assert_eq!(w.len(), 1, "{w:?}");
        let expected = nec_solver::first_delta_gap_feedpoint(&deck).expect("a delta-gap feedpoint");
        assert!(
            w[0].contains(&format!(
                "tag {} segment {}",
                expected.tag, expected.segment
            )),
            "must name the shared seam's answer: {}",
            w[0]
        );
        assert!(w[0].contains("tag 2 segment 3"), "{}", w[0]);
    }

    #[test]
    fn a_plane_wave_ex_is_not_mistaken_for_the_feedpoint() {
        // A plane-wave EX carries NTHETA/NPHI in the fields a voltage source uses
        // for tag and segment. Taking the first EX of any type would name a "tag"
        // and "segment" that are grid dimensions — the CLI's local path skips them
        // for exactly this reason.
        let (deck, segs) = deck_and_segs(
            "GW 1 21 -5.0 0 0.0 0.0 0 3.0 0.001\nGW 2 21 0.0 0 3.0 5.0 0 0.0 0.001\nGE 0\nEX 1 7 9 0 0.0 0.0\nEX 0 1 5 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let w = distributed_negative_resistance_warnings(-5.973, &deck, &segs, SolverMode::Hallen);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(
            w[0].contains("tag 1 segment 5"),
            "must name the voltage source, not the plane wave's NTHETA/NPHI: {}",
            w[0]
        );
    }

    #[test]
    fn a_non_hallen_distributed_run_gets_no_hallen_diagnosis() {
        // Only Hallén reaches the worker today, but that invariant lives in another
        // crate. If a worker ever gains the MPIE, this must not go on telling
        // someone already running it to cross-check with `--solver mpie`.
        let (deck, segs) = deck_and_segs(BENT);
        assert!(
            distributed_negative_resistance_warnings(-5.973, &deck, &segs, SolverMode::Mpie)
                .is_empty()
        );
    }

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

    /// The pick's boundaries, with the GPU's capacity injected: 499 / 500 for one
    /// point, 549 / 550 for a sweep. Never `Hybrid`.
    #[test]
    fn auto_pick_crosses_over_at_the_measured_sizes() {
        let gpu = || Some(16_384);
        let pick = |n: usize, points: usize| {
            auto_select_execution_mode(n, n + 2, points, Ok(()), gpu).mode
        };
        assert_eq!(pick(499, 1), ExecutionMode::Cpu);
        assert_eq!(pick(500, 1), ExecutionMode::Gpu);
        assert_eq!(pick(549, 24), ExecutionMode::Cpu);
        assert_eq!(pick(550, 24), ExecutionMode::Gpu);
        // A sweep below its own crossover stays on the CPU even above the
        // single-point one.
        assert_eq!(pick(520, 2), ExecutionMode::Cpu);
    }

    #[test]
    fn auto_pick_stays_on_the_cpu_without_a_gpu_or_a_deck_it_solves() {
        let none = auto_select_execution_mode(2000, 2002, 1, Ok(()), || None);
        assert_eq!(none.mode, ExecutionMode::Cpu);
        assert!(none.reason.contains("no hardware GPU"), "{}", none.reason);

        let full = auto_select_execution_mode(2000, 2002, 1, Ok(()), || Some(1000));
        assert_eq!(full.mode, ExecutionMode::Cpu);
        assert!(
            full.reason.contains("more than the GPU holds"),
            "{}",
            full.reason
        );

        let class = auto_select_execution_mode(2000, 2002, 1, Err("a bend"), || {
            panic!("an ineligible deck must not touch the GPU")
        });
        assert_eq!(class.mode, ExecutionMode::Cpu);
        assert!(class.reason.contains("a bend"), "{}", class.reason);

        // Below the crossover the device is not even asked about.
        let small = auto_select_execution_mode(51, 53, 1, Ok(()), || {
            panic!("a small deck must not touch the GPU")
        });
        assert_eq!(small.mode, ExecutionMode::Cpu);
    }
}
