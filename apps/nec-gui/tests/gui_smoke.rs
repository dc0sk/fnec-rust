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
        warnings: Vec::new(),
        feed_tag: 1,
        feed_seg: 26,
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
        warnings: Vec::new(),
        feed_tag: 1,
        feed_seg: 26,
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
    let result = solve_deck_str(DECK, nec_gui::solve::SolverKind::Hallen).expect("solve failed");
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

    let result = solve_deck_path(&deck_path, None, nec_gui::solve::SolverKind::Hallen)
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
        nec_gui::solve::SolverKind::Hallen,
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
    let result = solve_deck_str(DECK_NO_FR, nec_gui::solve::SolverKind::Hallen);
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
    assert!(matches!(state.sweep_phase, SweepPhase::Failed(..)));
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

/// GUI-CHK-003: viewport camera messages mutate the camera through `apply`, and
/// Reset View re-frames on the loaded geometry.
#[test]
fn viewport_camera_messages_move_and_reset() {
    use nec_gui::app_state::ViewportMsg;
    let deck = "CM\nCE\nGW 1 11 0 0 -5 0 0 5 0.001\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let geo = nec_gui::solve::load_geometry_str(deck).unwrap();
    let mut state = AppState::default();
    state.apply(&Message::GeometryLoaded(Ok(geo)));
    let fit = state.viewport.camera;

    // Orbit changes yaw/pitch.
    state.apply(&Message::Viewport(ViewportMsg::Orbit {
        d_yaw: 0.3,
        d_pitch: 0.1,
    }));
    assert!((state.viewport.camera.yaw - fit.yaw).abs() > 1e-6);
    // Zoom in shrinks distance.
    state.apply(&Message::Viewport(ViewportMsg::Zoom(1.0)));
    assert!(state.viewport.camera.distance < fit.distance);
    // Pan moves the look-at target.
    state.apply(&Message::Viewport(ViewportMsg::Pan { dx: 0.1, dy: 0.0 }));
    assert!(state.viewport.camera.target != fit.target);

    // Reset View restores the full loaded framing (orientation + fit).
    state.apply(&Message::Viewport(ViewportMsg::ResetView));
    assert_eq!(state.viewport.camera, fit);
}

/// GUI-CHK-004: solving currents attaches per-segment magnitudes, enables
/// coloring, exposes a legend range, and the toggle rebuilds the scene.
#[test]
fn currents_solve_colors_wires_and_toggles() {
    // Center-fed λ/2 dipole → current peaks at the feed (middle segment).
    let deck = "CM\nCE\nGW 1 11 0 0 -5 0 0 5 0.001\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let gc = nec_gui::solve::load_currents_str(deck, nec_gui::solve::SolverKind::Hallen)
        .expect("currents solve");
    assert_eq!(gc.currents_ma.len(), 11);
    // The peak |I| is at (or adjacent to) the center-fed segment, not a tip.
    let peak_i = gc
        .currents_ma
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert!(
        (4..=6).contains(&peak_i),
        "current should peak near center, got seg {peak_i}"
    );

    let mut state = AppState::default();
    let rev0 = state.viewport.scene_rev;
    state.apply(&Message::CurrentsSolved(Ok(gc)));
    assert!(state.viewport.show_currents, "currents coloring turns on");
    assert!(state.viewport.currents_ma.is_some());
    assert!(state.viewport.scene.is_some());
    assert!(state.viewport.scene_rev > rev0);
    let (lo, hi) = state.viewport.current_range_ma().expect("legend range");
    assert!(hi > lo && lo >= 0.0, "legend range {lo}–{hi}");

    // Toggling off rebuilds the scene (uniform color) and bumps the revision.
    let rev1 = state.viewport.scene_rev;
    state.apply(&Message::ToggleCurrents(false));
    assert!(!state.viewport.show_currents);
    assert!(state.viewport.scene_rev > rev1);
}

/// GUI-CHK-006: the pane-resize message is a pure layout concern (handled in the
/// iced binary), so `apply` leaves the core solver state untouched.
#[test]
fn pane_resize_is_a_layout_noop_on_core_state() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("deck.nec".into()));
    let before = state.deck_path.clone();
    state.apply(&Message::PaneResized(0.3));
    assert_eq!(
        state.deck_path, before,
        "pane resize must not touch core state"
    );
    assert!(state.can_solve());
}

/// GUI-CHK-005: solving the pattern builds a lobe overlay that toggles on/off.
#[test]
fn pattern_solve_builds_lobe_and_toggles() {
    let deck = "CM\nCE\nGW 1 11 0 0 -5 0 0 5 0.001\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let ps = nec_gui::solve::pattern_grid_str(deck, nec_gui::solve::SolverKind::Hallen)
        .expect("pattern solve");
    assert_eq!(ps.grid.gains_dbi.len(), ps.grid.n_theta * ps.grid.n_phi);

    let mut state = AppState::default();
    state.apply(&Message::Pattern3dComplete(Ok(ps)));
    assert!(state.viewport.show_pattern, "pattern overlay turns on");
    let lobe = state.viewport.lobe.as_ref().expect("lobe built");
    assert!(lobe.triangle_count() > 1000, "lobe has a triangle surface");
    assert!(
        state.viewport.scene.is_some(),
        "wires still present under the lobe"
    );
    let lrev = state.viewport.lobe_rev;

    // Toggling off drops the lobe and bumps its revision (renderer stops drawing).
    state.apply(&Message::TogglePattern(false));
    assert!(!state.viewport.show_pattern);
    assert!(state.viewport.lobe.is_none());
    assert!(state.viewport.lobe_rev > lrev);
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
    let pts = sweep_deck_str(
        DIPOLE_DECK,
        14.0,
        15.0,
        0.25,
        nec_gui::solve::SolverKind::Hallen,
    )
    .expect("sweep failed");
    // 14.0, 14.25, 14.5, 14.75, 15.0 → 5 points
    assert_eq!(pts.len(), 5, "expected 5 points, got {}", pts.len());
}

/// Frequencies in sweep output match the requested grid.
#[test]
fn sweep_deck_str_freqs_match_grid() {
    let pts = sweep_deck_str(
        DIPOLE_DECK,
        14.0,
        14.4,
        0.1,
        nec_gui::solve::SolverKind::Hallen,
    )
    .expect("sweep failed");
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
    let pts = sweep_deck_str(
        DIPOLE_DECK,
        13.0,
        16.0,
        1.0,
        nec_gui::solve::SolverKind::Hallen,
    )
    .expect("sweep failed");
    assert_eq!(pts.len(), 4);
    for pt in &pts {
        assert!(pt.z_re > 0.0, "Z_re must be positive, got {}", pt.z_re);
    }
}

/// sweep_deck_str rejects invalid parameters (step <= 0).
#[test]
fn sweep_deck_str_rejects_zero_step() {
    let result = sweep_deck_str(
        DIPOLE_DECK,
        14.0,
        15.0,
        0.0,
        nec_gui::solve::SolverKind::Hallen,
    );
    assert!(result.is_err(), "expected Err for zero step");
}

/// sweep_deck_str rejects start >= end.
#[test]
fn sweep_deck_str_rejects_start_ge_end() {
    let result = sweep_deck_str(
        DIPOLE_DECK,
        15.0,
        14.0,
        0.5,
        nec_gui::solve::SolverKind::Hallen,
    );
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
    let pts = pattern_slice_deck_str(DIPOLE_DECK, 0.0, nec_gui::solve::SolverKind::Hallen)
        .expect("pattern failed");
    // 0, 5, 10, … 180 deg → 37 points
    assert_eq!(pts.len(), 37, "expected 37 theta points, got {}", pts.len());
}

