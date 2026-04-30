// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

mod cli_args;
mod exec_profile;
mod solve_session;

use cli_args::{parse_args, ParsedArgs, USAGE};
use exec_profile::{
    auto_select_execution_mode, detect_compatibility_profile, startup_execution_probe,
    steer_execution_mode_by_profile, warn_compatibility_profile, CompatibilityProfile,
};
use nec_accel::{
    dispatch_frequency_point, execute_frequency_point, AccelRequestKind, DispatchDecision,
    ExecutionPath,
};
use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::{
    build_excitation_with_options, build_geometry, ground_model_from_deck, rp_card_points,
    wire_endpoints_from_segs, Ex3NormalizationMode, FarFieldPoint, GroundModel,
};
use rayon::prelude::*;
use solve_session::{
    build_hybrid_lane_plan, frequencies_from_fr, solve_frequency_point, FrequencySolveResult,
    SweepPointSummary,
};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn segment_intersection_error(segs: &[nec_solver::Segment]) -> Option<String> {
    const TOUCH_EPS: f64 = 1.0e-9;
    const CROSS_EPS: f64 = 1.0e-7;
    const INTERIOR_EPS: f64 = 1.0e-6;

    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            let a = &segs[i];
            let b = &segs[j];

            // Ignore same-wire neighboring segments and endpoint junctions.
            if a.tag == b.tag {
                continue;
            }
            if segments_share_endpoint(a, b, TOUCH_EPS) {
                continue;
            }

            let (dist, s, t) = segment_closest_distance_and_params(a.start, a.end, b.start, b.end);
            let a_interior = s > INTERIOR_EPS && s < 1.0 - INTERIOR_EPS;
            let b_interior = t > INTERIOR_EPS && t < 1.0 - INTERIOR_EPS;
            if dist <= CROSS_EPS && a_interior && b_interior {
                return Some(format!(
                    "unsupported intersecting-wire geometry between tag {} seg {} and tag {} seg {}; only endpoint junctions are currently supported",
                    a.tag, a.tag_index, b.tag, b.tag_index
                ));
            }
        }
    }

    None
}

fn source_risk_geometry_error(cards: &[Card], segs: &[nec_solver::Segment]) -> Option<String> {
    const MIN_SOURCE_LENGTH_TO_RADIUS_RATIO: f64 = 2.0;

    for card in cards {
        if let Card::Ex(ex) = card {
            let Some(seg) = segs
                .iter()
                .find(|s| s.tag == ex.tag && s.tag_index == ex.segment)
            else {
                continue;
            };

            if seg.radius <= 0.0 {
                continue;
            }

            let length_to_radius = seg.length / seg.radius;
            if length_to_radius < MIN_SOURCE_LENGTH_TO_RADIUS_RATIO {
                return Some(format!(
                    "unsupported source-risk geometry: EX on tiny segment tag {} seg {} (length={:.6e} m, radius={:.6e} m, L/r={:.3}). Increase segment length or reduce wire radius; tiny-loop/source-risk classes are deferred",
                    ex.tag,
                    ex.segment,
                    seg.length,
                    seg.radius,
                    length_to_radius,
                ));
            }
        }
    }

    None
}

fn buried_wire_geometry_error(
    segs: &[nec_solver::Segment],
    ground: &GroundModel,
) -> Option<String> {
    const BURIED_Z_EPS: f64 = 1.0e-9;

    // PH2-CHK-002 guardrail: buried-wire handling is not yet supported for
    // active image/finite-ground paths. Keep deferred/free-space behavior
    // unchanged so existing deferred contracts remain stable.
    if !matches!(
        ground,
        GroundModel::PerfectConductor | GroundModel::SimpleFiniteGround { .. }
    ) {
        return None;
    }

    for seg in segs {
        if seg.start[2] < -BURIED_Z_EPS || seg.end[2] < -BURIED_Z_EPS {
            return Some(format!(
                "unsupported buried-wire geometry for active ground model on tag {} seg {} (z < 0). Use free-space or move geometry to z >= 0; buried/near-ground classes are deferred",
                seg.tag, seg.tag_index
            ));
        }
    }

    None
}

fn segments_share_endpoint(a: &nec_solver::Segment, b: &nec_solver::Segment, eps: f64) -> bool {
    points_close(a.start, b.start, eps)
        || points_close(a.start, b.end, eps)
        || points_close(a.end, b.start, eps)
        || points_close(a.end, b.end, eps)
}

