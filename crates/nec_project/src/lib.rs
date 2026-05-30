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
///     history: Default::default(),
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
    /// Completed-run history.  Absent from the TOML file when empty.
    #[serde(default, skip_serializing_if = "RunHistory::is_empty")]
    pub history: RunHistory,
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
    /// Markdown import parsing failed.
    MarkdownParseError(String),
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
            ProjectError::MarkdownParseError(msg) => {
                write!(f, "project markdown parse error: {msg}")
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

    /// Deserialise a project from a Markdown manifest.
    ///
    /// Accepted format:
    /// - YAML frontmatter delimited by `---` with keys:
    ///   - `format: fnec-project-markdown`
    ///   - `version: 1`
    /// - One fenced TOML block tagged as a project payload:
    ///   - ````toml project````
    ///
    /// The fenced TOML payload must be a valid [`ProjectFile`] document.
    /// Frontmatter `version` must match payload `version`.
    pub fn from_markdown(s: &str) -> Result<Self, ProjectError> {
        let lines: Vec<&str> = s.lines().collect();
        if lines.len() < 3 || lines[0].trim() != "---" {
            return Err(ProjectError::MarkdownParseError(
                "missing YAML frontmatter opening delimiter".to_string(),
            ));
        }

        let mut fm_end = None;
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                fm_end = Some(i);
                break;
            }
        }
        let fm_end = fm_end.ok_or_else(|| {
            ProjectError::MarkdownParseError(
                "missing YAML frontmatter closing delimiter".to_string(),
            )
        })?;

        let mut format_value: Option<String> = None;
        let mut version_value: Option<u32> = None;
        for line in lines.iter().take(fm_end).skip(1) {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let (k, v) = trimmed.split_once(':').ok_or_else(|| {
                ProjectError::MarkdownParseError(format!("invalid frontmatter line: {trimmed}"))
            })?;
            let key = k.trim();
            let value = strip_yaml_scalar(v.trim());
            match key {
                "format" => format_value = Some(value.to_string()),
                "version" => {
                    let parsed = value.parse::<u32>().map_err(|_| {
                        ProjectError::MarkdownParseError(
                            "frontmatter version must be an integer".to_string(),
                        )
                    })?;
                    version_value = Some(parsed);
                }
                _ => {}
            }
        }

        if format_value.as_deref() != Some("fnec-project-markdown") {
            return Err(ProjectError::MarkdownParseError(
                "frontmatter format must be fnec-project-markdown".to_string(),
            ));
        }
        let frontmatter_version = version_value.ok_or_else(|| {
            ProjectError::MarkdownParseError("frontmatter version is required".to_string())
        })?;

        let mut in_project_toml = false;
        let mut project_toml = String::new();
        for line in lines.iter().skip(fm_end + 1) {
            let trimmed = line.trim();
            if !in_project_toml {
                if let Some(rest) = trimmed.strip_prefix("```") {
                    let info = rest.trim();
                    if info.starts_with("toml") && info.contains("project") {
                        in_project_toml = true;
                    }
                }
                continue;
            }

            if trimmed == "```" {
                break;
            }
            project_toml.push_str(line);
            project_toml.push('\n');
        }

        if project_toml.trim().is_empty() {
            return Err(ProjectError::MarkdownParseError(
                "missing fenced TOML project block (```toml project)".to_string(),
            ));
        }

        let project = ProjectFile::from_toml(&project_toml)?;
        if project.version != frontmatter_version {
            return Err(ProjectError::MarkdownParseError(format!(
                "frontmatter version {} does not match project payload version {}",
                frontmatter_version, project.version
            )));
        }
        Ok(project)
    }

    // --- run-history query API -------------------------------------------

    /// Number of completed runs recorded in the history.
    pub fn run_count(&self) -> usize {
        self.history.run_count()
    }

    /// The most recent run record, or `None` when the history is empty.
    pub fn last_run(&self) -> Option<&RunRecord> {
        self.history.last_run()
    }

    /// The run record at `index` (zero-based insertion order), or `None`
    /// when `index` is out of range.
    pub fn run_by_index(&self, index: usize) -> Option<&RunRecord> {
        self.history.run_by_index(index)
    }
}