/// Pattern theta values span 0..=180 in 5° steps.
#[test]
fn pattern_slice_theta_grid_is_correct() {
    let pts = pattern_slice_deck_str(DIPOLE_DECK, 0.0, nec_gui::solve::SolverKind::Hallen)
        .expect("pattern failed");
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
    let pts = pattern_slice_deck_str(DIPOLE_DECK, 0.0, nec_gui::solve::SolverKind::Hallen)
        .expect("pattern failed");
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
        nec_gui::solve::SolverKind::Hallen,
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
    let pts = current_distribution_deck_str(DIPOLE_DECK, nec_gui::solve::SolverKind::Hallen)
        .expect("currents failed");
    // DIPOLE_DECK has GW with 51 segments.
    assert_eq!(pts.len(), 51, "expected 51 segments, got {}", pts.len());
}

/// Peak current magnitude is at or near the feedpoint (segment ~26 for a 51-seg
/// half-wave dipole).
#[test]
fn current_distribution_peak_near_feedpoint() {
    let pts = current_distribution_deck_str(DIPOLE_DECK, nec_gui::solve::SolverKind::Hallen)
        .expect("currents failed");
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
        nec_gui::solve::SolverKind::Hallen,
    )
    .expect("currents failed for corpus dipole");
    assert!(!pts.is_empty(), "expected at least one segment");
    let any_nonzero = pts.iter().any(|p| p.current_mag_ma > 1e-6);
    assert!(any_nonzero, "all currents are effectively zero");
}

// ── Wire editor tests (GUI-CHK-007) ──────────────────────────────────────────

use nec_gui::model_doc::WireField;
use nec_gui::solve::load_model_doc_str;

const EDITOR_DECK: &str = "\
CM editor test
CE
GW 1 11 0 0 -2.5 0 0 2.5 0.001
GE 0
EX 0 1 6 0 1
FR 0 1 14.2 0
EN
";

fn loaded_editor() -> AppState {
    let mut state = AppState::default();
    let doc = load_model_doc_str(EDITOR_DECK).expect("parse doc");
    state.apply(&Message::EditDeckLoaded(Ok(doc)));
    state
}

/// Loading a deck into the editor populates the wire table and builds a live
/// 3-D preview, without marking the document dirty.
#[test]
fn editor_load_populates_table_and_previews() {
    let state = loaded_editor();
    assert!(state.editor.loaded);
    assert_eq!(state.editor.doc.wire_count(), 1);
    assert!(!state.editor.doc.dirty, "a fresh load is not dirty");
    assert!(state.editor.error.is_none());
    assert!(
        state.viewport.scene.is_some(),
        "preview mesh should be built on load"
    );
}

/// Editing a coordinate rebuilds the preview (new scene revision) and flags the
/// document dirty.
#[test]
fn editor_edit_rebuilds_preview_and_marks_dirty() {
    let mut state = loaded_editor();
    let rev_before = state.viewport.scene_rev;
    state.apply(&Message::EditWireField {
        row: 0,
        field: WireField::Z2,
        value: "3.0".into(),
    });
    assert!(state.editor.doc.dirty, "edit marks dirty");
    assert_ne!(
        state.viewport.scene_rev, rev_before,
        "editing should rebuild the preview mesh"
    );
    assert!(state.editor.error.is_none());
}

/// Add and delete operate on the table and keep the preview valid.
#[test]
fn editor_add_and_delete_wire() {
    let mut state = loaded_editor();
    state.apply(&Message::EditWireAdd);
    assert_eq!(state.editor.doc.wire_count(), 2);
    assert!(state.viewport.scene.is_some());
    state.apply(&Message::EditWireDelete(1));
    assert_eq!(state.editor.doc.wire_count(), 1);
}

/// An invalid edit records an error but leaves the last good preview on screen.
#[test]
fn editor_invalid_edit_sets_error_keeps_last_preview() {
    let mut state = loaded_editor();
    let good_scene = state.viewport.scene.clone();
    state.apply(&Message::EditWireField {
        row: 0,
        field: WireField::Radius,
        value: "0".into(), // radius must be > 0
    });
    assert!(
        state.editor.error.is_some(),
        "invalid radius reports an error"
    );
    assert!(
        state.viewport.scene.is_some(),
        "the last valid preview is retained"
    );
    // Fixing the value clears the error and rebuilds.
    state.apply(&Message::EditWireField {
        row: 0,
        field: WireField::Radius,
        value: "0.002".into(),
    });
    assert!(state.editor.error.is_none());
    assert!(state.viewport.scene.is_some());
    let _ = good_scene;
}

/// A successful save clears the dirty flag and reports the path.
#[test]
fn editor_save_marks_clean() {
    let mut state = loaded_editor();
    state.apply(&Message::EditWireField {
        row: 0,
        field: WireField::Z2,
        value: "3.0".into(),
    });
    assert!(state.editor.doc.dirty);
    state.apply(&Message::DeckSaved(Ok("/tmp/out.nec".into())));
    assert!(!state.editor.doc.dirty, "save clears the dirty flag");
    assert!(state.editor.save_status.contains("/tmp/out.nec"));
}

/// Editing invalidates a previously shown currents overlay (the solve is stale).
#[test]
fn editor_edit_clears_stale_currents_overlay() {
    let mut state = loaded_editor();
    // Simulate a currents solve having painted the wires.
    state.viewport.show_currents = true;
    state.viewport.currents_ma = Some(vec![1.0, 2.0]);
    state.apply(&Message::EditWireField {
        row: 0,
        field: WireField::Z1,
        value: "-3.0".into(),
    });
    assert!(
        !state.viewport.show_currents,
        "edit turns off stale coloring"
    );
    assert!(state.viewport.currents_ma.is_none());
}

/// Undo/redo via messages restores and re-applies a wire edit, and refreshes
/// the 3-D preview each time.
#[test]
fn editor_undo_redo_restores_state() {
    let mut state = loaded_editor();
    assert_eq!(state.editor.doc.wires[0].z2, "2.5");
    state.apply(&Message::EditWireField {
        row: 0,
        field: WireField::Z2,
        value: "3.0".into(),
    });
    assert_eq!(state.editor.doc.wires[0].z2, "3.0");
    let rev_after_edit = state.viewport.scene_rev;

    state.apply(&Message::EditUndo);
    assert_eq!(state.editor.doc.wires[0].z2, "2.5", "undo restores");
    assert_ne!(
        state.viewport.scene_rev, rev_after_edit,
        "undo rebuilds the preview"
    );

    state.apply(&Message::EditRedo);
    assert_eq!(state.editor.doc.wires[0].z2, "3.0", "redo re-applies");
}

/// Undo reverses an Add, and undo/redo on an empty history is a harmless no-op.
#[test]
fn editor_undo_add_and_empty_noop() {
    let mut state = loaded_editor();
    // Empty history: undo does nothing, no panic.
    state.apply(&Message::EditUndo);
    assert_eq!(state.editor.doc.wire_count(), 1);

    state.apply(&Message::EditWireAdd);
    assert_eq!(state.editor.doc.wire_count(), 2);
    state.apply(&Message::EditUndo);
    assert_eq!(
        state.editor.doc.wire_count(),
        1,
        "undo removes the added wire"
    );
}

// ── Control-card editor + Apply-and-Solve (GUI-CHK-008) ──────────────────────

use nec_gui::model_doc::{ControlEdit, PostSlot};

