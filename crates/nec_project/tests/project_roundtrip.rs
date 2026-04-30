// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Integration tests for `ProjectFile` TOML round-trip and error handling.

use nec_project::{
    NamedRun, ProjectError, ProjectFile, ResultSummary, RunHistory, RunRecord, SolverConfig,
};
use std::path::PathBuf;

fn minimal_project() -> ProjectFile {
    ProjectFile {
        version: 1,
        name: "test-project".to_string(),
        deck_path: PathBuf::from("corpus/dipole-freesp-51seg.nec"),
        solver: SolverConfig::default(),
        runs: vec![],
        history: RunHistory::default(),
    }
}

// --- round-trip tests -------------------------------------------------------

#[test]
fn roundtrip_minimal_project() {
    let project = minimal_project();
    let toml_str = project.to_toml().unwrap();
    let loaded = ProjectFile::from_toml(&toml_str).unwrap();
    assert_eq!(loaded, project);
}

#[test]
fn roundtrip_with_solver_config() {
    let project = ProjectFile {
        solver: SolverConfig {
            mode: "continuity".to_string(),
            pulse_rhs: "1/dl_lambda".to_string(),
        },
        ..minimal_project()
    };
    let toml_str = project.to_toml().unwrap();
    let loaded = ProjectFile::from_toml(&toml_str).unwrap();
    assert_eq!(loaded.solver.mode, "continuity");
    assert_eq!(loaded.solver.pulse_rhs, "1/dl_lambda");
}

#[test]
fn roundtrip_with_named_runs() {
    let project = ProjectFile {
        runs: vec![
            NamedRun {
                name: "baseline".to_string(),
                description: Some("Default Hallen solve".to_string()),
                solver: None,
            },
            NamedRun {
                name: "loaded".to_string(),
                description: None,
                solver: Some(SolverConfig {
                    mode: "hallen".to_string(),
                    pulse_rhs: "1".to_string(),
                }),
            },
        ],
        ..minimal_project()
    };
    let toml_str = project.to_toml().unwrap();
    let loaded = ProjectFile::from_toml(&toml_str).unwrap();
    assert_eq!(loaded.runs.len(), 2);
    assert_eq!(loaded.runs[0].name, "baseline");
    assert_eq!(
        loaded.runs[0].description.as_deref(),
        Some("Default Hallen solve")
    );
    assert!(loaded.runs[0].solver.is_none());
    assert_eq!(loaded.runs[1].name, "loaded");
    assert!(loaded.runs[1].solver.is_some());
    assert_eq!(loaded.runs[1].solver.as_ref().unwrap().mode, "hallen");
}

#[test]
fn roundtrip_run_description_optional_omitted_when_none() {
    let project = ProjectFile {
        runs: vec![NamedRun {
            name: "no-desc".to_string(),
            description: None,
            solver: None,
        }],
        ..minimal_project()
    };
    let toml_str = project.to_toml().unwrap();
    // `description` key must not appear in serialised output when None
    assert!(!toml_str.contains("description"));
    let loaded = ProjectFile::from_toml(&toml_str).unwrap();
    assert!(loaded.runs[0].description.is_none());
}

#[test]
fn roundtrip_empty_runs_list_omitted_from_toml() {
    let project = minimal_project();
    let toml_str = project.to_toml().unwrap();
    // `runs` key must not appear when the list is empty
    assert!(!toml_str.contains("runs"));
    let loaded = ProjectFile::from_toml(&toml_str).unwrap();
    assert!(loaded.runs.is_empty());
}

#[test]
fn roundtrip_deck_path_preserved() {
    let project = ProjectFile {
        deck_path: PathBuf::from("examples/dipole_14mhz.nec"),
        ..minimal_project()
    };
    let toml_str = project.to_toml().unwrap();
    let loaded = ProjectFile::from_toml(&toml_str).unwrap();
    assert_eq!(loaded.deck_path, PathBuf::from("examples/dipole_14mhz.nec"));
}

// --- error handling tests ---------------------------------------------------

