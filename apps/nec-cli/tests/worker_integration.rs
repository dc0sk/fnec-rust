// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Integration tests for the distributed worker — PH6-CHK-006.
//!
//! Tests 1 and 2 are pure library-level tests (no subprocess).
//! Tests 3 and 4 spawn `fnec worker --stdio` as a subprocess.

use base64::Engine;

const DIPOLE_DECK: &str = include_str!("../../../corpus/dipole-freesp-51seg.nec");

/// helper — encode a deck string to base64 (matches `nec_worker::encode_deck`)
fn b64(s: &str) -> String {
    use base64::engine::general_purpose::STANDARD;
    STANDARD.encode(s.as_bytes())
}

// ---------------------------------------------------------------------------
// Test 1 — hosts.toml round-trip
// ---------------------------------------------------------------------------
#[test]
fn test_hosts_config_from_str() {
    let toml_src = r#"
[[worker]]
hostname = "box1.local"
ssh_user = "dc0sk"
cpu_threads_override = 8

[[worker]]
hostname = "box2.local"
binary_path = "/opt/fnec/fnec"
gpu_weight_override = 6.0
"#;
    let cfg =
        nec_worker::HostsConfig::from_str(toml_src).expect("hosts.toml should parse without error");
    assert_eq!(cfg.worker.len(), 2);

    let w0 = &cfg.worker[0];
    assert_eq!(w0.hostname, "box1.local");
    assert_eq!(w0.ssh_user.as_deref(), Some("dc0sk"));
    assert_eq!(w0.cpu_threads_override, Some(8));
    assert!(w0.gpu_weight_override.is_none());

    let w1 = &cfg.worker[1];
    assert_eq!(w1.hostname, "box2.local");
    assert_eq!(w1.binary_path.as_deref(), Some("/opt/fnec/fnec"));
    assert!((w1.gpu_weight_override.unwrap() - 6.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Test 2 — CapabilityCache round-trip
// ---------------------------------------------------------------------------
#[test]
fn test_capability_cache_roundtrip() {
    let mut cache = nec_worker::CapabilityCache::new();
    assert!(cache.is_empty());

    let cap = nec_worker::Capability {
        cpu_threads: 16,
        gpu_available: true,
        wgpu_backend: Some("Vulkan".to_string()),
    };
    cache.insert("box1.local", cap.clone());
    assert_eq!(cache.len(), 1);

    let fetched = cache.get("box1.local").expect("entry should be present");
    assert_eq!(fetched.cpu_threads, 16);
    assert!(fetched.gpu_available);
    assert_eq!(fetched.wgpu_backend.as_deref(), Some("Vulkan"));

    assert!(cache.get("no-such-host").is_none());

    // Invalidation must make the entry disappear.
    cache.invalidate("box1.local");
    assert!(cache.get("box1.local").is_none());
    assert!(cache.is_empty());
}

// ---------------------------------------------------------------------------
// Test 3 — single-task round trip through fnec worker --stdio
// ---------------------------------------------------------------------------
#[test]
fn test_worker_single_task_round_trip() {
    let fnec = env!("CARGO_BIN_EXE_fnec");
    let mut worker = nec_worker::LocalWorkerHandle::spawn(fnec)
        .expect("should be able to spawn fnec worker --stdio");

    let task = nec_worker::TaskMessage {
        task_id: "t001".to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: b64(DIPOLE_DECK),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
        },
        frequency_hz: 14.175e6,
    };

    let result = worker.dispatch(&task).expect("dispatch should succeed");

    assert_eq!(result.task_id(), "t001", "task_id must be echoed back");
    assert!(
        result.is_ok(),
        "solve should succeed for dipole in free space"
    );

    if let nec_worker::TaskResult::Ok { impedance, .. } = &result {
        assert!(
            impedance.re_ohm > 30.0 && impedance.re_ohm < 120.0,
            "feedpoint resistance should be in 30-120 Ω range, got {} Ω",
            impedance.re_ohm
        );
    }

    worker.shutdown().expect("shutdown should succeed");
}

// ---------------------------------------------------------------------------
// Test 4 — two-worker dispatch, results match local solve
// ---------------------------------------------------------------------------
#[test]
fn test_worker_two_node_solve_matches_local() {
    let fnec = env!("CARGO_BIN_EXE_fnec");
    let mut w0 = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn worker 0");
    let mut w1 = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn worker 1");

    let freqs = [(14.0e6_f64, "t_14_0"), (14.5e6_f64, "t_14_5")];

    let build_task = |freq: f64, task_id: &str| nec_worker::TaskMessage {
        task_id: task_id.to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: b64(DIPOLE_DECK),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
        },
        frequency_hz: freq,
    };

    // Dispatch 14 MHz to w0, 14.5 MHz to w1.
    let r0 = w0
        .dispatch(&build_task(freqs[0].0, freqs[0].1))
        .expect("dispatch to w0 should succeed");
    let r1 = w1
        .dispatch(&build_task(freqs[1].0, freqs[1].1))
        .expect("dispatch to w1 should succeed");

    assert!(r0.is_ok(), "14 MHz solve should succeed");
    assert!(r1.is_ok(), "14.5 MHz solve should succeed");

    // Compare against local solve reference.
    for (result, freq_hz) in [(&r0, freqs[0].0), (&r1, freqs[1].0)] {
        let local = nec_worker::solve::solve_deck_at_frequency(DIPOLE_DECK, freq_hz, "hallen")
            .expect("local solve should succeed");

        if let nec_worker::TaskResult::Ok { impedance, .. } = result {
            let rel_re = ((impedance.re_ohm - local.impedance_re) / local.impedance_re.abs()).abs();
            let rel_im = if local.impedance_im.abs() > 1e-6 {
                ((impedance.im_ohm - local.impedance_im) / local.impedance_im.abs()).abs()
            } else {
                (impedance.im_ohm - local.impedance_im).abs()
            };
            assert!(
                rel_re < 1e-6,
                "re error at {freq_hz:.0} Hz: rel={rel_re:.2e} (worker={}, local={})",
                impedance.re_ohm,
                local.impedance_re
            );
            assert!(
                rel_im < 1e-6,
                "im error at {freq_hz:.0} Hz: rel={rel_im:.2e} (worker={}, local={})",
                impedance.im_ohm,
                local.impedance_im
            );
        }
    }

    w0.shutdown().ok();
    w1.shutdown().ok();
}