/// Editing a control card (FR frequency) updates the document and the rendered
/// deck text.
#[test]
fn editor_control_edit_changes_frequency() {
    let mut state = loaded_editor();
    let fr_slot = state
        .editor
        .doc
        .post_slots()
        .iter()
        .position(|s| matches!(s, PostSlot::Fr(_)))
        .expect("EDITOR_DECK has an FR card");
    state.apply(&Message::EditControl {
        slot: fr_slot,
        edit: ControlEdit::FrFrequency("21.0".into()),
    });
    assert!(state.editor.doc.dirty);
    let text = state.editor.doc.to_deck_string().expect("valid deck");
    assert!(
        text.contains("21"),
        "rendered deck should carry the new frequency:\n{text}"
    );
}

/// A control edit to the wrong slot kind is a no-op (defensive against stale UI
/// messages).
#[test]
fn editor_control_edit_kind_mismatch_is_ignored() {
    let mut state = loaded_editor();
    let fr_slot = state
        .editor
        .doc
        .post_slots()
        .iter()
        .position(|s| matches!(s, PostSlot::Fr(_)))
        .unwrap();
    let before = state.editor.doc.to_deck_string().unwrap();
    // Send an LD edit to the FR slot — must not change anything.
    state.apply(&Message::EditControl {
        slot: fr_slot,
        edit: ControlEdit::LdF1("999".into()),
    });
    assert_eq!(state.editor.doc.to_deck_string().unwrap(), before);
}

/// Apply + Solve validates the edited deck and enters the Solving phase; a
/// completion transitions to Done.
#[test]
fn editor_apply_solve_enters_solving_then_done() {
    let mut state = loaded_editor();
    state.apply(&Message::EditApplySolve);
    assert!(
        matches!(state.phase, SolvePhase::Solving),
        "a valid edited deck should start solving"
    );
    state.apply(&Message::SolveComplete(Ok(SolveResult {
        freq_mhz: 14.2,
        z_re: 73.0,
        z_im: 5.0,
        warnings: Vec::new(),
        feed_tag: 1,
        feed_seg: 26,
    })));
    assert!(matches!(state.phase, SolvePhase::Done(_)));
}

/// Apply + Solve on an invalid edit records the error and does not start solving.
#[test]
fn editor_apply_solve_rejects_invalid_deck() {
    let mut state = loaded_editor();
    state.apply(&Message::EditWireField {
        row: 0,
        field: WireField::Radius,
        value: "0".into(), // invalid → deck won't render
    });
    state.apply(&Message::EditApplySolve);
    assert!(
        !matches!(state.phase, SolvePhase::Solving),
        "an invalid deck must not start solving"
    );
    assert!(state.editor.error.is_some());
}

/// Adding a ground card to a free-space deck inserts a GN editor and keeps the
/// deck renderable; deleting it restores the original.
#[test]
fn editor_add_and_delete_ground_card() {
    use nec_gui::model_doc::ControlKind;
    let mut state = loaded_editor();
    let gn_before = state
        .editor
        .doc
        .post_slots()
        .iter()
        .filter(|s| matches!(s, PostSlot::Gn(_)))
        .count();
    assert_eq!(gn_before, 0, "EDITOR_DECK is free-space");

    state.apply(&Message::EditAddControl(ControlKind::Gn));
    let gn_slot = state
        .editor
        .doc
        .post_slots()
        .iter()
        .position(|s| matches!(s, PostSlot::Gn(_)))
        .expect("ground card was added");
    assert!(state.editor.doc.dirty);
    assert!(state.editor.doc.to_deck_string().is_ok(), "GN 1 is valid");

    state.apply(&Message::EditDeleteControl(gn_slot));
    assert!(
        !state
            .editor
            .doc
            .post_slots()
            .iter()
            .any(|s| matches!(s, PostSlot::Gn(_))),
        "ground card removed"
    );
}

/// Add-control is undoable in one step.
#[test]
fn editor_add_control_is_undoable() {
    use nec_gui::model_doc::ControlKind;
    let mut state = loaded_editor();
    state.apply(&Message::EditAddControl(ControlKind::Ld));
    assert!(state
        .editor
        .doc
        .post_slots()
        .iter()
        .any(|s| matches!(s, PostSlot::Ld(_))));
    state.apply(&Message::EditUndo);
    assert!(
        !state
            .editor
            .doc
            .post_slots()
            .iter()
            .any(|s| matches!(s, PostSlot::Ld(_))),
        "undo removes the added load"
    );
}

// ── Sweep chart cursor + metric (GUI-CHK-009) ────────────────────────────────

use nec_gui::plot::PlotMetric;

fn done_sweep() -> AppState {
    let mut state = AppState::default();
    // Through `RunSweep` first, as the app always does: the button sets `Running`
    // before the task is spawned, and a completion is now only accepted while a
    // sweep is actually in flight (so a switched-away sweep cannot refill a
    // cleared chart). Skipping it here made the helper unfaithful, not the guard
    // wrong.
    state.apply(&Message::RunSweep);
    let pts = vec![
        SweepPoint {
            freq_mhz: 14.0,
            z_re: 60.0,
            z_im: -20.0,
        },
        SweepPoint {
            freq_mhz: 15.0,
            z_re: 72.0,
            z_im: 0.0,
        },
        SweepPoint {
            freq_mhz: 16.0,
            z_re: 90.0,
            z_im: 30.0,
        },
    ];
    state.apply(&Message::SweepComplete(Ok(pts)));
    state
}

#[test]
fn sweep_metric_and_cursor_messages() {
    let mut state = AppState::default();
    assert_eq!(state.sweep_metric, PlotMetric::Swr);
    state.apply(&Message::SweepMetricSelected(PlotMetric::ZMag));
    assert_eq!(state.sweep_metric, PlotMetric::ZMag);
    // Cursor is clamped to [0, 1].
    state.apply(&Message::SweepCursorChanged(1.5));
    assert!((state.sweep_cursor - 1.0).abs() < 1e-6);
    state.apply(&Message::SweepCursorChanged(-0.2));
    assert!(state.sweep_cursor.abs() < 1e-6);
}

#[test]
fn sweep_cursor_selects_nearest_point() {
    let mut state = done_sweep();
    // Cursor at the far right → highest swept frequency.
    state.apply(&Message::SweepCursorChanged(1.0));
    let p = state
        .sweep_cursor_point()
        .expect("a point under the cursor");
    assert!((p.freq_mhz - 16.0).abs() < 1e-9);
    // Cursor at the left → lowest frequency.
    state.apply(&Message::SweepCursorChanged(0.0));
    assert!((state.sweep_cursor_point().unwrap().freq_mhz - 14.0).abs() < 1e-9);
    // Middle → the 15 MHz point.
    state.apply(&Message::SweepCursorChanged(0.5));
    assert!((state.sweep_cursor_point().unwrap().freq_mhz - 15.0).abs() < 1e-9);
}

#[test]
fn sweep_cursor_point_none_without_sweep() {
    let state = AppState::default();
    assert!(state.sweep_cursor_point().is_none());
    assert!(state.sweep_points().is_empty());
}

// ── Streaming sweep (GUI-CHK-009) ────────────────────────────────────────────

