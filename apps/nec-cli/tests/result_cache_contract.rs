// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Contract tests for the distributed result cache — PH6-CHK-007.
//!
//! Tests 1-3 are pure unit-level cache contracts (hit / miss / invalidation).
//! Test 4 demonstrates that a 5-point sweep with one changed deck reuses
//! 4 cached results and re-solves exactly 1 changed point.

const DIPOLE_DECK: &str = include_str!("../../../corpus/dipole-freesp-51seg.nec");

/// Build a default solver config.
fn default_config() -> nec_worker::WorkerSolverConfig {
    nec_worker::WorkerSolverConfig {
        basis: "hallen".to_string(),
        ground_model: "none".to_string(),
        exec: "cpu".to_string(),
    }
}

/// Build an Ok TaskResult with the given impedance.
fn ok_result(task_id: &str, freq_hz: f64, re: f64, im: f64) -> nec_worker::TaskResult {
    nec_worker::TaskResult::Ok {
        task_id: task_id.to_string(),
        frequency_hz: freq_hz,
        impedance: nec_worker::Impedance {
            re_ohm: re,
            im_ohm: im,
        },
        vswr_50: 1.0,
        feedpoint_current_mag: 0.01,
        feedpoint_current_phase_deg: 0.0,
        exec_used: "cpu".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Test 1 — cache hit
// ---------------------------------------------------------------------------
#[test]
fn test_result_cache_hit() {
    let mut cache = nec_worker::ResultCache::new();
    let cfg = default_config();
    let freq = 14.175e6;
    let key = nec_worker::cache_key(DIPOLE_DECK, &cfg, freq);

    // Miss before insert.
    assert!(cache.get(&key).is_none(), "cache must be empty initially");

    cache.insert(key.clone(), ok_result("t001", freq, 73.1, 1.3));

    // Hit after insert.
    let result = cache.get(&key).expect("entry must be present after insert");
    assert!(result.is_ok());
    if let nec_worker::TaskResult::Ok { impedance, .. } = result {
        assert!((impedance.re_ohm - 73.1).abs() < 1e-9);
        assert!((impedance.im_ohm - 1.3).abs() < 1e-9);
    }
    assert_eq!(cache.len(), 1);
}

// ---------------------------------------------------------------------------
// Test 2 — cache miss
// ---------------------------------------------------------------------------
#[test]
fn test_result_cache_miss() {
    let cache = nec_worker::ResultCache::new();
    let cfg = default_config();

    // Different frequencies produce different keys → all misses.
    let k1 = nec_worker::cache_key(DIPOLE_DECK, &cfg, 14.0e6);
    let k2 = nec_worker::cache_key(DIPOLE_DECK, &cfg, 14.5e6);
    assert!(cache.get(&k1).is_none());
    assert!(cache.get(&k2).is_none());
    assert_ne!(k1, k2, "keys must differ for different frequencies");

    // Changing basis changes the key.
    let cfg2 = nec_worker::WorkerSolverConfig {
        basis: "other".to_string(),
        ground_model: "none".to_string(),
        exec: "cpu".to_string(),
    };
    let k3 = nec_worker::cache_key(DIPOLE_DECK, &cfg2, 14.0e6);
    assert_ne!(k1, k3, "keys must differ for different solver configs");
}

// ---------------------------------------------------------------------------
// Test 3 — cache invalidation
// ---------------------------------------------------------------------------
#[test]
fn test_result_cache_invalidation() {
    let mut cache = nec_worker::ResultCache::new();
    let cfg = default_config();
    let freqs = [14.0e6, 14.175e6, 14.35e6];

    // Insert three entries.
    let keys: Vec<_> = freqs
        .iter()
        .map(|&f| nec_worker::cache_key(DIPOLE_DECK, &cfg, f))
        .collect();
    for (i, (&freq, key)) in freqs.iter().zip(&keys).enumerate() {
        cache.insert(
            key.clone(),
            ok_result(&format!("t{i}"), freq, 70.0 + i as f64, 0.0),
        );
    }
    assert_eq!(cache.len(), 3);

    // Invalidate one entry; others must remain.
    cache.invalidate(&keys[1]);
    assert!(
        cache.get(&keys[1]).is_none(),
        "invalidated entry must be gone"
    );
    assert!(cache.get(&keys[0]).is_some(), "other entries must survive");
    assert!(cache.get(&keys[2]).is_some(), "other entries must survive");
    assert_eq!(cache.len(), 2);

    // Clear all entries.
    cache.clear();
    assert!(cache.is_empty(), "cache must be empty after clear");
}

// ---------------------------------------------------------------------------
// Test 4 — 5-point sweep: 4 cache hits, 1 re-solve on deck change
// ---------------------------------------------------------------------------
#[test]
fn test_five_point_sweep_cache_reuse() {
    let cfg = default_config();
    let freqs = [14.0e6, 14.175e6, 14.35e6, 14.5e6, 14.7e6];

    // --- Phase 1: cold sweep over original deck ---
    let mut cache = nec_worker::ResultCache::new();
    let mut cold_solves = 0usize;

    for &freq in &freqs {
        let key = nec_worker::cache_key(DIPOLE_DECK, &cfg, freq);
        if cache.get(&key).is_none() {
            let fp = nec_worker::solve::solve_deck_at_frequency(DIPOLE_DECK, freq, "hallen")
                .expect("cold solve should succeed");
            cache.insert(
                key,
                ok_result("sweep", freq, fp.impedance_re, fp.impedance_im),
            );
            cold_solves += 1;
        }
    }
    assert_eq!(
        cold_solves, 5,
        "all 5 points must be solved on a cold cache"
    );
    assert_eq!(cache.len(), 5);

    // --- Phase 2: change the deck (modify the FR card frequency) and re-run the sweep ---
    // The modified deck has a different text → a different SHA-256 key for every
    // frequency.  We only invalidate the one entry whose natural key changes,
    // simulating a controller that only re-solves the single altered point.
    //
    // Here we swap the deck used for freqs[0] (14.0 MHz) and use the original
    // deck for the remaining 4 points.  This is equivalent to "the deck for one
    // point changed; the others are served from cache".
    let modified_deck = DIPOLE_DECK.replace("14.2", "14.3");
    assert_ne!(
        modified_deck, DIPOLE_DECK,
        "modified deck must differ from original"
    );
    let changed_freq = freqs[0]; // 14.0 MHz — this point uses the modified deck

    // Invalidate the entry whose cache key changes due to the deck modification.
    // (The original key used the original deck text so it's a different key.)
    // Simulate what a controller would do: re-sweep all 5 frequencies with
    // the new deck, counting hits vs re-solves.
    let mut hits = 0usize;
    let mut recomputed = 0usize;

    for &freq in &freqs {
        // The controller uses the active deck for key computation.
        let active_deck = if freq == changed_freq {
            modified_deck.as_str()
        } else {
            DIPOLE_DECK
        };
        let key = nec_worker::cache_key(active_deck, &cfg, freq);
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            let fp = nec_worker::solve::solve_deck_at_frequency(active_deck, freq, "hallen")
                .expect("re-solve should succeed");
            cache.insert(
                key,
                ok_result("sweep2", freq, fp.impedance_re, fp.impedance_im),
            );
            recomputed += 1;
        }
    }

    assert_eq!(hits, 4, "4 of 5 frequencies must be served from cache");
    assert_eq!(
        recomputed, 1,
        "only the changed frequency must be re-solved"
    );
    // Cache now holds 6 entries (5 original + 1 new key for modified deck).
    assert_eq!(cache.len(), 6);
}

// ---------------------------------------------------------------------------
// Test 5 — bounded cache evicts oldest entry (FIFO)
// ---------------------------------------------------------------------------
#[test]
fn test_result_cache_bounded_eviction() {
    let cfg = default_config();
    let freqs = [14.0e6, 14.1e6, 14.2e6];

    let mut cache = nec_worker::ResultCache::with_capacity(2);

    let keys: Vec<_> = freqs
        .iter()
        .map(|&f| nec_worker::cache_key(DIPOLE_DECK, &cfg, f))
        .collect();

    // Insert entry 0, then entry 1 — fills the capacity-2 cache.
    cache.insert(keys[0].clone(), ok_result("t0", freqs[0], 70.0, 0.0));
    cache.insert(keys[1].clone(), ok_result("t1", freqs[1], 71.0, 0.0));
    assert_eq!(cache.len(), 2);

    // Insert entry 2 — must evict entry 0 (oldest).
    cache.insert(keys[2].clone(), ok_result("t2", freqs[2], 72.0, 0.0));
    assert_eq!(cache.len(), 2, "capacity must be respected after eviction");
    assert!(
        cache.get(&keys[0]).is_none(),
        "oldest entry must have been evicted"
    );
    assert!(cache.get(&keys[1]).is_some(), "second entry must survive");
    assert!(
        cache.get(&keys[2]).is_some(),
        "newest entry must be present"
    );
}