fn strip_yaml_scalar(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

// ---------------------------------------------------------------------------
// Run history types
// ---------------------------------------------------------------------------

/// A compact result summary stored with each history entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultSummary {
    /// Real part of the feedpoint impedance (Ω).
    pub impedance_re: f64,
    /// Imaginary part of the feedpoint impedance (Ω).
    pub impedance_im: f64,
    /// Peak total gain (dBi).  `None` when no RP card was present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_gain_dbi: Option<f64>,
    /// Number of frequency-sweep points solved.  `1` for single-frequency runs.
    pub sweep_point_count: usize,
}

/// A single completed-run record stored in the project history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    /// UTC timestamp in ISO 8601 format (`YYYY-MM-DDTHH:MM:SSZ`).
    pub timestamp: String,
    /// Solver configuration snapshot used for this run.
    pub solver: SolverConfig,
    /// Compact result summary.
    pub result: ResultSummary,
}

/// Ordered collection of [`RunRecord`] entries with query helpers.
///
/// Stored in the project file as a TOML array of tables (`[[history]]`).
/// Absent (default) when no runs have been recorded yet.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunHistory(pub Vec<RunRecord>);

impl RunHistory {
    /// Number of recorded runs.
    pub fn run_count(&self) -> usize {
        self.0.len()
    }

    /// Most recent run record, or `None` when empty.
    pub fn last_run(&self) -> Option<&RunRecord> {
        self.0.last()
    }

    /// Run record at zero-based `index`, or `None` when out of range.
    pub fn run_by_index(&self, index: usize) -> Option<&RunRecord> {
        self.0.get(index)
    }