#[test]
fn streaming_sweep_accumulates_points_then_finalizes() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("d.nec".into()));
    state.apply(&Message::RunSweep); // → Running
    assert_eq!(state.sweep_phase, SweepPhase::Running);

    // Points arrive incrementally.
    state.apply(&Message::SweepPointComputed(SweepPoint {
        freq_mhz: 14.0,
        z_re: 60.0,
        z_im: -10.0,
    }));
    assert!(matches!(state.sweep_phase, SweepPhase::Streaming(ref p) if p.len() == 1));
    // The chart/table can already read the partial data, and Run is disabled.
    assert_eq!(state.sweep_points().len(), 1);
    assert!(!state.can_sweep());

    state.apply(&Message::SweepPointComputed(SweepPoint {
        freq_mhz: 15.0,
        z_re: 72.0,
        z_im: 0.0,
    }));
    assert!(state.sweep_status_text().contains('2'));

    state.apply(&Message::SweepStreamDone);
    assert!(matches!(state.sweep_phase, SweepPhase::Done(ref p) if p.len() == 2));
    assert!(state.can_sweep(), "Run re-enables once the sweep is done");
}

#[test]
fn streaming_sweep_empty_stream_is_a_failure() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("d.nec".into()));
    state.apply(&Message::RunSweep);
    // Force a Streaming phase with no points, then finish.
    state.apply(&Message::SweepPointComputed(SweepPoint {
        freq_mhz: 14.0,
        z_re: 1.0,
        z_im: 0.0,
    }));
    if let SweepPhase::Streaming(pts) = &mut state.sweep_phase {
        pts.clear();
    }
    state.apply(&Message::SweepStreamDone);
    assert!(matches!(state.sweep_phase, SweepPhase::Failed(..)));
}

// ── Sweep caveat lifetime (FND-014) ──────────────────────────────────────────

/// The caveat is rendered in every phase, so it must not outlive the sweep that
/// earned it. It did: `RunSweep` set the phase and left `sweep_caveat` alone, so
/// loading a clean deck after a junctioned one kept the old deck's
/// "N of M points report negative feedpoint resistance" line on screen beside
/// unrelated results — or beside an error, if the new run failed at `prepare`.
///
/// Nothing exercised this message *sequence* before: the unit tests covered the
/// caveat's content and the smoke tests covered the phase machine, and the gap
/// was between them.
#[test]
fn a_new_sweep_clears_the_previous_sweeps_caveat() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("bent.nec".into()));
    state.apply(&Message::RunSweep);
    state.apply(&Message::SweepPointComputed(SweepPoint {
        freq_mhz: 14.0,
        z_re: -6.0,
        z_im: -1100.0,
    }));
    state.apply(&Message::SweepCaveats(vec![
        "3 of 3 sweep points report negative feedpoint resistance".into(),
    ]));
    state.apply(&Message::SweepStreamDone);
    assert!(!state.sweep_caveats.is_empty(), "fixture assumption");

    // A second run on a different deck must start clean, before any point arrives.
    state.apply(&Message::DeckPathChanged("clean.nec".into()));
    state.apply(&Message::RunSweep);
    assert!(
        state.sweep_caveats.is_empty(),
        "a caveat from the previous deck must not survive into a new sweep: {:?}",
        state.sweep_caveats
    );
}

/// A sweep that fails partway still owes a caveat for everything it computed.
/// That is why the caveat is its own message rather than a payload on
/// `SweepStreamDone`, which this path never sends.
///
/// This pins the reducer. The *send* on that path is inside the stream closure in
/// `FnecGui::update` and is not pinned by anything — deleting it fails no test
/// (FND-034).
#[test]
fn a_sweep_that_fails_partway_still_carries_the_caveat_for_what_it_showed() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("bent.nec".into()));
    state.apply(&Message::RunSweep);
    state.apply(&Message::SweepPointComputed(SweepPoint {
        freq_mhz: 14.0,
        z_re: -6.0,
        z_im: -1100.0,
    }));
    state.apply(&Message::SweepCaveats(vec![
        "1 of 1 sweep points report negative feedpoint resistance".into(),
    ]));
    state.apply(&Message::SweepComplete(Err("worker died".into())));
    assert!(matches!(state.sweep_phase, SweepPhase::Failed(..)));
    assert!(
        !state.sweep_caveats.is_empty(),
        "the points already shown still earn their caveat"
    );
}

// ── Viewport view options (GUI-CHK-010) ──────────────────────────────────────

#[test]
fn viewport_axes_and_grid_toggles_rebuild_scene() {
    let mut state = AppState::default();
    // Load a grounded geometry so both axes and grid are present.
    let geo = nec_gui::solve::load_geometry_str("GW 1 5 0 0 1 0 0 3 0.001\nGE 1\nGN 1\nEN\n")
        .expect("geometry");
    state.apply(&Message::GeometryLoaded(Ok(geo)));
    assert!(state.viewport.scene_opts.show_axes);
    assert!(state.viewport.scene_opts.show_grid);
    let full = state.viewport.scene.as_ref().unwrap().vertices.len();

    // Turning axes off rebuilds with fewer vertices.
    let rev = state.viewport.scene_rev;
    state.apply(&Message::ToggleAxes(false));
    assert!(!state.viewport.scene_opts.show_axes);
    assert_ne!(state.viewport.scene_rev, rev);
    let no_axes = state.viewport.scene.as_ref().unwrap().vertices.len();
    assert!(no_axes < full, "axes off → fewer vertices");

    // Turning the grid off too removes more.
    state.apply(&Message::ToggleGrid(false));
    let bare = state.viewport.scene.as_ref().unwrap().vertices.len();
    assert!(bare < no_axes, "grid off → fewer still");
}

/// The Browse messages are handled by the binary (native dialogs); in the pure
/// state machine they are no-ops and must not alter or panic the state.
#[test]
fn browse_messages_are_noops_in_core_state() {
    let mut state = AppState::default();
    state.apply(&Message::DeckPathChanged("keep.nec".into()));
    let before = state.deck_path.clone();
    state.apply(&Message::BrowseDeck);
    state.apply(&Message::BrowseVars);
    state.apply(&Message::BrowseSaveDeck);
    assert_eq!(state.deck_path, before, "browse must not change core state");
}

// ── GUI solver caveats (pre-release fix 1a) ──────────────────────────────────

/// A degree-3 Y-junction deck still solves on the GUI's Hallén path, but the
/// result carries a warning pointing at the MPIE — in the GUI's own terms, since
/// the picker is right there — so the junction number is never presented as
/// trustworthy.
#[test]
fn solve_warns_on_high_degree_junction() {
    const Y: &str = "\
CM Y-junction
CE
GW 1 20 0.0 0.0 0.0 5.0 0.0 0.0 0.001
GW 2 20 0.0 0.0 0.0 -2.5 4.330127 0.0 0.001
GW 3 20 0.0 0.0 0.0 -2.5 -4.330127 0.0 0.001
GE 0
FR 0 1 0 0 14.2 0
EX 0 1 10 0 1.0 0.0
EN
";
    let r = solve_deck_str(Y, nec_gui::solve::SolverKind::Hallen)
        .expect("Y-junction still solves (unreliably)");
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains(nec_gui::solve::GUI_MPIE_REMEDY)),
        "expected an MPIE recommendation in GUI terms, got {:?}",
        r.warnings
    );
}

/// A plain free-space dipole is fully supported — no solver caveats.
#[test]
fn solve_clean_dipole_has_no_warnings() {
    let r = solve_deck_str(DIPOLE_DECK, nec_gui::solve::SolverKind::Hallen).expect("dipole solves");
    assert!(
        r.warnings.is_empty(),
        "unexpected warnings: {:?}",
        r.warnings
    );
}

