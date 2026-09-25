// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Hosts configuration — parsed from `hosts.toml`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_parses_two_workers() {
        let toml = r#"
[[worker]]
hostname = "box1"
ssh_user = "u1"
cpu_threads_override = 8

[[worker]]
hostname = "box2"
binary_path = "/opt/fnec"
gpu_weight_override = 6.0
"#;
        let cfg = HostsConfig::from_str(toml).unwrap();
        assert_eq!(cfg.worker.len(), 2);

        assert_eq!(cfg.worker[0].hostname, "box1");
        assert_eq!(cfg.worker[0].ssh_user.as_deref(), Some("u1"));
        assert_eq!(cfg.worker[0].cpu_threads_override, Some(8));
        assert!(cfg.worker[0].gpu_weight_override.is_none());

        assert_eq!(cfg.worker[1].hostname, "box2");
        assert_eq!(cfg.worker[1].binary_path.as_deref(), Some("/opt/fnec"));
        assert!((cfg.worker[1].gpu_weight_override.unwrap() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn from_str_empty_config() {
        let cfg = HostsConfig::from_str("").unwrap();
        assert!(cfg.worker.is_empty());
    }

    #[test]
    fn from_str_with_optional_fields_omitted() {
        let toml = r#"
[[worker]]
hostname = "minimal"
"#;
        let cfg = HostsConfig::from_str(toml).unwrap();
        assert_eq!(cfg.worker.len(), 1);
        assert!(cfg.worker[0].ssh_user.is_none());
        assert!(cfg.worker[0].binary_path.is_none());
        assert!(cfg.worker[0].cpu_threads_override.is_none());
        assert!(cfg.worker[0].gpu_weight_override.is_none());
    }

    #[test]
    fn from_str_invalid_toml_errors() {
        let result = HostsConfig::from_str("not valid toml {{{");
        assert!(result.is_err());
    }

    #[test]
    fn from_file_nonexistent_path_errors() {
        let result = HostsConfig::from_file(std::path::Path::new("/nonexistent/hosts.toml"));
        match result {
            Err(HostsConfigError::Io(_)) => {}
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    /// A field that does nothing is named, per host, rather than ignored in
    /// silence (FND-104). Both fields, each on its own host, so a producer that
    /// checked only one field or only the first host would fail here.
    #[test]
    fn ignored_fields_are_named_per_host() {
        let cfg = HostsConfig::from_str(
            "[[worker]]\nhostname = \"shared-box\"\ncpu_threads_override = 4\n\n\
             [[worker]]\nhostname = \"gpu-box\"\ngpu_weight_override = 6.0\n",
        )
        .expect("parses");
        let w = cfg.ignored_field_warnings();
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(
            w[0].contains("'shared-box'") && w[0].contains("cpu_threads_override"),
            "{w:?}"
        );
        assert!(
            w[1].contains("'gpu-box'") && w[1].contains("gpu_weight_override"),
            "{w:?}"
        );
        assert!(w.iter().all(|l| l.contains("no effect")), "{w:?}");
    }

    /// The negative control: an ordinary config produces no warning, so the test
    /// above cannot be satisfied by a producer that warns about everything.
    #[test]
    fn a_config_without_the_fields_warns_about_nothing() {
        let cfg = HostsConfig::from_str("[[worker]]\nhostname = \"plain\"\nssh_user = \"u\"\n")
            .expect("parses");
        assert!(cfg.ignored_field_warnings().is_empty());
    }

    #[test]
    fn display_error_roundtrip() {
        let io_err =
            HostsConfigError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "nope"));
        let msg = io_err.to_string();
        assert!(msg.contains("IO error"));
        assert!(msg.contains("nope"));
    }
}

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single worker node entry in `hosts.toml`.
///
/// # Example
/// ```toml
/// [[worker]]
/// hostname = "dc0sk-T480"
/// ssh_user = "dc0sk"
///
/// [[worker]]
/// hostname = "dc0sk-rpi51"
/// ssh_user = "dc0sk"
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostEntry {
    /// Hostname or IP address of the worker node.
    pub hostname: String,
    /// SSH login user on the remote.  When absent the SSH client's default applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    /// Path to the `fnec` binary on the remote.  Defaults to `fnec` (PATH lookup).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
    /// **Accepted and ignored.** Documented until v0.18.0 as a cap on task
    /// assignment for a shared node, but nothing ever read it (FND-104) — and
    /// under the scheduler that shipped it could not mean that: the pool runs one
    /// blocking thread per worker, so each worker already has at most one task in
    /// flight, and there is nothing lower to cap to. Kept so existing
    /// `hosts.toml` files still parse; setting it earns a warning from
    /// [`HostsConfig::ignored_field_warnings`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_threads_override: Option<usize>,
    /// **Accepted and ignored**, for the same reason. Documented as a weight that
    /// makes a node "attract more tasks", which only means something to a push
    /// scheduler; the shipped pool is a pull loop in which a faster node already
    /// takes more work by finishing sooner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_weight_override: Option<f64>,
}

/// Top-level structure of `hosts.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HostsConfig {
    /// List of worker nodes.
    #[serde(default)]
    pub worker: Vec<HostEntry>,
}

/// Error loading a hosts config file.
#[derive(Debug)]
pub enum HostsConfigError {
    Io(std::io::Error),
    Toml(toml::de::Error),
}

impl std::fmt::Display for HostsConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostsConfigError::Io(e) => write!(f, "IO error reading hosts config: {e}"),
            HostsConfigError::Toml(e) => write!(f, "TOML parse error in hosts config: {e}"),
        }
    }
}

impl std::error::Error for HostsConfigError {}

impl HostsConfig {
    /// Parse a `HostsConfig` from a TOML string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Load a `HostsConfig` from a TOML file on disk.
    pub fn from_file(path: &Path) -> Result<Self, HostsConfigError> {
        let s = std::fs::read_to_string(path).map_err(HostsConfigError::Io)?;
        toml::from_str(&s).map_err(HostsConfigError::Toml)
    }

    /// One warning line per field a user set that has no effect, naming the host.
    ///
    /// Returned rather than printed, following `worker_warning_lines` in the CLI:
    /// a loader that prints fires inside every test that parses a config with the
    /// field set, and is testable only by capturing stderr. The caller prints
    /// these before connecting to any worker, so the user sees them even when the
    /// run then fails for an unrelated reason.
    ///
    /// Silence was the defect here, not the fields. A user who set
    /// `cpu_threads_override = 4` to spare a shared node was told nothing and got
    /// nothing (FND-104).
    pub fn ignored_field_warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for w in &self.worker {
            if w.cpu_threads_override.is_some() {
                out.push(format!(
                    "warning: [hosts] worker '{}': cpu_threads_override is accepted but has \
                     no effect — each worker already runs one task at a time, so there is \
                     nothing to cap. Remove it.",
                    w.hostname
                ));
            }
            if w.gpu_weight_override.is_some() {
                out.push(format!(
                    "warning: [hosts] worker '{}': gpu_weight_override is accepted but has \
                     no effect — workers pull tasks, so a faster node already takes more \
                     work without a weight. Remove it.",
                    w.hostname
                ));
            }
        }
        out
    }
}
