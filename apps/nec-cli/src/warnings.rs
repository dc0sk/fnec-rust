// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_solver::{GroundModel, Segment};

use super::solve_session::SolverMode;

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

/// Say so when an explicit `--exec gpu` sends a deck below the crossover to the
/// GPU-resident dense solve.
///
/// PH7-CHK-003 measured the single-workgroup solve at **0.04x-0.48x** the CPU at
/// every size (FND-009). Since FND-185 it crosses over: on an NVIDIA GTX 1080 Ti,
/// whole CLI run, 0.18 s against the CPU's 0.009 s at 101 segments and 0.90 s
/// against 4.56 s at 1001; near 500 for one point since the triangular solves run per column. Without `--exec` fnec now picks
/// the faster side itself, so this fires only when the user forced the device
/// onto a deck where it loses — once per process, not once per sweep point.
pub(super) fn warn_gpu_resident_solve_is_slower(segments: usize) {
    use super::exec_profile::AUTO_GPU_MIN_SEGS_ONE_POINT;
    static ONCE: std::sync::Once = std::sync::Once::new();
    if segments >= AUTO_GPU_MIN_SEGS_ONE_POINT {
        return;
    }
    ONCE.call_once(|| {
        eprintln!(
            "warning: --exec gpu on a {segments}-segment deck: the GPU-resident dense solve \
             is slower than the CPU below about {AUTO_GPU_MIN_SEGS_ONE_POINT} segments \
             (GTX 1080 Ti: 0.18 s against 0.009 s at 101). Without --exec, fnec picks the \
             faster one"
        );
    });
}
