// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_model::card::Card;
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

/// The MPIE solver's reduced thin-wire kernel uses a single wire radius (the
/// first segment's) for the whole geometry. Warn once when a `--solver mpie` deck
/// mixes radii, since the differently-sized wires are then solved approximately.
pub(super) fn warn_mpie_mixed_radius(solver_mode: SolverMode, segs: &[Segment]) {
    if !matches!(solver_mode, SolverMode::Mpie) {
        return;
    }
    let Some(first) = segs.first() else { return };
    let r0 = first.radius;
    let mixed = segs
        .iter()
        .any(|s| (s.radius - r0).abs() > 1e-9 * r0.max(s.radius));
    if mixed {
        eprintln!(
            "warning: --solver mpie models a single wire radius; this deck mixes radii, so \
all segments are solved with the first wire's radius ({r0} m) — impedance for the \
differently-sized wires will be approximate"
        );
    }
}

pub(super) fn warn_deferred_ground_model(ground: &GroundModel) {
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

pub(super) fn warn_ge_ground_reflection_flag(deck: &nec_model::deck::NecDeck) {
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

// NT card support is implemented (PH8-CHK-004): NT cards are stamped in the solve
// path via `nec_solver::build_nt_stamps`, which warns on malformed/unsupported
// cards. The former blanket "deferred support" warning was removed.

// PT cards are applied to the current output in solve_session (PH9-CHK-004);
// no deferred-support warning is emitted.
