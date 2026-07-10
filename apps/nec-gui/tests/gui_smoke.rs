// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Headless smoke tests for nec-gui (PH3-CHK-009 + PH3-CHK-010 + PH3-CHK-011).
//
// These tests exercise the AppState state machine and the solve pipeline
// without opening an iced window.  They are the CI gate for this feature.

use nec_gui::app_state::{
    ActiveTab, AppState, CurrentsPhase, Message, PatternPhase, SolvePhase, SweepPhase, SweepSortCol,
};
use nec_gui::solve::{
    current_distribution_deck_str, pattern_slice_deck_str, solve_deck_path, solve_deck_str,
    sweep_deck_str, CurrentPoint, PatternPoint, SolveResult, SweepPoint,
};
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

    let result = solve_deck_path(&deck_path, None)
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
    let result = solve_deck_path(
        std::path::Path::new("/tmp/does-not-exist-fnec-gui-test.nec"),
        None,
    );
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

/// GUI-CHK-001: the 3-D viewport tab is selectable through the same headless
/// state machine (the shader widget itself renders only under a real display).
#[test]
fn viewport_tab_selectable() {
    let mut state = AppState::default();
    state.apply(&Message::TabSelected(ActiveTab::Viewport));
    assert_eq!(state.active_tab, ActiveTab::Viewport);
    // Switching away from the viewport works too (no state trapped in the tab).
    state.apply(&Message::TabSelected(ActiveTab::Solve));
    assert_eq!(state.active_tab, ActiveTab::Solve);
}

/// GUI-CHK-002: loading a deck's geometry builds a scene mesh, bumps the scene
/// revision, and frames the camera on the geometry — all headlessly.
#[test]
fn geometry_load_builds_scene_and_fits_camera() {
    // A center-fed λ/2 dipole along z, 0.5λ ≈ 10 m at ~14 MHz.
    let deck = "\
CM dipole\nCE\nGW 1 11 0 0 -5 0 0 5 0.001\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let geo = nec_gui::solve::load_geometry_str(deck).expect("geometry builds");
    assert_eq!(geo.wires.len(), 11, "11 segments → 11 wire lines");
    assert!(!geo.has_ground, "free-space deck has no ground");
    assert!((geo.bbox_min[2] + 5.0).abs() < 1e-3 && (geo.bbox_max[2] - 5.0).abs() < 1e-3);

    let mut state = AppState::default();
    assert!(state.viewport.scene.is_none());
    let rev0 = state.viewport.scene_rev;
    state.apply(&Message::GeometryLoaded(Ok(geo)));
    let vp = &state.viewport;
    assert!(vp.scene.is_some(), "scene mesh should be built");
    assert!(vp.scene_rev > rev0, "scene revision must bump");
    // Camera framed on the geometry: target at the dipole center, backed off.
    assert!(
        vp.camera.target.z.abs() < 1e-3,
        "camera target centered on wire"
    );
    assert!(vp.camera.distance > 5.0, "camera outside the geometry");
    assert!(
        vp.status.contains("11"),
        "status reports segment count: {}",
        vp.status
    );
}

/// GUI-CHK-002: a bad deck surfaces an error and leaves no scene.
#[test]
fn geometry_load_error_clears_scene() {
    let mut state = AppState::default();
    state.apply(&Message::GeometryLoaded(Err("no geometry".into())));
    assert!(state.viewport.scene.is_none());
    assert!(state.viewport.status.starts_with("Error:"));
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

// ── Pattern state machine tests (PH3-CHK-011) ─────────────────────────────────

/// Pattern state starts Idle with a default phi field.
#[test]
fn pattern_initial_state_is_idle() {
    let state = AppState::default();
    assert_eq!(state.pattern_phase, PatternPhase::Idle);
    assert!(!state.pattern_phi_deg.is_empty());
}

/// RunPattern transitions pattern phase to Running.
#[test]
fn run_pattern_transitions_to_running() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunPattern);
    assert_eq!(state.pattern_phase, PatternPhase::Running);
    assert!(
        !state.can_run_pattern(),
        "button should be disabled while running"
    );
}

/// PatternComplete(Ok) transitions to Done.
#[test]
fn pattern_complete_ok_transitions_to_done() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunPattern);
    let pts = vec![
        PatternPoint {
            theta_deg: 0.0,
            phi_deg: 0.0,
            gain_total_dbi: -10.0,
        },
        PatternPoint {
            theta_deg: 90.0,
            phi_deg: 0.0,
            gain_total_dbi: 2.15,
        },
        PatternPoint {
            theta_deg: 180.0,
            phi_deg: 0.0,
            gain_total_dbi: -10.0,
        },
    ];
    state.apply(&Message::PatternComplete(Ok(pts)));
    assert!(matches!(state.pattern_phase, PatternPhase::Done(_)));
    assert!(
        state.can_run_pattern(),
        "button should re-enable after done"
    );
}

