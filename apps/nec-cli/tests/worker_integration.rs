// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Integration tests for the distributed worker — PH6-CHK-006/007.
//!
//! Tests 1-2: library-level (hosts config, capability cache).
//! Tests 3-4: local subprocess worker round-trip.
//! Tests 5-7: SSH worker handle (graceful error handling, localhost round-trip).

mod common;

use base64::Engine;

use nec_worker::WorkerPool;

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
            exec: "cpu".to_string(),
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

/// FND-026 across a real process boundary.
///
/// The protocol round-trip test serialises and deserialises inside one build,
/// which cannot catch a worker binary that never fills the field. This spawns
/// `fnec worker --stdio` as an actual subprocess and asserts the caveat comes
/// back over the pipe — the only test that exercises the deployed shape.
///
/// A card note must reach the caller across the process boundary: an `NT` given
/// by segment 0, which fnec reads as the tag's centre and says so. Before this the
/// worker was the one frontend that said nothing. (The fixture was a malformed
/// `NT`, then an unsupported `LD`; both are refused now, FND-123 and FND-161.)
#[test]
fn a_skipped_card_warning_survives_the_wire_to_a_real_worker() {
    const SEGMENT_ZERO_NT: &str = "CM NT given by segment 0\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nNT 1 0 1 0 0.001 0 0 0 0 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

    let fnec = env!("CARGO_BIN_EXE_fnec");
    let mut worker = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn fnec worker --stdio");

    let task = nec_worker::TaskMessage {
        task_id: "nt-warn".to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: b64(SEGMENT_ZERO_NT),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.2e6,
    };

    let result = worker.dispatch(&task).expect("dispatch should succeed");
    let nec_worker::TaskResult::Ok { warnings, .. } = &result else {
        panic!("deck should still solve, got {result:?}")
    };
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("interpreting segment 0 as center segment 26")),
        "the skipped card must cross the wire: {warnings:?}"
    );

    // Negative control: a clean deck must not acquire a caveat in transit.
    let clean = nec_worker::TaskMessage {
        task_id: "clean".to_string(),
        deck_b64: b64(DIPOLE_DECK),
        ..task
    };
    let result = worker.dispatch(&clean).expect("dispatch should succeed");
    let nec_worker::TaskResult::Ok { warnings, .. } = &result else {
        panic!("clean deck should solve")
    };
    assert!(warnings.is_empty(), "{warnings:?}");

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
            exec: "cpu".to_string(),
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

// ---------------------------------------------------------------------------
// FND-297 d — the worker prices its feed row as the CLI does
// ---------------------------------------------------------------------------

/// Two `EX` cards on segment 11 of one 21-segment dipole.
const TWO_EX_ON_ONE_SEGMENT: &str = "\
CM two EX on one segment
CE
GW 1 21 0 0 -5.282 0 0 5.282 0.001
GE 0
EX 0 1 11 0 1 0
EX 0 1 11 0 0.5 0
FR 0 1 0 0 14.2 0
EN
";

/// The same dipole with one card: the negative control.
const ONE_EX: &str = "\
CM one EX
CE
GW 1 21 0 0 -5.282 0 0 5.282 0.001
GE 0
EX 0 1 11 0 1 0
FR 0 1 0 0 14.2 0
EN
";

/// fnec's row on the two-card deck (the summed drive), Ω.
const TWO_EX_R: f64 = 78.955035;
const TWO_EX_X: f64 = 43.483530;

fn worker_z(deck: &str, task_id: &str) -> (f64, f64) {
    let fnec = env!("CARGO_BIN_EXE_fnec");
    let mut worker = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn fnec worker");
    let task = nec_worker::TaskMessage {
        task_id: task_id.to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: b64(deck),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.2e6,
    };
    let result = worker.dispatch(&task).expect("dispatch should succeed");
    worker.shutdown().ok();
    let nec_worker::TaskResult::Ok { impedance, .. } = result else {
        panic!("{task_id}: the worker did not solve: {result:?}")
    };
    (impedance.re_ohm, impedance.im_ohm)
}

/// The main command's first feedpoint row, from its JSON record.
fn cli_z(deck: &str, file_name: &str) -> (f64, f64) {
    let path = common::TempDeck::new(file_name, deck);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--output-format", "json"])
        .arg(&path)
        .output()
        .expect("run fnec");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let records: Vec<serde_json::Value> =
        serde_json::from_slice(&out.stdout).expect("fnec JSON output");
    let z = |k: &str| records[0][k].as_f64().expect("a number");
    (z("z_re"), z("z_im"))
}

