// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Hosts configuration — parsed from `hosts.toml`.

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
/// cpu_threads_override = 4
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
    /// Override the CPU thread count used for capacity-weighted assignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_threads_override: Option<usize>,
    /// Override the GPU weight added to the assignment score (default 4.0 when GPU present).
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
}
