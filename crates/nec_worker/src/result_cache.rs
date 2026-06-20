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
        if let Some(entry) = self.entries.get_mut(&key) {
            *entry = CacheEntry { result };
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result(task_id: &str) -> TaskResult {
        TaskResult::Ok {
            task_id: task_id.into(),
            frequency_hz: 14.2e6,
            impedance: crate::protocol::Impedance {
                re_ohm: 74.24,
                im_ohm: 13.90,
            },
            vswr_50: 1.5,
            feedpoint_current_mag: 0.5,
            feedpoint_current_phase_deg: 10.0,
        }
    }

    fn sample_config() -> WorkerSolverConfig {
        WorkerSolverConfig {
            basis: "hallen".into(),
            ground_model: "none".into(),
        }
    }

    #[test]
    fn cache_key_stable_for_identical_inputs() {
        let k1 = cache_key("deck content", &sample_config(), 14.2e6);
        let k2 = cache_key("deck content", &sample_config(), 14.2e6);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64);
    }

    #[test]
    fn cache_key_changes_on_deck_change() {
        let k1 = cache_key("deck A", &sample_config(), 14.2e6);
        let k2 = cache_key("deck B", &sample_config(), 14.2e6);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_on_frequency_change() {
        let k1 = cache_key("deck", &sample_config(), 14.0e6);
        let k2 = cache_key("deck", &sample_config(), 14.2e6);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_on_config_change() {
        let c2 = WorkerSolverConfig {
            basis: "sinusoidal".into(),
            ..sample_config()
        };
        let k1 = cache_key("deck", &sample_config(), 14.2e6);
        let k2 = cache_key("deck", &c2, 14.2e6);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_hit_after_insert() {
        let mut cache = ResultCache::new();
        let key = "k1";
        assert!(cache.get(key).is_none());
        cache.insert(key, sample_result("t1"));
        assert!(cache.get(key).is_some());
    }

    #[test]
    fn cache_miss_for_unknown_key() {
        let cache = ResultCache::new();
        assert!(cache.get("no-such-key").is_none());
    }

    #[test]
    fn cache_invalidate_removes_entry() {
        let mut cache = ResultCache::new();
        cache.insert("k", sample_result("t"));
        assert!(cache.get("k").is_some());
        cache.invalidate("k");
        assert!(cache.get("k").is_none());
    }

    #[test]
    fn cache_clear_removes_all() {
        let mut cache = ResultCache::new();
        cache.insert("a", sample_result("t1"));
        cache.insert("b", sample_result("t2"));
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_fifo_eviction_when_at_capacity() {
        let mut cache = ResultCache::with_capacity(2);
        cache.insert("k1", sample_result("t1"));
        cache.insert("k2", sample_result("t2"));
        assert_eq!(cache.len(), 2);
        cache.insert("k3", sample_result("t3"));
        assert_eq!(cache.len(), 2);
        assert!(cache.get("k1").is_none());
        assert!(cache.get("k2").is_some());
        assert!(cache.get("k3").is_some());
    }

    #[test]
    fn cache_replace_in_place_does_not_grow_queue() {
        let mut cache = ResultCache::with_capacity(2);
        cache.insert("k1", sample_result("t1"));
        cache.insert("k2", sample_result("t2"));
        cache.insert("k1", sample_result("t1-again"));
        assert_eq!(cache.len(), 2);
        assert!(cache.get("k1").is_some());
        assert!(cache.get("k2").is_some());
    }

    #[test]
    fn invalidate_by_deck_hash_matches_task_id_prefix() {
        let mut cache = ResultCache::new();
        cache.insert(
            "key-a",
            TaskResult::Ok {
                task_id: "hash123-freq1".into(),
                frequency_hz: 14.0e6,
                impedance: crate::protocol::Impedance {
                    re_ohm: 70.0,
                    im_ohm: 10.0,
                },
                vswr_50: 1.2,
                feedpoint_current_mag: 0.6,
                feedpoint_current_phase_deg: 5.0,
            },
        );
        cache.insert(
            "key-b",
            TaskResult::Ok {
                task_id: "hash999-freq1".into(),
                frequency_hz: 14.2e6,
                impedance: crate::protocol::Impedance {
                    re_ohm: 74.0,
                    im_ohm: 14.0,
                },
                vswr_50: 1.3,
                feedpoint_current_mag: 0.55,
                feedpoint_current_phase_deg: 8.0,
            },
        );
        assert_eq!(cache.len(), 2);
        cache.invalidate_by_deck_hash("hash123");
        assert_eq!(cache.len(), 1);
        assert!(cache.get("key-a").is_none());
        assert!(cache.get("key-b").is_some());
    }

    #[test]
    fn cache_is_empty_on_create() {
        let cache = ResultCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn unbounded_cache_never_evicts() {
        let mut cache = ResultCache::new();
        for i in 0..10_000 {
            cache.insert(format!("k{i}"), sample_result(&format!("t{i}")));
        }
        assert_eq!(cache.len(), 10_000);
    }
}
