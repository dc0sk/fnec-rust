---
project: fnec-rust
doc: docs/project/implementation-map.md
status: living
last_updated: 2026-07-02
---

# Implementation map

The **implementation layer**: every crate and app with its role, its key modules,
and the requirements/checklists it serves. This is the bridge from design docs to
the test catalog.

Workspace layout: 7 library crates (`crates/`), 2 binaries (`apps/`), 1 Python
binding (`bindings/fnec_py/`).

## nec_parser — deck front end
Text → typed AST. Parses NEC cards into `NecDeck`; provides `$VAR` templating.
Serves **FR-003, COMP-001, PH3-CHK-007**.

- `src/lib.rs` — parses `CM/CE/GW/GE/GM/GR/GN/EX/FR/RP/LD/TL/NT/PT/EN` into `NecDeck`; `ParseError`; unknown cards non-fatal.
- `src/template.rs` — `$VARNAME` substitution (`$$` escapes `$`); `TemplateError` on undefined vars.

## nec_model — shared data model
Typed card structs, deck container, and plugin-extension traits. No solving logic.
Serves **FR-001, FR-006, FR-009 (validators), DEC-006, EP-1/EP-4**.

- `src/card.rs` — per-card structs (`GwCard`, `ExCard`, `FrCard`, `LdCard`, `TlCard`, `NtCard`, `PtCard`, `GnCard`, …) + `Card` enum. **PH8-CHK-002 will extend `ExCard` with the plane-wave polarization field.**
- `src/deck.rs` — `NecDeck` container (cards in source order).
- `src/lib.rs` — `DeckPostProcessor` (EP-1) + `DeckValidator` (EP-4) traits, `ValidationDiagnostic`, `run_validators`.

## nec_solver — MoM numerical core
Deck → geometry → impedance/excitation systems → currents → derived quantities.
Serves **FR-001, DEC-010/011, PRT-001/002/008, NFR-004, PH8-CHK-001..006**.

- `src/lib.rs` — facade re-exporting geometry/matrix/excitation/basis/linear/loads/tl/farfield.
- `src/geometry.rs` — `GW` → `Segment` lists; `GroundModel` extraction; junction detection.
- `src/matrix.rs` — Hallén A-matrix (+ Pocklington/Z variants, ground); Gauss-Legendre quadrature + self-term singularity subtraction; `ZMatrix`.
- `src/excitation.rs` — complex RHS from `EX` cards; `build_hallen_rhs` (delta-gap); pulse-RHS scaling; `ExcitationError`. **PH8-CHK-002 adds the plane-wave forcing RHS here.**
- `src/basis.rs` — `ContinuityTransform`, `SinusoidalTransform` basis mappings.
- `src/linear.rs` — dense complex LU (partial pivoting); pulse/Hallén/continuity/sinusoidal solve entries; `SolveError`.
- `src/loads.rs` — `LD` → per-segment complex loads (RLC/RL/RC/Z/conductivity); `LoadWarning`.
- `src/tl.rs` — `TL` → sparse Z stamps (`TlStamp`, lossless 2-port cot/csc); `TlWarning`. **PH8-CHK-004/005 extend (NT stamp, lossy TL).**
- `src/farfield.rs` — RP patterns, directivity (dBi), radiated-power integration, RP point generation, bilinear gain interpolation.

## nec_accel — optional GPU acceleration
CPU reference kernel + real wgpu compute shaders + per-frequency dispatch seam.
Serves **DEC-003/008, NFR-003, PRT-011, Gates G3–G7, PH7-CHK-001..006**. wgpu is
feature-gated (`--features wgpu`).

- `src/lib.rs` — crate overview + `dispatch_frequency_point` scheduling seam (returns CPU fallback when GPU not wired); documents kernel status.
- `src/gpu_kernels.rs` — CPU-reference Hallén far-field kernel (`compute_hallen_fr_point_cpu`/`_batch_cpu`), GPU-ready layouts (`GpuSegment`, `GpuFarFieldPoint`), `KernelTiming`. The parity baseline for shaders (PH7-CHK-001 renamed `*_stub` → `*_cpu`).
- `src/wgpu_device.rs` — real wgpu dispatch: adapter enumeration, no-op gate, WGSL shaders for RP far-field, Z-matrix fill, and GPU-resident Hallén solve; `microbench_zmatrix_dispatch` (PH7-CHK-002); `solve_hallen_gpu_resident` (PH7-CHK-003).
- `src/shaders/*.wgsl` — `zmatrix_fill.wgsl`, RP far-field, `hallen_normal_solve.wgsl` (GPU-resident solve: Jacobi equilibration + complex LU + Björck refinement).

