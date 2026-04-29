use std::path::PathBuf;

use super::{BenchFormat, Ex3I4Mode, ExecutionMode, PulseRhsMode, SolverMode};

pub const USAGE: &str = "Usage: fnec [--solver <pulse|hallen|continuity|sinusoidal>] [--pulse-rhs <raw|nec2>] [--exec <cpu|hybrid|gpu>] [--allow-noncollinear-hallen] [--ex3-i4-mode <legacy|divide-by-i4>] [--bench] [--bench-format <human|csv|json>] [--gpu-fr] <deck.nec>";

#[derive(Debug, Clone)]
pub struct ParsedArgs {
    pub solver_mode: SolverMode,
    pub pulse_rhs_mode: PulseRhsMode,
    pub execution_mode: ExecutionMode,
    pub hallen_allow_non_collinear: bool,
    pub enable_benchmarking: bool,
    pub bench_format: BenchFormat,
    pub ex3_i4_mode: Ex3I4Mode,
    pub enable_gpu_fr: bool,
    pub path: PathBuf,
}

pub fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut solver_mode = SolverMode::Hallen;
    let mut pulse_rhs_mode = PulseRhsMode::Nec2;
    let mut execution_mode = ExecutionMode::Cpu;
    let mut hallen_allow_non_collinear = false;
    let mut enable_benchmarking = false;
    let mut bench_format = BenchFormat::Human;
    let mut enable_gpu_fr = false;
    let mut ex3_i4_mode = Ex3I4Mode::Legacy;
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
            "--gpu-fr" => {
                enable_gpu_fr = true;
            }
            "--ex3-i4-mode" => {
                i += 1;
                if i >= args.len() {
                    return Err(
                        "missing value after --ex3-i4-mode (expected: legacy|divide-by-i4)"
                            .to_string(),
                    );
                }
                ex3_i4_mode = match args[i].as_str() {
                    "legacy" => Ex3I4Mode::Legacy,
                    "divide-by-i4" => Ex3I4Mode::DivideByI4,
                    other => {
                        return Err(format!(
                            "invalid --ex3-i4-mode value '{other}' (expected: legacy|divide-by-i4)"
                        ))
                    }
                };
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
        pulse_rhs_mode,
        execution_mode,
        hallen_allow_non_collinear,
        enable_benchmarking,
        bench_format,
        ex3_i4_mode,
        enable_gpu_fr,
        path,
    })
}
