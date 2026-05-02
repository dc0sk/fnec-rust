// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Headless smoke tests for nec-gui (PH3-CHK-009 + PH3-CHK-010).
//
// These tests exercise the AppState state machine and the solve pipeline
// without opening an iced window.  They are the CI gate for this feature.

use nec_gui::app_state::{ActiveTab, AppState, Message, SolvePhase, SweepPhase, SweepSortCol};
use nec_gui::solve::{solve_deck_path, solve_deck_str, sweep_deck_str, SolveResult, SweepPoint};
use std::path::PathBuf;

// ── State machine tests ──────────────────────────────────────────────────────

/// Newly created state is idle and has an empty deck path.
#[test]
fn initial_state_is_idle_with_empty_path() {
    let state = AppState::default();
    assert_eq!(state.deck_path, "");
    assert_eq!(state.phase, SolvePhase::Idle);
    assert!(!state.can_solve(), "should not be solvable with empty path");
}

/// Typing a path enables the Solve button.
#[test]
fn deck_path_changed_enables_solve() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("some/path.nec".into()));
    assert_eq!(state.deck_path, "some/path.nec");
    assert!(state.can_solve());
}

/// Solve message transitions state to Solving.
#[test]
fn solve_message_transitions_to_solving() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::Solve);
    assert_eq!(state.phase, SolvePhase::Solving);
    assert!(
        !state.can_solve(),
        "Solve button should be disabled while solving"
    );
}

/// SolveComplete(Ok) transitions state to Done.
#[test]
fn solve_complete_ok_transitions_to_done() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::Solve);
    let result = SolveResult {
        freq_mhz: 14.2,
        z_re: 73.1,
        z_im: -1.5,
    };
    state.apply(&Message::SolveComplete(Ok(result.clone())));
    assert_eq!(state.phase, SolvePhase::Done(result));
    assert!(
        state.can_solve(),
        "Solve button should re-enable after completion"
    );
}

/// SolveComplete(Err) transitions state to Failed.
#[test]
fn solve_complete_err_transitions_to_failed() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::Solve);
    state.apply(&Message::SolveComplete(Err("no FR card".into())));
    assert!(matches!(state.phase, SolvePhase::Failed(_)));
    assert!(
        state.can_solve(),
        "Solve button should re-enable after failure"
    );
}

/// Changing the path after a failure clears the error state.
#[test]
fn deck_path_change_clears_failed_state() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::Solve);
    state.apply(&Message::SolveComplete(Err("oops".into())));
    assert!(matches!(state.phase, SolvePhase::Failed(_)));
    state.apply(&Message::DeckPathChanged("bar.nec".into()));
    assert_eq!(state.phase, SolvePhase::Idle);
}

/// Status text contains "Ready" in idle state.
#[test]
fn status_text_idle() {
    let state = AppState::default();
    assert!(
        state.status_text().contains("Ready"),
        "unexpected: {}",
        state.status_text()
    );
}

/// Status text contains "Solving" while in Solving phase.
#[test]
fn status_text_solving() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("a.nec".into()));
    state.apply(&Message::Solve);
    assert!(
        state.status_text().contains("Solving"),
        "unexpected: {}",
        state.status_text()
    );
}

/// Status text in Done phase contains the frequency and impedance.
#[test]
fn status_text_done_contains_impedance() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("a.nec".into()));
    state.apply(&Message::Solve);
    state.apply(&Message::SolveComplete(Ok(SolveResult {
        freq_mhz: 14.2,
        z_re: 73.1,
        z_im: -1.5,
    })));
    let s = state.status_text();
    assert!(s.contains("14.2") || s.contains("MHz"), "freq missing: {s}");
    assert!(s.contains("73"), "Z_re missing: {s}");
}

// ── Solve pipeline tests ─────────────────────────────────────────────────────

/// solve_deck_str produces a plausible impedance for a simple dipole.
#[test]
fn solve_deck_str_dipole_produces_impedance() {
    const DECK: &str = "\
GW 1 51 0 0 -5.232 0 0 5.232 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
";
    let result = solve_deck_str(DECK).expect("solve failed");
    assert!(
        (result.freq_mhz - 14.2).abs() < 0.001,
        "freq mismatch: {}",
        result.freq_mhz
    );
    // At near-resonance the resistance should be roughly 50-100 Ω.
    assert!(
        result.z_re > 40.0 && result.z_re < 120.0,
        "Z_re = {} Ω out of range",
        result.z_re
    );
    // Reactance should be small near resonance.
    assert!(
        result.z_im.abs() < 20.0,
        "Z_im = {} Ω unexpectedly large",
        result.z_im
    );
}