/// A runaway sweep (mistyped tiny step over a wide range) is rejected before it
/// can queue millions of solves and freeze the GUI.
#[test]
fn sweep_point_count_is_capped() {
    use nec_gui::solve::{SweepJob, MAX_SWEEP_POINTS};
    // 1..1000 MHz at 0.0001 MHz ≈ 10^7 points — must be refused.
    let err = match SweepJob::prepare(
        DIPOLE_DECK,
        1.0,
        1000.0,
        0.0001,
        nec_gui::solve::SolverKind::Hallen,
    ) {
        Err(e) => e,
        Ok(_) => panic!("runaway sweep must be rejected"),
    };
    assert!(err.contains("max") && err.contains(&MAX_SWEEP_POINTS.to_string()));
    // A sane sweep still prepares fine.
    assert!(SweepJob::prepare(
        DIPOLE_DECK,
        14.0,
        14.4,
        0.1,
        nec_gui::solve::SolverKind::Hallen
    )
    .is_ok());
}

// ── Additional app_state arm coverage (pre-release) ──────────────────────────

#[test]
fn editor_deck_load_error_sets_error_and_clears_save_status() {
    let mut state = AppState::default();
    // Prime a save status, then a failed load must record the error and clear it.
    state.editor.save_status = "Saved to x".into();
    state.apply(&Message::EditDeckLoaded(Err("bad deck".into())));
    assert_eq!(state.editor.error.as_deref(), Some("bad deck"));
    assert!(state.editor.save_status.is_empty());
    assert!(!state.editor.loaded);
}

#[test]
fn status_texts_cover_all_phases() {
    let mut state = AppState::default();
    // Currents.
    assert!(state.currents_status_text().contains("Run Currents"));
    state.apply(&Message::RunCurrents);
    assert!(state.currents_status_text().contains("Computing"));
    state.apply(&Message::CurrentsComplete(Err("boom".into())));
    assert!(state.currents_status_text().contains("boom"));
    // Pattern.
    assert!(
        state.pattern_status_text().contains("azimuth")
            || state.pattern_status_text().contains("φ")
    );
    state.apply(&Message::RunPattern);
    assert!(state.pattern_status_text().contains("Computing"));
    state.apply(&Message::PatternComplete(Err("nope".into())));
    assert!(state.pattern_status_text().contains("nope"));
    // Sweep failure text.
    state.apply(&Message::SweepComplete(Err("range".into())));
    assert!(state.sweep_status_text().contains("range"));
}

#[test]
fn viewport_toggles_without_geometry_are_safe() {
    // Toggling currents/pattern/axes/grid before any geometry is loaded must not
    // panic and must leave the scene empty.
    let mut state = AppState::default();
    for m in [
        Message::ToggleCurrents(true),
        Message::TogglePattern(true),
        Message::ToggleAxes(false),
        Message::ToggleGrid(false),
    ] {
        state.apply(&m);
    }
    assert!(state.viewport.scene.is_none());
}

// ---------------------------------------------------------------------------
// Deck-level caveats reach the state that every tab renders from
// ---------------------------------------------------------------------------

/// The caveats used to live only in `SolveResult`, so they appeared only on the
/// Solve panel — a user who ran nothing but sweeps or patterns saw none of them.
/// They now sit in `AppState`, above the tab content.
#[test]
fn deck_warnings_land_in_state_for_every_tab_to_render() {
    let mut state = AppState::default();
    assert!(state.deck_warnings.is_empty(), "starts clean");

    state.apply(&Message::DeckWarnings(vec![
        "geometry contains a closed loop …".to_string(),
        "antenna is 0.050 λ above finite ground …".to_string(),
    ]));
    assert_eq!(state.deck_warnings.len(), 2);

    // The strip is deck-level, so switching tabs must not clear it — that is the
    // whole point of moving it out of the Solve panel.
    for tab in [
        ActiveTab::Sweep,
        ActiveTab::Pattern,
        ActiveTab::Currents,
        ActiveTab::Solve,
    ] {
        let label = format!("{tab:?}");
        state.apply(&Message::TabSelected(tab));
        assert_eq!(
            state.deck_warnings.len(),
            2,
            "caveats must survive a switch to {label}"
        );
    }
}

/// Caveats describe one deck. Showing the previous deck's caveats against a new
/// one would be worse than showing none.
#[test]
fn changing_the_deck_path_clears_stale_caveats() {
    let mut state = AppState::default();
    state.apply(&Message::DeckWarnings(vec!["stale caveat".to_string()]));
    assert_eq!(state.deck_warnings.len(), 1);
    state.apply(&Message::DeckPathChanged("other.nec".into()));
    assert!(
        state.deck_warnings.is_empty(),
        "caveats for the previous deck must not persist: {:?}",
        state.deck_warnings
    );
}

// ---------------------------------------------------------------------------
// The solver picker reproduces the CLI, on every path (FND-007)
// ---------------------------------------------------------------------------
//
// The CLI pins these three values in `apps/nec-cli/tests/mpie_solver_cli.rs`.
// A picker whose numbers merely look physical is not the requirement — the
// requirement is that the same deck gives the same answer whichever frontend
// the user reaches for. The Y-junction is the load-bearing case: it is the
// topology the MPIE exists for, and Hallén answers it with R≈8 garbage, so a
// picker wired to the wrong solver fails here loudly.

const MPIE_Y_JUNCTION: &str = "\
CM Y-junction, feed mid arm 1
CE
GW 1 20 0.0 0.0 0.0 5.0 0.0 0.0 0.001
GW 2 20 0.0 0.0 0.0 -2.5 4.330127 0.0 0.001
GW 3 20 0.0 0.0 0.0 -2.5 -4.330127 0.0 0.001
GE 0
FR 0 1 0 0 14.2 0
EX 0 1 10 0 1.0 0.0
EN
";

const MPIE_DIPOLE: &str = "\
CM half-wave dipole 14.2 MHz
CE
GW 1 41 0.0 0.0 -5.2782 0.0 0.0 5.2782 0.001
GE 0
FR 0 1 0 0 14.2 0
EX 0 1 21 0 1.0 0.0
EN
";

#[test]
fn the_gui_mpie_reproduces_the_cli_dipole() {
    let r = solve_deck_str(MPIE_DIPOLE, nec_gui::solve::SolverKind::Mpie).expect("MPIE solve");
    assert!(
        (r.z_re - 74.437414).abs() < 0.05 && (r.z_im - 41.753720).abs() < 0.05,
        "GUI MPIE dipole {} + j{} != CLI 74.437414 + j41.753720",
        r.z_re,
        r.z_im
    );
}

#[test]
fn the_gui_mpie_reproduces_the_cli_y_junction() {
    let r = solve_deck_str(MPIE_Y_JUNCTION, nec_gui::solve::SolverKind::Mpie).expect("MPIE solve");
    assert!(
        (r.z_re - 63.673674).abs() < 0.05 && (r.z_im - -322.199211).abs() < 0.05,
        "GUI MPIE Y-junction {} + j{} != CLI 63.673674 - j322.199211",
        r.z_re,
        r.z_im
    );
}

