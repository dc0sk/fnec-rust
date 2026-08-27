// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_solver::{GroundModel, Segment};

use super::exec_profile::ExecutionMode;
use super::solve_session::SolverMode;

pub(super) fn warn_execution_mode_fallback(execution_mode: ExecutionMode) {
    match execution_mode {
        ExecutionMode::Cpu => {}
        ExecutionMode::Hybrid => {}
        ExecutionMode::Gpu => {
            // The RP and Z-matrix-fill kernels run on the GPU via wgpu; the dense
            // linear solve still runs on CPU (GPU-resident solve tracked as
            // PH7-CHK-003). `dispatch_frequency_point` is the per-frequency
            // scheduling seam, which is CPU-only until PH7-CHK-004.
            match nec_accel::dispatch_frequency_point(nec_accel::AccelRequestKind::GpuOnly, 0.0) {
                nec_accel::DispatchDecision::FallbackToCpu { reason } => {
                    eprintln!("warning: --exec gpu requested, but {reason}; using CPU solve path");
                }
                // Reserved for PH7-CHK-004; real per-frequency GPU dispatch needs
                // no fallback warning.
                nec_accel::DispatchDecision::RunOnGpu => {}
            }
        }
    }
}

pub(super) fn warn_pulse_mode_experimental(solver_mode: SolverMode) {
    if !matches!(solver_mode, SolverMode::Pulse | SolverMode::Continuity) {
        return;
    }
    eprintln!(
        "warning: pulse/continuity solver modes are EXPERIMENTAL and known-inaccurate for \
thin-wire antennas. The pulse-basis Pocklington EFIE diverges from the physical solution \
as segment count increases. Use --solver hallen or --solver sinusoidal for accurate results."
    );
}

/// Print the shared mixed-radius caveat when this is an MPIE run.
///
/// The caveat itself lives in `nec_solver::validate` so every frontend can show
/// it; the CLI only decides that its own solver mode is the MPIE and writes to
/// stderr.
pub(super) fn warn_mpie_mixed_radius(solver_mode: SolverMode, segs: &[Segment]) {
    if !matches!(solver_mode, SolverMode::Mpie) {
        return;
    }
    if let Some(w) = nec_solver::validate::mpie_mixed_radius_caveat(segs) {
        eprintln!("warning: {w}");
    }
}

pub(super) fn warn_deferred_ground_model(ground: &GroundModel) {
    if let Some(w) = nec_solver::validate::deferred_ground_warning(ground) {
        eprintln!("warning: {w}");
    }
}

pub(super) fn warn_ge_ground_reflection_flag(deck: &nec_model::deck::NecDeck) {
    if let Some(w) = nec_solver::validate::ge_ground_reflection_warning(deck) {
        eprintln!("warning: {w}");
    }
}

// NT card support is implemented (PH8-CHK-004): NT cards are stamped in the solve
// path via `nec_solver::build_deck_stamps`, which warns on malformed/unsupported
// cards. The former blanket "deferred support" warning was removed.

// PT cards are applied to the current output in solve_session (PH9-CHK-004);
// no deferred-support warning is emitted.