fn segment_closest_distance_and_params(
    p1: [f64; 3],
    q1: [f64; 3],
    p2: [f64; 3],
    q2: [f64; 3],
) -> (f64, f64, f64) {
    const SMALL_NUM: f64 = 1.0e-12;

    let u = [q1[0] - p1[0], q1[1] - p1[1], q1[2] - p1[2]];
    let v = [q2[0] - p2[0], q2[1] - p2[1], q2[2] - p2[2]];
    let w = [p1[0] - p2[0], p1[1] - p2[1], p1[2] - p2[2]];

    let a = dot3(u, u);
    let b = dot3(u, v);
    let c = dot3(v, v);
    let d = dot3(u, w);
    let e = dot3(v, w);
    let mut s_d = a * c - b * b;
    let mut t_d = s_d;

    let mut s_n;
    let mut t_n;

    if s_d < SMALL_NUM {
        s_n = 0.0;
        s_d = 1.0;
        t_n = e;
        t_d = c;
    } else {
        s_n = b * e - c * d;
        t_n = a * e - b * d;

        if s_n < 0.0 {
            s_n = 0.0;
            t_n = e;
            t_d = c;
        } else if s_n > s_d {
            s_n = s_d;
            t_n = e + b;
            t_d = c;
        }
    }

    if t_n < 0.0 {
        t_n = 0.0;
        if -d < 0.0 {
            s_n = 0.0;
        } else if -d > a {
            s_n = s_d;
        } else {
            s_n = -d;
            s_d = a;
        }
    } else if t_n > t_d {
        t_n = t_d;
        if -d + b < 0.0 {
            s_n = 0.0;
        } else if -d + b > a {
            s_n = s_d;
        } else {
            s_n = -d + b;
            s_d = a;
        }
    }

    let s_c = if s_n.abs() < SMALL_NUM {
        0.0
    } else {
        s_n / s_d
    };
    let t_c = if t_n.abs() < SMALL_NUM {
        0.0
    } else {
        t_n / t_d
    };

    let dx = w[0] + s_c * u[0] - t_c * v[0];
    let dy = w[1] + s_c * u[1] - t_c * v[1];
    let dz = w[2] + s_c * u[2] - t_c * v[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    (dist, s_c, t_c)
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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
    let GroundModel::Deferred {
        gn_type,
        eps_r,
        sigma,
    } = ground
    else {
        return;
    };
    let params = match (eps_r, sigma) {
        (Some(e), Some(s)) => format!(" [parsed: EPSE={e}, SIG={s} S/m]"),
        (Some(e), None) => format!(" [parsed: EPSE={e}]"),
        (None, Some(s)) => format!(" [parsed: SIG={s} S/m]"),
        (None, None) => String::new(),
    };
    eprintln!(
        "warning: GN type {gn_type} is not yet supported; treating this deck as free-space{params}"
    );
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

fn warn_ex_type3_normalization_semantics(deck: &nec_model::deck::NecDeck, ex3_i4_mode: Ex3I4Mode) {
    let has_non_default_i4 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 3 && ex.i4 != 0
        } else {
            false
        }
    });

    let has_ex_type3 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 3
        } else {
            false
        }
    });

    if has_ex_type3 && matches!(ex3_i4_mode, Ex3I4Mode::DivideByI4) {
        eprintln!(
            "warning: --ex3-i4-mode=divide-by-i4 enables experimental EX type 3 normalization semantics (I4 divisor when I4>0)"
        );
    }

    if has_non_default_i4 && matches!(ex3_i4_mode, Ex3I4Mode::Legacy) {
        eprintln!(
            "warning: EX type 3 with non-default I4 is currently treated like EX type 0; full normalization semantics are pending"
        );
    }
}

fn warn_ex_type1_portability_semantics(deck: &nec_model::deck::NecDeck, solver_mode: SolverMode) {
    let has_ex_type1 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 1
        } else {
            false
        }
    });

    if has_ex_type1 && !matches!(solver_mode, SolverMode::Pulse) {
        eprintln!(
            "warning: EX type 1 is currently treated like EX type 0; current-source semantics are pending"
        );
    }
}

fn warn_ex_type2_portability_semantics(deck: &nec_model::deck::NecDeck) {
    let has_ex_type2 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 2
        } else {
            false
        }
    });

    if has_ex_type2 {
        eprintln!(
            "warning: EX type 2 is currently treated like EX type 0; incident-plane-wave semantics are pending"
        );
    }
}

fn warn_ex_type4_portability_semantics(deck: &nec_model::deck::NecDeck, solver_mode: SolverMode) {
    let has_ex_type4 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 4
        } else {
            false
        }
    });

    if has_ex_type4 && !matches!(solver_mode, SolverMode::Pulse) {
        eprintln!(
            "warning: EX type 4 is currently treated like EX type 0; segment-current semantics are pending"
        );
    }
}

fn warn_ex_type5_portability_semantics(deck: &nec_model::deck::NecDeck, solver_mode: SolverMode) {
    let has_ex_type5 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 5
        } else {
            false
        }
    });

    if has_ex_type5 && !matches!(solver_mode, SolverMode::Pulse) {
        eprintln!(
            "warning: EX type 5 is currently treated like EX type 0; qdsrc semantics are pending"
        );
    }
}

fn warn_pt_card_deferred_support(deck: &nec_model::deck::NecDeck) {
    let has_pt = deck.cards.iter().any(|c| matches!(c, Card::Pt(_)));
    if has_pt {
        eprintln!(
            "warning: PT card support is currently deferred; PT cards are parsed for portability but ignored at runtime"
        );
    }
}

fn warn_nt_card_deferred_support(deck: &nec_model::deck::NecDeck) {
    let has_nt = deck.cards.iter().any(|c| matches!(c, Card::Nt(_)));
    if has_nt {
        eprintln!(
            "warning: NT card support is currently deferred; NT cards are parsed for portability but ignored at runtime"
        );
    }
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