/// solve_deck_path succeeds on the corpus free-space dipole.
#[test]
fn solve_corpus_dipole_freesp() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");

    let result = solve_deck_path(&deck_path)
        .unwrap_or_else(|e| panic!("solve failed for corpus dipole: {e}"));

    // Reference impedance: Z ≈ 73 + j42 Ω (Hallen, 14.2 MHz).
    assert!(
        result.z_re > 50.0 && result.z_re < 120.0,
        "Z_re = {:.3} Ω out of expected range",
        result.z_re
    );
    // Frequency should come from the FR card.
    assert!(result.freq_mhz > 0.0, "frequency must be positive");
}

/// solve_deck_path returns Err for a non-existent file.
#[test]
fn solve_deck_path_nonexistent_file_returns_err() {
    let result = solve_deck_path(std::path::Path::new(
        "/tmp/does-not-exist-fnec-gui-test.nec",
    ));
    assert!(result.is_err(), "expected Err for nonexistent file");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("cannot read"),
        "unexpected error message: {msg}"
    );
}

/// solve_deck_str with a deck missing a FR card returns Err.
#[test]
fn solve_deck_str_no_fr_card_returns_err() {
    const DECK_NO_FR: &str = "\
GW 1 51 0 0 -5.0 0 0 5.0 0.001
GE
EX 0 1 26 0 1.0 0.0
EN
";
    let result = solve_deck_str(DECK_NO_FR);
    assert!(result.is_err(), "expected Err with missing FR card");
}

// ── Sweep state machine tests (PH3-CHK-010) ──────────────────────────────────

const DIPOLE_DECK: &str = "\
GW 1 51 0 0 -5.232 0 0 5.232 0.001
GE
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
EN
";

/// Sweep state starts Idle with default frequency fields.
#[test]
fn sweep_initial_state() {
    let state = AppState::default();
    assert_eq!(state.sweep_phase, SweepPhase::Idle);
    assert!(
        !state.sweep_start.is_empty(),
        "sweep_start should have a default"
    );
    assert!(
        !state.sweep_end.is_empty(),
        "sweep_end should have a default"
    );
    assert!(
        !state.sweep_step.is_empty(),
        "sweep_step should have a default"
    );
}

/// Editing sweep frequency fields updates the state.
#[test]
fn sweep_field_changes_update_state() {
    let mut state = AppState::default();
    state.apply(&Message::SweepStartChanged("10.0".into()));
    state.apply(&Message::SweepEndChanged("20.0".into()));
    state.apply(&Message::SweepStepChanged("1.0".into()));
    assert_eq!(state.sweep_start, "10.0");
    assert_eq!(state.sweep_end, "20.0");
    assert_eq!(state.sweep_step, "1.0");
}

/// RunSweep transitions sweep phase to Running.
#[test]
fn run_sweep_transitions_to_running() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunSweep);
    assert_eq!(state.sweep_phase, SweepPhase::Running);
    assert!(
        !state.can_sweep(),
        "Run Sweep button should be disabled while running"
    );
}

/// SweepComplete(Ok) transitions sweep phase to Done with correct point count.
#[test]
fn sweep_complete_ok_transitions_to_done() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunSweep);
    let pts = vec![
        SweepPoint {
            freq_mhz: 14.0,
            z_re: 70.0,
            z_im: -2.0,
        },
        SweepPoint {
            freq_mhz: 15.0,
            z_re: 75.0,
            z_im: 5.0,
        },
        SweepPoint {
            freq_mhz: 16.0,
            z_re: 80.0,
            z_im: 12.0,
        },
    ];
    state.apply(&Message::SweepComplete(Ok(pts.clone())));
    assert!(matches!(state.sweep_phase, SweepPhase::Done(_)));
    assert_eq!(state.sorted_sweep_rows().len(), 3);
}

/// SweepComplete(Err) transitions sweep phase to Failed.
#[test]
fn sweep_complete_err_transitions_to_failed() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunSweep);
    state.apply(&Message::SweepComplete(Err("parse failed".into())));
    assert!(matches!(state.sweep_phase, SweepPhase::Failed(_)));
    assert!(
        state.can_sweep(),
        "Run Sweep button should re-enable after failure"
    );
}