fn assert_same_z(what: &str, worker: (f64, f64), cli: (f64, f64)) {
    let scale = cli.0.hypot(cli.1);
    assert!(
        (worker.0 - cli.0).abs() <= 1e-9 * scale && (worker.1 - cli.1).abs() <= 1e-9 * scale,
        "{what}: worker {worker:?} vs CLI {cli:?}"
    );
}

/// FND-297 d: the worker's feed row is priced from the drive the solve applied,
/// through `nec_solver::feedpoint_drive_voltage` — the seam the CLI, the GUI and
/// the bindings use. The worker read V from the EX card instead, which differs only
/// when two cards share a segment: the solve sums them (1.5 V), and the worker
/// divided one card's 1 V by the summed drive's current, 52.64 + j28.99 Ω where
/// the CLI says 78.96 + j43.48 Ω.
///
/// This pins fnec's **summing** semantics, not nec2c's: nec2c applies the last card
/// (0.5 V) and prices each row V_card/I — 158.07 + j91.71 and 79.03 + j45.86 Ω on
/// this deck (nec2c 1.3.1; fnec's one-card row is 78.96 + j43.48). That is FND-305; its fix re-points this test.
#[test]
fn a_worker_feed_row_is_priced_from_the_drive_the_solve_applied() {
    let worker = worker_z(TWO_EX_ON_ONE_SEGMENT, "two-ex");
    let cli = cli_z(TWO_EX_ON_ONE_SEGMENT, "fnd297d-two-ex.nec");
    assert_same_z("two EX on one segment", worker, cli);
    // Absolute, so a seam that moved both frontends at once still fails here.
    assert!(
        (worker.0 - TWO_EX_R).abs() < 0.01 && (worker.1 - TWO_EX_X).abs() < 0.01,
        "two EX on one segment: {worker:?}"
    );

    // Negative control: one card, where the card's V and the applied drive agree.
    let worker = worker_z(ONE_EX, "one-ex");
    let cli = cli_z(ONE_EX, "fnd297d-one-ex.nec");
    assert_same_z("one EX", worker, cli);
}

