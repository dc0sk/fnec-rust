use super::ExecutionMode;
use nec_accel::{dispatch_frequency_point, AccelRequestKind, DispatchDecision};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompatibilityProfile {
    Native,
    FourNec2DropIn,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StartupExecutionProbe {
    pub(super) cpu_threads: usize,
    pub(super) freq_points: usize,
    pub(super) gpu_available: bool,
    pub(super) hybrid_gpu_lane_available: bool,
}

pub(super) fn detect_compatibility_profile(argv0: &str) -> CompatibilityProfile {
    let stem = Path::new(argv0)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if stem.contains("nec2dxs") || stem.contains("4nec2") {
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

pub(super) fn startup_execution_probe(freq_points: usize) -> StartupExecutionProbe {
    let cpu_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let gpu_available = matches!(
        dispatch_frequency_point(AccelRequestKind::GpuOnly, 14.2e6),
        DispatchDecision::RunOnGpu
    );
    let hybrid_gpu_lane_available = matches!(
        dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, 14.2e6),
        DispatchDecision::RunOnGpu
    );

    StartupExecutionProbe {
        cpu_threads,
        freq_points,
        gpu_available,
        hybrid_gpu_lane_available,
    }
}

pub(super) fn auto_select_execution_mode(
    suggested_default: ExecutionMode,
    probe: StartupExecutionProbe,
) -> ExecutionMode {
    // For single-point solves CPU is typically best today due scheduling overhead.
    let cpu_multithread_viable = probe.cpu_threads > 1 && probe.freq_points > 1;

    if probe.gpu_available && probe.hybrid_gpu_lane_available && cpu_multithread_viable {
        return ExecutionMode::Hybrid;
    }
    if probe.gpu_available {
        return ExecutionMode::Gpu;
    }
    if cpu_multithread_viable {
        return ExecutionMode::Hybrid;
    }

    suggested_default
}