/// The picker must reach the Sweep, Currents and Pattern paths too, not just
/// Solve. A picker that changed one tab would be FND-038 one solver over.
#[test]
fn the_picker_reaches_every_solve_path() {
    use nec_gui::solve::SolverKind;

    // Sweep: its own solve path, so it gets its own comparison against the
    // single solve rather than being assumed to follow it.
    let job = nec_gui::solve::SweepJob::prepare(MPIE_Y_JUNCTION, 14.2, 14.3, 0.1, SolverKind::Mpie)
        .expect("MPIE sweep prepares");
    let pt = job.solve_at(14.2).expect("MPIE sweep solves");
    assert!(
        (pt.z_re - 63.673674).abs() < 0.05,
        "MPIE sweep {} != CLI 63.673674",
        pt.z_re
    );

    // Currents and pattern: a Hallén solve of this deck is garbage, so if the
    // picker did not reach them these would differ from the MPIE currents.
    let mpie_currents =
        nec_gui::solve::current_distribution_deck_str(MPIE_Y_JUNCTION, SolverKind::Mpie)
            .expect("MPIE currents");
    let hallen_currents =
        nec_gui::solve::current_distribution_deck_str(MPIE_Y_JUNCTION, SolverKind::Hallen)
            .expect("Hallén currents");
    let peak = |v: &[nec_gui::solve::CurrentPoint]| {
        v.iter().map(|p| p.current_mag_ma).fold(0.0_f64, f64::max)
    };
    assert!(
        (peak(&mpie_currents) - peak(&hallen_currents)).abs() > 1e-6,
        "the currents view ignored the picker: both solvers gave the same peak"
    );

    assert!(
        nec_gui::solve::pattern_grid_str(MPIE_Y_JUNCTION, SolverKind::Mpie).is_ok(),
        "the pattern view must solve on the MPIE too"
    );
}

/// The MPIE cannot stamp a load, so the GUI refuses such a deck rather than
/// solving it with the `LD` silently ignored — and says so before any solve.
#[test]
fn the_gui_refuses_an_mpie_deck_the_solver_cannot_represent() {
    const LOADED: &str = "\
CM loaded dipole
CE
GW 1 41 0.0 0.0 -5.2782 0.0 0.0 5.2782 0.001
GE 0
LD 0 1 21 21 10.0 0.0 0.0
FR 0 1 0 0 14.2 0
EX 0 1 21 0 1.0 0.0
EN
";
    let err = solve_deck_str(LOADED, nec_gui::solve::SolverKind::Mpie)
        .expect_err("an MPIE solve of a loaded deck must be refused");
    assert!(
        err.contains("LD"),
        "the refusal must name the offending card, got: {err}"
    );
    // ...and the same deck is fine on Hallén, so this is the solver's limit and
    // not a broken deck.
    assert!(
        solve_deck_str(LOADED, nec_gui::solve::SolverKind::Hallen).is_ok(),
        "the loaded deck must still solve on the Hallén path"
    );
}

/// A sweep must refuse such a deck *when it is prepared*, not on the first point.
///
/// The distinction is the user-visible one: `prepare` runs before the progress
/// bar appears, so refusing there says "this cannot work" immediately, while a
/// refusal at the first solve says "your sweep failed" after queueing every
/// point. `solve_mpie_session`'s own guard would catch it either way, which is
/// exactly why this needs its own test — the safety net hides the missing check.
#[test]
fn an_mpie_sweep_refuses_an_unstampable_deck_up_front() {
    const LOADED: &str = "\
CM loaded dipole
CE
GW 1 41 0.0 0.0 -5.2782 0.0 0.0 5.2782 0.001
GE 0
LD 0 1 21 21 10.0 0.0 0.0
FR 0 1 0 0 14.2 0
EX 0 1 21 0 1.0 0.0
EN
";
    let err = match nec_gui::solve::SweepJob::prepare(
        LOADED,
        14.0,
        14.4,
        0.1,
        nec_gui::solve::SolverKind::Mpie,
    ) {
        Ok(_) => panic!("preparing an MPIE sweep of a loaded deck must fail"),
        Err(e) => e,
    };
    assert!(
        err.contains("LD"),
        "the refusal must name the offending card, got: {err}"
    );
    assert!(
        nec_gui::solve::SweepJob::prepare(
            LOADED,
            14.0,
            14.4,
            0.1,
            nec_gui::solve::SolverKind::Hallen
        )
        .is_ok(),
        "the same sweep must still prepare on the Hallén path"
    );
}

// ---------------------------------------------------------------------------
// Switching solver discards what the other solver produced
// ---------------------------------------------------------------------------
//
// The changelog and `docs/gui-guide.md` both promise "changing it clears solved
// results". These pin that promise. Before them, three of the five solved views
// survived the switch, so a Hallén pattern could sit beside an MPIE impedance
// with nothing on screen saying so — the exact frontend-disagreement the picker
// exists to prevent.

fn solved_on_hallen() -> AppState {
    let mut st = AppState::default();
    st.apply(&Message::SolveComplete(Ok(nec_gui::solve::SolveResult {
        freq_mhz: 14.2,
        z_re: 8.0,
        z_im: -960.0,
        warnings: vec![],
        feed_tag: 1,
        feed_seg: 10,
    })));
    st
}

#[test]
fn switching_solver_clears_every_solved_view() {
    let mut st = AppState::default();
    // Put each view into a Done state the way its own task would.
    st.apply(&Message::Solve);
    st.apply(&Message::SolveComplete(Ok(nec_gui::solve::SolveResult {
        freq_mhz: 14.2,
        z_re: 8.0,
        z_im: -960.0,
        warnings: vec![],
        feed_tag: 1,
        feed_seg: 10,
    })));
    st.apply(&Message::RunPattern);
    st.apply(&Message::PatternComplete(Ok(vec![])));
    st.apply(&Message::RunCurrents);
    st.apply(&Message::CurrentsComplete(Ok(vec![])));
    st.deck_warnings = vec!["a Hallén-era caveat".into()];

    assert!(matches!(st.phase, SolvePhase::Done(_)), "setup: solve done");
    assert!(
        matches!(st.pattern_phase, PatternPhase::Done(_)),
        "setup: pattern done"
    );
    assert!(
        matches!(st.currents_phase, CurrentsPhase::Done(_)),
        "setup: currents done"
    );

    st.apply(&Message::SolverSelected(nec_gui::solve::SolverKind::Mpie));

    assert!(matches!(st.phase, SolvePhase::Idle), "impedance survived");
    assert!(
        matches!(st.pattern_phase, PatternPhase::Idle),
        "the pattern survived the switch — it would sit beside an MPIE impedance"
    );
    assert!(
        matches!(st.currents_phase, CurrentsPhase::Idle),
        "the currents survived the switch"
    );
    assert!(
        st.deck_warnings.is_empty(),
        "the caveat strip survived: it is solver-dependent, so the leftover one \
         would tell a user who just picked MPIE to pick MPIE"
    );
    assert!(
        st.viewport.currents_ma.is_none() && st.viewport.grid.is_none(),
        "the 3-D viewport kept the other solver's currents or pattern grid"
    );
}

/// A task launched before the switch is still running, and its result must not
/// resurrect the view that was just discarded.
#[test]
fn an_in_flight_result_does_not_survive_a_solver_switch() {
    let mut st = AppState::default();
    st.apply(&Message::Solve); // task launched on Hallén
    st.apply(&Message::SolverSelected(nec_gui::solve::SolverKind::Mpie));
    // ...and now the Hallén task completes.
    st.apply(&Message::SolveComplete(Ok(nec_gui::solve::SolveResult {
        freq_mhz: 14.2,
        z_re: 8.0,
        z_im: -960.0,
        warnings: vec![],
        feed_tag: 1,
        feed_seg: 10,
    })));
    assert!(
        matches!(st.phase, SolvePhase::Idle),
        "an in-flight Hallén solve repopulated the panel under an MPIE picker"
    );
}

