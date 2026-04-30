// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_model::card::Card;
use nec_solver::GroundModel;

use super::{ExecutionMode, SolverMode};

pub(super) fn warn_execution_mode_fallback(execution_mode: ExecutionMode) {
    match execution_mode {
        ExecutionMode::Cpu => {}
        ExecutionMode::Hybrid => {}
        ExecutionMode::Gpu => {
            match nec_accel::dispatch_frequency_point(nec_accel::AccelRequestKind::GpuOnly, 0.0) {
                nec_accel::DispatchDecision::FallbackToCpu { reason } => {
                    eprintln!("warning: --exec gpu requested, but {reason}; using CPU solve path");
                }
                nec_accel::DispatchDecision::RunOnGpu => {
                    eprintln!(
                    "warning: --exec gpu dispatched to accelerator stub backend; solving with CPU emulation"
                );
                }
            }
        }
    }
}

pub(super) fn warn_pulse_mode_experimental(solver_mode: SolverMode) {
    if !matches!(
        solver_mode,
        SolverMode::Pulse | SolverMode::Continuity | SolverMode::Sinusoidal
    ) {
        return;
    }
    eprintln!(
        "warning: pulse/continuity/sinusoidal solver modes are EXPERIMENTAL and known-inaccurate for \
thin-wire antennas. The pulse-basis Pocklington EFIE diverges from the physical solution \
as segment count increases. Use --solver hallen for accurate results. \
(Sinusoidal-basis EFIE fix tracked in backlog.)"
    );
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

pub(super) fn warn_ex_type1_portability_semantics(
    deck: &nec_model::deck::NecDeck,
    solver_mode: SolverMode,
) {
    let has_ex_type1 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 1
        } else {
            false
        }
    });

    if has_ex_type1 && !matches!(solver_mode, SolverMode::Pulse) {
        eprintln!(
            "warning: EX type 1 is currently treated like EX type 0; current-source semantics are pending"
        );
    }
}

pub(super) fn warn_ex_type2_portability_semantics(deck: &nec_model::deck::NecDeck) {
    let has_ex_type2 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 2
        } else {
            false
        }
    });

    if has_ex_type2 {
        eprintln!(
            "warning: EX type 2 is currently treated like EX type 0; incident-plane-wave semantics are pending"
        );
    }
}

pub(super) fn warn_ex_type4_portability_semantics(
    deck: &nec_model::deck::NecDeck,
    solver_mode: SolverMode,
) {
    let has_ex_type4 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 4
        } else {
            false
        }
    });

    if has_ex_type4 && !matches!(solver_mode, SolverMode::Pulse) {
        eprintln!(
            "warning: EX type 4 is currently treated like EX type 0; segment-current semantics are pending"
        );
    }
}

pub(super) fn warn_ex_type5_portability_semantics(
    deck: &nec_model::deck::NecDeck,
    solver_mode: SolverMode,
) {
    let has_ex_type5 = deck.cards.iter().any(|c| {
        if let Card::Ex(ex) = c {
            ex.excitation_type == 5
        } else {
            false
        }
    });

    if has_ex_type5 && !matches!(solver_mode, SolverMode::Pulse) {
        eprintln!(
            "warning: EX type 5 is currently treated like EX type 0; qdsrc semantics are pending"
        );
    }
}

pub(super) fn warn_pt_card_deferred_support(deck: &nec_model::deck::NecDeck) {
    let has_pt = deck.cards.iter().any(|c| matches!(c, Card::Pt(_)));
    if has_pt {
        eprintln!(
            "warning: PT card support is currently deferred; PT cards are parsed for portability but ignored at runtime"
        );
    }
}

pub(super) fn warn_nt_card_deferred_support(deck: &nec_model::deck::NecDeck) {
    let has_nt = deck.cards.iter().any(|c| matches!(c, Card::Nt(_)));
    if has_nt {
        eprintln!(
            "warning: NT card support is currently deferred; NT cards are parsed for portability but ignored at runtime"
        );
    }
}
