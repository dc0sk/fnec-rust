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
            // What this reports is the per-frequency SCHEDULING SEAM, which is a
            // hybrid-lane concern and takes no work. It is NOT a claim that the
            // solve ran on the CPU: on this flag a free-space, stamp-free deck of
            // >= 16 segments does take the GPU-resident solve
            // (`maybe_gpu_resident_hallen`), and `warn_gpu_resident_solve_is_slower`
            // reports that separately. Saying "using CPU solve path" here while
            // the next line describes the resident solve losing on time was two
            // adjacent contradictory sentences, and the first one was false for
            // exactly the deck class this flag exists for.
            match nec_accel::dispatch_frequency_point(nec_accel::AccelRequestKind::GpuOnly, 0.0) {
                nec_accel::DispatchDecision::FallbackToCpu { reason } => {
                    eprintln!(
                        "warning: --exec gpu requested; the per-frequency scheduling seam takes \
                         no work ({reason}). The wgpu Z-fill and far-field kernels still run on \
                         the GPU, and a free-space deck with no LD/TL/NT stamps and >= 16 \
                         segments also takes the GPU-resident solve"
                    );
                }
                // The seam's other arm: real per-frequency GPU dispatch would
                // need no fallback warning.
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

/// Say that the GPU-resident dense solve is measured slower than the CPU.
///
/// PH7-CHK-003 measured it at **0.04x-0.48x** the CPU across every tested size,
/// and identified the cause as structural rather than tuning: the LU dispatches
/// one workgroup, so it runs on a single compute unit and more GPU hardware
/// cannot help. Its own recommendation was to treat the path as not-recommended
/// until the LU is re-implemented across workgroups — recorded in the document
/// and never acted on, so a user asking for `--exec gpu` still got the slow path
/// silently (FND-009).
///
/// A warning rather than a removal: the Z-fill and RP kernels on the same flag
/// earn their place decisively (100-290x and 56-234x), so `--exec gpu` is worth
/// asking for. It is the dense solve alone that loses, and the honest thing is
/// to say which part.
pub(super) fn warn_gpu_resident_solve_is_slower() {
    eprintln!(
        "warning: the GPU-resident dense solve was measured at 0.04x-0.48x the CPU \
at every tested size (PH7-CHK-003). The cause is structural: its LU dispatches one \
workgroup, so it runs on a single compute unit. The Z-fill and far-field kernels on \
this flag are much faster than the CPU; it is the solve that loses. Use --exec cpu \
if wall-clock matters."
    );
}
