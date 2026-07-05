---
project: fnec-rust
doc: docs/changelog.md
status: living
last_updated: 2026-07-05
---

# Changelog

All notable documentation process changes are recorded here.

## [Unreleased]
### Added

- **PH9-CHK-002 receive-side junctions CLI-wired (degree-2, plane wave)** — the CLI
  plane-wave receive path (`solve_plane_wave_hallen`) now routes degree-2 junctioned
  geometry through the conductor-path solver, so a **receiving** bent or connected
  antenna (bend, start-to-start / end-to-end split, inverted-V) solves and emits a
  `RECEIVE_PATTERN` where it previously failed fast with `JunctionedGeometryNotSupported`.
  Reducible decks (single wires, collinear chains, parallel arrays) keep the
  validated per-wire path; degree-3+ (T/Y) and closed loops still fail fast.
  End-to-end gate: a start-to-start split dipole's receive sweep shows the correct
  z-dipole shape and matches its own transmit gain pattern by reciprocity to
  0.025 dB (`apps/nec-cli/tests/receive_junction.rs`). `docs/card-support-matrix.md`
  EX type 1 updated. See `docs/ph9-chk-002-general-junction.md`.

- **PH9-CHK-002 receive-side junction solve core (degree-2, plane wave)** — the
  conductor-path model now backs a *distributed*-excitation solver, so a
  **receiving** bent or connected antenna solves on continuous paths. A plane wave
  induces an asymmetric current, so each conductor path carries **two** homogeneous
  constants (`cos`/`sin`) with `I = 0` at its two free ends only
  (`solve_hallen_planewave_paths`); the forcing sums the incident field over the
  whole path with the traversal-sign + signed-arc-length convention
  (`build_planewave_hallen_paths`). Validated internally: a start-to-start split
  dipole (one arm reversed) reproduces the validated per-wire receive solver to
  machine precision (~1e-11) on the identical mesh, and a bent inverted-V's induced
  feed current tracks its transmit far-field by reciprocity to 1.5 % across a ~8×
  gain range. This is the self-contained solve core (new solver + validation, no
  CLI/corpus churn); routing it into the CLI receive path is the follow-up
  increment. See `docs/ph9-chk-002-general-junction.md`.

- **PH9-CHK-002 general junction basis (degree-2)** — the Hallén delta-gap solve now
  handles **any degree-2 conductor chain** — bends, start-to-start / end-to-end
  splits, and inverted-V apex feeds — not just collinear splits. `build_conductor_paths`
  walks the wire-endpoint graph into continuous *conductor paths* and the solve
  carries a per-segment traversal sign and signed arc-length, so the homogeneous
  `cos(k·s)` basis stays continuous across the junction with one shared constant per
  path (`build_hallen_rhs_paths` / `solve_hallen_paths`). A λ/2 dipole split at the
  feed now solves 74.41 + j14.52 Ω whether the join is end-to-start or start-to-start
  (was −34 − j1447); a 30°/45°/90° inverted-V matches nec2c's radiation resistance to
  2–4 %. The junction-fed feedpoint warning is suppressed for these now-correct
  cases; degree-3+ (T/Y) junctions, closed loops, and receive-side junctions remain
  guarded (PH9-CHK-005). Zero regression (594 tests). See
  `docs/ph9-chk-002-general-junction.md`.

- **PH9-CHK-004 near electric and magnetic field (`NE` / `NH` cards)** — fnec can now compute the near
  electric field on a rectangular grid of observation points (`NE I1 NX NY NZ X0
  Y0 Z0 DX DY DZ`), emitting a `NEAR_FIELD` report section. The field is the
  Hertzian-element sum over the solved segment currents (full 1/r, 1/r², 1/r³
  terms). Validated: at 200 λ it is transverse and its magnitude matches the
  independently gain-derived far field to 0.02 %; on a dipole's equatorial axis it
  is axis-polarized with the cross-component vanishing by symmetry. Point-element
  accuracy holds away from the wire surface; very-near-the-wire (extended kernel)
  and spherical grids are out of scope. The `NH` card is the exact magnetic
  companion (`NEAR_H_FIELD` section), validated by the far-field `|E| = η·|H|`
  relationship. `docs/card-support-matrix.md` `NE`/`NH` → Partial.

- **PH9-CHK-004 `PT` print-control** — the `PT` (print-control) card is now applied
  at runtime instead of being parsed-and-ignored: `I1 ≤ −1` suppresses the segment
  current output, `I1 = 0` prints all currents (default), and `I1 ≥ 1` restricts
  the output to tag `I2` and the optional segment range `I3..I4` (last `PT` card
  wins). The former "PT card support is currently deferred" warning is removed;
  `docs/card-support-matrix.md` `PT` → Partial.

## [0.9.0] — 2026-07-05 — Phase 9 progress: receive patterns, ground gain, junction robustness
### Added

- **Negative-resistance guardrail (PH9-CHK-005)** — a passive antenna cannot have a
  negative input resistance, so a negative `Re(Z)` on the Hallén path now warns that
  the result is unphysical (a junctioned-geometry limitation; see PH9-CHK-002). This
  complements the junction-*fed* warning by catching cases fed *away* from a bad
  junction (e.g. a bent dipole fed mid-arm). Scoped to `--solver hallen` (the pulse
  current-source path has documented negative-`R` values); no valid Hallén corpus
  case trips it.

- **PH9-CHK-002 collinear junction fix** — a straight conductor split across
  several `GW` cards is now solved as one wire. Root cause: fnec's Hallén
  homogeneous solution (`cos(k·s)` + constant) was built per `GW` wire and reset at
  each junction. `merge_collinear_wire_endpoints` merges end-to-start, equal-radius,
  collinear wire chains into one logical conductor for the homogeneous basis; a λ/2
  dipole split at its feed now solves **74.41 + j14.52 Ω** (was −34 − j1447 —
  negative resistance). The merge is a strict no-op for single wires, parallel
  arrays, bends, and stepped-radius junctions, so those are byte-for-byte unchanged.
  Non-collinear junctions (bends, T/Y) remain guarded by PH9-CHK-005.

- **PH9-CHK-005 junction-fed feedpoint guardrail** — feeding a segment that sits
  at a wire junction gives an unphysical impedance in fnec's per-segment `V/I` (a
  half-wave dipole split into two wires and fed at the junction reports
  −34−j1447 Ω instead of the true 74+j14 Ω, because the feed current splits across
  the joined wires). The CLI now **warns** when the driven segment is on a
  junction instead of silently reporting the wrong impedance; the accurate fix is
  PH9-CHK-002. Feeds away from junctions and single-wire geometries are unaffected.

- **PH9-CHK-001 incident-plane-wave receive-pattern sweep** — a plane-wave `EX`
  card with an incidence-angle grid (NTHETA×NPHI, Δθ/Δφ) now produces a
  `RECEIVE_PATTERN` section: the antenna's response vs the wave's arrival
  direction. The per-angle response is the peak induced current — resolved
  empirically to match the transmit gain pattern by reciprocity to <0.01 dB, so no
  arbitrary terminal is needed. `ExCard` gains F4/F5 (Δθ/Δφ). EX types 1/2/3 →
  angle sweeps supported.

- **PH9-CHK-003 absolute gain over finite ground** — the radiation pattern over a
  lossy finite ground now reports **gain** (not directivity): it is scaled by the
  radiation efficiency `η = P_radiated / P_input` (the ground-absorbed power), so
  the reported dBi matches nec2c's absolute gain. Closes the ~1.3 dB
  directivity-vs-gain offset documented in PH8-CHK-006. The normalization constant
  is validated by a lossless free-space dipole (η = 0.9996 ≈ 1); a horizontal
  dipole over average ground matches nec2c's absolute gain to 0.06 dB. Free-space /
  PEC (lossless, η ≈ 1) are unchanged. New public `radiation_efficiency`.

### Docs