/// The streaming sweep is the worst case: it delivers many messages over many
/// seconds, and its accumulator used to start a fresh `Streaming` phase from
/// *any* state — so a discarded chart refilled point by point.
#[test]
fn an_in_flight_sweep_does_not_refill_a_discarded_chart() {
    let mut st = AppState::default();
    st.apply(&Message::RunSweep);
    st.apply(&Message::SweepPointComputed(nec_gui::solve::SweepPoint {
        freq_mhz: 14.2,
        z_re: 8.0,
        z_im: -960.0,
    }));
    assert!(
        !st.sweep_points().is_empty(),
        "setup: the sweep is streaming"
    );

    st.apply(&Message::SolverSelected(nec_gui::solve::SolverKind::Mpie));
    assert!(st.sweep_points().is_empty(), "the switch cleared the chart");

    // The old sweep is still running and keeps sending.
    st.apply(&Message::SweepPointComputed(nec_gui::solve::SweepPoint {
        freq_mhz: 14.3,
        z_re: 8.0,
        z_im: -960.0,
    }));
    st.apply(&Message::SweepCaveats(vec![
        "a Hallén-era sweep caveat".into()
    ]));
    assert!(
        st.sweep_points().is_empty(),
        "the discarded chart refilled with the other solver's points"
    );
    assert!(
        st.sweep_caveats.is_empty(),
        "the discarded sweep's caveats came back"
    );
}

/// Picking a solver must trigger the session save, like the chart-metric picker
/// beside it. Tested through the named predicate rather than the round-trip
/// alone: the field, the restore path and the round-trip all existed while the
/// choice was still discarded on quit, because the trigger was an untestable
/// line in `update` (FND-034's shape).
#[test]
fn picking_a_solver_triggers_the_session_save() {
    use nec_gui::app_state::persists_to_session;
    assert!(
        persists_to_session(&Message::SolverSelected(nec_gui::solve::SolverKind::Mpie)),
        "the solver picker must persist, as the metric picker beside it does"
    );
    assert!(
        persists_to_session(&Message::SweepMetricSelected(PlotMetric::ZMag)),
        "control: the analogous picker persists"
    );
    assert!(
        !persists_to_session(&Message::Solve),
        "control: running a solve is not a settings change"
    );
}

/// ...and it must refresh the caveat strip, which is solver-dependent now.
#[test]
fn picking_a_solver_refreshes_the_deck_caveats() {
    use nec_gui::app_state::refreshes_deck_warnings;
    assert!(
        refreshes_deck_warnings(&Message::SolverSelected(nec_gui::solve::SolverKind::Mpie)),
        "a stale strip would tell a user who just picked MPIE to pick MPIE"
    );
    assert!(
        refreshes_deck_warnings(&Message::Solve),
        "control: a solve still refreshes them"
    );
    assert!(
        !refreshes_deck_warnings(&Message::TabSelected(ActiveTab::Sweep)),
        "control: switching tabs does not re-read the deck"
    );
}

/// Picking a solver must be persisted, like the chart-metric picker beside it —
/// otherwise the canonical flow (open, switch to MPIE, solve, quit) reopens on
/// Hallén having silently discarded the choice.
#[test]
fn the_selected_solver_round_trips_through_a_session() {
    let mut st = solved_on_hallen();
    st.apply(&Message::SolverSelected(nec_gui::solve::SolverKind::Mpie));
    let session = nec_gui::session::Session::from_state(&st);
    let toml = session.to_toml().expect("serialize");
    let parsed = nec_gui::session::Session::from_toml(&toml).expect("parse");
    let mut restored = AppState::default();
    parsed.apply_to(&mut restored);
    assert_eq!(restored.solver, nec_gui::solve::SolverKind::Mpie);
}

/// The GUI's sweep range never reaches the deck's `FR` card, so the shared
/// `frequency_error` cannot see it — a sweep from -5 to +5 ran and plotted the
/// source voltage back at every point at or below 0 MHz (FND-056).
#[test]
fn a_gui_sweep_range_must_start_above_zero() {
    let mut st = AppState {
        sweep_start: "-5.0".into(),
        sweep_end: "5.0".into(),
        sweep_step: "0.5".into(),
        ..AppState::default()
    };
    let err = st
        .sweep_params()
        .expect_err("a sweep from -5 MHz must be refused");
    assert!(
        err.contains("positive frequency"),
        "the refusal must name the cause: {err}"
    );

    // Zero is refused for the same reason: the current goes to zero there.
    st.sweep_start = "0.0".into();
    assert!(
        st.sweep_params().is_err(),
        "a sweep from 0 MHz must be refused"
    );

    // Negative control: an ordinary range still parses. `end` moves too — it is
    // still 5.0 from the case above, and leaving it there would trip the
    // start < end check instead, making this control pass for the wrong reason.
    st.sweep_start = "14.0".into();
    st.sweep_end = "14.4".into();
    assert!(
        st.sweep_params().is_ok(),
        "an ordinary sweep range must parse"
    );
}

/// `parse::<f64>()` accepts "NaN", and every comparison against `NaN` is false —
/// so the ordering tests alone waved it straight through.
#[test]
fn a_gui_sweep_range_rejects_non_finite_fields() {
    let mut st = AppState {
        sweep_start: "14.0".into(),
        sweep_end: "14.4".into(),
        ..AppState::default()
    };
    for bad in ["NaN", "inf", "-inf"] {
        st.sweep_step = bad.into();
        let err = st
            .sweep_params()
            .expect_err("a non-finite step must be refused");
        assert!(
            err.contains("finite"),
            "the refusal must name finiteness for {bad}: {err}"
        );
    }
}

/// ...and the same guard on the job itself, which is a separate entry point.
#[test]
fn a_sweep_job_refuses_a_range_starting_at_or_below_zero() {
    let deck = "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nFR 0 1 0 0 14.2 0\n\
                EX 0 1 11 0 1.0 0.0\nEN\n";
    for start in [-5.0, 0.0] {
        assert!(
            nec_gui::solve::SweepJob::prepare(
                deck,
                start,
                5.0,
                0.5,
                nec_gui::solve::SolverKind::Hallen
            )
            .is_err(),
            "a job swept from {start} MHz must be refused"
        );
    }
    assert!(
        nec_gui::solve::SweepJob::prepare(
            deck,
            14.0,
            14.4,
            0.1,
            nec_gui::solve::SolverKind::Hallen
        )
        .is_ok(),
        "control: an ordinary range still prepares"
    );
}

/// FND-039: the caveat strip is where this defect was visible, so it is tested
/// there and not only at the producer. A deck with an unrecognised `EX` type used
/// to render as clean while typing and fail the moment Solve was pressed —
/// measured: `deck_warnings` returned `[]`, then
/// `EX: unknown excitation type (I1=9, ...)`.
#[test]
fn an_unrecognised_excitation_warns_while_typing_rather_than_at_solve() {
    const EX9: &str = "CM unrecognised EX type\nCE\nGW 1 21 0 0 -5.2782 0 0 5.2782 0.001\n\
                       GE 0\nFR 0 1 0 0 14.2 0\nEX 9 1 11 0 1.0 0.0\nEN\n";
    let w = nec_gui::solve::deck_warnings(EX9, nec_gui::solve::SolverKind::Hallen);
    assert!(
        w.iter().any(|m| m.contains("type 9")),
        "the strip must name the unrecognised type while typing: {w:?}"
    );

    // ...and the solve still refuses it. The caveat warns, it does not license.
    assert!(
        solve_deck_str(EX9, nec_gui::solve::SolverKind::Hallen).is_err(),
        "an unrecognised EX type must still refuse at solve"
    );

    // Negative control: an ordinary deck earns no such caveat, so the check is
    // not simply warning about every deck.
    let clean = nec_gui::solve::deck_warnings(DIPOLE_DECK, nec_gui::solve::SolverKind::Hallen);
    assert!(
        !clean
            .iter()
            .any(|m| m.contains("not a recognised excitation")),
        "a plain dipole must not be told its excitation is unrecognised: {clean:?}"
    );
}

