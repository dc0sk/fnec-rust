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

    pub(crate) fn as_diag_str(self) -> &'static str {
        match self {
            ExecutionMode::Cpu => "cpu",
            ExecutionMode::Hybrid => "hybrid",
            ExecutionMode::Gpu => "gpu(cpu-fallback)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompatibilityProfile {
    Native,
    FourNec2DropIn,
}

/// The smallest deck the automatic pick sends to the GPU for one frequency point.
///
/// Measured 2026-10-01 on an NVIDIA GTX 1080 Ti against a 24-thread CPU, a λ/2
/// dipole, whole process, best of 3, with the triangular solves one dispatch per
/// column: 401 segments 0.17 s CPU / 0.22 s GPU, 451 0.235 / 0.224 (a tie), 501
/// 0.31 / 0.25, 601 0.52 / 0.27. Set at the first clear GPU win, 501. The device
/// pays a fixed ≈ 0.18 s to start, the CPU grows as N³. (It was 600 when the
/// triangular solves ran in one invocation.)
pub(super) const AUTO_GPU_MIN_SEGS_ONE_POINT: usize = 500;

/// The same for a sweep, where the CPU solves its points in parallel and the GPU
/// in turn. 24 points, measured as above: 401 segments 0.40 s CPU / 1.20 s GPU,
/// 501 1.24 / 1.64, 551 2.65 / 1.89, 601 3.71 / 2.20, 701 6.79 / 2.95 (it was 800
/// with the serial triangular solves). It moves with the controller's thread count
/// — fewer cores, a lower crossover — and is not modelled: guessing high errs
/// toward the CPU, which was the default before this pick existed, so the pick is
/// never slower than it.
pub(super) const AUTO_GPU_MIN_SEGS_SWEEP: usize = 550;

/// What `--exec` resolves to when it is not given, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AutoExecChoice {
    pub(super) mode: ExecutionMode,
    pub(super) reason: String,
}

/// Pick the CPU or the GPU for a run without `--exec`.
///
/// `deck_class` is whether the deck is one the device solves (the shared class,
/// and no stamps of any kind); `device_capacity` is asked only once the deck
/// qualifies on everything else, so a small or ineligible deck never touches the
/// GPU. It returns the device's dense capacity, or `None` without a hardware
/// adapter.
///
/// Only ever `Cpu` or `Gpu`. It used to pick `Hybrid` for every multi-point run,
/// which since the parallel CPU sweep is the CPU path plus a warning about a lane
/// nobody asked for.
pub(super) fn auto_select_execution_mode(
    segments: usize,
    unknowns: usize,
    freq_points: usize,
    deck_class: Result<(), &str>,
    device_capacity: impl FnOnce() -> Option<usize>,
) -> AutoExecChoice {
    let cpu = |reason: String| AutoExecChoice {
        mode: ExecutionMode::Cpu,
        reason,
    };
    if let Err(why) = deck_class {
        return cpu(format!("not a deck the GPU solves: {why}"));
    }
    let (threshold, what) = if freq_points > 1 {
        (AUTO_GPU_MIN_SEGS_SWEEP, "a sweep")
    } else {
        (AUTO_GPU_MIN_SEGS_ONE_POINT, "one point")
    };
    if segments < threshold {
        return cpu(format!(
            "{segments} segments, below the GPU crossover for {what} ({threshold})"
        ));
    }
    match device_capacity() {
        None => cpu("no hardware GPU".to_string()),
        Some(capacity) if unknowns > capacity => cpu(format!(
            "{unknowns} unknowns, more than the GPU holds ({capacity})"
        )),
        Some(_) => AutoExecChoice {
            mode: ExecutionMode::Gpu,
            reason: format!(
                "{segments} segments, at or above the GPU crossover for {what} ({threshold}); \
                 the device solve is f32, residual-checked, and within 2 Ω of the CPU — \
                 pass --exec cpu for the CPU's digits"
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
            requested_execution_mode.as_diag_str()
        );
    } else {
        eprintln!(
            "warning: 4nec2 drop-in compatibility profile detected by binary name; default execution path steered to exec={}",
            effective_execution_mode.as_diag_str()
        );
    }
}