// ---------------------------------------------------------------------------
// Test 5 — SshWorkerHandle dispatch against unreachable remote fails
// ---------------------------------------------------------------------------
#[test]
fn test_ssh_worker_dispatch_failure() {
    // Connect to a host that doesn't have sshd running.  With BatchMode=yes
    // and ConnectTimeout=5 the ssh client will exit quickly.  The spawn()
    // itself succeeds, but dispatch() will fail with a broken-pipe or EOF
    // error.
    let entry = nec_worker::HostEntry {
        hostname: "127.0.0.2".to_string(),
        ssh_user: Some("nobody".to_string()),
        binary_path: None,
        cpu_threads_override: None,
        gpu_weight_override: None,
    };

    let mut handle = match nec_worker::SshWorkerHandle::connect(&entry) {
        Ok(h) => h,
        Err(_) => {
            // No ssh binary — skip.
            eprintln!("info: ssh not available, skipping test");
            return;
        }
    };

    let task = nec_worker::TaskMessage {
        task_id: "t-fail".to_string(),
        deck_hash: "x".to_string(),
        deck_b64: String::new(),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.0e6,
    };
    let result = handle.dispatch(&task);
    assert!(
        result.is_err(),
        "expected dispatch to fail when remote is unreachable"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — SshWorkerHandle round-trip via localhost SSH
// ---------------------------------------------------------------------------
// Opt-in: a real localhost SSH round-trip. The `connect()` guard below cannot
// tell a fully-working localhost SSH from one that accepts a connection but
// cannot complete the worker round-trip (seen in sandboxed/CI runners), so this
// test is `#[ignore]` to keep the default `cargo test` gate hermetic. Run it in
// an SSH-provisioned environment with: `cargo test -- --ignored`.
#[test]
#[ignore = "requires a working localhost SSH round-trip; run with --ignored"]
fn test_ssh_worker_localhost_round_trip() {
    let fnec = env!("CARGO_BIN_EXE_fnec");

    // Check that ssh binary is available.
    if std::process::Command::new("ssh")
        .arg("-V")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("info: ssh not available, skipping test");
        return;
    }

    // Try connecting to localhost via SSH.
    let entry = nec_worker::HostEntry {
        hostname: "127.0.0.1".to_string(),
        ssh_user: None,
        binary_path: Some(fnec.to_string()),
        cpu_threads_override: None,
        gpu_weight_override: None,
    };

    let mut handle = match nec_worker::SshWorkerHandle::connect(&entry) {
        Ok(h) => h,
        Err(_) => {
            eprintln!("info: localhost SSH not configured (missing key-based auth), skipping test");
            return;
        }
    };

    // Send a solve task and verify the result.
    let task = nec_worker::TaskMessage {
        task_id: "t-ssh-local".to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: b64(DIPOLE_DECK),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.175e6,
    };

    let result = handle
        .dispatch(&task)
        .expect("SSH dispatch to localhost should succeed");
    assert_eq!(result.task_id(), "t-ssh-local");
    assert!(result.is_ok(), "solve should succeed");

    if let nec_worker::TaskResult::Ok { impedance, .. } = &result {
        assert!(
            impedance.re_ohm > 30.0 && impedance.re_ohm < 120.0,
            "feedpoint resistance should be in 30-120 Ω range, got {} Ω",
            impedance.re_ohm
        );
        // Compare against local solve reference.
        let local = nec_worker::solve::solve_deck_at_frequency(DIPOLE_DECK, 14.175e6, "hallen")
            .expect("local solve should succeed");
        let rel_re = ((impedance.re_ohm - local.impedance_re) / local.impedance_re.abs()).abs();
        assert!(
            rel_re < 1e-6,
            "re error: rel={rel_re:.2e} (ssh={}, local={})",
            impedance.re_ohm,
            local.impedance_re
        );
    }

    handle.shutdown().expect("shutdown should succeed");
}

// ---------------------------------------------------------------------------
// Test 7 — reconnection: SSH worker re-establishes connection after drop
// ---------------------------------------------------------------------------
// Opt-in for the same reason as test_ssh_worker_localhost_round_trip.
#[test]
#[ignore = "requires a working localhost SSH round-trip; run with --ignored"]
fn test_ssh_worker_reconnect_after_disconnect() {
    let fnec = env!("CARGO_BIN_EXE_fnec");

    if std::process::Command::new("ssh")
        .arg("-V")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("info: ssh not available, skipping test");
        return;
    }

    let entry = nec_worker::HostEntry {
        hostname: "127.0.0.1".to_string(),
        ssh_user: None,
        binary_path: Some(fnec.to_string()),
        cpu_threads_override: None,
        gpu_weight_override: None,
    };

    let mut handle = match nec_worker::SshWorkerHandle::connect(&entry) {
        Ok(h) => h,
        Err(_) => {
            eprintln!("info: localhost SSH not configured, skipping test");
            return;
        }
    };

    let task = |id: &str, freq: f64| nec_worker::TaskMessage {
        task_id: id.to_string(),
        deck_hash: "x".to_string(),
        deck_b64: b64(DIPOLE_DECK),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: freq,
    };

    let r1 = handle
        .dispatch(&task("t-rc-1", 14.0e6))
        .expect("first dispatch");
    assert!(r1.is_ok());

    // Kill the SSH subprocess to simulate a dropped connection,
    // then verify that reconnect restores communication.
    handle.reconnect().expect("manual reconnect");

    let r2 = handle
        .dispatch(&task("t-rc-2", 14.5e6))
        .expect("dispatch after reconnect");
    assert!(r2.is_ok(), "solve after reconnect should succeed");
    assert_eq!(r2.task_id(), "t-rc-2");

    handle.shutdown().expect("shutdown");
}

// ---------------------------------------------------------------------------
// Test — concurrent batch dispatch across a worker pool (review-260719 FIND-009)
// ---------------------------------------------------------------------------
//
// `WorkerPool::dispatch` blocks until one worker answers, so driving a sweep
// through it left every other worker idle: M frequency points over N workers cost
// M × latency instead of M/N × latency. `dispatch_batch` gives each worker a
// thread pulling from a shared cursor.
//
// The timing assertion is self-calibrating: the same tasks are run sequentially
// in the same test, on the same machine, so it compares against a control rather
// than against an absolute wall-clock budget that a slow CI runner would fail.

/// A 201-segment dipole: enough work per task that the solve, not the process
/// round trip, dominates — otherwise there is no latency to overlap and the
/// comparison measures scheduling noise.
const HEAVY_DECK: &str = "\
CM concurrency fixture
CE
GW 1 201 0.0 0.0 -5.2782 0.0 0.0 5.2782 0.001
GE 0
EX 0 1 101 0 1.0 0.0
FR 0 1 0 0 14.2 0
EN
";

