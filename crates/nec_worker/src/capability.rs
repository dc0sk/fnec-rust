// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Per-node capability model and in-memory cache.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Capabilities of a single worker node, as detected or overridden in `hosts.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    /// Number of logical CPU threads available for compute.
    pub cpu_threads: usize,
    /// Whether a usable wgpu GPU adapter was detected on this node.
    pub gpu_available: bool,
    /// Name of the wgpu backend reported by the node, if any.
    /// Examples: `"Vulkan"`, `"Metal"`, `"Dx12"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wgpu_backend: Option<String>,
}

impl Capability {
    /// Capacity weight for round-robin task assignment.
    ///
    /// `weight = cpu_threads + gpu_weight` where `gpu_weight` is
    /// `gpu_weight_override` when present, else `4.0` if GPU is available, else `0.0`.
    pub fn assignment_weight(&self, gpu_weight_override: Option<f64>) -> f64 {
        let gpu_w = if self.gpu_available {
            gpu_weight_override.unwrap_or(4.0)
        } else {
            0.0
        };
        self.cpu_threads as f64 + gpu_w
    }
}

struct CachedEntry {
    capability: Capability,
    cached_at: SystemTime,
}

/// In-memory per-node capability cache.
///
/// Entries have no automatic TTL unless one is configured via [`CapabilityCache::with_ttl`].
/// The cache is invalidated explicitly (on reconnect) via [`CapabilityCache::invalidate`].
#[derive(Default)]
pub struct CapabilityCache {
    entries: HashMap<String, CachedEntry>,
    ttl: Option<Duration>,
}

impl CapabilityCache {
    /// Create a cache with no TTL.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a cache where entries older than `ttl` are treated as misses.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Some(ttl),
        }
    }

    /// Insert or replace the capability record for `hostname`.
    pub fn insert(&mut self, hostname: &str, capability: Capability) {
        self.entries.insert(
            hostname.to_string(),
            CachedEntry {
                capability,
                cached_at: SystemTime::now(),
            },
        );
    }

    /// Look up the capability for `hostname`.
    ///
    /// Returns `None` on a miss or if the entry has exceeded the configured TTL.
    pub fn get(&self, hostname: &str) -> Option<&Capability> {
        let entry = self.entries.get(hostname)?;
        if let Some(ttl) = self.ttl {
            if entry.cached_at.elapsed().unwrap_or(Duration::MAX) > ttl {
                return None;
            }
        }
        Some(&entry.capability)
    }

    /// Remove the capability record for `hostname` (used on reconnect or node failure).
    pub fn invalidate(&mut self, hostname: &str) {
        self.entries.remove(hostname);
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
