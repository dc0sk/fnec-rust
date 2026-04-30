//! Project-container and workflow metadata scope for future frontends.
//!
//! This crate is intentionally minimal today, but its planned responsibility is
//! narrower than "anything that is not the solver": it is the home for
//! Markdown-based project manifests, run metadata/history, and result-storage
//! conventions that let CLI/GUI/TUI workflows share one project model.
//!
//! FR-004 tracks Markdown-based project import/export as an explicit product
//! requirement. Until that lands, this crate serves as the documented scope
//! boundary for that work rather than an implicit placeholder.
//!
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Solver configuration for a project or a named run.
///
/// Both fields are free-form strings that correspond to the values accepted
/// by the `--solver` and `--pulse-rhs` CLI flags.  Using strings rather than
/// enums keeps the project file format stable when new modes are added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolverConfig {
    /// Solver mode: `"hallen"`, `"continuity"`, `"sinusoidal"`, or `"auto"`.
    pub mode: String,
    /// Pulse-RHS normalisation: `"auto"`, `"1"`, or `"1/dl_lambda"`.
    pub pulse_rhs: String,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            mode: "hallen".to_string(),
            pulse_rhs: "auto".to_string(),
        }
    }
}

/// A named run variant inside a project.
///
/// A run inherits the project-level solver configuration unless it provides
/// its own `solver` override.  The `name` field is used as a human-readable
/// identifier and must be unique within a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedRun {
    /// Short identifier for this run (e.g. `"baseline"`, `"loaded-50ohm"`).
    pub name: String,
    /// Optional free-form description shown in reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional per-run solver override.  When absent the project-level
    /// [`SolverConfig`] is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver: Option<SolverConfig>,
}

/// A versioned fnec-rust project file.
///
/// Project files are serialised as TOML.  The `version` field is checked on
/// load; currently only version `1` is supported.  Unknown versions return an
/// error from [`ProjectFile::from_toml`].
///
/// # Example round-trip
///
/// ```
/// use nec_project::{ProjectFile, SolverConfig, NamedRun};
/// use std::path::PathBuf;
///
/// let project = ProjectFile {
///     version: 1,
///     name: "dipole-14mhz".to_string(),
///     deck_path: PathBuf::from("corpus/dipole-freesp-51seg.nec"),
///     solver: SolverConfig::default(),
///     runs: vec![
///         NamedRun {
///             name: "baseline".to_string(),
///             description: Some("Default Hallen solve".to_string()),
///             solver: None,
///         },
///     ],
/// };
///
/// let toml_str = project.to_toml().unwrap();
/// let loaded = ProjectFile::from_toml(&toml_str).unwrap();
/// assert_eq!(loaded, project);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    /// Format version.  Must be `1` for this release.
    pub version: u32,
    /// Human-readable project name.
    pub name: String,
    /// Path to the NEC deck file, relative to the project file's directory.
    pub deck_path: PathBuf,
    /// Default solver configuration applied to all runs unless overridden.
    pub solver: SolverConfig,
    /// Named run variants.  May be empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<NamedRun>,
}

/// The only supported project file format version.
pub const PROJECT_FILE_VERSION: u32 = 1;

/// Error type for project file load/save operations.
#[derive(Debug)]
pub enum ProjectError {
    /// TOML serialisation failed.
    SerialiseError(toml::ser::Error),
    /// TOML deserialisation failed.
    DeserialiseError(toml::de::Error),
    /// The file declares an unsupported format version.
    UnsupportedVersion(u32),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::SerialiseError(e) => write!(f, "project serialise error: {e}"),
            ProjectError::DeserialiseError(e) => write!(f, "project deserialise error: {e}"),
            ProjectError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "unsupported project file version {v} (expected {PROJECT_FILE_VERSION})"
                )
            }
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<toml::ser::Error> for ProjectError {
    fn from(e: toml::ser::Error) -> Self {
        ProjectError::SerialiseError(e)
    }
}

impl From<toml::de::Error> for ProjectError {
    fn from(e: toml::de::Error) -> Self {
        ProjectError::DeserialiseError(e)
    }
}

impl ProjectFile {
    /// Serialise this project to a TOML string.
    pub fn to_toml(&self) -> Result<String, ProjectError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Deserialise a project from a TOML string.
    ///
    /// Returns [`ProjectError::UnsupportedVersion`] if the `version` field is
    /// not [`PROJECT_FILE_VERSION`].
    pub fn from_toml(s: &str) -> Result<Self, ProjectError> {
        let project: Self = toml::from_str(s)?;
        if project.version != PROJECT_FILE_VERSION {
            return Err(ProjectError::UnsupportedVersion(project.version));
        }
        Ok(project)
    }
}
