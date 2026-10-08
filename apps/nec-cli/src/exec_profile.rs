use std::path::Path;

const FOURNEC2_DROPIN_KERNEL_STEMS: &[&str] = &[
    "nec2dxs500",
    "nec2dxs1k5",
    "nec2dxs3k0",
    "nec2dxs5k0",
    "nec2dxs8k0",
    "nec2dxs11k",
];

fn contains_ascii_token(haystack: &str, token: &str) -> bool {
    haystack.match_indices(token).any(|(start, _)| {
        let end = start + token.len();
        let left = haystack[..start].chars().next_back();
        let right = haystack[end..].chars().next();
        let left_ok = left.map_or(true, |c| !c.is_ascii_alphanumeric());
        let right_ok = right.map_or(true, |c| !c.is_ascii_alphanumeric());
        left_ok && right_ok
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExecutionMode {
    Cpu,
    Hybrid,
    Gpu,
}

impl ExecutionMode {
    pub(crate) fn as_cli_str(self) -> &'static str {
        match self {
            ExecutionMode::Cpu => "cpu",
            ExecutionMode::Hybrid => "hybrid",
            ExecutionMode::Gpu => "gpu",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompatibilityProfile {
    Native,
    FourNec2DropIn,
}

// The CPU/GPU crossover is this host's, measured by `fnec calibrate`
// (`exec_calibration`). It used to be two constants measured on one machine and
// shipped to every user (550 on an RTX 2080 Ti beside a 24-thread CPU, 500 on a
// GTX 1080 Ti — FND-219 records the measurements); a single host's crossover is
// not a property of fnec.

/// What `--exec` resolves to when it is not given, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AutoExecChoice {
    pub(super) mode: ExecutionMode,
    pub(super) reason: String,
}

/// Pick the CPU or the GPU for a run without `--exec`.
///
/// `deck_class` is whether the deck is one the device solves (the shared class,
/// and no stamps of any kind). `calibration` is this host's measured crossover
/// (`fnec calibrate`): `Ok(None)` when there is none, `Err` when the file cannot be
/// read. Without one the pick is the CPU and says how to get one. `device` is asked
/// only once the deck qualifies on everything else, so a small or ineligible deck
/// never touches the GPU; it returns the device's dense capacity and the host key
/// the calibration must match, or `None` without a hardware adapter.
///
/// Only ever `Cpu` or `Gpu`. It used to pick `Hybrid` for every multi-point run,
/// which since the parallel CPU sweep is the CPU path plus a warning about a lane
/// nobody asked for.
#[allow(clippy::too_many_arguments)] // the pick's inputs; a struct would only rename them
pub(super) fn auto_select_execution_mode(
    segments: usize,
    unknowns: usize,
    freq_points: usize,
    threads: usize,
    deck_class: Result<(), &str>,
    calibration: Result<Option<&super::exec_calibration::Calibration>, &str>,
    device: impl FnOnce() -> Option<(usize, super::exec_calibration::HostKey)>,
) -> AutoExecChoice {
    let cpu = |reason: String| AutoExecChoice {
        mode: ExecutionMode::Cpu,
        reason,
    };
    if let Err(why) = deck_class {
        return cpu(format!("not a deck the GPU solves: {why}"));
    }
    let cal = match calibration {
        Err(why) => return cpu(format!("the host's calibration could not be read: {why}")),
        Ok(None) => {
            return cpu(
                "no calibration for this host — run `fnec calibrate` to let the GPU be picked"
                    .to_string(),
            )
        }
        Ok(Some(c)) => c,
    };
    let sweep = freq_points > 1;
    let what = if sweep { "a sweep" } else { "one point" };
    if sweep && cal.sweep_threads != threads {
        return cpu(format!(
            "this host's sweep calibration was measured with {} CPU threads, this run has              {threads} — run `fnec calibrate` again",
            cal.sweep_threads
        ));
    }
    let threshold = if sweep {
        cal.sweep_min_segs
    } else {
        cal.one_point_min_segs
    };
    let Some(threshold) = threshold else {
        return cpu(format!(
            "the GPU did not beat the CPU for {what} on this host up to {} segments \
             (`fnec calibrate`)",
            cal.measured_up_to
        ));
    };
    if segments < threshold {
        return cpu(format!(
            "{segments} segments, below this host's GPU crossover for {what} ({threshold})"
        ));
    }
    match device() {
        None => cpu("no hardware GPU".to_string()),
        Some((_, key)) if key != cal.key => cpu(format!(
            "this host's calibration was measured on {} (driver {}) beside {}, not on this \
             {} (driver {}) beside {} — run `fnec calibrate` again",
            cal.key.adapter, cal.key.driver, cal.key.cpu, key.adapter, key.driver, key.cpu
        )),
        Some((capacity, _)) if unknowns > capacity => cpu(format!(
            "{unknowns} unknowns, more than the GPU holds ({capacity})"
        )),
        Some(_) => AutoExecChoice {
            mode: ExecutionMode::Gpu,
            reason: format!(
                "{segments} segments, at or above this host's GPU crossover for {what} \
                 ({threshold}); the device solve is f32, residual-checked, and within 2 Ω of \
                 the CPU — pass --exec cpu for the CPU's digits"
            ),
        },
    }
}

pub(super) fn detect_compatibility_profile(argv0: &str) -> CompatibilityProfile {
    let stem = Path::new(argv0)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let is_known_dropin_kernel_name = FOURNEC2_DROPIN_KERNEL_STEMS
        .iter()
        .any(|name| contains_ascii_token(&stem, name));
    if is_known_dropin_kernel_name || stem.contains("4nec2") {
        CompatibilityProfile::FourNec2DropIn
    } else {
        CompatibilityProfile::Native
    }
}

pub(super) fn steer_execution_mode_by_profile(
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

pub(super) fn warn_compatibility_profile(
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
            requested_execution_mode.as_cli_str()
        );
    } else {
        eprintln!(
            "warning: 4nec2 drop-in compatibility profile detected by binary name; default execution path steered to exec={}",
            effective_execution_mode.as_cli_str()
        );
    }
}
