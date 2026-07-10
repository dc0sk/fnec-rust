use std::path::PathBuf;

use super::bench::BenchFormat;
use super::exec_profile::ExecutionMode;
use super::solve_session::{GroundSolver, PulseRhsMode, SolverMode};

pub const USAGE: &str = "Usage: fnec [--solver <pulse|hallen|continuity|sinusoidal|mpie>] [--ground-solver <rcm|sommerfeld>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--sin-fallback-rel-max <value>] [--bench] [--bench-format <human|csv|json>] [--output-format <text|json>] [--sweep-config <file.toml>] [--vars <vars.toml|vars.json>] [--hosts <hosts.toml>] <deck.nec>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct ParsedArgs {
    pub solver_mode: SolverMode,
    pub ground_solver: GroundSolver,
    pub pulse_rhs_mode: PulseRhsMode,
    pub execution_mode: ExecutionMode,
    pub enable_benchmarking: bool,
    pub bench_format: BenchFormat,
    pub output_format: OutputFormat,
    pub sweep_config_path: Option<PathBuf>,
    pub vars_path: Option<PathBuf>,
    pub sin_fallback_rel_max_cli: Option<f64>,
    pub hosts_path: Option<PathBuf>,
    pub path: PathBuf,
}

pub fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut solver_mode = SolverMode::Hallen;
    let mut ground_solver = GroundSolver::Rcm;
    let mut pulse_rhs_mode = PulseRhsMode::Nec2;
    let mut execution_mode = ExecutionMode::Cpu;
    let mut enable_benchmarking = false;
    let mut bench_format = BenchFormat::Human;
    let mut output_format = OutputFormat::Text;
    let mut sweep_config_path: Option<PathBuf> = None;
    let mut vars_path: Option<PathBuf> = None;
    let mut sin_fallback_rel_max_cli: Option<f64> = None;
    let mut hosts_path: Option<PathBuf> = None;
    let mut deck_path: Option<PathBuf> = None;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--solver" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --solver (expected: hallen|pulse|continuity|sinusoidal|mpie)"
                            .to_string(),
                    );
                }
                solver_mode = match args[i].as_str() {
                    "hallen" => SolverMode::Hallen,
                    "pulse" => SolverMode::Pulse,
                    "continuity" => SolverMode::Continuity,
                    "sinusoidal" => SolverMode::Sinusoidal,
                    "mpie" => SolverMode::Mpie,
                    other => {
                        return Err(format!(
                            "invalid --solver value '{other}' (expected: hallen|pulse|continuity|sinusoidal|mpie)"
                        ))
                    }
                };
            }
            "--ground-solver" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --ground-solver (expected: rcm|sommerfeld)"
                            .to_string(),
                    );
                }
                ground_solver = match args[i].as_str() {
                    "rcm" => GroundSolver::Rcm,
                    "sommerfeld" => GroundSolver::Sommerfeld,
                    other => {
                        return Err(format!(
                            "invalid --ground-solver value '{other}' (expected: rcm|sommerfeld)"
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
                // flag removed in phase-1 simplification — silently ignore for backward compat
            }
            "--ex3-i4-mode" => {
                // flag removed in phase-1 simplification — silently ignore for backward compat
                i += 1; // skip the value argument
            }
            "--bench" => {
                enable_benchmarking = true;
            }
            "--bench-format" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --bench-format (expected: human|csv|json)".to_string(),
                    );
                }
                bench_format = match args[i].as_str() {
                    "human" => BenchFormat::Human,
                    "csv" => BenchFormat::Csv,
                    "json" => BenchFormat::Json,
                    other => {
                        return Err(format!(
                            "invalid --bench-format value '{other}' (expected: human|csv|json)"
                        ))
                    }
                };
                if bench_format != BenchFormat::Human {
                    enable_benchmarking = true;
                }
            }
            "--output-format" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --output-format (expected: text|json)".to_string()
                    );
                }
                output_format = match args[i].as_str() {
                    "text" => OutputFormat::Text,
                    "json" => OutputFormat::Json,
                    other => {
                        return Err(format!(
                            "invalid --output-format value '{other}' (expected: text|json)"
                        ))
                    }
                };
            }
            "--sweep-config" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --sweep-config (expected: path to .toml file)"
                            .to_string(),
                    );
                }
                sweep_config_path = Some(PathBuf::from(&args[i]));
            }
            "--vars" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --vars (expected: path to .toml or .json file)"
                            .to_string(),
                    );
                }
                vars_path = Some(PathBuf::from(&args[i]));
            }
            "--hosts" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --hosts (expected: path to hosts.toml file)"
                            .to_string(),
                    );
                }
                hosts_path = Some(PathBuf::from(&args[i]));
            }
            "--sin-fallback-rel-max" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --sin-fallback-rel-max (expected: positive number)"
                            .to_string(),
                    );
                }
                let raw = &args[i];
                let parsed = raw.parse::<f64>().map_err(|_| {
                    format!(
                        "invalid --sin-fallback-rel-max value '{raw}' (expected: positive number)"
                    )
                })?;
                if !parsed.is_finite() || parsed <= 0.0 {
                    return Err(format!(
                        "invalid --sin-fallback-rel-max value '{raw}' (expected: positive number)"
                    ));
                }
                sin_fallback_rel_max_cli = Some(parsed);
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
    Ok(ParsedArgs {
        solver_mode,
        ground_solver,
        pulse_rhs_mode,
        execution_mode,
        enable_benchmarking,
        bench_format,
        output_format,
        sweep_config_path,
        vars_path,
        sin_fallback_rel_max_cli,
        hosts_path,
        path,
    })
}
