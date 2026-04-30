// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Integration tests for `ProjectFile` TOML round-trip and error handling.

use nec_project::{NamedRun, ProjectError, ProjectFile, SolverConfig};
use std::path::PathBuf;

fn minimal_project() -> ProjectFile {
    ProjectFile {
        version: 1,
        name: "test-project".to_string(),
        deck_path: PathBuf::from("corpus/dipole-freesp-51seg.nec"),
        solver: SolverConfig::default(),
        runs: vec![],
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
