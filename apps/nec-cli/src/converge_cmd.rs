// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `fnec converge`: the convergence-study helper (GAP-012, BL-IMPR-017).
//!
//! Solves a deck at increasing segmentation — the deck itself, then meshed 3× and
//! 9× finer by [`nec_solver::refine`] — and reports every feedpoint's impedance per
//! mesh with whether it has settled, by the rule the corpus holds nec2c's own
//! series to ([`nec_solver::convergence`]): the geometric tail of the last two
//! steps must be within the tolerance on R and on X. Two meshes give a step, not a
//! verdict; a series that drifts or diverges is a result, reported and exited 0,
//! because NEC-2's own series does that at a junction or beside a delta gap
//! (FND-256, FND-278).
//!
//! ```text
//! fnec converge [--factors 1,3,9] [--ends keep|centre] [--tol-pct 1] [--tol-ohm 0.5]
//!               [--solver hallen|sinusoidal|mpie] [--ground-solver rcm|sommerfeld]
//!               [--exec cpu|hybrid|gpu] [--vars <file>] [--output-format text|json] <deck.nec>
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use nec_solver::convergence::{remaining_drift, trend, Trend};
use nec_solver::refine::{refine_deck, EndRule, MappedRef};

use crate::cli_args::{OutputFormat, UNVALIDATED_SOLVER_CAVEAT};
use crate::exec_profile::ExecutionMode;
use crate::solve_session::{
    self, GroundSolver, ProbeConfig, ProbeFrequencies, SolverMode, CLI_MPIE_REMEDY,
    CLI_SOMMERFELD_REMEDY,
};
use crate::VERSION;

const CONVERGE_USAGE: &str = "Usage: fnec converge [--factors 1,3,9] [--ends keep|centre] [--tol-pct 1] [--tol-ohm 0.5] [--solver <hallen|sinusoidal|mpie>] [--ground-solver <rcm|sommerfeld>] [--exec <cpu|hybrid|gpu>] [--vars <vars.toml|vars.json>] [--output-format <text|json>] <deck.nec>\n\
    Solves the deck at each factor's mesh (odd factors in geometric progression) and reports\n\
    every feedpoint impedance per mesh and whether its series has settled.";

/// nec2c's thin-wire kernel limit without `EK`, segment length over radius; fnec's
/// own is [`nec_solver::validate::MIN_LENGTH_TO_RADIUS_RATIO`]. A mesh below it is
/// a note for a user who will compare with nec2c, not a refusal.
const NEC2C_THIN_KERNEL_RATIO: f64 = 8.0;

struct Args {
    factors: Vec<u32>,
    ends: EndRule,
    tol_pct: f64,
    tol_ohm: f64,
    solver_mode: SolverMode,
    ground_solver: GroundSolver,
    execution_mode: ExecutionMode,
    vars_path: Option<PathBuf>,
    output_format: OutputFormat,
    path: PathBuf,
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("fnec {}\n{CONVERGE_USAGE}", VERSION);
    eprintln!("error: {msg}");
    ExitCode::from(2)
}

