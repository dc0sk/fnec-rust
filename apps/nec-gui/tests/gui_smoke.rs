// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Headless smoke tests for nec-gui (PH3-CHK-009).
//
// These tests exercise the AppState state machine and the solve pipeline
// without opening an iced window.  They are the CI gate for this feature.

use nec_gui::app_state::{AppState, Message, SolvePhase};
use nec_gui::solve::{solve_deck_path, solve_deck_str, SolveResult};
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