#[test]
fn unsupported_version_returns_error() {
    let toml_str = r#"
version = 99
name = "bad"
deck_path = "corpus/dipole-freesp-51seg.nec"

[solver]
mode = "hallen"
pulse_rhs = "auto"
"#;
    match ProjectFile::from_toml(toml_str) {
        Err(ProjectError::UnsupportedVersion(99)) => {}
        other => panic!("expected UnsupportedVersion(99), got {other:?}"),
    }
}

#[test]
fn missing_required_field_returns_deserialise_error() {
    // `name` is required; omitting it must produce a deserialise error
    let toml_str = r#"
version = 1
deck_path = "corpus/dipole-freesp-51seg.nec"

[solver]
mode = "hallen"
pulse_rhs = "auto"
"#;
    match ProjectFile::from_toml(toml_str) {
        Err(ProjectError::DeserialiseError(_)) => {}
        other => panic!("expected DeserialiseError, got {other:?}"),
    }
}

// --- run history tests ------------------------------------------------------

fn make_record(
    timestamp: &str,
    mode: &str,
    re: f64,
    im: f64,
    gain: Option<f64>,
    sweep: usize,
) -> RunRecord {
    RunRecord {
        timestamp: timestamp.to_string(),
        solver: SolverConfig {
            mode: mode.to_string(),
            pulse_rhs: "auto".to_string(),
        },
        result: ResultSummary {
            impedance_re: re,
            impedance_im: im,
            peak_gain_dbi: gain,
            sweep_point_count: sweep,
        },
    }
}

#[test]
fn history_query_api_empty() {
    let project = minimal_project();
    assert_eq!(project.run_count(), 0);
    assert!(project.last_run().is_none());
    assert!(project.run_by_index(0).is_none());
}

#[test]
fn history_query_api_after_push() {
    let mut project = minimal_project();
    project.history.push(make_record(
        "2026-04-30T10:00:00Z",
        "hallen",
        72.1,
        -3.5,
        Some(2.1),
        1,
    ));
    project.history.push(make_record(
        "2026-04-30T11:00:00Z",
        "continuity",
        68.4,
        0.2,
        None,
        5,
    ));

    assert_eq!(project.run_count(), 2);
    assert_eq!(
        project.last_run().unwrap().timestamp,
        "2026-04-30T11:00:00Z"
    );
    assert_eq!(
        project.run_by_index(0).unwrap().timestamp,
        "2026-04-30T10:00:00Z"
    );
    assert!(project.run_by_index(2).is_none());
}

#[test]
fn history_roundtrip_toml() {
    let mut project = minimal_project();
    project.history.push(make_record(
        "2026-04-30T09:00:00Z",
        "hallen",
        50.0,
        25.0,
        Some(3.5),
        1,
    ));

    let toml_str = project.to_toml().unwrap();
    let loaded = ProjectFile::from_toml(&toml_str).unwrap();

    assert_eq!(loaded.run_count(), 1);
    let rec = loaded.run_by_index(0).unwrap();
    assert_eq!(rec.timestamp, "2026-04-30T09:00:00Z");
    assert_eq!(rec.solver.mode, "hallen");
    assert!((rec.result.impedance_re - 50.0).abs() < 1e-9);
    assert_eq!(rec.result.sweep_point_count, 1);
    assert!((rec.result.peak_gain_dbi.unwrap() - 3.5).abs() < 1e-9);
}

#[test]
fn history_peak_gain_optional_omitted_when_none() {
    let mut project = minimal_project();
    project.history.push(make_record(
        "2026-04-30T08:00:00Z",
        "hallen",
        70.0,
        -10.0,
        None,
        1,
    ));

    let toml_str = project.to_toml().unwrap();
    assert!(!toml_str.contains("peak_gain_dbi"));

    let loaded = ProjectFile::from_toml(&toml_str).unwrap();
    assert!(loaded
        .run_by_index(0)
        .unwrap()
        .result
        .peak_gain_dbi
        .is_none());
}

#[test]
fn history_empty_omitted_from_toml() {
    let project = minimal_project();
    let toml_str = project.to_toml().unwrap();
    assert!(!toml_str.contains("history"));
}