- **PH9-CHK-002 junction accuracy diagnosed** — a verified root-cause analysis of
  why junctioned multi-wire feedpoints are mis-solved. A controlled experiment
  (single 52-seg wire → 74.41+j14.52 Ω; the same dipole as two wires → negative
  resistance; *merging* the wire grouping does **not** help) pins the cause to the
  Hallén **homogeneous solution**: the `cos(k·s)` along-wire coordinate resets per
  `GW` wire and the homogeneous constant is independent per wire, so the basis is
  discontinuous across a junction. It is *not* the current-continuity constraint.
  The collinear case of this fix is now implemented (see the PH9-CHK-002 Added entry above); bends/T-junctions remain. See `docs/ph9-chk-002-junction-feed-diagnosis.md`.

- **Phase 9 drafted** (`docs/roadmap.md` "Phase 9: accuracy frontier & scattering
  breadth") — six planned items grounded in the surviving `PRT-*` gaps and the
  Phase 8 frontier deferrals: incidence-angle sweeps + receive pattern, junctioned
  multi-wire receive solves, absolute gain over lossy ground, PT + full RP output
  modes, a difficult-geometry accuracy corpus, and a first Sommerfeld/buried
  near-ground increment. A draft for review; first-frontier priority is a product
  decision.

### Fixed

- **`RP` card XNDA field** — the radiation-pattern card parser now accepts the
  canonical 8-field NEC form (`RP mode Nθ Nφ XNDA θ0 φ0 Δθ Δφ`) in addition to
  fnec's legacy 7-field form. Previously a standard 8-field `RP` card mis-parsed
  θ0 (it read the XNDA/I4 value as θ0), so real 4nec2 pattern decks produced an
  all-null pattern. Distinguished by field count; XNDA does not affect the angle
  grid.

## [0.8.0] — 2026-07-04 — Phase 8 complete: mainstream deck portability
### Added

- **PH8-CHK-005 lossy transmission line** — `TL` cards with `tl_type != 0` now
  stamp a lossy line, `Z0·coth(γℓ)` / `Z0·csch(γℓ)` with complex `γℓ = αℓ + jβℓ`
  (velocity factor 1, `F3` = matched-line loss in dB). Reduces exactly to the
  lossless `−jZ0·cot/csc` at 0 dB. Validated: lossless limit <1e-9, attenuation
  monotone with loss, high-loss input impedance → Z0. **Completes the Phase 8
  checklist (PH8-CHK-001..006).** `docs/card-support-matrix.md` `TL other` →
  Partial.

- **PH8-CHK-006 radiation pattern over finite ground** — the far-field over a
  finite (imperfect) ground now uses the Fresnel reflection-coefficient
  approximation instead of the free-space pattern (only PEC ground had an image
  before). A horizontal/vertical antenna over real earth now shows the correct
  ground lobe and horizon null. Validated: PEC limit matches to <0.05 dB; the
  pattern shape matches nec2c to 0.053 dB (horizontal dipole over average ground).
  fnec reports directivity; the ~1.3 dB offset vs nec2c gain (ground-loss
  efficiency) is documented.

- **PH8-CHK-003 EX type 5 (voltage source)** — EX type 5 (voltage source,
  current-slope discontinuity) now solves: fnec models it via its applied-field
  method, so the feedpoint impedance equals type 0's, on both `--solver hallen`
  and `--solver pulse`. This completes the EX-source family (types 0–5).
  Deck-portability (CP-003): type-5 decks run instead of failing. NEC's separate
  current-slope numerics (~6% different) are a documented non-goal.
  `docs/card-support-matrix.md` EX type 5 → Partial.

- **PH8-CHK-001/002 non-junctioned multi-wire** — incident plane waves and
  current sources now solve on **one or more straight, non-junctioned wires**
  (e.g. a parallel dipole array), not just a single wire. The plane-wave Hallén
  forcing is per-wire (own axis, own along-wire coordinate, same-wire kernel
  sum). Validated: each wire's induced-current shape matches nec2c (~10%); a
  symmetric-broadside wave induces equal currents on two parallel wires (5e-11);
  a two-wire current-source port impedance matches the voltage source. Junctioned
  geometry fails fast.
- **PH8-CHK-002 elliptic plane waves (EX types 2/3)** — right- and left-hand
  elliptic incident plane waves now solve on `--solver hallen`. The incident
  field uses a complex polarization vector (`ê = û_maj + j·sense·AR·û_minor`,
  axial ratio from EX F6, handedness from the type). Validated: on a z-wire (or
  axial ratio 0) elliptic reduces exactly to linear; on a tilted wire the induced
  currents match nec2c's elliptic reference (5.4% shape). `ExCard` gains a
  `polarization_ratio` field. The legacy `--ex3-i4-mode` flag is now an obsolete
  no-op (type 3 is a plane wave). EX types 2/3 → Partial.
- **PH8-CHK-004 NT two-port network** — user-runnable: `NT` cards are stamped
  into the Z matrix by converting their admittance parameters to impedance
  parameters (`[Z]=[Y]⁻¹`), mirroring the TL stamp. A well-formed reciprocal NT
  reproduces the equivalent TL feedpoint impedance end to end
  (`dipole-nt-tl-equiv-freesp-51seg`, matches to ~1e-5 Ω). The blanket "NT
  deferred" warning is removed; malformed / singular-admittance / missing-endpoint
  cards warn and are skipped. `docs/card-support-matrix.md` NT → Partial.
- **PH8-CHK-001 current source (NEC2 EX type 4)** — user-runnable end to end:
  `solve_hallen_current_source` treats the port voltage as an unknown and forces
  `I[src]=i0`, the exact dual of the delta-gap voltage source; validated by
  impedance-consistency (current-source Z equals voltage-source Z to 2×10⁻⁴). The
  CLI routes single-straight-wire type-4 decks on `--solver hallen` and reports
  `FEEDPOINTS Z=V/i0`; the `dipole-ex4` corpus case validates the impedance.
  Multi-wire geometry and non-Hallén solvers fail fast.
  `docs/card-support-matrix.md` EX type 4 → Partial.
- **Project traceability layer** (`docs/project/`): a consolidated
  requirement → design → implementation → tests → results matrix with a
  per-push maintenance rule (#256).
- **PH8-CHK-002 CLI wiring** — incident plane-wave decks are now user-runnable:
  `--solver hallen` on a single straight wire with a linear plane wave (EX type 1)
  produces a receiving-antenna solve — induced `CURRENTS`, no feedpoint impedance.
  Elliptic polarization (types 2/3), multi-wire geometry, and non-Hallén solvers
  fail fast with actionable diagnostics. `docs/card-support-matrix.md` EX type 1
  → Partial.
- **PH8-CHK-002 solve core** — incident plane-wave Hallén solve:
  `nec_solver::planewave` builds the plane-wave forcing RHS (tangential incident
  field integrated with the delta-gap Hallén normalization), and
  `solve_hallen_planewave` solves it with a two-DOF (cos+sin) homogeneous system
  — the freedom classical Hallén needs for an asymmetric receive current. The
  shared delta-gap `solve_hallen` is untouched. Validated: nec2c induced-current
  shape parity 4.3%, broadside symmetry exact, Rayleigh–Carson reciprocity vs the
  validated transmit far-field exact. Not yet wired into the CLI (next
  increment).
- **PH8-CHK-002 code foundation** — NEC2 EX-type alignment in code: an
  `ExcitationKind` classifier (single source of the NEC2 0–5 numbering),
  `ExCard.polarization_deg` (plane-wave polarization field F3, read by the
  parser), and a NEC2-category-accurate reject diagnostic (e.g. *"incident
  plane wave, linear polarization (type 1) … is not yet supported"*). EX types
  1–5 still fail fast — the plane-wave/current-source solves are later
  increments — so no corpus contract changed. `docs/card-support-matrix.md` EX
  rows corrected to NEC2 numbering.

### Changed

- **Dependency hygiene**: documented, scoped exception for two `quick-xml` DoS
  advisories (RUSTSEC-2026-0194/0195) in `.cargo/audit.toml` + `deny.toml` —
  build-time-only Wayland proc-macro path, root fix blocked upstream. Revisit
  when wayland-scanner ships `quick-xml >= 0.41`.

## [0.7.0] — 2026-06-27 — Phase 7 complete: GPU productionization
### Added

- **PH7-CHK-006 — native ROCm/SYCL backend: dated deferral**. `docs/multi-vendor-gpu.md`
  records a verified, dated deferral: the AMD target (Renoir `gfx90c` APU) is
  outside AMD's ROCm support matrix, no ROCm/HIP/OpenCL/SYCL toolchain is present,
  and a native backend would duplicate kernels for no correctness gain over the
  already-validated RADV Vulkan path. Concrete blockers + revisit trigger and the
  backend matrix updated; corrected a stale "GPU dispatch deferred" note now that
  PH7-CHK-003/004 dispatch real kernels.

- **PH7-CHK-005 — real discrete-GPU benchmark evidence**: harness
  `apps/nec-cli/examples/gpu_crossover.rs` measures the Z-matrix-fill and RP
  kernels against CPU on a real AMD GPU (`RADV RENOIR`, Vulkan). Artifact
  `benchmarks/real-gpu-crossover.json`; crossover documented in `docs/benchmarks.md`
  (Z-fill kernel-only: GPU beats CPU below 32 segments, up to ~240× at 1536;
  RP wall-clock 1.5–1.8× faster). Refreshes the retired `FNEC_ACCEL_STUB_GPU`
  references in `docs/benchmarks.md`. See `docs/ph7-chk-005-real-gpu-benchmark.md`.

- **PH7-CHK-002 — in-process GPU microbenchmark**: `nec_accel::microbench_zmatrix_dispatch`
  pays the wgpu device-initialization cost once and times many reused kernel
  dispatches, so per-dispatch time is isolated from device-init (which the
  across-process G5 gate cannot separate). Returns `GpuMicrobench { device_init_us,
  dispatch_min_us, dispatch_median_us, .. }`. Artifact schema documents the
  optional `gpu_microbench` object (and corrects the retired `FNEC_ACCEL_STUB_GPU`
  reference). Measured ~61 ms device-init vs ~0.27 ms dispatch; non-flaky over 10
  runs. See `docs/ph7-chk-002-gpu-microbenchmark.md`.

- **PH7-CHK-004 — distributed GPU execution**: `--exec gpu` is wired through the
  `nec_worker` SSH pool. New `WorkerSolverConfig.exec` request and
  `TaskResult.exec_used` report fields (serde-default for wire back-compat);
  `solve_deck_at_frequency_with_exec` dispatches the GPU-resident solve
  (PH7-CHK-003) on a node with a wgpu adapter for the supported deck class, and
  falls back to the CPU solve otherwise. `nec_worker` now depends on `nec_accel`
  (wgpu). See `docs/ph7-chk-004-distributed-gpu-execution.md`.

- **PH7-CHK-003 — GPU-resident Hallén solve**: `solve_hallen_gpu_resident`
  (`crates/nec_accel`, `shaders/hallen_normal_solve.wgsl`) fills the Z-matrix and
  solves the regularized normal-equations system entirely on the GPU — Jacobi
  equilibration + complex LU (partial pivoting) + Björck least-squares refinement
  — returning only the solution vector (the N×N matrix never leaves the device).
  Wired into CLI `--exec gpu` for the supported Hallén class (free-space, no
  LD/TL). Matches the f64 CPU solve to ~0.01 Ω on the reference dipole. f32
  precision; the f64 CPU solve stays the corpus-gate reference. See
  `docs/ph7-chk-003-gpu-resident-solve.md`.

### Changed

- **PH7-CHK-001 — retired the GPU CPU-emulation scaffold**: removed every code path
  that reported CPU compute as GPU work. `nec_accel::gpu_kernels` is now documented
  and named as the **CPU reference** far-field kernel (parity baseline for the wgpu
  shaders); `compute_hallen_fr_*_stub` renamed to `*_cpu`. Removed the
  `FNEC_ACCEL_STUB_GPU` env hack, `ExecutionPath::GpuStubEmulation`,
  `execute_frequency_point`, the dead `HallenRhsGpuKernel`/`PocklingtonMatrixGpuKernel`
  structs, and the "accelerator stub backend … CPU emulation" warnings. See
  `docs/ph7-chk-001-gpu-stub-retirement.md`.

### Removed

- **`--gpu-fr` CLI flag**: it only ran a CPU computation labelled as GPU. Superseded by
  `--exec gpu`, which dispatches the real wgpu RP / Z-matrix-fill kernels.

## [0.6.0] — 2026-05-05
### Added

- **Phase 6 complete** — all seven PH6-CHK items done (CI benchmark dashboard, NEC-5 frontier decision, sinusoidal-basis EFIE, multi-vendor GPU validation, distributed execution design, SSH-backed worker deployment, SHA-256 result cache).
- **`nec_worker` crate**: new library crate implementing the distributed worker protocol — `TaskMessage`/`TaskResult` JSON-lines protocol, `HostsConfig` TOML, per-node `CapabilityCache`, `solve_deck_at_frequency()` Hallén pipeline, `run_worker_stdio()` event loop, and `LocalWorkerHandle` subprocess controller.
- **`fnec worker --stdio` subcommand**: worker node mode added to `nec-cli`; spawns a JSON-lines solve loop on stdin/stdout for SSH-pipe transport.
- **SHA-256 result cache (`ResultCache`)**: deterministic cache keyed on `hash(deck + solver_config + freq_hz)`; FIFO-bounded capacity; hit/miss/invalidation contract tests; 5-point sweep reuse demonstrated.
- **CI benchmark dashboard (PH6-CHK-001)**: GitHub Actions workflow publishing benchmark JSON artifacts; regression delta threshold enforced.
- **NEC-5 frontier decision doc** (`docs/nec5-frontier.md`): explicit wire-only continuation decision; ≥3 corpus expansion cases mapped to PH6N5-* rows.
- **Sinusoidal-basis EFIE (PH6-CHK-003)**: piecewise-sinusoidal matrix assembly in `nec_solver`; EXPERIMENTAL warning retired.
- **Multi-vendor GPU doc** (`docs/multi-vendor-gpu.md`): Vulkan/Metal/DX12/OpenCL backend matrix; AMD validation; ROCm/SYCL deferred path documented.
- **Distributed execution design doc** (`docs/distributed-execution-design.md`): SSH stdio transport, ed25519 authN, worker contract, frequency-point work-split, result-cache design.
- **Worker deployment guide** (`docs/worker-deployment.md`): per-node SSH key setup, `hosts.toml` reference, wire protocol examples, troubleshooting.

## [0.5.0] — 2026-05-04
### Added

- **Phase 2 complete** — all eight PH2-CHK items done (ground models, buried-wire guardrails, source/load/network semantics, report/table parity, corpus truth expansion, geometry diagnostics, NEC-5 validation matrix, scriptability preservation).
- **Phase 5 complete** — all seven PH5-CHK GPU acceleration items done (G1–G7 gates: architecture decision, wgpu scaffold, RP WGSL kernel, CLI `--exec gpu` wiring, CPU-vs-GPU benchmark gate, Z-matrix fill WGSL kernel, full GPU Hallén solve path).

## [0.4.0] — 2026-05-02
### Added

- **PH5-CHK-007 (Full GPU Hallén solve path — gate G7)**: `--exec gpu` now uses `fill_zmatrix_wgpu` (from PH5-CHK-006) to fill the Hallén A-matrix on the GPU for free-space and deferred-ground decks, then feeds the result to the existing CPU LU (`solve_hallen`). Ground-augmented models (PEC, finite ground) retain the CPU fill path. `ZMatrix::from_flat` constructor added to `nec_solver` for building a `ZMatrix` from a flat row-major `Vec<Complex64>`. GPU path falls back to CPU with a `stderr` warning when no wgpu adapter is available. New gate G7 end-to-end test `crates/nec_accel/tests/gpu_hallen_solve.rs`: builds a 51-segment dipole at 14 MHz, fills Z on GPU, solves with CPU Hallén, checks feedpoint impedance within ±2 Ω of all-CPU reference; achieved ΔR=0.000 Ω, ΔX=0.000 Ω (GPU f32 precision is sufficient for accurate solve).

- **PH5-CHK-006 (GPU Z-matrix fill WGSL kernel — gate G6)**: New `crates/nec_accel/src/shaders/zmatrix_fill.wgsl` — WGSL compute shader that fills the N×N Hallén A-matrix; each thread computes one element Z[i,j]. Off-diagonal elements use 8-point GL with reduced kernel; self elements use 4-point GL smooth part + analytic log singularity subtraction (identical algorithm to CPU `assemble_z_matrix`). New public async `fill_zmatrix_wgpu(segments, freq_hz)` in `wgpu_device.rs` packs f64 segment data (including radius) into a `GpuSegmentZ` buffer, dispatches `ceil(N²/64)` workgroups, and reads back `Vec<ZElem>` (re, im f32 pairs, row-major). New `ZSegmentInput` type in `nec_accel` avoids circular dependency with `nec_solver`. New parity test `crates/nec_accel/tests/gpu_zmatrix_parity.rs`: builds a 51-segment dipole, compares GPU vs CPU Z-matrix with max relative error ≤ 1×10⁻⁴; passes vacuously when no GPU adapter is available. Achieved max rel err = 2.12×10⁻⁶ on local hardware.

- **PH5-CHK-004 (CLI `--exec gpu` wired to wgpu RP kernel — gate G4)**: `--exec gpu` now dispatches the RP far-field computation through the real wgpu compute kernel (`run_rp_farfield_batch_wgpu`) instead of the CPU stub. New `run_rp_farfield_batch_wgpu()` in `wgpu_device.rs` reuses the wgpu device, compiled pipeline, and segment/current buffers across all observation points — only the 16-byte uniforms buffer is updated per point via `queue.write_buffer`. When no adapter is available a stderr warning is emitted and the code gracefully falls back to the CPU path. New `pub fn integrate_radiated_power()` exported from `nec_solver` computes the total radiated power normalisation integral needed to convert GPU `U_θ/U_φ` outputs to gain (dBi). `nec-cli` now depends on `nec_accel` with `features = ["wgpu"]` and `pollster` for synchronous dispatch. New integration test `gpu_rp_exec.rs`: two tests — gain parity check (≤0.5 dBi) vs CPU reference, and exec diag field assertion. All `cargo test -p nec-cli` tests pass.

- **PH5-CHK-003 (RP WGSL kernel — milestone gate G3)**: New `crates/nec_accel/src/shaders/rp_farfield.wgsl` — a WGSL compute shader that computes far-field radiation intensity components `(U_θ, U_φ)` for one observation direction by summing over all wire segments (matches the algorithm in `gpu_kernels::far_field_components` exactly). New public async function `run_rp_farfield_wgpu()` in `wgpu_device.rs` dispatches the shader end-to-end: packs f64 segment/current data into f32 GPU buffers, sets up bind group and pipeline from the embedded shader, dispatches one workgroup, and reads back `RpGpuResult { u_theta, u_phi }`. `bytemuck = "1"` added to workspace and `nec_accel` (wgpu-feature-gated) for zero-copy buffer packing. New parity test `wgpu_rp_farfield_parity_vs_cpu_stub` asserts GPU gains match CPU stub within 0.5 dBi across 5 observation directions on a 3-segment dipole; vacuously passes when no adapter is available (headless CI safe). All 15 `nec_accel` tests pass with `--features wgpu`.

- **PH5-CHK-002 (wgpu scaffold — milestone gate G2)**: Added `wgpu = "29"` to `nec_accel` behind `--features wgpu` flag. New `crates/nec_accel/src/wgpu_device.rs`: `enumerate_compute_adapters()` lists all runtime-visible adapters; `run_noop_compute_pipeline()` compiles and dispatches a trivial WGSL no-op shader end-to-end, returning `NoOpPipelineResult::Success` or `NoOpPipelineResult::NoAdapterAvailable` (graceful on headless CI). `pollster` added as dev-dependency for blocking async tests. Two new tests in `nec_accel`: adapter enumeration (no panic) and no-op pipeline (success or graceful skip). Baseline (no-feature) build unchanged.

- **PH5-CHK-001 (GPU architecture decision)**: New `docs/gpu-arch.md` locking the Phase 5 GPU acceleration architecture: wgpu (Rust-native, Vulkan/Metal/DX12/OpenCL) chosen as primary API; WGSL as compute shader language; RP far-field gain computation chosen as first-offload candidate (embarrassingly parallel, existing stub baseline in `nec_accel::gpu_kernels`); real-hardware validation minimum defined (G3 gate on workstation + Pi5 before matrix-fill work); 7-gate milestone sequence G1–G7 defined; CPU fallback contract specified. Resolves GAP-007. Phase 5 checklist PH5-CHK-001…007 added to `docs/roadmap.md`.

- **PH4-CHK-007 (Phase 5 entry criteria)**: New `docs/phase5-entry-criteria.md` defining 5 measurable go/no-go criteria before GPU acceleration work begins: (1) CPU baseline benchmarks locked on 2+ targets, (2) solver tolerance validated on 4+ corpus decks, (3) Phase 4 plugin surface (EP-1…EP-4) declared stable, (4) `cargo deny` policy clean, (5) Phase 4 checklist complete. All 5 criteria are met as of 2026-05-03. References `docs/benchmarks.md` baseline tables and `docs/requirements.md` tolerance matrix. Passes frontmatter CI gate.

- **PH4-CHK-006 (automation guide)**: New `docs/automation-guide.md` documenting all automation surfaces: JSON output consumption, batch sweep patterns, `--vars` template workflows, resonance targeting, optimizer loop patterns (golden-section and scipy), and the `fnec_py` Python binding. New `examples/optimize_swr.py`: a self-contained end-to-end script (stdlib only) that drives `fnec --output-format json` to find the dipole half-length minimising SWR at 14.2 MHz. Runs end-to-end in ~18 solver calls. Fixed pre-existing frontmatter failures in `docs/json-output-schema.md` and `docs/python-bindings.md` (wrong key names — now compliant with CI gate).

- **PH4-CHK-005 (EP-4 DeckValidator)**: Added `DeckValidator` trait, `ValidationDiagnostic` struct, `DiagnosticLevel` enum, and `run_validators()` helper to `nec_model`. Validators receive a read-only `&NecDeck` and return a `Vec<ValidationDiagnostic>`; `run_validators` aggregates results across all validators without short-circuiting. CLI wires in a built-in `NoExCardValidator` (warning-level) on every solve path, emitting `warning: [validator] …` to stderr. Error-level diagnostics produce a non-zero exit code. `docs/plugin-api-design.md` updated: EP-4 section added, pipeline diagram updated, EP-4 removed from the "Planned" table. Tests: 7 unit tests in `crates/nec_model`, 2 doctests (`DeckPostProcessor`, `DeckValidator`), 4 integration tests in `apps/nec-cli/tests/deck_validator.rs`.

- **PH4-CHK-004 (Python bindings)**: New `bindings/fnec_py/` crate (PyO3 0.23, cdylib). Exposes `solve_deck_str(deck: str) -> dict` and `sweep_deck_str(deck: str) -> list[dict]` returning `{freq_mhz, tag, seg, z_re, z_im, z_abs, z_arg_deg}`. Uses Hallen solver internally. Build: `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 maturin develop` from `bindings/fnec_py/`. 8 smoke tests in `bindings/fnec_py/tests/test_smoke.py`. Build instructions in `docs/python-bindings.md`.

- **PH4-CHK-003 (`--output-format json`)**: `fnec` now accepts `--output-format json` on all solve/sweep paths. Output is a JSON array — one record per frequency point — with fields `freq_mhz`, `tag`, `seg`, `z_re`, `z_im`, `z_abs`, `z_arg_deg`. Text output unchanged when flag is omitted. Schema locked in `docs/json-output-schema.md` (schema v1). 5 contract tests in `apps/nec-cli/tests/json_output_contract.rs`.

- **PH4-CHK-002 (EP-3 custom report sections)**: Added `ReportSection` trait and `render_text_report_with_sections()` to `nec_report`. Callers pass a `&[&dyn ReportSection]` slice; each section's `render()` output is appended after the standard report sections. Two doctests (`ImpedanceSummary`, `Banner`) and 4 unit tests (identity, single-section append, multi-section ordering, `PeakImpedanceSection` worked example). `docs/plugin-api-design.md` updated with EP-3 section description, revised pipeline diagram, and updated future-EP table (EP-4/5/6). `cargo test -p nec_report`: 11 unit tests + 3 doctests.

- **PH4-CHK-001 (dependency policy + cargo-deny)**: Authored `docs/dependency-policy.md` resolving BLK-005. Covers the SPDX allowlist (13 identifiers), deny-list (GPL-2.0-only, AGPL, SSPL, BUSL, proprietary), GPLv2 vs. GPLv3 compatibility rules, exception request process, duplicate-version and source policies, and tooling instructions. Added `deny.toml` with `cargo-deny` v2 schema: unconditional allowlist, `self_cell` exception (Apache-2.0 option), advisory deny, duplicate-version warn, sources deny for unknown registries and git deps. `cargo deny check licenses` passes cleanly. BLK-005 marked resolved. Fixed stale SBOM format flag in `docs/steering.md` (`spdx-json` → `spdx_json_2_3`). Added Phase 4 implementation checklist (PH4-CHK-001..007) to `docs/roadmap.md`.

- **PH3-CHK-012 (Phase 3 usability benchmark)**: Authored `docs/usability-benchmark-ph3.md` satisfying all three Phase 3 usability acceptance minima. Benchmark 1 records the 5-point frequency sweep from a blank `fnec-gui` project in exactly **7 explicit actions** with a step-by-step table. Benchmark 2 records an edit-run-inspect workflow comparison against xnec2c: fnec-gui completes in 4 steps (~15 s) vs. xnec2c's 5 steps (~22 s). The document includes the acceptance-minima checklist with all items ticked.

- **PH3-CHK-011 (nec-gui pattern slice + current-distribution views)**: Added two new tabs to `fnec-gui`: Pattern and Currents. The Pattern tab computes an elevation-plane (fixed φ) radiation-pattern slice in 5° θ steps (37 points) using the existing `nec_solver::compute_radiation_pattern` API; the Currents tab shows per-segment current magnitudes as a text bar chart. Implementation: `solve.rs` gains `PatternPoint`, `CurrentPoint`, `pattern_slice_deck_str/path`, `current_distribution_deck_str/path`, and a shared `solve_for_currents()` helper that builds geometry once. `app_state.rs` extended with `ActiveTab::Pattern/Currents`, `PatternPhase`, `CurrentsPhase`, `PatternDisplayRow`, `CurrentDisplayBar` (data-to-plot mapping structs), `can_run_pattern()`, `can_run_currents()`, `pattern_phi()`, `pattern_display_rows()`, `current_display_bars()`, `pattern_status_text()`, `currents_status_text()`. `main.rs` updated with four-tab bar, `pattern_view()`, `currents_view()`, `pattern_table()`, `currents_bars()` helpers. Added 20 new headless tests (6 pattern state machine, 3 currents state machine, 4 data-to-plot mapping, 4 pattern pipeline, 3 current pipeline) for a total of **47 smoke tests**.

- **PH3-CHK-010 (nec-gui sweep views)**: Added frequency-range sweep setup and result inspection views to `fnec-gui`. The GUI gains a Solve/Sweep tab bar switching between the existing single-frequency panel and a new sweep panel. The sweep panel provides Start/End/Step (MHz) text inputs, a Run Sweep button, a progress/status line, and a sortable four-column result table (Freq, Z_re, Z_im, |Z|). Column headers are clickable sort buttons with ascending/descending toggle indicators. Implementation: `app_state.rs` extended with `ActiveTab`, `SweepPhase`, `SweepSortCol`, `SweepSetup` fields, new `Message` variants (`TabSelected`, `SweepStartChanged`, `SweepEndChanged`, `SweepStepChanged`, `RunSweep`, `SweepComplete`, `SweepSortBy`), `can_sweep()`, `sweep_params()`, `sorted_sweep_rows()`, `sweep_status_text()`. `solve.rs` gains `SweepPoint` struct and `sweep_deck_str` / `sweep_deck_path` functions that build geometry once and iterate the impedance-matrix solve over each frequency. `main.rs` updated with tab bar, `sweep_view()`, `sweep_result_table()`, `sweep_row()` helpers. Added 14 new headless tests to `gui_smoke.rs` covering sweep state machine (8 tests) and sweep pipeline (5 tests), for a total of 27 smoke tests.

- **PH3-CHK-009 (nec-gui iced desktop window)**: Implemented the `fnec-gui` desktop frontend using `iced` 0.13. The binary presents a dark-themed window with a deck path text input, a Solve button, and a result panel showing frequency, Z_re, Z_im, and |Z|. The solve pipeline runs asynchronously via `Task::perform`. Implementation split: `apps/nec-gui/src/lib.rs` + `app_state.rs` (state machine — no iced dep, fully headless-testable) + `solve.rs` (Hallen solve wrapper calling `nec_solver` directly). Added 13 headless smoke tests in `apps/nec-gui/tests/gui_smoke.rs` covering state machine transitions (8 tests) and solve pipeline correctness (5 tests). Added `.github/workflows/gui-smoke.yml` CI gate running `cargo test -p nec-gui --test gui_smoke`.
### Added

- **PH3-CHK-008 (resonance-targeting helper)**: Added `fnec sweep --resonance <file.nec.toml>` subcommand that binary-searches one template variable to find the feedpoint reactance closest to a target (typically 0 Ω for series resonance). The `.nec.toml` file embeds both a `[search]` table (variable name, lo/hi bounds, target reactance, tolerance, max iterations) and a `[deck]` table containing the NEC template string. Implementation: `apps/nec-cli/src/resonance_search.rs` (`ResonanceFile` TOML struct, `bisect()` function, `print_result()`). Integrates with the template engine from PH3-CHK-007 and re-runs the full geometry/solve pipeline for each probe point. Added `examples/resonance-search.nec.toml` worked example (14.2 MHz dipole resonance search); added 3 contract tests in `apps/nec-cli/tests/resonance_contract.rs` (convergence, unbounded-range error, missing-flag usage error).

- **PH3-CHK-007 (variable-substitution engine)**: Added `nec_parser::template` module with a `substitute()` function that replaces `$VAR` tokens in NEC deck strings from a `HashMap<String, String>`. `$$` produces a literal `$`; undefined tokens return a `TemplateError` with the variable name and 1-based line number. CLI: `--vars <file>` flag loads a flat TOML or JSON key→value map and applies substitution before parsing. Added `apps/nec-cli/src/vars_config.rs` (TOML via `toml` crate; JSON via minimal hand-rolled parser). Added 5 contract tests in `apps/nec-cli/tests/template_contract.rs`. Corpus example: `corpus/variable-dipole.nec` (template) + `corpus/dipole-vars.toml` (vars). `--vars` documented in `docs/cli-guide.md` synopsis and options table.

- **PH3-CHK-006 (`--sweep-config` CLI flag)**: Added `--sweep-config <file.toml>` flag to the `fnec` binary. A TOML sweep-config file specifies a frequency list as either a linear range (`start_mhz`, `end_mhz`, `step_mhz`) or an explicit point list (`points_mhz = [...]`). When supplied, the sweep-config frequencies replace those derived from the deck's `FR` card; the full solve pipeline runs once per point and emits one structured output block per frequency on stdout. Implementation: `apps/nec-cli/src/sweep_config.rs` (TOML reader + validation); `apps/nec-cli/Cargo.toml` gains `serde` and `toml` workspace deps; `apps/nec-cli/tests/sweep_contract.rs` adds 5 contract tests (single-point explicit, multi-point explicit, range point-count, ordering stability, machine-parseability); `examples/sweep-spec.toml` provides a range-based reference example.

- **PH3-CHK-005 (run history API)**: Extended `nec_project` with `RunHistory` (transparent `Vec<RunRecord>`), `RunRecord` (ISO 8601 timestamp, `SolverConfig` snapshot, `ResultSummary`), and `ResultSummary` (impedance Re/Im, optional peak gain dBi, sweep point count). `ProjectFile` gains `run_count()`, `last_run()`, and `run_by_index()` query methods plus `RunHistory::push`. History is absent from TOML when empty; `peak_gain_dbi` is omitted when `None`. 5 history tests added (13 integration + 1 doctest total).

- **PH3-CHK-004 (nec_project TOML format)**: Implemented `ProjectFile`, `SolverConfig`, and `NamedRun` structs with serde/toml round-trip in `crates/nec_project/src/lib.rs`. Public API: `ProjectFile::from_toml` / `to_toml`; `ProjectError` with version-guard (`UnsupportedVersion`). 8 integration tests + 1 doctest in `crates/nec_project/tests/project_roundtrip.rs`. Project TOML format documented in `docs/project-format.md`.

- **PH3-CHK-003 (plugin API design)**: Added `docs/plugin-api-design.md` covering the extension surface, safety model (no network/filesystem/FFI through the trait interface), pipeline diagram, and future EP-3..5 scope. Implemented two working extension points: `DeckPostProcessor` trait (EP-1) in `crates/nec_model/src/lib.rs` (called after parse, before geometry build) and `ResultFilter` trait (EP-2) in `crates/nec_report/src/lib.rs` (called after solve, before report rendering). Both are exercised by doctests. BLK-004 updated to resolved.

- **PH3-CHK-002 (contributing guide)**: Added `docs/contributing.md` covering build workflow, pre-push sequence (`cargo fmt` → `cargo check` → `cargo test`), branch conventions, PR process, corpus-gate requirements, documentation frontmatter rules, and architecture orientation for new contributors. Added contributor orientation cross-references to `docs/architecture.md` and `docs/design.md`. The `validate-doc-frontmatter` CI gate picks up the new file automatically via its existing `docs/*.md` glob.

- **PH3-CHK-001 (card-status index)**: Added `## PH3-CHK-001 complete card status index` section to `docs/nec4-support.md` with a 25-row flat table listing every known NEC-2/NEC-4 mnemonic, its parser status (`recognized` / `unknown`), and functional status. Documents the GM/GR gap (geometry builder implemented but parser not yet wired). `par001_card_status_table_complete` test in `apps/nec-cli/tests/corpus_validation.rs` enforces all 12 parser-recognized mnemonics and 3 out-of-scope entries are present in CI.

- **Non-collinear multi-wire Hallen support (Phase 2)**: The Hallen solver now handles junctioned and non-collinear multi-wire topologies (e.g. `dipole-loaded` top-hat geometry, inverted-V, Yagi with passive elements) via a segmented hybrid reformulation:
  - `build_hallen_rhs` now computes per-wire local cos(k·s) homogeneous vectors using each wire's own midpoint as s=0, replacing the old global s-axis.
  - Passive (non-driven) wires receive rhs=0; all EX cards contribute to the source map (multi-source support).
  - `detect_wire_junctions()` in `geometry.rs` identifies shared wire endpoints; `solve_hallen` enforces KCL continuity rows for junction segments instead of the default I=0 endpoint condition.
  - `--allow-noncollinear-hallen` flag is now silently accepted (no-op) rather than deferred; non-collinear geometries are supported by default.
  - `dipole-loaded` corpus gate now passes: Z ≈ 12.39 − j918 Ω (external NEC2 reference: 13.46 − j896 Ω).
  - References for TL-coupled multi-dipole cases and Yagi 5-element case updated to reflect correct passive-wire rhs=0 behavior.

### Changed

- Extracted geometry validation helpers (`sinusoidal_a4_topology_supported`, `segment_intersection_error`, `source_risk_geometry_error`, `buried_wire_geometry_error`, and private math/graph helpers) into `apps/nec-cli/src/geometry_validation.rs`, and extracted all warning functions into `apps/nec-cli/src/warnings.rs`. `main.rs` is now reduced to frontend wiring, enums/constants, bench-emit helpers, and `fn main()`.
- Extracted per-frequency solve-session logic from `apps/nec-cli/src/main.rs` into a new `apps/nec-cli/src/solve_session.rs` module: all math helpers (`l2_norm`, `matrix_diagonal_spread`, `residual_zi_minus_v`, `residual_hallen`), pulse-source constraint helpers, report builders (`build_feedpoint_rows`, `build_source_rows`, `build_load_rows`), frequency/dispatch helpers (`frequencies_from_fr`, `build_hybrid_lane_plan`), all four structs (`FrequencySolveResult`, `SweepPointSummary`, `PulseCurrentSourceConstraint`, `HybridLanePlan`), and `solve_frequency_point` now live in `solve_session`. The function gains an explicit `sinusoidal_topology_supported: bool` parameter, computed once in `main()` before the solve closure, replacing the internal call to `sinusoidal_a4_topology_supported` inside the solve path.

- Continued CLI decomposition by extracting execution-profile policy logic (4nec2 drop-in detection/steering and startup auto-probe mode selection) from `apps/nec-cli/src/main.rs` into `apps/nec-cli/src/exec_profile.rs`.
- Started three accepted review follow-ups: parser fuzz scaffolding now exists under `fuzz/`, CLI argument parsing/usage text now lives in `apps/nec-cli/src/cli_args.rs`, and `nec_solver` now carries a first property-based Hallen reciprocity invariant test.
- Review follow-up triage now assigns owners and concrete closure criteria for the remaining GAP items, adds measurable Phase 3 usability minima, documents experimental residual budgets and the scoped GN0/GN2 finite-ground validity envelope, and starts documenting crate-level public surfaces for `nec_report` and `nec_project`.
- Report contract coverage now locks combined sweep-plus-operator-table ordering on stdout: multi-frequency runs with `LD` cards must emit one full per-frequency block in `FEEDPOINTS -> SOURCES -> LOADS -> CURRENTS` order before the final `SWEEP_POINTS` summary.
- Added a supported low above-ground GN2 near-ground corpus contract (`dipole-gn2-near-ground-51seg`) and tightened PH2-CHK-002 docs/tests so supported near-ground coverage is distinguished from buried active-ground fail-fast guardrails.
- Geometry diagnostics now also fail fast for source-risk tiny segments: `EX` requests on `L/r < 2` emit an actionable deferred-class error before solve.
- GN type 0 is now active as a simple finite-ground model in Hallen impedance assembly (complex Fresnel-style image scaling from EPSE/SIG) instead of the prior deferred free-space fallback warning path.
- Phase 2 current/phase corpus coverage now includes both `dipole-freesp-51seg` and `dipole-ground-51seg`, so CI locks representative free-space and PEC-ground current magnitude/phase samples instead of only the base dipole case.
- EX type 1 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- EX type 2 is now accepted as a staged portability fallback: the CLI warns that incident-plane-wave semantics are still pending, and current runtime behavior treats EX type 2 like EX type 0 until a dedicated implementation lands.
- EX type 4 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- EX type 5 now has a first real implementation slice for `--solver pulse`: the pulse solver enforces the requested driven-segment current and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still keep the staged portability fallback warning.
- TL `NSEG>1` cards for lossless lines (`type=0`) are now accepted in the executable network subset using the same uniform-line stamp semantics as `NSEG=1`; the previous deferred "TL with NSEG=... not yet supported" runtime warning path is removed.
- Phase 2 traceability coverage is now stricter: the enforced PH2-CHK-007 matrix explicitly maps newer EX current-source, LD load-family, TL subset, and PT/NT deferred-portability corpus classes, and CI now requires those row IDs to remain present.
- PT cards are now parsed for staged portability and emit an explicit deferred-support warning at runtime; PT electrical semantics are still pending and currently ignored.
- NT cards are now parsed for staged portability and emit an explicit deferred-support warning at runtime; NT electrical semantics are still pending and currently ignored.
- CLI report contract v1 now includes stable operator tables for source/load definitions: `SOURCES` (`TYPE TAG SEG I4 V_RE V_IM`) and `LOADS` (`TYPE TAG SEG_FIRST SEG_LAST F1 F2 F3`) sections, emitted in deterministic order between `FEEDPOINTS` and `CURRENTS`.
- Scriptability contracts now explicitly lock stdout ordering around the new tables (`FEEDPOINTS -> SOURCES -> CURRENTS`) and enforce that `LOADS` table output stays report-only on stdout while warnings remain stderr-only.
- Loaded-case tracking now also locks the default Hallen hard-fail contract on `dipole-loaded` (non-collinear topology error, exit code 1, and no report on stdout) to keep Phase 1 gap behavior explicit and deterministic.

### Added

- RP card execution is now wired into the CLI report path.
- Text reports now include a `RADIATION_PATTERN` section when one or more `RP` cards are present.
- Added corpus regression deck `corpus/dipole-freesp-rp-51seg.nec` and contract coverage for pattern-table rendering.
- Added `docs/benchmarks.md` with a validated three-host baseline comparison (local workstation, T480, Raspberry Pi 5).
- Added a collaboration efficiency guide with rate-limit-aware prompting patterns at `docs/copilot-efficiency-guide.md`.
- Added `docs/par011-dropin-evidence-memo.md` as a dedicated evidence scaffold for deferred 4nec2 drop-in compatibility work.
- **GPU kernel stubs** (Phase A expansion): Extended `nec_accel::gpu_kernels` module with additional kernel scaffolds:
  - `HallenRhsGpuKernel` for Hallén RHS vector computation with excitation handling
  - `PocklingtonMatrixGpuKernel` for matrix assembly with segment-pair element distribution
  - `KernelTiming` struct for capturing prep/exec/retrieval timing data (microsecond resolution)
  - 4 new unit tests for kernel construction and sizing (12 total nec_accel lib tests)
  - GPU-compatible data structures prepared for future CUDA/OpenCL replacement
- **CLI GPU FR integration** (Phase B): Added `--gpu-fr` command-line flag to dispatch radiation pattern computation to GPU kernel stub:
  - Far-field points routed through `HallenFrGpuKernel` when flag is enabled
  - Maintains full output parity with CPU far-field path
  - Integration tested with 6 GPU stub tests + existing exec_modes contract tests
- **Performance benchmarking** (Phase D): Added optional timing instrumentation for GPU kernel operations:
  - `--bench` CLI flag to enable benchmarking mode
  - `--bench-format <human|csv|json>` to emit machine-readable benchmark records while preserving the standard human-readable report output
  - `FNEC_GPU_BENCH` environment variable control (set to "1" to enable timing collection)
  - `compute_hallen_fr_point_with_timing()` API returns `(result, KernelTiming)` tuples
  - Timing breakdown: prep (coordinate transform), exec (far-field summation), retrieval (stub: zero)
  - Ready for future GPU timing collection once real CUDA/OpenCL kernels are wired
- Corpus validation framework already supports pattern and current-gate scenarios (Phase C); enhancements documented for future use.

### Changed

- Added missing `GE` cards to three corpus decks (`dipole-ld-series-rc-51seg`, `dipole-ld-series-rl-51seg`, `tl-two-dipoles-linked-seg0`) so `corpus_deck_sanity` passes consistently in local hooks and CI.
- Native CLI startup now auto-selects execution mode when `--exec` is omitted by running a quick execution probe (CPU threads, frequency-point count, and accelerator dispatch availability) and choosing among `cpu`/`hybrid`/`gpu` heuristically for the current workload shape.
- Consolidated benchmark documentation into a single canonical file (`docs/benchmarks.md`) and removed the duplicate `docs/benchmark.md` shim.
- Benchmark docs now explicitly map reported numbers to four execution modes: CPU single-thread, CPU multithread, GPU, and hybrid (CPU multithread + GPU), with a dedicated local four-mode coverage result block.
- Sinusoidal topology gating advanced through A4: the solver now accepts collinear wire-chain geometries (including multi-wire chains) with orientation/order-agnostic endpoint connectivity checks, and still falls back for disconnected/branched/unsupported topologies.
- Added a gitignored benchmark host env pattern (`.benchmark-hosts.env` with tracked `.benchmark-hosts.env.example`) and updated `scripts/pi-remote-benchmark.sh` to accept env defaults (`FNEC_BENCH_TARGET`, `FNEC_REMOTE_REPO_SUBDIR`).
- Remote benchmark tooling now supports execution-mode sweeps (`FNEC_BENCH_EXECS`) and records `diag_spread` plus `sin_rel_res` in benchmark CSV output and comparison reports.
- Added `scripts/pi-benchmark-summary.sh` to summarize a single benchmark CSV without pandas or ad hoc shell commands.
- Added `sin_rel_res` to CLI diagnostics: the sinusoidal basis relative residual captured before any fallback decision, enabling solver-quality trending across runs (0.0 for non-sinusoidal modes).
- Added `diag_spread` to CLI diagnostics as a conditioning proxy (ratio of max/min diagonal magnitudes of the solved system matrix), enabling quick stability checks in automation.
- Added sinusoidal A2 regression checks that compare sinusoidal-mode impedance output against Hallen on `dipole-freesp-51seg` and `frequency-sweep-dipole` corpus decks.
- Sinusoidal solver routing is now topology-gated for A1: it runs only on single-wire collinear decks and otherwise falls back explicitly to pulse with `sinusoidal->pulse(topology)` diagnostics.
- Completed PAR-008 coverage-matrix scope: NEC-5 validation scenario classes are now explicitly mapped to current corpus-backed in-scope equivalents, with out-of-scope classes and rationale documented for phased deferral.
- Updated support and CLI docs to mark RP pattern output as implemented in the text-report path (with remaining export/near-field scope still deferred).
- Corpus validation now numerically checks stored RP pattern samples instead of only asserting pattern-table presence.
- Corpus validation now also checks the stored vertical/horizontal gain columns and axial ratio for locked RP sample angles.
- RP corpus angle coverage was expanded from 2 locked sample angles to 7 locked angles across the theta sweep.
- Added a second RP corpus case with non-z-axis geometry and multi-phi sample locking to validate true azimuth-cut coverage.
- Corpus validation now also records external-reference deltas for RP pattern samples when `external_reference_candidate.pattern_samples` is present.
- Added `nec2c` external RP sample candidates for the multi-phi x-axis corpus case so parity tracking now covers both current RP decks.
- RP corpus cases can now opt into external-pattern CI gates via `ExternalGain_absolute_dB` and `ExternalAxialRatio_absolute` in `tolerance_gates`.
- Corpus validation now also supports optional external impedance CI gates (`ExternalR_*`/`ExternalX_*`) for scalar, multi-source, and frequency-sweep candidates.
- Enabled the first external impedance CI-gated case (`frequency-sweep-dipole`) with absolute candidate thresholds (`ExternalR_absolute_ohm=15.0`, `ExternalX_absolute_ohm=50.0`).
- Enabled a second external impedance CI-gated case (`dipole-ground-51seg`) with absolute candidate thresholds (`ExternalR_absolute_ohm=10.0`, `ExternalX_absolute_ohm=30.0`).
- Roadmap now defines a required benchmark-mode matrix across all target classes: CPU single-threaded, CPU multithreaded, and GPU offload.
- CLI now accepts `--exec <cpu|hybrid|gpu>` for real runs; `hybrid`/`gpu` are scaffolded execution modes that currently fall back to CPU with explicit diagnostics.
- `--exec hybrid` now performs coarse-grain multithreaded FR sweep solving (parallel per-frequency solve with ordered report output); GPU execution remains scaffolded.
- `--exec hybrid` now uses split-lane FR scheduling (CPU-parallel lane + GPU-candidate lane) with deterministic ordered report output; GPU-candidate lane points currently emit explicit fallback warnings and execute on CPU until GPU kernels are wired.
- Hybrid and GPU-mode fallback routing now flows through a concrete `nec_accel` dispatch API (`dispatch_frequency_point`) so future GPU kernel wiring has a stable integration seam.
- Added an opt-in accelerator stub dispatch path (`FNEC_ACCEL_STUB_GPU=1`) so `DispatchDecision::RunOnGpu` can be exercised end-to-end in CLI hybrid and gpu execution flows without changing output contracts.
- Added a tracked parity item for filename-steered 4nec2 solver-binary drop-in compatibility mode, including contract-preservation and throughput validation goals.
- Retargeted 4nec2 external-kernel drop-in compatibility work to a farther-future window (Phase 4-5) after assessing real NEC2MP replacement artifacts and integration scope.
- Expanded PAR-011 with an implementation discovery checklist (binary-name matrix, install/invocation contract, file side effects, dependency surface, fixtures, and benchmark protocol) to reduce future re-research cost.
- Added GNU NEC (`https://sourceforge.net/projects/gnu-nec/`) as an additional open-source reference candidate in architecture and PAR-011 source notes.
- Refined filename-steered 4nec2 compatibility warnings to explicitly report whether execution was auto-steered or an explicit `--exec` value was preserved.
- Extended drop-in compatibility contract tests to cover both `nec2dxs*` and `4nec2*` alias-name detection paths.
- Populated `docs/par011-dropin-evidence-memo.md` with concrete NEC2MP artifact evidence (inventory, readme findings, SHA256 fingerprints) and a phased docs-only PAR-011 implementation plan with `AT-PAR011-*` acceptance tests.
- Explicitly postponed PAR-011 compatibility harness-skeleton work in current scope (option 3 deferred).
- Explicitly postponed PAR-011 compatibility harness-skeleton work in current scope (option 3 deferred).
- **PH2-CHK-003 — LD/TL/NT implemented semantics (2026-05-10)**: LD cards (types 0–5) and TL lossless-line cards (`type=0`) are now parsed in `nec_parser` and applied as impedance stamps in the solver; NT cards are parsed for staged portability and emit a deferred-support warning instead of an unknown-card warning. 5 `ld_loads.rs` and 3 `tl_cards.rs` integration tests updated to Phase-2 assertions; 14 corpus reference entries in `reference-results.json` updated (3 LD loaded-value cases, 4 TL coupled-dipole cases, 7 NT deferred-warning cases); `parser_warnings.rs`, `report_contract.rs`, and `scriptability_contract.rs` tests updated to Phase-2 contracts.
- **PH2-CHK-007 — NEC-5 validation matrix ticked done (2026-04-30)**: The PH2-CHK-007 traceability matrix in `docs/corpus-validation-strategy.md` (row IDs `PH2N5-001` … `PH2N5-010`) carries explicit `in-scope implemented` / `in-scope deferred` / `out-of-scope` statuses with corpus case mappings, and `phase2_nec5_matrix_rows_are_traceable_to_corpus_cases` in `apps/nec-cli/tests/corpus_validation.rs` enforces row-ID presence, status validity, and corpus-case existence in CI. The PH2-CHK-007 done signal is therefore already met by prior PH2-CHK-005 work; this entry records the roadmap tick.
- **PH2-CHK-002 — Buried/near-ground guardrails ticked done (2026-04-30)**: `buried_wire_geometry_error` in `apps/nec-cli/src/geometry_validation.rs` fails fast with an actionable diagnostic when active-ground decks include `z<0` segments; `buried_wire_with_active_ground_fails_fast_with_actionable_error` and `near_ground_wire_with_active_ground_runs_without_deferred_warning` regression tests lock both branches in `apps/nec-cli/tests/ground_diagnostics.rs`; supported `dipole-gn2-near-ground-51seg` and unsupported `dipole-gn2-buried-unsupported` corpus fixtures are gated by warning / forbidden-warning / `expected_hallen_error_contains` contracts; `par002_ground_checklist_cases_are_present_and_contracted` enforces the matrix. The PH2-CHK-002 done signal is therefore already met by prior PH2-CHK-001 work; this entry records the roadmap tick.
- **PH2-CHK-004 — Report/table parity ticked done (2026-04-30)**: All 6 table sections implemented and CI-locked — `FEEDPOINTS`, `SOURCES`, `LOADS`, `CURRENTS`, `RADIATION_PATTERN`, `SWEEP_POINTS`; 5 report-contract tests in `apps/nec-cli/tests/report_contract.rs` lock headers, row parsing, section presence, and per-frequency block ordering (`FEEDPOINTS → SOURCES → LOADS → CURRENTS → SWEEP_POINTS`); 7 scriptability-contract tests in `apps/nec-cli/tests/scriptability_contract.rs` enforce machine-parseable stdout and stderr-only warnings. The PH2-CHK-004 done signal is already met by prior PH2-CHK-003 + 0.3.0 report work; this entry records the roadmap tick.
- **PH2-CHK-008 — Scriptability preservation ticked done (2026-04-30)**: 7 scriptability-contract tests lock stdout-only report stream, stderr-only warnings/bench records, `LOADS`-on-stdout (Phase-2), and exit-code contracts (code 1 on file-read error, code 2 on bad args); 11 core-flags-contract tests lock `--solver`, `--pulse-rhs`, `--exec`, `--bench-format` error/usage contracts and combined-flag success run. All 18 tests pass with zero regression after Phase-2 table and diagnostic additions; this entry records the roadmap tick.

## 0.2.0 — 2026-05-01

### Added

- **GM/GR card support**: GM (Geometry Move) and GR (Geometry Repeat) cards are now parsed and
  applied during geometry expansion. GM rotates/translates wire ranges (in-place or as copies with
  incremented tags); GR repeats all existing wires by successive z-axis rotations.
- **Segment current distribution table**: CLI output now includes a `CURRENTS` section listing
  TAG, SEG, I_RE, I_IM, I_MAG, I_PHASE (deg) for every segment after the feedpoint table.
- **Multi-wire Hallen fix**: per-wire homogeneous constants and endpoint constraints; passive wires
  now correctly receive zero RHS. Yagi and multi-source corpus validation now produces correct
  impedances (Yagi: 30.6+j5.0 Ω, multi-source: 152.4+j31.6 Ω each port).

### Changed

- GE I1=-1 warning now says "requests below-ground wire handling (no image method);
  treating as free-space" instead of a generic "not yet supported" message.
- GE I1=other unknown values now include the valid range hint
  `(valid values: 0=free-space, 1=PEC image, -1=below-ground)`.
- Updated corpus reference values for yagi-5elm-51seg and multi-source decks.

## 2026-04-24

### Added

- Added Phase 1 `GN` card support for perfect-ground (`GN 1`) Hallen runs.
- Added PEC image-method contribution path in Hallen matrix assembly.
- Added parser and solver tests that cover GN parsing and ground-aware matrix behavior.

### Changed

- Updated corpus ground regression reference (`dipole-ground-51seg`) to GN-aware Hallen values.
- Updated support boundary documentation to reflect current GN status (`GN 1` supported; Sommerfeld/Norton deferred).

## 2026-04-22

### Added

- Standard frontmatter requirements for all docs under `docs/`.
- Requirements, steering, roadmap, architecture, design, backlog, SBOM, and memory structure.
- CI automation design for docs stamping and validation.

### Changed

- Documented recent MoM kernel investigations and convergence behavior in new solver notes.
- Added an applied-math reference document with key EFIE/Pocklington/Hallen formulas.
- Added an implementation plan for continuity-enforcing rooftop/sinusoidal basis work.
- Added prominent README support/sponsoring note.
- Added project-local temporary work folder ignore guidance.
- Added regression tests for Hallén RHS symmetry/shape and Hallén/continuity solver behavior.
- Added CLI solver mode selection (`--solver hallen|pulse|continuity`) and single-chain continuity routing.
- Added documented mode benchmark deltas across segment counts in solver findings.
- Added explicit Hallen vs Pocklington matrix routing by solver mode and post-change benchmark notes.
- Added NEC2 reference-inspired pulse RHS wavelength normalization path:
  $$\\frac{1}{dl\\,\\lambda}$$
  and validation notes.