    /// `true` when no runs have been recorded.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Append a run record to the history.
    pub fn push(&mut self, record: RunRecord) {
        self.0.push(record);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Helpers ─────────────────────────────────────────────────────────────

    fn minimal_project() -> ProjectFile {
        ProjectFile {
            version: 1,
            name: "test-dipole".to_string(),
            deck_path: PathBuf::from("corpus/dipole-freesp-51seg.nec"),
            solver: SolverConfig::default(),
            runs: vec![],
            history: RunHistory::default(),
        }
    }

    fn make_run_record(label: &str, z_re: f64, z_im: f64) -> RunRecord {
        RunRecord {
            timestamp: "2026-05-05T12:00:00Z".to_string(),
            solver: SolverConfig {
                mode: label.to_string(),
                pulse_rhs: "auto".to_string(),
            },
            result: ResultSummary {
                impedance_re: z_re,
                impedance_im: z_im,
                peak_gain_dbi: None,
                sweep_point_count: 1,
            },
        }
    }

    // ── SolverConfig ────────────────────────────────────────────────────────

    #[test]
    fn solver_config_default_is_hallen_auto() {
        let sc = SolverConfig::default();
        assert_eq!(sc.mode, "hallen");
        assert_eq!(sc.pulse_rhs, "auto");
    }

    // ── ProjectFile round-trip ───────────────────────────────────────────────

    #[test]
    fn project_file_round_trips_minimal() {
        let project = minimal_project();
        let toml = project.to_toml().expect("serialise should succeed");
        let loaded = ProjectFile::from_toml(&toml).expect("deserialise should succeed");
        assert_eq!(loaded, project);
    }

    #[test]
    fn project_file_round_trips_with_runs_and_history() {
        let mut project = minimal_project();
        project.runs.push(NamedRun {
            name: "baseline".to_string(),
            description: Some("Default Hallen solve".to_string()),
            solver: None,
        });
        project.runs.push(NamedRun {
            name: "loaded".to_string(),
            description: None,
            solver: Some(SolverConfig {
                mode: "hallen".to_string(),
                pulse_rhs: "1".to_string(),
            }),
        });
        project
            .history
            .push(make_run_record("hallen", 74.24, 13.90));

        let toml = project.to_toml().expect("serialise should succeed");
        let loaded = ProjectFile::from_toml(&toml).expect("deserialise should succeed");
        assert_eq!(loaded, project);
    }

    #[test]
    fn project_file_toml_omits_empty_runs_and_history() {
        let project = minimal_project();
        let toml = project.to_toml().expect("serialise should succeed");
        // Empty runs and empty history should not appear in the TOML output.
        assert!(!toml.contains("[[runs]]"), "empty runs should be omitted");
        assert!(
            !toml.contains("[[history]]"),
            "empty history should be omitted"
        );
    }

    #[test]
    fn project_file_round_trips_with_peak_gain() {
        let mut project = minimal_project();
        project.history.push(RunRecord {
            timestamp: "2026-05-05T14:00:00Z".to_string(),
            solver: SolverConfig::default(),
            result: ResultSummary {
                impedance_re: 74.24,
                impedance_im: 13.90,
                peak_gain_dbi: Some(2.15),
                sweep_point_count: 3,
            },
        });

        let toml = project.to_toml().expect("serialise should succeed");
        let loaded = ProjectFile::from_toml(&toml).expect("deserialise should succeed");
        assert_eq!(loaded.last_run().unwrap().result.peak_gain_dbi, Some(2.15));
    }

    // ── Version guard ────────────────────────────────────────────────────────

    #[test]
    fn unsupported_version_returns_error() {
        let bad_toml = r#"
version = 99
name = "bad"
deck_path = "deck.nec"
[solver]
mode = "hallen"
pulse_rhs = "auto"
"#;
        let result = ProjectFile::from_toml(bad_toml);
        assert!(
            matches!(result, Err(ProjectError::UnsupportedVersion(99))),
            "expected UnsupportedVersion(99), got {result:?}",
        );
    }

    #[test]
    fn missing_required_field_returns_deserialise_error() {
        let bad_toml = r#"version = 1"#; // missing name, deck_path, solver
        let result = ProjectFile::from_toml(bad_toml);
        assert!(
            matches!(result, Err(ProjectError::DeserialiseError(_))),
            "expected DeserialiseError, got {result:?}",
        );
    }

    // ── ProjectError Display ─────────────────────────────────────────────────

    #[test]
    fn project_error_unsupported_version_display() {
        let msg = ProjectError::UnsupportedVersion(42).to_string();
        assert!(msg.contains("42"), "display should mention the bad version");
        assert!(
            msg.contains(&PROJECT_FILE_VERSION.to_string()),
            "display should mention the expected version"
        );
    }

    #[test]
    fn markdown_parse_error_display_includes_reason() {
        let msg = ProjectError::MarkdownParseError("bad schema".to_string()).to_string();
        assert!(msg.contains("markdown parse error"));
        assert!(msg.contains("bad schema"));
    }

    // ── RunHistory API ───────────────────────────────────────────────────────

    #[test]
    fn run_history_starts_empty() {
        let h = RunHistory::default();
        assert!(h.is_empty());
        assert_eq!(h.run_count(), 0);
        assert!(h.last_run().is_none());
        assert!(h.run_by_index(0).is_none());
    }

    #[test]
    fn run_history_push_and_query() {
        let mut h = RunHistory::default();
        h.push(make_run_record("hallen", 74.24, 13.90));
        h.push(make_run_record("continuity", 50.0, 0.0));

        assert_eq!(h.run_count(), 2);
        assert!(!h.is_empty());
        assert_eq!(h.last_run().unwrap().solver.mode, "continuity");
        assert_eq!(h.run_by_index(0).unwrap().solver.mode, "hallen");
        assert_eq!(h.run_by_index(1).unwrap().solver.mode, "continuity");
        assert!(h.run_by_index(2).is_none());
    }

    // ── ProjectFile run-history delegation ───────────────────────────────────

    #[test]
    fn project_file_run_count_delegates_to_history() {
        let mut project = minimal_project();
        assert_eq!(project.run_count(), 0);
        assert!(project.last_run().is_none());
        project
            .history
            .push(make_run_record("hallen", 74.24, 13.90));
        assert_eq!(project.run_count(), 1);
        assert!(project.last_run().is_some());
        assert!(project.run_by_index(0).is_some());
        assert!(project.run_by_index(1).is_none());
    }
}