/// The factor list: odd integers, strictly increasing, in geometric progression
/// (the settle rule prices a geometric tail, which 1,3,5 has none of), at least
/// two.
fn parse_factors(raw: &str) -> Result<Vec<u32>, String> {
    let mut factors = Vec::new();
    for tok in raw.split(',') {
        let f: u32 = tok.trim().parse().map_err(|_| {
            format!("invalid --factors value '{raw}' (expected: odd integers, e.g. 1,3,9)")
        })?;
        if f == 0 || f % 2 == 0 {
            return Err(format!(
                "invalid --factors value '{raw}': {f} is not odd — an odd factor keeps a segment \
                 centre at a segment centre"
            ));
        }
        factors.push(f);
    }
    if factors.len() < 2 {
        return Err(format!(
            "invalid --factors value '{raw}': a study needs two meshes or more (three for a verdict)"
        ));
    }
    for w in factors.windows(2) {
        if w[1] <= w[0] {
            return Err(format!(
                "invalid --factors value '{raw}': factors must increase"
            ));
        }
    }
    let ratio = factors[1] / factors[0];
    if factors[1] % factors[0] != 0 || ratio < 3 {
        return Err(format!(
            "invalid --factors value '{raw}': each factor must be the one before times an odd \
             integer of 3 or more (the settle rule prices a geometric tail; 1,3,5 has none)"
        ));
    }
    for w in factors.windows(2) {
        if w[1] != w[0] * ratio {
            return Err(format!(
                "invalid --factors value '{raw}': factors must be in geometric progression \
                 (the settle rule prices a geometric tail; 1,3,5 has none)"
            ));
        }
    }
    Ok(factors)
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut factors = vec![1, 3, 9];
    let mut ends = EndRule::Keep;
    let mut tol_pct = 1.0;
    let mut tol_ohm = 0.5;
    let mut solver_mode = SolverMode::Hallen;
    let mut ground_solver = GroundSolver::Rcm;
    let mut execution_mode = ExecutionMode::Cpu;
    let mut vars_path = None;
    let mut output_format = OutputFormat::Text;
    let mut path = None;
    let mut experimental = false;
    let mut i = 2usize;
    let value = |i: &mut usize, flag: &str, expected: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("missing value after {flag} (expected: {expected})"))
    };
    let positive = |flag: &str, raw: &str| -> Result<f64, String> {
        match raw.parse::<f64>() {
            Ok(v) if v.is_finite() && v >= 0.0 => Ok(v),
            _ => Err(format!(
                "invalid {flag} value '{raw}' (expected: a non-negative number)"
            )),
        }
    };
    while i < args.len() {
        match args[i].as_str() {
            "--factors" => {
                factors = parse_factors(&value(&mut i, "--factors", "odd integers, e.g. 1,3,9")?)?;
            }
            "--ends" => {
                let v = value(&mut i, "--ends", "keep|centre")?;
                ends = match v.as_str() {
                    "keep" => EndRule::Keep,
                    "centre" | "center" => EndRule::Centre,
                    other => {
                        return Err(format!(
                            "invalid --ends value '{other}' (expected: keep|centre)"
                        ))
                    }
                };
            }
            "--tol-pct" => {
                tol_pct = positive("--tol-pct", &value(&mut i, "--tol-pct", "percent")?)?;
            }
            "--tol-ohm" => tol_ohm = positive("--tol-ohm", &value(&mut i, "--tol-ohm", "ohms")?)?,
            "--solver" => {
                let v = value(&mut i, "--solver", &SolverMode::flag_alternation())?;
                solver_mode = SolverMode::from_flag(&v).ok_or_else(|| {
                    format!(
                        "invalid --solver value '{v}' (expected: {})",
                        SolverMode::flag_alternation()
                    )
                })?;
            }
            "--ground-solver" => {
                let v = value(&mut i, "--ground-solver", "rcm|sommerfeld")?;
                ground_solver = match v.as_str() {
                    "rcm" => GroundSolver::Rcm,
                    "sommerfeld" => GroundSolver::Sommerfeld,
                    other => {
                        return Err(format!(
                            "invalid --ground-solver value '{other}' (expected: rcm|sommerfeld)"
                        ))
                    }
                };
            }
            "--exec" => {
                let v = value(&mut i, "--exec", "cpu|hybrid|gpu")?;
                execution_mode = match v.as_str() {
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
            "--vars" => {
                vars_path = Some(PathBuf::from(value(
                    &mut i,
                    "--vars",
                    "path to .toml or .json file",
                )?));
            }
            "--output-format" => {
                let v = value(&mut i, "--output-format", "text|json")?;
                output_format = match v.as_str() {
                    "text" => OutputFormat::Text,
                    "json" => OutputFormat::Json,
                    other => {
                        return Err(format!(
                            "invalid --output-format value '{other}' (expected: text|json)"
                        ))
                    }
                };
            }
            "--experimental-solver" => experimental = true,
            flag if flag.starts_with('-') => {
                return Err(format!("unknown converge option: {flag}"))
            }
            p => {
                if path.is_some() {
                    return Err(format!("unexpected extra argument: {p}"));
                }
                path = Some(PathBuf::from(p));
            }
        }
        i += 1;
    }
    // FND-080: the pulse bases run only on explicit request, as on the main command.
    if matches!(solver_mode, SolverMode::Pulse | SolverMode::Continuity) && !experimental {
        return Err(format!(
            "--solver {} is not a validated solver and runs only with --experimental-solver. \
             {UNVALIDATED_SOLVER_CAVEAT}",
            solver_mode.as_flag()
        ));
    }
    Ok(Args {
        factors,
        ends,
        tol_pct,
        tol_ohm,
        solver_mode,
        ground_solver,
        execution_mode,
        vars_path,
        output_format,
        path: path.ok_or_else(|| "missing deck path".to_string())?,
    })
}

/// One feedpoint of one solved frequency: `(tag, seg, z_re, z_im)`.
type FeedRow = (usize, usize, f64, f64);

/// One solved frequency of a mesh: `(freq_hz, route, rows)`.
type MeshPoint = (f64, String, Vec<FeedRow>);

/// One mesh of the study: its factor, segment count, the refined deck's
/// references, and each frequency's feedpoint rows with the route that solved it.
struct Mesh {
    factor: u32,
    segments: usize,
    refs: Vec<MappedRef>,
    points: Vec<MeshPoint>,
}

/// One feedpoint's series across the meshes, and its verdict.
struct Row {
    freq_mhz: f64,
    tag: usize,
    seg: usize,
    /// `(factor, segments, z_re, z_im, route)` per mesh.
    meshes: Vec<(u32, usize, f64, f64, String)>,
    steps_r: Vec<f64>,
    steps_x: Vec<f64>,
    /// `None` below three meshes.
    tail: Option<(f64, f64)>,
    trend: Option<Trend>,
    extrapolated: Option<(f64, f64)>,
    /// `Some(true|false)`, or `None` for "undecided" (two meshes).
    settled: Option<bool>,
}

/// The `route=` field of a `diag:` line.
fn route_of(diag_line: &str) -> String {
    diag_line
        .split_whitespace()
        .find_map(|f| f.strip_prefix("route="))
        .unwrap_or("?")
        .to_string()
}

/// The verdict of one series: tail, trend, extrapolation and whether it settled
/// within `tol = max(tol_ohm, tol_pct % × |Z_last|)` on each axis.
fn judge(row: &mut Row, tol_pct: f64, tol_ohm: f64) {
    let r: Vec<f64> = row.meshes.iter().map(|m| m.2).collect();
    let x: Vec<f64> = row.meshes.iter().map(|m| m.3).collect();
    row.steps_r = r.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    row.steps_x = x.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    if r.len() < 3 {
        return;
    }
    // Full-precision values: no printed-unit widening.
    let (tail_r, tail_x) = (remaining_drift(&r, 0.0), remaining_drift(&x, 0.0));
    row.tail = Some((tail_r, tail_x));
    // One class for the point: the worse of the two axes' trends.
    let rank = |t: Trend| match t {
        Trend::Settling => 0,
        Trend::LogDrift => 1,
        Trend::Diverging => 2,
    };
    row.trend = match (trend(&r), trend(&x)) {
        (Some(a), Some(b)) => Some(if rank(b) > rank(a) { b } else { a }),
        (a, b) => a.or(b),
    };
    let (z_re, z_im) = (*r.last().unwrap(), *x.last().unwrap());
    let tol = tol_ohm.max(tol_pct / 100.0 * z_re.hypot(z_im));
    row.settled = Some(tail_r <= tol && tail_x <= tol);
    // Extrapolated only where it is priced: settled on both axes.
    if row.settled == Some(true) {
        let n = r.len();
        let sign = |v: &[f64]| (v[n - 1] - v[n - 2]).signum();
        row.extrapolated = Some((z_re + sign(&r) * tail_r, z_im + sign(&x) * tail_x));
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `ALL_SETTLED`: false when any row has not settled, else undecided when any row
/// has two meshes, else true — true only when every row is.
fn all_settled_word(rows: &[Row]) -> &'static str {
    if rows.iter().any(|r| r.settled == Some(false)) {
        "false"
    } else if rows.iter().any(|r| r.settled.is_none()) {
        "undecided"
    } else {
        "true"
    }
}

/// The study on stdout. Every row carries the study's references (each with
/// where every mesh put it) and its notes, so a row read alone is complete.
fn print_study(args: &Args, meshes: &[Mesh], rows: &[Row], notes: &[String]) {
    // Every reference, in the first mesh's order, with where each mesh put it.
    let refs: Vec<(MappedRef, Vec<(u32, u32)>)> = meshes
        .first()
        .map(|first| {
            first
                .refs
                .iter()
                .enumerate()
                .map(|(k, r)| {
                    let at: Vec<(u32, u32)> = meshes
                        .iter()
                        .filter_map(|m| m.refs.get(k).map(|mr| (m.factor, mr.mapped)))
                        .collect();
                    (r.clone(), at)
                })
                .collect()
        })
        .unwrap_or_default();
    let factors: Vec<String> = args.factors.iter().map(u32::to_string).collect();
    let settled_word = |s: Option<bool>| match s {
        Some(true) => "true",
        Some(false) => "false",
        None => "undecided",
    };
    match args.output_format {
        OutputFormat::Text | OutputFormat::Nec2d => {
            println!("CONVERGENCE_STUDY");
            println!("SOLVER {}", args.solver_mode.as_flag());
            println!("FACTORS {}", factors.join(","));
            println!("ENDS {}", args.ends.label());
            println!("TOL_OHM {:.6} TOL_PCT {:.6}", args.tol_ohm, args.tol_pct);
            for (i, row) in rows.iter().enumerate() {
                println!(
                    "ROW {} FREQ_MHZ {:.6} TAG {} SEG {}",
                    i + 1,
                    row.freq_mhz,
                    row.tag,
                    row.seg
                );
                for (r, at) in &refs {
                    let at: Vec<String> = at.iter().map(|(f, m)| format!("{f}x {m}")).collect();
                    println!(
                        "REF {} {} {} -> {}",
                        r.card,
                        r.tag,
                        r.segment,
                        at.join(", ")
                    );
                }
                for n in notes {
                    println!("NOTE {n}");
                }
                for (factor, segments, z_re, z_im, route) in &row.meshes {
                    println!(
                        "MESH {factor} SEGMENTS {segments} Z_RE {z_re:.6} Z_IM {z_im:.6} ROUTE {route}"
                    );
                }
                let list = |v: &[f64]| {
                    v.iter()
                        .map(|s| format!("{s:.6}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                println!("STEPS_R {}", list(&row.steps_r));
                println!("STEPS_X {}", list(&row.steps_x));
                if let Some((tr, tx)) = row.tail {
                    println!("TAIL_R {tr:.6} TAIL_X {tx:.6}");
                }
                if let Some(t) = row.trend {
                    println!("CLASS {}", t.label());
                }
                if let Some((re, im)) = row.extrapolated {
                    println!("Z_EXTRAPOLATED {re:.6} {im:.6}");
                }
                println!("SETTLED {}", settled_word(row.settled));
            }
            println!("ALL_SETTLED {}", all_settled_word(rows));
        }
        OutputFormat::Json => {
            let num = |v: f64| {
                // Debug keeps a float a float in JSON (`1.0`, not `1`).
                if v.is_finite() {
                    format!("{v:?}")
                } else {
                    "null".to_string()
                }
            };
            let refs_json: Vec<String> = refs
                .iter()
                .map(|(r, at)| {
                    let mapped: Vec<String> = at.iter().map(|(_, m)| m.to_string()).collect();
                    format!(
                        "{{\"card\":{},\"tag\":{},\"seg\":{},\"touches_end\":{},\"mapped\":[{}]}}",
                        json_string(r.card),
                        r.tag,
                        r.segment,
                        r.touches_end,
                        mapped.join(",")
                    )
                })
                .collect();
            let notes_json: Vec<String> = notes.iter().map(|n| json_string(n)).collect();
            let rows_json: Vec<String> = rows
                .iter()
                .map(|row| {
                    let meshes: Vec<String> = row
                        .meshes
                        .iter()
                        .map(|(factor, segments, z_re, z_im, route)| {
                            format!(
                                "{{\"factor\":{factor},\"segments\":{segments},\"z_re\":{},\"z_im\":{},\"route\":{}}}",
                                num(*z_re),
                                num(*z_im),
                                json_string(route)
                            )
                        })
                        .collect();
                    let list = |v: &[f64]| v.iter().map(|s| num(*s)).collect::<Vec<_>>().join(",");
                    let tail = row
                        .tail
                        .map(|(r, x)| format!("{{\"r\":{},\"x\":{}}}", num(r), num(x)))
                        .unwrap_or_else(|| "null".to_string());
                    let class = row
                        .trend
                        .map(|t| json_string(t.label()))
                        .unwrap_or_else(|| "null".to_string());
                    let extrapolated = row
                        .extrapolated
                        .map(|(re, im)| format!("{{\"z_re\":{},\"z_im\":{}}}", num(re), num(im)))
                        .unwrap_or_else(|| "null".to_string());
                    let settled = match row.settled {
                        Some(b) => b.to_string(),
                        None => "\"undecided\"".to_string(),
                    };
                    format!(
                        "{{\"freq_mhz\":{},\"tag\":{},\"seg\":{},\"refs\":[{}],\"meshes\":[{}],\"steps\":{{\"r\":[{}],\"x\":[{}]}},\"tail\":{tail},\"class\":{class},\"extrapolated\":{extrapolated},\"settled\":{settled},\"notes\":[{}]}}",
                        num(row.freq_mhz),
                        row.tag,
                        row.seg,
                        refs_json.join(","),
                        meshes.join(","),
                        list(&row.steps_r),
                        list(&row.steps_x),
                        notes_json.join(","),
                    )
                })
                .collect();
            let all = match all_settled_word(rows) {
                "undecided" => "\"undecided\"",
                word => word,
            };
            println!(
                "{{\"schema\":\"fnec-converge/1\",\"solver\":{},\"factors\":[{}],\"ends\":{},\"tol\":{{\"ohm\":{},\"pct\":{}}},\"rows\":[{}],\"all_settled\":{all}}}",
                json_string(args.solver_mode.as_flag()),
                factors.join(","),
                json_string(args.ends.label()),
                num(args.tol_ohm),
                num(args.tol_pct),
                rows_json.join(","),
            );
        }
    }
}

/// Entry point for `fnec converge`.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let args = match parse_args(args) {
        Ok(a) => a,
        Err(e) => return usage_error(&e),
    };

    let input = match std::fs::read_to_string(&args.path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", args.path.display());
            return ExitCode::FAILURE;
        }
    };
    let input = match &args.vars_path {
        Some(vp) => {
            let vars = match crate::vars_config::load_vars(vp) {
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
        }
        None => input,
    };
    let parsed = match nec_parser::parse(&input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    for w in &parsed.warnings {
        eprintln!("warning: {w}");
    }
    let deck = &parsed.deck;

    // Every mesh is refined and checked before any is solved: a factor that takes
    // a wire under the thin-wire kernel's validity, or the structure over the
    // segment budget, is refused up front naming the largest usable factor; a
    // refined deck the main command would refuse is refused with its sentence and
    // the factor. No half-finished study is printed for either.
    let mut refined = Vec::with_capacity(args.factors.len());
    let mut notes: Vec<String> = Vec::new();
    let mut size_refusal: Option<String> = None;
    for &factor in &args.factors {
        let r = match refine_deck(deck, factor, args.ends) {
            Ok(r) => r,
            Err(e) => return refuse(&in_converge_words(&e)),
        };
        // The segment budget (MAX_SEGMENTS) is the geometry builder's refusal.
        let segs = match args.solver_mode.geometry(&r.deck) {
            Ok(s) => s,
            Err(e) => {
                let e = e.with_mpie_remedy(CLI_MPIE_REMEDY);
                size_refusal = Some(format!("converge: at {factor}x: {e}"));
                break;
            }
        };
        // The deck's own segments, not the solver's (Hallén splits a free end's
        // segment in thirds, FND-227): their count and their length in radii.
        let deck_segs = match nec_solver::build_deck_geometry(&r.deck) {
            Ok(s) => s,
            Err(e) => {
                size_refusal = Some(format!("converge: at {factor}x: {e}"));
                break;
            }
        };
        let deck_segments = deck_segs.len();
        // The shortest segment in radii, over every segment of the structure.
        let thinnest = deck_segs
            .iter()
            .filter(|s| s.radius > 0.0)
            .map(|s| (s.length / s.radius, s.tag))
            .fold(None, |acc: Option<(f64, u32)>, v| match acc {
                Some(a) if a.0 <= v.0 => Some(a),
                _ => Some(v),
            });
        if let Some((ratio, tag)) = thinnest {
            let limit = nec_solver::validate::MIN_LENGTH_TO_RADIUS_RATIO;
            if ratio < limit {
                size_refusal = Some(format!(
                    "converge: at {factor}x tag {tag}'s segments are {ratio:.2} radii long, \
                     under the thin-wire kernel's {limit}"
                ));
                break;
            }
            if ratio < NEC2C_THIN_KERNEL_RATIO {
                let n = format!(
                    "at {factor}x the shortest segment is {ratio:.1} radii (tag {tag}), below \
                     nec2c's thin-kernel limit of {NEC2C_THIN_KERNEL_RATIO:.0} (its `EK` extended \
                     kernel takes {limit:.0}); fnec's kernel is valid to {limit:.0}"
                );
                if !notes.contains(&n) {
                    notes.push(n);
                }
            }
        }
        // FND-173: every refusal the main command makes, on every mesh.
        let ground = nec_solver::ground_model_from_deck(&r.deck);
        if let Some(e) = nec_solver::validate::pre_solve_error(&r.deck, &segs, &ground) {
            return refuse(&format!("converge: at {factor}x: {e}"));
        }
        for n in &r.notes {
            if !notes.contains(n) {
                notes.push(n.clone());
            }
        }
        refined.push((factor, deck_segments, r));
    }
    if let Some(why) = size_refusal {
        let usable: Vec<u32> = refined.iter().map(|(f, _, _)| *f).collect();
        let remedy = match usable.as_slice() {
            [] => {
                "no factor in the list is usable — use a thinner wire or fewer segments".to_string()
            }
            [one] => format!("only {one}x is usable, and a study needs two meshes or more"),
            list => format!(
                "the largest usable factor is {} (`--factors {}`)",
                list.last().unwrap(),
                list.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        };
        return refuse(&format!("{why}; {remedy}"));
    }

    let mut meshes: Vec<Mesh> = Vec::with_capacity(refined.len());
    let mut failed = false;
    for (factor, segments, r) in refined {
        // Each mesh's stderr carries its own prefix: the caveats are the mesh's
        // own (a one-segment run at 1x is three at 3x), printed for every mesh.
        let prefix = format!("mesh {factor}x: ");
        eprintln!("{prefix}info: {segments} segments");
        let cfg = probe_config(&args, &prefix);
        match solve_session::probe_feedpoints(&r.deck, &cfg, ProbeFrequencies::All, true) {
            Ok(points) => meshes.push(Mesh {
                factor,
                segments,
                refs: r.refs,
                points: points
                    .into_iter()
                    .map(|p| {
                        let rows = p
                            .feedpoints
                            .iter()
                            .map(|f| (f.tag, f.seg, f.z_in.re, f.z_in.im))
                            .collect();
                        eprintln!("{prefix}{}", p.diag_line);
                        (p.freq_hz, route_of(&p.diag_line), rows)
                    })
                    .collect(),
            }),
            Err(e) => {
                // What solved is still printed; the exit says the study is not
                // the one asked for.
                eprintln!("error: converge: at {factor}x: {e}");
                failed = true;
                break;
            }
        }
    }

    // One row per (frequency, source) of the first mesh, its series across the
    // meshes; the row's tag and segment are the deck's own.
    let mut rows: Vec<Row> = Vec::new();
    if let Some(first) = meshes.first() {
        for (fi, (freq_hz, _, feeds)) in first.points.iter().enumerate() {
            for (ri, (tag, seg, _, _)) in feeds.iter().enumerate() {
                let mut per_mesh = Vec::with_capacity(meshes.len());
                for m in &meshes {
                    let Some((_, route, rows_m)) = m.points.get(fi) else {
                        break;
                    };
                    let Some((_, _, z_re, z_im)) = rows_m.get(ri) else {
                        break;
                    };
                    per_mesh.push((m.factor, m.segments, *z_re, *z_im, route.clone()));
                }
                let mut row = Row {
                    freq_mhz: freq_hz / 1e6,
                    tag: *tag,
                    seg: *seg,
                    meshes: per_mesh,
                    steps_r: Vec::new(),
                    steps_x: Vec::new(),
                    tail: None,
                    trend: None,
                    extrapolated: None,
                    settled: None,
                };
                judge(&mut row, args.tol_pct, args.tol_ohm);
                rows.push(row);
            }
        }
    }
    if !meshes.is_empty() {
        print_study(&args, &meshes, &rows, &notes);
    }
    if failed {
        return ExitCode::FAILURE;
    }
    if rows.is_empty() {
        return refuse("converge: the solve priced no feedpoint");
    }
    for (i, row) in rows.iter().enumerate() {
        match row.settled {
            Some(false) => eprintln!(
                "warning: converge: row {} ({:.6} MHz, tag {} seg {}) has not settled within \
                 the tolerance{}",
                i + 1,
                row.freq_mhz,
                row.tag,
                row.seg,
                row.trend
                    .map(|t| format!(" ({})", t.label()))
                    .unwrap_or_default()
            ),
            None => eprintln!(
                "warning: converge: row {} has two meshes, a step and no verdict — three meshes \
                 or more decide",
                i + 1
            ),
            Some(true) => {}
        }
    }
    ExitCode::SUCCESS
}

/// The main command's solve, its remedies, and this mesh's stderr prefix.
fn probe_config<'a>(args: &Args, prefix: &'a str) -> ProbeConfig<'a> {
    ProbeConfig {
        solver_mode: args.solver_mode,
        ground_solver: args.ground_solver,
        execution_mode: args.execution_mode,
        refusal_mpie_remedy: CLI_MPIE_REMEDY,
        remedies: nec_solver::validate::Remedies {
            mpie: Some(CLI_MPIE_REMEDY),
            mpie_low_ground: Some(CLI_MPIE_REMEDY),
            sommerfeld: (args.ground_solver == GroundSolver::Rcm).then_some(CLI_SOMMERFELD_REMEDY),
        },
        no_frequency_remedy: "Add an `FR` card to the deck.",
        stderr_prefix: prefix,
    }
}

/// A refusal: on stderr, exit 1.
fn refuse(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}

/// A refinement refusal in this command's words: `refine: …` → `converge: …`.
fn in_converge_words(e: &str) -> String {
    match e.strip_prefix("refine: ") {
        Some(rest) => format!("converge: {rest}"),
        None => format!("converge: {e}"),
    }
}