fn heavy_tasks(count: usize) -> Vec<nec_worker::TaskMessage> {
    (0..count)
        .map(|i| nec_worker::TaskMessage {
            task_id: format!("batch-{i}"),
            deck_hash: "batch".to_string(),
            deck_b64: b64(HEAVY_DECK),
            solver_config: nec_worker::WorkerSolverConfig {
                basis: "hallen".to_string(),
                ground_model: "none".to_string(),
                exec: "cpu".to_string(),
            },
            // Spread the points so a mixed-up ordering would be visible.
            frequency_hz: 14.0e6 + (i as f64) * 0.1e6,
        })
        .collect()
}

fn impedances(results: &[nec_worker::DispatchOutcome]) -> Vec<(f64, f64)> {
    results
        .iter()
        .map(|r| match r {
            Ok((nec_worker::TaskResult::Ok { impedance, .. }, _)) => {
                (impedance.re_ohm, impedance.im_ohm)
            }
            other => panic!("task did not solve: {other:?}"),
        })
        .collect()
}

#[test]
fn batch_dispatch_returns_results_in_task_order() {
    let fnec = env!("CARGO_BIN_EXE_fnec");
    let tasks = heavy_tasks(6);

    let mut batch_pool = WorkerPool::new_local(3, fnec).expect("spawn 3 local workers");
    let batch = batch_pool.dispatch_batch(&tasks);
    batch_pool.shutdown_all();

    let mut seq_pool = WorkerPool::new_local(1, fnec).expect("spawn 1 local worker");
    let sequential: Vec<nec_worker::DispatchOutcome> =
        tasks.iter().map(|t| seq_pool.dispatch(t)).collect();
    seq_pool.shutdown_all();

    assert_eq!(batch.len(), tasks.len());
    // Which worker served a task is free to vary; which task each slot holds is not.
    for (i, r) in batch.iter().enumerate() {
        match r {
            Ok((res, _)) => assert_eq!(
                res.task_id(),
                format!("batch-{i}"),
                "slot {i} holds the wrong task"
            ),
            Err(e) => panic!("task {i} failed: {e}"),
        }
    }
    assert_eq!(
        impedances(&batch),
        impedances(&sequential),
        "concurrency must not change the answers"
    );
}

