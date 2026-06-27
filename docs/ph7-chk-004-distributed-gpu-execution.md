---
project: fnec-rust
doc: docs/ph7-chk-004-distributed-gpu-execution.md
status: living
last_updated: 2026-06-27
---

# PH7-CHK-004: distributed GPU execution via the SSH worker pool

## Requirement / change

Roadmap checklist `PH7-CHK-004` (Phase 7): wire `--exec gpu` through the
`nec_worker` SSH pool so a worker that advertises GPU availability in its
capability cache solves its assigned frequency points on its GPU; a CPU-only
node falls back. Add ≥3 integration tests (GPU-capable node uses GPU, CPU-only
node falls back, mixed-pool dispatch).

## Design

The worker protocol already carries a `WorkerSolverConfig` per task and a
per-node `Capability { gpu_available, .. }` cache. The controller decides *who*
is GPU-capable; the worker decides *whether it actually can* run on the GPU and
falls back gracefully. Two small protocol additions make this honest and
observable:

- **`WorkerSolverConfig.exec`** (`"cpu"` | `"gpu"`, serde-default `"cpu"`): the
  controller's request. `--exec gpu` sets it to `"gpu"`.
- **`TaskResult::Ok.exec_used`** (`"cpu"` | `"gpu"`, serde-default `"cpu"`): the
  path the worker actually took — proof a GPU node used its GPU, and that a
  CPU-only node fell back. Both fields default for wire back-compat with
  pre-PH7-CHK-004 peers.

### Worker solve path

`solve.rs` gains `solve_deck_at_frequency_with_exec(deck, freq, basis, exec)`.
When `exec == "gpu"` **and** the deck is in the GPU-resident supported class
(free-space/deferred ground, no LD/TL host matrix stamps, ≥ 16 segments), it
calls `nec_accel::solve_hallen_gpu_resident` (PH7-CHK-003) and reports
`exec_used = "gpu"`. Otherwise — out of class, or `solve_hallen_gpu_resident`
returns `None` (no wgpu adapter) — it falls back to the f64 CPU `solve_hallen`
and reports `exec_used = "cpu"`. The original `solve_deck_at_frequency` becomes a
thin `exec = "cpu"` wrapper, preserving all existing callers.

`worker.rs::process_task` reads `solver_config.exec`, calls the new entry point,
and threads `exec_used` into `TaskResult::Ok`.

### Controller / CLI

`run_distributed_solve` takes the resolved `ExecutionMode` and sets
`solver_config.exec = "gpu"` when it is `Gpu`. Capability-based routing is not
required for correctness — every worker honors the request and falls back if it
has no GPU — so a heterogeneous pool yields correct impedance on every node, with
GPU nodes using their GPU.

### Dependency

`nec_worker` gains `nec_accel` (with the `wgpu` feature) and `pollster`, so a
worker binary deployed on a GPU node can actually dispatch the GPU-resident
solve.

## Tests

- `crates/nec_worker/tests/gpu_exec.rs`:
  1. **GPU-capable node uses GPU** — `exec = "gpu"` on the reference dipole: when
     a wgpu adapter is present, `exec_used == "gpu"` and the impedance matches the
     CPU solve within 2 Ω; skips the strict assert (CPU fallback) with a note when
     no adapter is available.
  2. **CPU-only / out-of-class falls back** — `exec = "gpu"` on a loaded (LD) deck
     is deterministically `exec_used == "cpu"` and still correct.
  3. **Mixed-pool dispatch** — a local worker pool dispatches several `exec=gpu`
     tasks; all succeed and the impedance matches the reference, exercising the
     pool path end-to-end.

## Test results (2026-06-27, real discrete GPU)

- `crates/nec_worker/tests/gpu_exec.rs`:
  - GPU-capable node: `exec_used = "gpu"`, worker `Z = 74.234 + j13.898 Ω` vs CPU
    `74.243 + j13.900 Ω` (Δ ≈ 0.009 Ω, ≤ 2 Ω).
  - Loaded (LD) deck with `exec = "gpu"`: deterministically `exec_used = "cpu"`,
    impedance identical to the CPU solve.
- `apps/nec-cli/tests/worker_gpu_exec.rs` — mixed local pool through the spawned
  `fnec` worker binary: gpu-lane `exec_used = "gpu"` and cpu-lane
  `exec_used = "cpu"`, both within 2 Ω of the local reference.
- `cargo test --workspace` — 538 tests pass. `cargo clippy --workspace` clean.

The `exec`/`exec_used` protocol fields default for back-compat, so a
pre-PH7-CHK-004 peer still interoperates (treated as `cpu`).