/// FND-053: `compute_radiation_pattern` returns **directivity**. Over lossy
/// ground the CLI converts it to gain (PH9-CHK-003) and the GUI did not, so one
/// deck's pattern read as gain on one frontend and directivity on the other with
/// nothing saying which.
///
/// Measured on `corpus/dipole-gn2-near-ground-51seg.nec`: the CLI reports a peak
/// `GAIN_DB` of 0.2997, and the GUI reported **6.3355** — overstating gain by
/// 6.04 dB, which is the ground loss it was not accounting for.
#[test]
fn the_gui_pattern_reports_gain_over_lossy_ground_as_the_cli_does() {
    let deck = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/dipole-gn2-near-ground-51seg.nec"
    ))
    .expect("corpus deck");

    let slice =
        nec_gui::solve::pattern_slice_deck_str(&deck, 0.0, nec_gui::solve::SolverKind::Hallen)
            .expect("pattern slice");
    let peak = slice
        .iter()
        .map(|p| p.gain_total_dbi)
        .fold(f64::MIN, f64::max);
    assert!(
        (peak - 0.2997).abs() < 0.01,
        "GUI peak {peak:.4} dBi must match the CLI's 0.2997; \
         6.34 would mean the ground loss is unaccounted for"
    );
}

/// FND-129: the GUI reaches the shared refusal, on BOTH of its entry points.
///
/// It calls `pre_solve_error` from two places — the solve path and the pattern
/// path — so one of them can lose the wiring while the other keeps it, which is
/// the shape of half the findings in this ledger.
#[test]
fn a_deck_with_two_current_sources_is_refused_on_both_gui_paths() {
    let deck = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/dipole-two-current-sources-freesp.nec"
    ))
    .expect("corpus deck");

    let solve = nec_gui::solve::solve_deck_str(&deck, nec_gui::solve::SolverKind::Hallen);
    let pattern =
        nec_gui::solve::pattern_slice_deck_str(&deck, 0.0, nec_gui::solve::SolverKind::Hallen);

    for (label, err) in [
        ("solve", solve.err()),
        ("pattern", pattern.err().map(|e| e.to_string())),
    ] {
        let msg = err.unwrap_or_else(|| panic!("the {label} path must refuse this deck"));
        assert!(
            msg.contains("2 current sources"),
            "the {label} path must give the shared reason, got: {msg}"
        );
    }
}

/// FND-114 — the same gate for a CURRENT source, which the voltage-drive test
/// above cannot reach.
///
/// The gain correction divides by the power delivered into the feedpoints, and
/// that power was read from the excitation vector — which `apply_ex` leaves all
/// zeros for an `EX 4` drive. So `P_in` was exactly 0, `gain_correction_db`
/// declined, `unwrap_or(0.0)` swallowed it, and this view reported directivity
/// as gain. Measured before the fix: **6.3372 dBi here against the CLI's
/// 0.5590**, a 5.78 dB overstatement, on a deck differing from
/// `dipole-gn2-near-ground-51seg.nec` by its EX card alone.
///
/// The pin is ABSOLUTE, against the CLI's own number, deliberately. A test that
/// merely compared the two drives to each other would pass if the correction
/// were skipped for BOTH — and it would also be asserting something false, since
/// the two drives legitimately differ by ~0.26 dB here (that gap is FND-118,
/// which is about the solver, not this seam).
#[test]
fn the_gui_pattern_corrects_a_current_source_drive_as_the_cli_does() {
    let deck = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/dipole-ex4-gn2-near-ground-51seg.nec"
    ))
    .expect("corpus deck");

    let slice =
        nec_gui::solve::pattern_slice_deck_str(&deck, 0.0, nec_gui::solve::SolverKind::Hallen)
            .expect("pattern slice");
    let peak = slice
        .iter()
        .map(|p| p.gain_total_dbi)
        .fold(f64::MIN, f64::max);
    assert!(
        (peak - 0.5590).abs() < 0.01,
        "GUI peak {peak:.4} dBi must match the CLI's 0.5590 for this current-source \
         deck; 6.34 would mean the ground loss is unaccounted for, which is exactly \
         what a current source used to do"
    );
}

/// The free-space control, and an honest note on what it does *not* prove.
///
/// It pins that a free-space pattern is still the textbook ~2.15 dBi, so a
/// correction with the wrong sign or magnitude fails here. It does **not** prove
/// the ground-type guard is load-bearing: removing that guard leaves this test
/// green, because radiation efficiency in free space is ~1 and the correction is
/// then ~0 dB anyway. Verified by sabotage rather than assumed. The guard earns
/// its place on the PEC-ground path, not this one.
#[test]
fn a_free_space_pattern_is_not_shifted_by_the_ground_correction() {
    let deck = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/dipole-freesp-51seg.nec"
    ))
    .expect("corpus deck");
    let slice =
        nec_gui::solve::pattern_slice_deck_str(&deck, 0.0, nec_gui::solve::SolverKind::Hallen)
            .expect("pattern slice");
    let peak = slice
        .iter()
        .map(|p| p.gain_total_dbi)
        .fold(f64::MIN, f64::max);
    // A half-wave dipole in free space is ~2.15 dBi.
    assert!(
        (peak - 2.15).abs() < 0.3,
        "free-space peak {peak:.4} dBi is not the textbook ~2.15"
    );
}

/// FND-033: a sweep that failed partway discarded every point it had already
/// computed. `SweepPhase::Failed` carried only the error, so 399 real answers
/// vanished at the moment the 400th failed — and #395's negative-resistance
/// caveat, which describes exactly those points, was left standing beside an
/// error with nothing to point at.
#[test]
fn a_failed_sweep_keeps_the_points_it_computed() {
    let mut st = AppState::default();
    st.apply(&Message::RunSweep);
    for f in [14.0_f64, 14.1, 14.2] {
        st.apply(&Message::SweepPointComputed(nec_gui::solve::SweepPoint {
            freq_mhz: f,
            z_re: 70.0,
            z_im: 0.0,
        }));
    }
    assert_eq!(st.sweep_points().len(), 3, "setup: three points streamed");

    st.apply(&Message::SweepComplete(
        Err("solver blew up at 14.3".into()),
    ));

    assert!(matches!(st.sweep_phase, SweepPhase::Failed(..)));
    assert_eq!(
        st.sweep_points().len(),
        3,
        "the points computed before the failure must survive it"
    );
    // ...and the status says both what happened and how far it got.
    let status = st.sweep_status_text();
    assert!(status.contains("3 point"), "{status}");
    assert!(status.contains("blew up"), "{status}");
}

/// The negative control: a sweep that fails having computed nothing must not
/// claim partial results. Without this, "keep the points" could be satisfied by
/// inventing some.
#[test]
fn a_sweep_that_fails_immediately_reports_no_points() {
    let mut st = AppState::default();
    st.apply(&Message::RunSweep);
    st.apply(&Message::SweepComplete(Err("could not prepare".into())));
    assert!(st.sweep_points().is_empty());
    let status = st.sweep_status_text();
    assert!(!status.contains("point("), "{status}");
}