/// PatternComplete(Err) transitions to Failed.
#[test]
fn pattern_complete_err_transitions_to_failed() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunPattern);
    state.apply(&Message::PatternComplete(Err("no FR card".into())));
    assert!(matches!(state.pattern_phase, PatternPhase::Failed(_)));
}

/// PatternPhiChanged updates the phi field.
#[test]
fn pattern_phi_changed_updates_field() {
    let mut state = AppState::default();
    state.apply(&Message::PatternPhiChanged("90.0".into()));
    assert_eq!(state.pattern_phi_deg, "90.0");
    let phi = state.pattern_phi().expect("valid float");
    assert!((phi - 90.0).abs() < 1e-9);
}

/// pattern_phi rejects a non-float string.
#[test]
fn pattern_phi_rejects_non_float() {
    let mut state = AppState::default();
    state.apply(&Message::PatternPhiChanged("bad".into()));
    assert!(state.pattern_phi().is_err());
}

// ── Currents state machine tests (PH3-CHK-011) ────────────────────────────────

/// Currents state starts Idle.
#[test]
fn currents_initial_state_is_idle() {
    let state = AppState::default();
    assert_eq!(state.currents_phase, CurrentsPhase::Idle);
}

/// RunCurrents transitions to Running.
#[test]
fn run_currents_transitions_to_running() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunCurrents);
    assert_eq!(state.currents_phase, CurrentsPhase::Running);
    assert!(!state.can_run_currents());
}

/// CurrentsComplete(Ok) transitions to Done.
#[test]
fn currents_complete_ok_transitions_to_done() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunCurrents);
    let pts = vec![
        CurrentPoint {
            seg_idx: 0,
            position_m: 0.0,
            current_mag_ma: 0.5,
        },
        CurrentPoint {
            seg_idx: 1,
            position_m: 0.1,
            current_mag_ma: 1.0,
        },
    ];
    state.apply(&Message::CurrentsComplete(Ok(pts)));
    assert!(matches!(state.currents_phase, CurrentsPhase::Done(_)));
}

// ── Data-to-plot mapping tests (PH3-CHK-011) ──────────────────────────────────

/// pattern_display_rows returns one row per point with frac in [0, 1].
#[test]
fn pattern_display_rows_frac_in_range() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunPattern);
    let pts = vec![
        PatternPoint {
            theta_deg: 0.0,
            phi_deg: 0.0,
            gain_total_dbi: -10.0,
        },
        PatternPoint {
            theta_deg: 90.0,
            phi_deg: 0.0,
            gain_total_dbi: 2.15,
        },
        PatternPoint {
            theta_deg: 180.0,
            phi_deg: 0.0,
            gain_total_dbi: -5.0,
        },
    ];
    state.apply(&Message::PatternComplete(Ok(pts)));
    let rows = state.pattern_display_rows();
    assert_eq!(rows.len(), 3);
    for r in &rows {
        assert!(
            r.bar_width_frac >= 0.0 && r.bar_width_frac <= 1.0,
            "bar_width_frac out of range: {}",
            r.bar_width_frac
        );
    }
    // Peak gain row gets frac = 1.0
    let peak = rows
        .iter()
        .max_by(|a, b| a.gain_dbi.partial_cmp(&b.gain_dbi).unwrap())
        .unwrap();
    assert!(
        (peak.bar_width_frac - 1.0).abs() < 1e-9,
        "peak bar_width_frac should be 1.0, got {}",
        peak.bar_width_frac
    );
}

/// current_display_bars returns correct normalisation: peak segment gets frac = 1.
#[test]
fn current_display_bars_peak_is_one() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("foo.nec".into()));
    state.apply(&Message::RunCurrents);
    let pts = vec![
        CurrentPoint {
            seg_idx: 0,
            position_m: 0.0,
            current_mag_ma: 0.1,
        },
        CurrentPoint {
            seg_idx: 1,
            position_m: 0.05,
            current_mag_ma: 5.0,
        },
        CurrentPoint {
            seg_idx: 2,
            position_m: 0.1,
            current_mag_ma: 2.0,
        },
    ];
    state.apply(&Message::CurrentsComplete(Ok(pts)));
    let bars = state.current_display_bars();
    assert_eq!(bars.len(), 3);
    let peak = bars
        .iter()
        .max_by(|a, b| a.current_mag_ma.partial_cmp(&b.current_mag_ma).unwrap())
        .unwrap();
    assert!(
        (peak.bar_width_frac - 1.0).abs() < 1e-9,
        "peak frac should be 1.0, got {}",
        peak.bar_width_frac
    );
    for b in &bars {
        assert!(
            b.bar_width_frac >= 0.0 && b.bar_width_frac <= 1.0,
            "bar_width_frac out of range: {}",
            b.bar_width_frac
        );
    }
}