/// Tab switching updates active_tab without affecting solve or sweep state.
#[test]
fn tab_switching_changes_active_tab() {
    let mut state = AppState::default();
    assert_eq!(state.active_tab, ActiveTab::Solve);
    state.apply(&Message::TabSelected(ActiveTab::Sweep));
    assert_eq!(state.active_tab, ActiveTab::Sweep);
    state.apply(&Message::TabSelected(ActiveTab::Solve));
    assert_eq!(state.active_tab, ActiveTab::Solve);
}

/// sorted_sweep_rows returns rows sorted by |Z| descending when requested.
#[test]
fn sorted_sweep_rows_zmag_descending() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunSweep);
    let pts = vec![
        SweepPoint {
            freq_mhz: 14.0,
            z_re: 3.0,
            z_im: 4.0,
        }, // |Z|=5
        SweepPoint {
            freq_mhz: 15.0,
            z_re: 6.0,
            z_im: 8.0,
        }, // |Z|=10
        SweepPoint {
            freq_mhz: 16.0,
            z_re: 0.0,
            z_im: 1.0,
        }, // |Z|=1
    ];
    state.apply(&Message::SweepComplete(Ok(pts)));
    // Sort by |Z| ascending first click, then toggle to descending.
    state.apply(&Message::SweepSortBy(SweepSortCol::ZMag));
    state.apply(&Message::SweepSortBy(SweepSortCol::ZMag));
    let rows = state.sorted_sweep_rows();
    assert!(
        rows[0].freq_mhz == 15.0,
        "expected highest |Z| first, got freq_mhz = {}",
        rows[0].freq_mhz
    );
}

/// sweep_params rejects start >= end.
#[test]
fn sweep_params_start_ge_end_is_error() {
    let mut state = AppState::default();
    state.apply(&Message::SweepStartChanged("20.0".into()));
    state.apply(&Message::SweepEndChanged("10.0".into()));
    state.apply(&Message::SweepStepChanged("1.0".into()));
    assert!(state.sweep_params().is_err());
}

/// sweep_params rejects non-positive step.
#[test]
fn sweep_params_zero_step_is_error() {
    let mut state = AppState::default();
    state.apply(&Message::SweepStartChanged("10.0".into()));
    state.apply(&Message::SweepEndChanged("20.0".into()));
    state.apply(&Message::SweepStepChanged("0.0".into()));
    assert!(state.sweep_params().is_err());
}

// ── Sweep pipeline tests (PH3-CHK-010) ───────────────────────────────────────

/// sweep_deck_str produces correct number of points for a 5-step sweep.
#[test]
fn sweep_deck_str_produces_five_points() {
    let pts = sweep_deck_str(DIPOLE_DECK, 14.0, 15.0, 0.25).expect("sweep failed");
    // 14.0, 14.25, 14.5, 14.75, 15.0 → 5 points
    assert_eq!(pts.len(), 5, "expected 5 points, got {}", pts.len());
}

/// Frequencies in sweep output match the requested grid.
#[test]
fn sweep_deck_str_freqs_match_grid() {
    let pts = sweep_deck_str(DIPOLE_DECK, 14.0, 14.4, 0.1).expect("sweep failed");
    let expected = [14.0_f64, 14.1, 14.2, 14.3, 14.4];
    assert_eq!(pts.len(), expected.len());
    for (pt, exp) in pts.iter().zip(expected.iter()) {
        assert!(
            (pt.freq_mhz - exp).abs() < 1e-6,
            "freq mismatch: {} vs {exp}",
            pt.freq_mhz
        );
    }
}

/// Impedance values from a sweep are physically plausible for a near-resonant dipole.
#[test]
fn sweep_deck_str_impedances_are_plausible() {
    let pts = sweep_deck_str(DIPOLE_DECK, 13.0, 16.0, 1.0).expect("sweep failed");
    assert_eq!(pts.len(), 4);
    for pt in &pts {
        assert!(pt.z_re > 0.0, "Z_re must be positive, got {}", pt.z_re);
    }
}

/// sweep_deck_str rejects invalid parameters (step <= 0).
#[test]
fn sweep_deck_str_rejects_zero_step() {
    let result = sweep_deck_str(DIPOLE_DECK, 14.0, 15.0, 0.0);
    assert!(result.is_err(), "expected Err for zero step");
}

/// sweep_deck_str rejects start >= end.
#[test]
fn sweep_deck_str_rejects_start_ge_end() {
    let result = sweep_deck_str(DIPOLE_DECK, 15.0, 14.0, 0.5);
    assert!(result.is_err(), "expected Err for start >= end");
}
