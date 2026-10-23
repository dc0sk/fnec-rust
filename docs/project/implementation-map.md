---
project: fnec-rust
doc: docs/project/implementation-map.md
status: living
last_updated: 2026-10-01
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

- `src/lib.rs` — parses `CM/CE/GW/GE/GM/GR/GN/EX/FR/RP/NE/NH/LD/TL/NT/PT/EN` into `NecDeck`; `ParseError`; every other NEC card is in `OTHER_CARDS`, ignored with a warning when it cannot change the antenna and refused when it would, and an unknown mnemonic is refused (FND-236).
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
- `src/refine.rs` — a deck meshed S times finer for `fnec converge`: every GW/GA/GH count × S, every EX/LD/NT/TL reference on the sub-segment modelling the same thing (`EndRule`: an end reference keeps its end or takes the centre), lumped ranges expanded, per-metre ranges stretched, RP/NE/NH/PT dropped.
- `src/convergence.rs` — the settle rule of a mesh series (FND-255/256): `remaining_drift` (the geometric tail), `classify`/`trend`; one rule for the corpus gate and `fnec converge`.
- `src/matrix.rs` — Hallén A-matrix (+ Pocklington/Z variants, ground); Gauss-Legendre quadrature + self-term singularity subtraction; `ZMatrix`.
- `src/excitation.rs` — complex RHS from `EX` cards; `build_hallen_rhs` (delta-gap); pulse-RHS scaling; `ExcitationError`. **PH8-CHK-002 adds the plane-wave forcing RHS here.**
- `src/basis.rs` — `ContinuityTransform`, `SinusoidalTransform` basis mappings.
- `src/linear.rs` — dense complex LU (partial pivoting); pulse/Hallén/continuity/sinusoidal solve entries; `SolveError`.
- `src/loads.rs` — `LD` → per-segment complex loads (RLC/RL/RC/Z/conductivity); `LoadWarning`.
- `src/tl.rs` — `TL` → sparse Z stamps (`TlStamp`, lossless 2-port cot/csc); `TlWarning`. **PH8-CHK-004/005 extend (NT stamp, lossy TL).** *Superseded 2026-09-26 (FND-111/FND-123): TL and NT are no longer stamped into the matrix; they are two-port networks connected across the port gaps (NEC-2 model), and `TL` uses the NEC-2 layout.*
- `src/farfield.rs` — RP patterns, directivity (dBi), radiated-power integration, RP point generation, bilinear gain interpolation.
- `src/hallen_session.rs` — which Hallén solve a deck gets, decided once for every frontend: `hallen_route`, `solve_hallen_routed`, `solve_hallen_planewave_routed`/`_planned`, the FND-270 node-gap rule (`NodeGaps`).
- `src/section_graph.rs` — Hallén on a graph of sections: degree-3+ junctions and closed loops (FND-162 stages 2+3).
- `src/corner.rs` — the corner term of the Hallén bend condition (FND-162 stage 1b).
- `src/current_source.rs` — the `EX 4` impressed-current solve (`solve_current_source_hallen`, `scale_to_impressed_current`).
- `src/planewave.rs` — incident plane-wave forcing (`EX 1/2/3`) for the receive solve.
- `src/ground_contact.rs` — wires touching a perfect ground, solved by explicit images (FND-082).
- `src/network.rs` — `TL`/`NT` two-ports connected across the port gaps (`build_networks`, `solve_with_networks`; FND-123).
- `src/stamps.rs` — the deck's matrix contributions (lumped loads), built once (`build_deck_stamps`).
- `src/feedpoint.rs` — a solved feedpoint to an impedance, or a refusal when the current is zero.
- `src/frequency.rs` — the frequencies a deck asks for, one derivation for every frontend.
- `src/validate.rs` — frontend-agnostic pre-solve validation and diagnostics: buried / intersecting wires, ground contact, the junction-feed, low-ground and topology warnings (the checks `apps/nec-cli`'s old `geometry_validation.rs` held).
- `src/sommerfeld.rs` — the Sommerfeld half-space reflected field (PH9-CHK-006): the feedpoint correction and the reflected-E dyadic.
- `src/mpie.rs` — the MPIE solver (PH9-CHK-007): triangle basis, free-space fill, the perfect-ground image (`add_pec_image`) and the finite-ground remainder (`add_ground_remainder`, FND-239).
- `src/mpie_session.rs` — driving the MPIE from a deck: geometry, feeds, loads, the refusals.
- `src/lnetwork.rs` — lossless L-network matching for `fnec match` (GAP-012).
- `src/taper.rs` — Leeson step-tapered-radius correction for `fnec taper` (BL-IMPR-014).

## nec_accel — optional GPU acceleration
CPU reference kernel + real wgpu compute shaders + per-frequency dispatch seam.
Serves **DEC-003/008, NFR-003, PRT-011, Gates G3–G7, PH7-CHK-001..006**. wgpu is
feature-gated (`--features wgpu`).

- `src/lib.rs` — crate overview, dispatch policy (the GPU is chosen per deck by the CLI; the per-frequency seam was removed) and kernel status.
- `src/kernel_reference.rs` — CPU-reference Hallén far-field kernel (`compute_hallen_fr_point_cpu`/`_batch_cpu`), GPU-ready layouts (`GpuSegment`, `GpuFarFieldPoint`), `KernelTiming`. The parity baseline for shaders (PH7-CHK-001 renamed `*_stub` → `*_cpu`).
- `src/wgpu_device.rs` — real wgpu dispatch: adapter enumeration, no-op gate, WGSL shaders for RP far-field, Z-matrix fill, and GPU-resident Hallén solve; `microbench_zmatrix_dispatch` (PH7-CHK-002); `solve_hallen_gpu_resident` (PH7-CHK-003).
- `src/shaders/*.wgsl` — `zmatrix_fill.wgsl`, RP far-field, `hallen_lu_solve.wgsl` (GPU-resident solve: direct LU of the column-scaled M, one dispatch per phase, refinement — FND-185).
- `src/gpu_test.rs` — the one re-run policy for tests that run on the GPU (FND-215).

## nec_report — presentation only
Formats solved results into the versioned text report. Serves **FR-005,
PH2-CHK-004, EP-2/EP-3**.

- `src/lib.rs` — `ReportInput` + row types (`CurrentRow`, `FeedpointRow`, `SourceRow`, pattern rows); `render_text_report` (`FORMAT_VERSION 1`); `ReportSection` (EP-3), `ResultFilter` (EP-2). No solving.
- `src/nec2d.rs` — the NEC-2D report layout (PAR-011), so 4nec2 can read fnec's answer in place of its kernel's.
- `src/fortran.rs` — a Fortran FORMAT interpreter for the subset NEC-2's output statements use.

## nec_project — project/workflow model
Serde project/run configuration; TOML + Markdown round-trip. Serves **FR-004,
GAP-010/015, PH3-CHK-004/005**.

- `src/lib.rs` — `ProjectFile`, `SolverConfig`, `NamedRun`, `RunHistory`/`RunRecord`/`ResultSummary`; `from/to_toml`, `from/to_markdown`; version-guard `ProjectError`.

## nec_worker — distributed execution
Controller/worker protocol, local + SSH handles, worker pool, capability model.
Serves **PRT-011, CP-011, PH6-CHK-005/006, PH7-CHK-004** (PH6-CHK-007, the result
cache, was withdrawn — FND-155).

- `src/lib.rs` — facade + `encode_deck` base64 helper.
- `src/protocol.rs` — NDJSON wire types (`TaskMessage`, `TaskResult`, `Impedance`, `WorkerSolverConfig` incl. `exec`, `ErrorCode`); serde-default for wire back-compat.
- `src/capability.rs` — `Capability` (CPU threads, GPU/wgpu backend), `CapabilityCache`. Not consulted by the CLI's `--hosts` path (FND-104); `assignment_weight` was removed as the code form of a weighting scheme that was never built.
- `src/hosts.rs` — `HostsConfig`/`HostEntry` from `hosts.toml`; `HostsConfigError`; `ignored_field_warnings` for the two accepted-but-ignored override keys.
- `src/controller.rs` — `LocalWorkerHandle`: local `fnec worker --stdio` subprocess.
- `src/ssh_worker.rs` — `SshWorkerHandle`: remote worker over `ssh`; `connect_all` (probes; unused by the CLI).
- `src/pool.rs` — `WorkerPool`/`WorkerHandle` (Local/Ssh); `dispatch` (sequential, unused by the CLI) and `dispatch_batch` (the pull loop `--hosts` runs).
- `src/solve.rs` — in-worker Hallén solve (`solve_deck_at_frequency`/`_with_exec`); GPU-resident dispatch for supported class (PH7-CHK-004).
- `src/worker.rs` — `run_worker_stdio` event loop (stdin tasks → stdout results).
- `src/pipe.rs` — the child-process pipe a worker handle talks over, with deadlines.

## apps/nec-cli (`fnec`) — CLI frontend & orchestrator
Args → validate → solve (single/sweep/hybrid GPU/distributed) → report/bench.
Serves **FR-002, FR-007/008, NFR-005, PRT-003, all `--exec`/sweep/vars flags**.

- `src/main.rs` — entry wiring parse → validate → solve → report; mode select; sweep dispatch (local or `WorkerPool`).
- `src/cli_args.rs` — arg parsing (`parse_args`, `ParsedArgs`, `USAGE`, `OutputFormat`).
- `src/solve_session.rs` — solve orchestration: `SolverMode`/`PulseRhsMode`, per-point solve, sweep, residual metrics, feedpoint/source/load rows, hybrid-lane planning. **PH8-CHK-002 touches the EX-type routing here.**
- `src/exec_profile.rs` — `Cpu/Hybrid/Gpu` selection; 4nec2 drop-in recognition; GPU probe.
- `src/match_cmd.rs` — `fnec match`, the L-network matching helper (GAP-012).
- `src/project_cmd.rs` — `fnec project convert`, project-file import/export (GAP-015).
- `src/kernel.rs` — `fnec nec2d-kernel`, 4nec2's external-kernel contract (PAR-011).
- `src/nec2d_out.rs` — `--output-format nec2d`: the solve in NEC-2D's layout (PAR-011).
- `src/exec_calibration.rs` — this host's CPU/GPU crossover, measured by `fnec calibrate`.
- `src/laplace_config.rs` — `--loads-config` TOML → Laplace-domain loads.
- `src/resonance_search.rs` — bisection resonance targeting (PH3-CHK-008).
- `src/converge_cmd.rs` — `fnec converge`, the convergence-study helper (GAP-012): refines the deck per factor (`nec_solver::refine`), refuses up front what no mesh may solve, solves each mesh through `solve_session::probe_feedpoints` (caveats under a `mesh Nx:` prefix), and judges each row by `nec_solver::convergence`; text and `fnec-converge/1` JSON.
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
- `src/viewport/mod.rs`, `src/viewport/primitive.rs` — the GPU 3-D viewport and its wire/axes/grid line primitive (GUI-CHK-002).
- `src/camera.rs` — orbit camera for the viewport (GUI-CHK-002/003).
- `src/mesh.rs` — pure line-mesh construction for the viewport (GUI-CHK-002).
- `src/model_doc.rs`, `src/deck_write.rs` — the editable deck document and the NEC deck writer behind the visual editor (GUI-CHK-007).
- `src/plot.rs`, `src/sweep_plot.rs` — sweep-chart math and its canvas (GUI-CHK-009).
- `src/sweep_stream.rs` — the streaming sweep task, extracted so its messages can be tested.

## bindings/fnec_py — Python binding
`pyo3` cdylib exposing `solve_deck_str`/`sweep_deck_str`. Serves **FR-008,
COMP-012, PH4-CHK-004**.
