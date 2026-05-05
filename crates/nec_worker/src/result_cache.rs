// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! SHA-256-keyed result cache for distributed frequency sweeps.
//!
//! # Cache key
//!
//! A cache key is the lower-hex SHA-256 digest of the concatenation:
//!
//! ```text
//! {deck_str}\0{basis}\0{ground_model}\0{freq_hz_bits}
//! ```
//!
//! where `freq_hz_bits` is the big-endian IEEE-754 bit pattern of `freq_hz`
//! formatted as a 16-character hex string.  This guarantees exact, bit-stable
//! matching regardless of floating-point rounding in the caller.
//!
//! # Eviction policy
//!
//! The cache is bounded by an optional `max_entries` capacity.  When the
//! limit is reached, the **oldest inserted entry** is evicted (FIFO order).
//! The insertion order is tracked with a [`std::collections::VecDeque`] of
//! keys; on eviction the front of the queue is removed from both the deque
//! and the HashMap.
//!
//! When `max_entries` is `None` the cache is unbounded.

use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};

use crate::protocol::{TaskResult, WorkerSolverConfig};

/// Compute the cache key for a given deck, solver config, and frequency.
///
/// The key is stable: identical inputs always produce the same 64-character
/// hex string, and any change to deck content, solver config, or frequency
/// point produces a different key.
pub fn cache_key(deck_str: &str, config: &WorkerSolverConfig, freq_hz: f64) -> String {
    let freq_bits = format!("{:016x}", freq_hz.to_bits());
    let mut hasher = Sha256::new();
    hasher.update(deck_str.as_bytes());
    hasher.update(b"\x00");
    hasher.update(config.basis.as_bytes());
    hasher.update(b"\x00");
    hasher.update(config.ground_model.as_bytes());
    hasher.update(b"\x00");
    hasher.update(freq_bits.as_bytes());
    format!("{:x}", hasher.finalize())
}

struct CacheEntry {
    result: TaskResult,
}

/// In-memory SHA-256-keyed result cache.
///
/// ## Eviction policy
///
/// When `max_entries` is set, the oldest-inserted entry is evicted (FIFO)
/// when the capacity limit is reached.  This ensures bounded memory usage
/// during long frequency sweeps without complex LRU bookkeeping.
///
/// ## Usage pattern
///
/// 1. Before dispatching a task, call [`ResultCache::get`] with the key from
///    [`cache_key`].  On a hit, skip the remote solve and return the cached
///    result.
/// 2. On a miss, run the remote solve and call [`ResultCache::insert`].
/// 3. When the deck or solver config changes, call [`ResultCache::invalidate`]
///    or [`ResultCache::clear`] to prevent stale results.
pub struct ResultCache {
    entries: HashMap<String, CacheEntry>,
    insertion_order: VecDeque<String>,
    max_entries: Option<usize>,
}

impl Default for ResultCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultCache {
    /// Create an unbounded cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
            max_entries: None,
        }
    }

    /// Create a cache bounded to `max_entries` (FIFO eviction when full).
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(max_entries),
            insertion_order: VecDeque::with_capacity(max_entries),
            max_entries: Some(max_entries),
        }
    }

    /// Insert a result under `key`.  If the key already exists it is replaced
    /// (no duplicate in the insertion-order queue is added).
    ///
    /// If the cache is at capacity, the oldest entry is evicted first.
    pub fn insert(&mut self, key: impl Into<String>, result: TaskResult) {
        let key = key.into();
        if self.entries.contains_key(&key) {
            // Replace in-place; don't grow the insertion-order queue.
            self.entries.insert(key, CacheEntry { result });
            return;
        }
        // Evict oldest if at capacity.
        if let Some(max) = self.max_entries {
            while self.entries.len() >= max {
                if let Some(oldest) = self.insertion_order.pop_front() {
                    self.entries.remove(&oldest);
                } else {
                    break;
                }
            }
        }
        self.insertion_order.push_back(key.clone());
        self.entries.insert(key, CacheEntry { result });
    }

    /// Look up a result by key.  Returns `None` on a miss.
    pub fn get(&self, key: &str) -> Option<&TaskResult> {
        self.entries.get(key).map(|e| &e.result)
    }

    /// Remove a single entry by key.  No-op if the key is not present.
    pub fn invalidate(&mut self, key: &str) {
        if self.entries.remove(key).is_some() {
            self.insertion_order.retain(|k| k != key);
        }
    }

    /// Remove all entries whose `task_id` starts with `deck_hash`.
    ///
    /// This is a convenience helper for invalidating all cached results for a
    /// particular deck when the deck content changes.  The caller should use
    /// the same deck hash that was embedded in the `TaskMessage.deck_hash`
    /// field when the results were computed.
    pub fn invalidate_by_deck_hash(&mut self, deck_hash: &str) {
        let to_remove: Vec<String> = self
            .entries
            .iter()
            .filter_map(|(k, e)| {
                let task_id = match &e.result {
                    TaskResult::Ok { task_id, .. } => task_id,
                    TaskResult::Error { task_id, .. } => task_id,
                };
                if task_id.starts_with(deck_hash) {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect();
        for k in &to_remove {
            self.entries.remove(k);
            self.insertion_order.retain(|ik| ik != k);
        }
    }

    /// Remove all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.insertion_order.clear();
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
