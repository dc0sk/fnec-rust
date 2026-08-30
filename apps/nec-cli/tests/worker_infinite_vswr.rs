// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-117 — one unusable result must not destroy the worker pool.
//!
//! `vswr()` returns `f64::INFINITY` whenever `Re(Z) <= 0`, which is the honest
//! answer and not rare: Hallén drives junctioned decks to negative resistance
//! routinely, and `--solver mpie` (which does not) is refused alongside
//! `--hosts`, so those decks are forced down exactly this path.
//!
//! `serde_json` writes a non-finite `f64` as `null`, a plain `f64` field could
//! not read it back, and `WorkerPool::dispatch` treated every `Err` as a dead
//! worker. So one negative-resistance frequency evicted every host in the pool,
//! one dispatch at a time, and every later frequency then failed with "all
//! workers in pool failed".
//!
//! Two independent defects, so two independent assertions: the result must
//! survive the wire, AND a task fault must not be read as a worker fault.

use base64::Engine;
use nec_worker::{TaskResult, WorkerPool};

/// Re(Z) = -5.97 Ω on this deck, so its VSWR is genuinely infinite.
const NEGATIVE_R_DECK: &str = "corpus/inverted-v-negative-r-freesp.nec";
const HEALTHY_DECK: &str = "corpus/dipole-freesp-51seg.nec";

fn task(id: &str, deck_path: &str) -> nec_worker::TaskMessage {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let deck = std::fs::read_to_string(root.join(deck_path))
        .unwrap_or_else(|e| panic!("read {deck_path}: {e}"));
    nec_worker::TaskMessage {
        task_id: id.to_string(),
        deck_hash: "h".to_string(),
        deck_b64: base64::engine::general_purpose::STANDARD.encode(&deck),
        solver_config: nec_worker::WorkerSolverConfig {
            basis: "hallen".to_string(),
            ground_model: "none".to_string(),
            exec: "cpu".to_string(),
        },
        frequency_hz: 14.2e6,
    }
}

/// The result itself must cross. Before the fix this line failed to
/// deserialise outright.
#[test]
fn an_infinite_vswr_reaches_the_controller() {
    let mut pool = WorkerPool::new_local(1, env!("CARGO_BIN_EXE_fnec")).expect("spawn worker");
    let out = pool.dispatch(&task("t-inf", NEGATIVE_R_DECK));
    pool.shutdown_all();

    let (result, _label) = out.expect("an infinite VSWR is an answer, not a transport failure");
    let TaskResult::Ok {
        vswr_50, impedance, ..
    } = result
    else {
        panic!("expected a solved result, got {result:?}");
    };
    assert!(
        impedance.re_ohm < 0.0,
        "the fixture must actually have negative R, or this test proves nothing: {impedance:?}"
    );
    assert!(
        vswr_50.is_infinite() && vswr_50 > 0.0,
        "Re(Z) < 0 means infinite VSWR, got {vswr_50}"
    );
}

/// The eviction taxonomy has its own test binary, `worker_task_fault.rs` — it
/// must be the only test in its process. See the note there.
///
/// End-to-end: the negative-resistance deck that started this, driven FIRST and
/// then followed by a healthy one through the same single-worker pool. Before
/// the fix the first dispatch emptied the pool and the second returned "all
/// workers in pool failed".
///
/// Note what this does and does not prove. It discriminates the WIRE fix only.
/// Once an infinite VSWR round-trips, this deck no longer produces a task fault
/// at all, so this test passes with the eviction bug fully restored — measured,
/// not assumed. The eviction taxonomy is discriminated by
/// `an_unusable_result_fails_the_task_and_keeps_the_worker` above, which
/// manufactures the fault directly.
#[test]
fn a_negative_resistance_deck_flows_through_the_pool() {
    let mut pool = WorkerPool::new_local(1, env!("CARGO_BIN_EXE_fnec")).expect("spawn worker");
    let _first = pool.dispatch(&task("t-1", NEGATIVE_R_DECK));
    assert_eq!(
        pool.len(),
        1,
        "a task fault must not evict the worker that reported it"
    );

    let second = pool.dispatch(&task("t-2", HEALTHY_DECK));
    pool.shutdown_all();

    let (result, _) = second.expect("the pool must still answer after an odd result");
    assert!(
        matches!(result, TaskResult::Ok { .. }),
        "expected the healthy deck to solve, got {result:?}"
    );
}
