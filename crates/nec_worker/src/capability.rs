// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Per-node capability model and in-memory cache.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_weight_cpu_only() {
        let cap = Capability {
            cpu_threads: 8,
            gpu_available: false,
            wgpu_backend: None,
        };
        assert!((cap.assignment_weight(None) - 8.0).abs() < 1e-9);
        assert!((cap.assignment_weight(Some(2.0)) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn assignment_weight_gpu_default_weight() {
        let cap = Capability {
            cpu_threads: 4,
            gpu_available: true,
            wgpu_backend: Some("Vulkan".into()),
        };
        assert!((cap.assignment_weight(None) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn assignment_weight_gpu_with_override() {
        let cap = Capability {
            cpu_threads: 16,
            gpu_available: true,
            wgpu_backend: Some("Metal".into()),
        };
        assert!((cap.assignment_weight(Some(10.0)) - 26.0).abs() < 1e-9);
    }

    #[test]
    fn capability_cache_insert_get_invalidate() {
        let mut cache = CapabilityCache::new();
        assert!(cache.is_empty());

        let cap = Capability {
            cpu_threads: 8,
            gpu_available: false,
            wgpu_backend: None,
        };
        cache.insert("node-a", cap.clone());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("node-a").map(|c| c.cpu_threads), Some(8));
        assert!(cache.get("node-b").is_none());

        cache.invalidate("node-a");
        assert!(cache.get("node-a").is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn capability_cache_overwrite_replaces() {
        let mut cache = CapabilityCache::new();
        cache.insert(
            "n",
            Capability {
                cpu_threads: 2,
                gpu_available: false,
                wgpu_backend: None,
            },
        );
        cache.insert(
            "n",
            Capability {
                cpu_threads: 16,
                gpu_available: true,
                wgpu_backend: Some("Vulkan".into()),
            },
        );
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("n").unwrap().cpu_threads, 16);
        assert!(cache.get("n").unwrap().gpu_available);
    }

    #[test]
    fn capability_cache_ttl_expires_entry() {
        let mut cache = CapabilityCache::with_ttl(Duration::from_secs(0));
        cache.insert(
            "ephemeral",
            Capability {
                cpu_threads: 1,
                gpu_available: false,
                wgpu_backend: None,
            },
        );
        // TTL is 0 seconds, so elapsed() will be > TTL immediately.
        assert!(cache.get("ephemeral").is_none());
    }

    #[test]
    fn capability_cache_serialize_roundtrip() {
        let cap = Capability {
            cpu_threads: 32,
            gpu_available: true,
            wgpu_backend: Some("Dx12".into()),
        };
        let json = serde_json::to_string(&cap).unwrap();
        let back: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, back);
    }
}

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