## nec_report — presentation only
Formats solved results into the versioned text report. Serves **FR-005,
PH2-CHK-004, EP-2/EP-3**.

- `src/lib.rs` — `ReportInput` + row types (`CurrentRow`, `FeedpointRow`, `SourceRow`, pattern rows); `render_text_report` (`FORMAT_VERSION 1`); `ReportSection` (EP-3), `ResultFilter` (EP-2). No solving.

## nec_project — project/workflow model
Serde project/run configuration; TOML + Markdown round-trip. Serves **FR-004,
GAP-010/015, PH3-CHK-004/005**.

- `src/lib.rs` — `ProjectFile`, `SolverConfig`, `NamedRun`, `RunHistory`/`RunRecord`/`ResultSummary`; `from/to_toml`, `from/to_markdown`; version-guard `ProjectError`.

## nec_worker — distributed execution
Controller/worker protocol, local + SSH handles, worker pool, capability model,
result cache. Serves **PRT-011, CP-011, PH6-CHK-005/006/007, PH7-CHK-004**.

- `src/lib.rs` — facade + `encode_deck` base64 helper.
- `src/protocol.rs` — NDJSON wire types (`TaskMessage`, `TaskResult`, `Impedance`, `WorkerSolverConfig` incl. `exec`, `ErrorCode`); serde-default for wire back-compat.
- `src/capability.rs` — `Capability` (CPU threads, GPU/wgpu backend), `assignment_weight`, `CapabilityCache`.
- `src/hosts.rs` — `HostsConfig`/`HostEntry` from `hosts.toml`; `HostsConfigError`.
- `src/controller.rs` — `LocalWorkerHandle`: local `fnec worker --stdio` subprocess.
- `src/ssh_worker.rs` — `SshWorkerHandle`: remote worker over `ssh`; `connect_all`.
- `src/pool.rs` — `WorkerPool`/`WorkerHandle` (Local/Ssh); round-robin dispatch.
- `src/solve.rs` — in-worker Hallén solve (`solve_deck_at_frequency`/`_with_exec`); GPU-resident dispatch for supported class (PH7-CHK-004).
- `src/worker.rs` — `run_worker_stdio` event loop (stdin tasks → stdout results).
- `src/result_cache.rs` — SHA-256 `cache_key(deck, config, freq)`; FIFO `ResultCache`.

## apps/nec-cli (`fnec`) — CLI frontend & orchestrator
Args → validate → solve (single/sweep/hybrid GPU/distributed) → report/bench.
Serves **FR-002, FR-007/008, NFR-005, PRT-003, all `--exec`/sweep/vars flags**.

- `src/main.rs` — entry wiring parse → validate → solve → report; mode select; sweep dispatch (local or `WorkerPool`).
- `src/cli_args.rs` — arg parsing (`parse_args`, `ParsedArgs`, `USAGE`, `OutputFormat`).
- `src/solve_session.rs` — solve orchestration: `SolverMode`/`PulseRhsMode`, per-point solve, sweep, residual metrics, feedpoint/source/load rows, hybrid-lane planning. **PH8-CHK-002 touches the EX-type routing here.**
- `src/exec_profile.rs` — `Cpu/Hybrid/Gpu` selection; 4nec2 drop-in recognition; GPU probe.
- `src/geometry_validation.rs` — intersection / buried-wire / risky-source checks.
- `src/resonance_search.rs` — bisection resonance targeting (PH3-CHK-008).
- `src/sweep_config.rs` — `--sweep-config` TOML (linear/point list).
- `src/vars_config.rs` — `--vars` JSON/TOML map loader.
- `src/bench.rs` — benchmark record emission (human/CSV/JSON).
- `src/warnings.rs` — non-fatal user warnings (deferred ground, NT/PT deferral, pulse-experimental, GPU fallback). **PH8-CHK-002 replaces the EX plane-wave warning with real semantics.**

## apps/nec-gui (`nec-gui`) — iced desktop frontend
Testable state machine over `nec_solver`: solve, sweep, pattern, currents.
Serves **FR-002, PRT-004, PH3-CHK-009/010/011**.

- `src/main.rs` — iced app (`FnecGui`) wrapping `AppState`; async solve/sweep/pattern/currents tasks.
- `src/lib.rs` — library facade (`app_state`, `solve`) for binary + headless tests.
- `src/app_state.rs` — iced-free state machine: tabs, per-pipeline phases, `Message`, `AppState::apply`.
- `src/solve.rs` — thin `nec_solver` wrappers (solve/sweep/pattern/currents).

## bindings/fnec_py — Python binding
`pyo3` cdylib exposing `solve_deck_str`/`sweep_deck_str`. Serves **FR-008,
COMP-012, PH4-CHK-004**.