/// pattern_display_rows returns empty Vec when pattern is not Done.
#[test]
fn pattern_display_rows_empty_when_not_done() {
    let state = AppState::default();
    assert!(state.pattern_display_rows().is_empty());
}

/// current_display_bars returns empty Vec when currents are not Done.
#[test]
fn current_display_bars_empty_when_not_done() {
    let state = AppState::default();
    assert!(state.current_display_bars().is_empty());
}

// ── Pattern pipeline tests (PH3-CHK-011) ──────────────────────────────────────

/// pattern_slice_deck_str produces 37 elevation points for a free-space dipole.
#[test]
fn pattern_slice_deck_str_produces_elevation_slice() {
    let pts = pattern_slice_deck_str(DIPOLE_DECK, 0.0).expect("pattern failed");
    // 0, 5, 10, … 180 deg → 37 points
    assert_eq!(pts.len(), 37, "expected 37 theta points, got {}", pts.len());
}

/// Pattern theta values span 0..=180 in 5° steps.
#[test]
fn pattern_slice_theta_grid_is_correct() {
    let pts = pattern_slice_deck_str(DIPOLE_DECK, 0.0).expect("pattern failed");
    for (i, pt) in pts.iter().enumerate() {
        let expected = i as f64 * 5.0;
        assert!(
            (pt.theta_deg - expected).abs() < 1e-9,
            "theta[{i}] = {} expected {expected}",
            pt.theta_deg
        );
    }
}

/// For a free-space dipole the equatorial gain (θ=90°) should exceed the
/// end-fire gain (θ=0°) — the dipole radiates broadside, not end-fire.
#[test]
fn pattern_slice_dipole_broadside_exceeds_endfire() {
    let pts = pattern_slice_deck_str(DIPOLE_DECK, 0.0).expect("pattern failed");
    let endfire = pts
        .iter()
        .find(|p| p.theta_deg == 0.0)
        .unwrap()
        .gain_total_dbi;
    let broadside = pts
        .iter()
        .find(|p| p.theta_deg == 90.0)
        .unwrap()
        .gain_total_dbi;
    assert!(
        broadside > endfire,
        "broadside ({broadside:.2} dBi) should exceed end-fire ({endfire:.2} dBi)"
    );
}

/// pattern_slice_deck_str on the corpus free-space dipole renders correctly.
#[test]
fn pattern_slice_corpus_dipole_freesp() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");
    let pts = pattern_slice_deck_str(
        &std::fs::read_to_string(&deck_path)
            .unwrap_or_else(|e| panic!("cannot read corpus file: {e}")),
        0.0,
    )
    .expect("pattern failed for corpus dipole");
    assert_eq!(pts.len(), 37);
    // Peak gain for a half-wave dipole should be close to 2.15 dBi.
    let max_gain = pts
        .iter()
        .map(|p| p.gain_total_dbi)
        .filter(|&g| g > -500.0)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        max_gain > 1.5 && max_gain < 3.5,
        "peak gain {max_gain:.2} dBi outside expected 1.5–3.5 dBi range"
    );
}

// ── Current distribution pipeline tests (PH3-CHK-011) ────────────────────────

/// current_distribution_deck_str returns one entry per segment.
#[test]
fn current_distribution_segment_count() {
    let pts = current_distribution_deck_str(DIPOLE_DECK).expect("currents failed");
    // DIPOLE_DECK has GW with 51 segments.
    assert_eq!(pts.len(), 51, "expected 51 segments, got {}", pts.len());
}

/// Peak current magnitude is at or near the feedpoint (segment ~26 for a 51-seg
/// half-wave dipole).
#[test]
fn current_distribution_peak_near_feedpoint() {
    let pts = current_distribution_deck_str(DIPOLE_DECK).expect("currents failed");
    let peak_idx = pts
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.current_mag_ma.partial_cmp(&b.current_mag_ma).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    // Feedpoint is segment 25 (0-based middle of 51), allow ±3.
    assert!(
        (22..=28).contains(&peak_idx),
        "peak current at segment {peak_idx}, expected near 25"
    );
}

/// current_distribution_deck_str on the corpus dipole produces valid data.
#[test]
fn current_distribution_corpus_dipole_freesp() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = workspace_root.join("corpus/dipole-freesp-51seg.nec");
    let pts = current_distribution_deck_str(
        &std::fs::read_to_string(&deck_path)
            .unwrap_or_else(|e| panic!("cannot read corpus file: {e}")),
    )
    .expect("currents failed for corpus dipole");
    assert!(!pts.is_empty(), "expected at least one segment");
    let any_nonzero = pts.iter().any(|p| p.current_mag_ma > 1e-6);
    assert!(any_nonzero, "all currents are effectively zero");
}