#[test]
fn batch_dispatch_uses_more_than_one_worker_at_a_time() {
    let fnec = env!("CARGO_BIN_EXE_fnec");
    const WORKERS: usize = 4;
    let tasks = heavy_tasks(WORKERS * 2);

    // Control: the same tasks through one worker, measured on this machine.
    let mut seq_pool = WorkerPool::new_local(1, fnec).expect("spawn 1 local worker");
    let t0 = std::time::Instant::now();
    let sequential: Vec<nec_worker::DispatchOutcome> =
        tasks.iter().map(|t| seq_pool.dispatch(t)).collect();
    let seq_elapsed = t0.elapsed();
    seq_pool.shutdown_all();
    assert!(sequential.iter().all(Result::is_ok), "control run failed");

    let mut pool = WorkerPool::new_local(WORKERS, fnec).expect("spawn local workers");
    let t0 = std::time::Instant::now();
    let batch = pool.dispatch_batch(&tasks);
    let batch_elapsed = t0.elapsed();
    pool.shutdown_all();
    assert!(batch.iter().all(Result::is_ok), "batch run failed");

    eprintln!(
        "FIND-009: {} tasks — sequential {:?}, {WORKERS}-worker batch {:?}",
        tasks.len(),
        seq_elapsed,
        batch_elapsed
    );

    // A generous bound: real overlap on 4 workers should approach 1/4, and any
    // margin below 1.0 is impossible without concurrency. 0.7 leaves room for a
    // CI runner with fewer cores than workers without becoming vacuous.
    assert!(
        batch_elapsed.as_secs_f64() < 0.7 * seq_elapsed.as_secs_f64(),
        "batch dispatch shows no overlap: {batch_elapsed:?} vs sequential {seq_elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// FND-290 — the worker validates the frequency it is asked to solve, not the
// deck's `FR`
// ---------------------------------------------------------------------------

fn task_at(task_id: &str, deck: &str, frequency_hz: f64) -> nec_worker::TaskMessage {
    nec_worker::TaskMessage {
        task_id: task_id.to_string(),
        deck_hash: "ignored".to_string(),
        deck_b64: b64(deck),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz,
    }
}

/// The worker asked `pre_solve_error`, which reads the deck's `FR`: a deck whose
/// `FR` is −5 MHz was refused (`unsupported_config: FR: -5 MHz is not a usable
/// frequency`) at a wire frequency of 14.2 MHz, so under `--hosts --sweep-config`
/// — which the controller passes — every point failed on the worker.
///
/// Gated against nec2c 1.3.1 absolutely (the same dipole with `FR 14.2` and `XQ`:
/// 79.348 + j46.223, within the corpus's stated nec2c tolerance for this deck,
/// 2 Ω / 8 Ω), and equal to `fnec --sweep-config` on the same deck to 1e-9 — a
/// route-consistency check that the worker and the local run solve one thing.
#[test]
fn a_worker_task_is_not_refused_for_the_fr_its_frequency_replaces() {
    const NEG_FR: &str =
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 -5 0\nEN\n";
    let fnec = env!("CARGO_BIN_EXE_fnec");
    let mut worker = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn fnec worker --stdio");
    let result = worker
        .dispatch(&task_at("negfr", NEG_FR, 14.2e6))
        .expect("dispatch should succeed");
    worker.shutdown().ok();
    let nec_worker::TaskResult::Ok { impedance, .. } = &result else {
        panic!("the wire's 14.2 MHz is usable; the deck's FR never runs: {result:?}")
    };
    let (r, x) = (impedance.re_ohm, impedance.im_ohm);
    assert!(
        (r - 79.348).abs() < 2.0 && (x - 46.223).abs() < 8.0,
        "{r} + j{x} vs nec2c 79.348 + j46.223"
    );

    // The local run with the same frequency list.
    let dir = std::env::temp_dir().join(format!("fnec-fnd290-worker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let deck_path = dir.join("negfr.nec");
    let sweep_path = dir.join("sw.toml");
    std::fs::write(&deck_path, NEG_FR).expect("write deck");
    std::fs::write(&sweep_path, "[frequency]\npoints_mhz = [14.2]\n").expect("write sweep");
    let out = std::process::Command::new(fnec)
        .args(["--solver", "hallen", "--exec", "cpu", "--sweep-config"])
        .arg(&sweep_path)
        .arg(&deck_path)
        .output()
        .expect("run fnec");
    std::fs::remove_dir_all(&dir).ok();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let row = stdout
        .lines()
        .find(|l| l.starts_with("1 26 ") && l.split_whitespace().count() == 8)
        .unwrap_or_else(|| panic!("no feedpoint row in:\n{stdout}"));
    let f: Vec<f64> = row
        .split_whitespace()
        .map(|t| t.parse().expect("number"))
        .collect();
    let (cr, cx) = (f[6], f[7]);
    // The report prints six decimals, so the comparison is to its resolution.
    assert!(
        (r - cr).abs() <= 1e-6 + 1e-9 * cr.abs() && (x - cx).abs() <= 1e-6 + 1e-9 * cx.abs(),
        "worker {r} + j{x} vs local --sweep-config {cr} + j{cx}"
    );
}

/// The worker's network check priced the deck's `TL` at the deck's `FR`. A line
/// λ/2 long at 30 MHz on a deck whose `FR` is 29.9 passed the gate at 30 MHz and
/// failed later in the solve; it is now refused by the gate, at the wire's
/// frequency, naming it. Negative control: the same deck at 29.9 MHz solves.
#[test]
fn a_worker_task_prices_the_deck_networks_at_its_own_frequency() {
    const TL_HALF_WAVE_AT_30: &str = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\n\
        GW 2 21 1 0 -2.5 1 0 2.5 0.001\nGE 0\nEX 0 1 11 0 1.0 0.0\n\
        TL 1 11 2 11 50 4.996540966666667 0 0 0 0\nFR 0 1 0 0 29.9 0\nEN\n";
    let fnec = env!("CARGO_BIN_EXE_fnec");
    let mut worker = nec_worker::LocalWorkerHandle::spawn(fnec).expect("spawn fnec worker --stdio");
    let refused = worker
        .dispatch(&task_at("tl30", TL_HALF_WAVE_AT_30, 30.0e6))
        .expect("dispatch should succeed");
    let solved = worker
        .dispatch(&task_at("tl299", TL_HALF_WAVE_AT_30, 29.9e6))
        .expect("dispatch should succeed");
    worker.shutdown().ok();
    let nec_worker::TaskResult::Error {
        error_message: message,
        ..
    } = &refused
    else {
        panic!("a line with no admittance matrix at 30 MHz must be refused: {refused:?}")
    };
    assert!(
        message.contains("TL 1 11 2 11") && message.contains("at 30 MHz, from frequency_hz"),
        "{message}"
    );
    assert!(solved.is_ok(), "29.9 MHz must solve: {solved:?}");
}
