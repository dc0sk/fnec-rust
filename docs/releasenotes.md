---
project: fnec-rust
doc: docs/releasenotes.md
status: living
last_updated: 2026-07-10
---

# Release Notes

## 0.11.0 — MPIE second solver + Sommerfeld surface-wave ground

This release ships the two largest remaining Phase-9 efforts. Both are **opt-in**;
the default Hallén solver and scalar-Γ ground are unchanged, so nothing in the
validated corpus moves.

### `--solver mpie` — a second solver (PH9-CHK-007)

fnec's default Hallén solver is fast and accurate for the mainstream case, but its
formulation folds the scalar potential into a per-wire homogeneous `cos(k·s)` term.
Three important geometry classes live *in* that scalar potential, so Hallén cannot
represent them and instead guards or mis-solves them: degree-3 (T/Y) junctions,
closed loops, and the near-ground surface wave.

`--solver mpie` is a subsectional **mixed-potential EFIE** with a piecewise-linear
(triangle) current basis that carries the vector and scalar potentials separately.
It retires all three frontiers:

- **Degree-3 junctions.** A degree-N junction node carries N−1 arm-pair basis
  functions, so Kirchhoff's current law holds by construction — no explicit
  constraint row. A symmetric Y-junction (3 × 5 m arms at 120°) converges
  *monotonically* to nec2c's 71.5 Ω under mesh refinement (68.75 / 69.33 / 69.84 Ω
  at 10 / 20 / 40 segments per arm). The earlier entire-domain Hallén prototype
  *diverged* on this case (radiation resistance climbed past 80 Ω).
- **Closed loops.** A loop is a cyclic all-degree-2 chain with no free end; the same
  basis handles it with no endpoint condition. A 1 λ square loop converges to
  nec2c's 109.7 − j146.2 Ω. (The Hallén periodic closure never validated: it gave
  ≈20 − j1210 Ω.)
- **Near-ground currents and patterns.** The Sommerfeld reflected potential kernels
  (horizontal wires) and a reflected-E-field-dyadic reaction (any straight or bent
  orientation) are added to the impedance matrix, so the surface wave enters the
  *current solution* — not just the feedpoint Z. A horizontal λ/2 dipole over
  average ground (GN2) matches nec2c to <8 %, a vertical dipole to ~7 %, and an
  apex-fed inverted-V captures the surface-wave reactance shift.

Because the MPIE keeps the scalar potential explicit, its absolute reactance tracks
nec2c without the Hallén ~32 Ω offset — a λ/2 dipole reports 74 + j42 Ω versus
Hallén's 74 + j5 Ω (nec2c 78.85 + j44.70). Free-space radiation patterns and gain
reuse the existing radiation sum (λ/2 dipole 2.15 dBi, planar Y-junction 1.94 dBi,
both matching nec2c).

**Scope.** The MPIE models geometry driven by voltage sources (`EX` type 0). Loads
(`LD`), transmission lines (`TL`), networks (`NT`), incident plane waves, and
current sources are rejected on this path — use the Hallén solver for those. Over
ground it handles any wire (straight or bent, any orientation) *above* the `z = 0`
plane; a wire that crosses the ground plane (buried geometry) is out of scope.

**Usage.**

```
fnec --solver mpie deck.nec
```

The feed is a delta-gap at the graph node nearest the `EX`-driven segment (a
half-segment offset from NEC's segment-gap feed that vanishes under refinement).
See `docs/cli-guide.md` and `docs/mpie-solver-scope.md`.

### `--ground-solver sommerfeld` — surface-wave near-ground impedance (PH9-CHK-006)

On the Hallén path, finite ground uses a normal-incidence scalar reflection
coefficient (RCM). That is accurate for antenna heights ≥ ~0.2 λ but misses the
surface wave below that — at 0.025 λ the scalar model even gets the *sign* of the
resistance shift wrong. `--ground-solver sommerfeld` replaces the scalar image with
the exact Sommerfeld half-space correction for a straight wire's feedpoint
impedance (nec2c GN2), including the low-height sign flip. The default (`rcm`) is
unchanged.

```
fnec --ground-solver sommerfeld deck.nec
```

For near-ground *currents and patterns* (not just feedpoint Z), use `--solver mpie`.
See `docs/ph9-chk-006-sommerfeld-ground.md`.

### No migration needed

Both features are additive and opt-in. Existing decks and flags behave exactly as
in 0.10.0.

## 0.10.0 — Phase 9: general junction basis, junction receive/current-source, near-ground impedance

This release carries the second, larger wave of Phase 9 (the accuracy & scattering
frontier). The headline is the **general degree-2 junction basis** — bent, split, and
connected antennas now solve correctly across all three excitation classes — together
with a **foundational fix to near-ground impedance** and honest guards for the
geometries that are still out of scope. Phase 9 is still not complete (degree-3+ T/Y
junctions, closed-loop solving, and the Sommerfeld surface wave remain), but every
increment here is validated against nec2c or by reciprocity/consistency.

### General junction basis (PH9-CHK-002)

The Hallén homogeneous solution (`cos(k·s)` + constant) was previously built per `GW`
wire and reset at each junction, so any bent or split geometry mis-solved (often to a
negative resistance). It is now solved on continuous **conductor paths** with a
per-segment traversal sign and signed arc-length, across **all three excitation
classes**:

- **Transmit (voltage delta-gap).** Bends, start-to-start / end-to-end splits, and
  inverted-V apex feeds now solve. A λ/2 dipole split at its feed gives
  **74.41 + j14.52 Ω** (was −34 − j1447); 30°/45°/90° inverted-Vs match nec2c
  radiation resistance to 2–4 %.
- **Plane-wave receive.** A receiving bent/connected antenna solves on continuous
  paths (two homogeneous DOF for the asymmetric induced current) and emits a
  `RECEIVE_PATTERN` where it previously failed fast. Validated by reciprocity: the
  CLI split-dipole receive sweep matches its transmit pattern to 0.025 dB.
- **Current source (EX type 4).** The forced-current solve on junctioned geometry
  reports a feedpoint `Z = V/i0` that matches the voltage-source impedance to
  ~2–3×10⁻⁴ (split dipole and inverted-V).

Out-of-scope topologies — **closed loops** and **degree-3+ (T/Y) junctions** — are now
**guarded**: a whole-geometry warning fires regardless of feed placement, so a loop
fed mid-wire no longer silently returns a wrong impedance (a 1λ loop reported
≈20 − j1210 Ω vs the true ≈111 − j146). A closed-loop solve was prototyped against
nec2c but its periodic closure did not validate, so it stays deferred rather than
shipped unvalidated.

### Near-ground impedance (PH9-CHK-006)

- **Ground-image sign fix.** The method-of-images reflection term in the Hallén Z
  matrix used the image current `(Jx, Jy, −Jz)` instead of the correct PEC image
  `(−Jx, −Jy, +Jz)` — the exact negation — so *every* near-ground feedpoint impedance
  had the wrong-signed ground effect (a horizontal dipole 0.1 λ over ground reported
  92 − j48 Ω where nec2c gives ≈52 + j63). The separately-correct far-field image
  hid it. Validated against nec2c via the ground-induced ΔZ across four geometries.
- **Accuracy boundary + guard.** fnec's finite-ground impedance is now accurate
  (≈ Sommerfeld, ~10 %) for antenna heights ≥ ~0.2 λ (gated vs nec2c GN2) and degrades
  below; a low-height warning fires under 0.1 λ that the impedance is a
  reflection-coefficient approximation without the Sommerfeld surface wave.

### Near fields and output control (PH9-CHK-004)

- **Near electric and magnetic fields.** `NE` and `NH` cards compute the near E/H
  field on a rectangular grid (Hertzian-element sum over the solved currents),
  emitting `NEAR_FIELD` / `NEAR_H_FIELD` sections; validated against the far field at
  range (0.02 %) and the `|E| = η·|H|` relation.
- **`PT` print-control** is applied at runtime (suppress / all / tag-and-segment
  restriction).

### Tooling

- **Benchmark Dashboard CI** was fixed (it had never passed): invalid heredoc YAML is
  corrected, the noisy real-run timing comparison is now informational, and the
  gh-pages deploy has explicit write permission.

### Known limitations (deferred to a later release)

- **Degree-3+ (T/Y) junction solving** and **closed-loop solving** — guarded, not
  solved; both need a genuinely different basis (branching KCL / periodic closure).
- **Sommerfeld/Norton surface-wave ground** — the exact near-ground correction for
  antennas below ~0.1 λ; the reflection-coefficient model is used and guarded there.
- fnec's Hallén operator carries a documented ~32 Ω systematic reactance offset vs
  nec2c; validate impedance by shape / delta / reciprocity, not absolute parity.

## 0.9.0 — Phase 9 progress: receive patterns, ground gain, junction robustness

This release consolidates the first wave of Phase 9 (the accuracy & scattering
frontier). Phase 9 is **not complete** — the general junction-basis reformulation
and Sommerfeld ground remain — but these increments are validated and worth
shipping.

### Receiving antennas and scattering

- **Incident-plane-wave receive pattern.** A plane-wave `EX` card with an
  incidence-angle grid (NTHETA×NPHI, Δθ/Δφ) now produces a `RECEIVE_PATTERN`
  section — the antenna's response versus the wave's arrival direction. The
  per-angle response is the peak induced current, which was shown to match the
  transmit gain pattern by reciprocity to < 0.01 dB.

### Ground

- **Absolute gain over finite ground.** The radiation pattern over a lossy ground
  is now reported as **gain** (not directivity): it is scaled by the radiation
  efficiency `η = P_radiated / P_input`, so the reported dBi matches nec2c's
  absolute gain (0.06 dB on a horizontal dipole over average ground). This closes
  the ~1.3 dB directivity-vs-gain offset noted in 0.8.0.

### Junction robustness

- **Collinear junction fix.** A straight conductor split across several `GW` cards
  is now solved as one wire. Root cause: fnec's Hallén homogeneous solution
  (`cos(k·s)` + constant) was built per `GW` wire and reset at each junction. A
  λ/2 dipole split at its feed now solves **74.41 + j14.52 Ω** (was −34 − j1447 —
  a negative resistance). The fix is a strict no-op for single wires, parallel
  arrays, bends, and stepped-radius junctions.
- **Junction guardrails.** Two complementary checks make the *remaining* junction
  limitations visible instead of silently wrong: a pre-solve warning when a
  feedpoint sits on a wire junction, and a post-solve warning when the Hallén
  feedpoint resistance is negative (physically impossible for a passive antenna).
  A result without a warning can be trusted to be physical.
- **Diagnosis.** The junction failure mode is documented with a verified
  root-cause analysis and a scoped fix plan (`docs/ph9-chk-002-junction-feed-diagnosis.md`).

### Fixed

- **`RP` card `XNDA` field.** The radiation-pattern parser now accepts the
  canonical 8-field NEC `RP` card (with the `XNDA`/I4 output-options field), not
  only fnec's legacy 7-field form. Real 4nec2 pattern decks previously mis-parsed
  θ0 and produced an all-null pattern.

## 0.8.0 — Phase 8 complete: mainstream deck portability

This release closes the remaining source, network, transmission-line, and
ground-pattern gaps that forced users to hand-simplify mainstream NEC-2 / 4nec2
decks. Every card below is user-runnable and validated; where fnec's Hallén model
diverges from NEC the trade-off is documented.

### Excitation sources (EX)

- **NEC2 EX-type alignment.** fnec's EX-type numbering now matches NEC2: type 0
  voltage source, types 1/2/3 incident plane waves (linear / right- / left-elliptic),
  type 4 current source, type 5 voltage source. Real 4nec2 decks are no longer
  misread.
- **Incident plane wave (EX 1/2/3)** — a receiving-antenna solve on `--solver hallen`:
  induced `CURRENTS`, no feedpoint. Linear and elliptic polarization (axial ratio
  from EX F6); one or more straight, non-junctioned wires (parallel arrays).
  Validated against `nec2c` induced-current shape and by Rayleigh–Carson
  reciprocity against the transmit far-field.
- **Current source (EX 4)** — forces a specified current and reports the feedpoint
  `Z = V/I`; validated by impedance-consistency with the voltage source (2×10⁻⁴).
  Also supports non-junctioned multi-wire arrays.
- **EX type 5** — solved as a voltage source (applied-field model), so type-5 decks
  run. NEC's separate current-slope numerics (~6 %) are a documented non-goal.

### Networks and transmission lines

- **NT two-port networks** — the network's admittance parameters are converted to
  impedance parameters (`[Z] = [Y]⁻¹`) and stamped into the matrix like a TL. A
  well-formed NT reproduces the equivalent TL feedpoint impedance end to end.
- **Lossy transmission line** (`tl_type ≠ 0`) — stamps `Z0·coth(γℓ)` / `Z0·csch(γℓ)`
  with complex `γℓ = αℓ + jβℓ` (`F3` = matched-line loss in dB). Reduces exactly to
  the lossless line at 0 dB.

### Ground

- **Radiation pattern over finite ground** — the far field over imperfect ground now
  uses the Fresnel reflection-coefficient model (was free-space). Antennas over real
  earth show the correct ground lobe and horizon null; the pattern shape matches
  `nec2c` to 0.05 dB. fnec reports directivity (a documented ~1.3 dB offset from
  `nec2c` gain reflects ground-loss efficiency).

### Project

- **Traceability layer** (`docs/project/`) — a consolidated requirement → design →
  implementation → tests → results matrix, kept current before every push.

### Deferred (documented frontiers)

Junctioned-multi-wire plane wave, NTHETA/NPHI angle sweeps, buried-wire / Sommerfeld
ground, non-reciprocal NT, and the `RP`-card `XNDA` parser field — each recorded with
its specific blocker.

## 0.7.0 — Phase 7 complete: GPU productionization

This release turns the GPU path from a working-but-host-bound scaffold into a
production accelerator, and makes the GPU surface honest end-to-end.

### GPU-resident solve

- **`--exec gpu` now solves on the device.** For Hallén decks in the supported
  class (free-space ground, no `LD`/`TL` cards), the impedance matrix is filled
  **and** the regularized normal-equations system is solved entirely on the GPU —
  Jacobi equilibration + complex LU with partial pivoting + Björck least-squares
  refinement — and only the solution vector returns. The N×N matrix never leaves
  the device. f32 precision; matches the f64 CPU solve to ~0.01 Ω on the
  reference dipole. The f64 CPU solve (`--exec cpu`) remains the accuracy
  reference for tolerance-gated work.

### Distributed GPU execution

- **`fnec --exec gpu --hosts hosts.toml`** asks each worker to solve on its GPU.
  GPU-capable nodes use their GPU; CPU-only nodes (or out-of-class decks) fall
  back transparently, so a heterogeneous pool returns correct impedance on every
  node. New `exec` request / `exec_used` report fields are serde-default, so
  pre-0.7 peers interoperate.

### Benchmarking and evidence

- **In-process GPU microbenchmark** isolates per-kernel dispatch time from the
  one-time wgpu device-init (which the across-process gate cannot separate).
- **Real discrete-GPU crossover** measured on AMD (RADV RENOIR, Vulkan): once the
  device is initialized, the GPU Z-fill beats CPU below 32 segments and scales to
  ~240× by 1,536 segments; RP wall-clock is 1.5–1.8× faster. See `docs/benchmarks.md`.

### Honesty / cleanup

- **Retired the GPU CPU-emulation scaffold.** No code path reports CPU compute as
  GPU time anymore. Removed the `FNEC_ACCEL_STUB_GPU` env hack, the
  `ExecutionPath::GpuStubEmulation` path, and dead stub structs.
- **Removed the `--gpu-fr` flag** (it ran a CPU computation labelled as GPU);
  superseded by `--exec gpu`.

### Deferred

- **Native ROCm/SYCL** backend is deferred with a dated, verified rationale (the
  AMD target's Renoir APU is outside ROCm's support matrix; the wgpu Vulkan path
  already covers AMD). See `docs/multi-vendor-gpu.md`.

## 0.6.0 — Phase 6 complete: distributed execution, multi-vendor GPU, sinusoidal EFIE

### Distributed worker deployment

- **`fnec worker --stdio`**: new worker node mode — spawns a JSON-lines solve loop on stdin/stdout for SSH-pipe transport. Run one worker per node; the controller dispatches frequency-point tasks and collects results.
- **`nec_worker` crate**: `TaskMessage`/`TaskResult` protocol, `HostsConfig` TOML node list, per-node `CapabilityCache` (CPU threads, GPU availability, wgpu backend), `LocalWorkerHandle` subprocess controller.
- **SHA-256 result cache**: `ResultCache` keyed on `hash(deck + solver_config + freq_hz)`; FIFO-bounded capacity; cache hit skips the remote solve. A 5-point sweep with one changed deck reuses 4 cached results and re-solves only the changed point.
- **Deployment guide**: `docs/worker-deployment.md` — SSH key setup, `hosts.toml` field reference, wire protocol examples, troubleshooting.

### Solver and accuracy

- **Sinusoidal-basis EFIE**: piecewise-sinusoidal matrix assembly now fully implemented in `nec_solver`. The EXPERIMENTAL warning is retired; all corpus dipole decks pass the impedance tolerance gate in sinusoidal mode.

### Multi-vendor GPU

- **`docs/multi-vendor-gpu.md`**: Vulkan/Metal/DX12/OpenCL backend matrix; AMD Vulkan validation result; Intel ANV, Nvidia MX150, and Pi 5 V3DV coverage; ROCm/SYCL deferred path rationale.

### CI and observability

- **Benchmark dashboard**: GitHub Actions workflow runs the CPU/GPU/multithreaded matrix on every push to `main`, publishes JSON artifacts to Actions summary, and fails on configurable regression deltas.

### Architecture decisions

- **NEC-5 frontier**: `docs/nec5-frontier.md` documents the explicit wire-only continuation decision with ≥3 new difficult-geometry corpus cases mapped to `PH6N5-*` validation rows.
- **Distributed execution design**: `docs/distributed-execution-design.md` — SSH stdio transport, ed25519 authN, worker contract, frequency-point work-split, and result-cache design.

## 0.5.0 — Phase 2 + Phase 5 complete

### GPU acceleration (Phase 5)

- **`--exec gpu`**: full Hallén solve path — GPU Z-matrix fill (WGSL compute shader) + CPU LU solve. Free-space and deferred-ground decks with N ≥ 128 segments use the GPU path; smaller problems and ground-augmented models retain the CPU path. Falls back gracefully to CPU when no wgpu adapter is available.
- **RP far-field GPU kernel**: `--exec gpu` dispatches the radiation-pattern far-field computation through a real wgpu WGSL compute shader (gate G4 onward). Gain parity ≤ 0.5 dBi vs CPU on all corpus RP cases.
- **`ZMatrix::from_flat`**: new constructor for building a `ZMatrix` from GPU-produced flat row-major data.
- **CPU-vs-GPU benchmark gate (G5)**: GPU path asserted no more than 25% slower than CPU on large RP grid (37×73 = 2701 points); gate is skipped gracefully in CI without hardware GPU.
- Gate G6: GPU Z-matrix fill max relative error 2.12×10⁻⁶ vs CPU (limit 1×10⁻⁴) on 51-segment dipole at 14 MHz.
- Gate G7: GPU fill + CPU solve feedpoint ΔR=0 Ω, ΔX=0 Ω vs all-CPU reference.

### Ground and geometry (Phase 2)

- **GN2 near-ground**: above-ground GN type 2 decks solve correctly with a near-ground corpus fixture and tolerance gate.
- **Buried-wire guardrails**: buried-wire requests on active ground models fail fast with an actionable diagnostic; supported near-ground class is corpus-gated.
- **GN0 Fresnel finite ground**: Hallen matrix assembly uses a complex Fresnel-style reflection factor from EPSE/SIG for GN type 0 simple finite-ground decks.
- **PEC ground RP**: ground-plane image contribution correctly applied to far-field computation with above-horizon normalization and below-horizon null contract.
- **Geometry diagnostics**: intersecting wires, tiny source segments (L/r < 2), and invalid junction topologies detected before solve with actionable error messages.

### Source, load, and network (Phase 2)

- **EX type 5 (pulse-mode current source)**: driven-segment current path implemented; suppresses legacy portability warning on `--solver pulse`.
- **LD family**: distributed and lumped load semantics implemented and corpus-gated.
- **TL subset**: transmission-line card semantics wired into solve path.

### Report and scriptability (Phase 2)

- **SOURCES / LOADS sections**: stable, machine-parseable report sections with deterministic ordering (`FEEDPOINTS → SOURCES → LOADS → CURRENTS`).
- **SWEEP_POINTS summary**: per-frequency sweep summary section after all report blocks.
- **Scriptability preserved**: stderr-only diagnostics and stable stdout machine stream remain hard contracts after all Phase 2 additions.

## 0.4.0 — Phase 3 complete

### GUI

- **`fnec-gui` desktop application** (iced 0.13): dark-themed window with deck path field and four-tab layout: Solve, Sweep, Pattern, and Currents.
- **Solve tab**: one-click single-frequency Hallen solve; displays frequency, Z_re, Z_im, and |Z|.
- **Sweep tab**: frequency range input (Start / End / Step MHz), Run Sweep button, sortable four-column result table (Freq, Z_re, Z_im, |Z|). Column headers are clickable sort toggles.
- **Pattern tab**: elevation-plane radiation pattern slice (37 points, 0–180° θ in 5° steps at a user-chosen φ angle) rendered as a text bar chart normalised to the peak gain.
- **Currents tab**: per-segment current magnitude distribution bar chart for the loaded deck. Peak segment gets a full-width bar; bars are normalised 0–1.
- Headless state-machine architecture: all GUI logic lives in `app_state.rs` (no iced dependency), tested by 47 smoke tests.

### CLI

- **`--sweep-config <file.toml>`**: batch frequency sweep from a TOML spec (linear range or explicit point list); one structured output block per frequency point.
- **`--vars <file>`**: variable-substitution engine (`$VAR` tokens in NEC deck templates replaced from a flat TOML/JSON map at parse time).
- **`fnec sweep --resonance <file.nec.toml>`**: binary-search resonance targeting; finds the wire length that minimises feedpoint reactance within user-defined bounds.

### Project file

- **`nec_project` crate**: versioned TOML project format (`ProjectFile`, `SolverConfig`, `NamedRun`) with serde round-trip and version-guard (`UnsupportedVersion`).
- **Run history**: `RunHistory` / `RunRecord` / `ResultSummary` appended on each solve; queryable by count, last-run, and index.

### Solver

- GN type 0 finite-ground model active in Hallen impedance assembly (Fresnel-style complex image scaling from EPSE/SIG).
- Non-collinear multi-wire Hallen support: junction detection (KCL rows), per-wire local cos(k·s) homogeneous vectors, passive-wire rhs=0.
- EX type 1/4/5 first implementation slice in pulse-solver mode.
- EX type 2 staged portability fallback (warning; treated as EX type 0).
- PT and NT cards parsed with staged portability warnings.
- TL `NSEG>1` lossless-line acceptance.
- GN2 near-ground corpus contract added and passing.

### Documentation

- `docs/contributing.md` — build/test workflow, branch conventions, corpus-gate requirements.
- `docs/plugin-api-design.md` — extension surface, safety model, EP-1 `DeckPostProcessor`, EP-2 `ResultFilter`.
- `docs/project-format.md` — TOML project file format reference.
- `docs/usability-benchmark-ph3.md` — Phase 3 usability benchmarks: 7-action 5-point sweep, edit-run-inspect comparison vs. xnec2c.
- All Phase 3 usability acceptance minima satisfied.

## 0.2.0

### Solver

- **Multi-wire Hallen fix**: three correlated bugs corrected — passive wires now receive zero RHS,
  each wire uses its own arc-length coordinate for the cos(k·s) term, and each wire gets an
  independent homogeneous constant C_w with its own endpoint constraints. This makes Yagi and
  multi-source antenna analysis correct.
- Corpus validation passing for yagi-5elm-51seg and multi-source decks.

### Parser / Geometry

- **GM card** (Geometry Move): parse and apply rotate + translate transformations to wire tag ranges.
  When `tag_increment == 0` wires are modified in place; when > 0 new copies are appended with
  incremented tag numbers.
- **GR card** (Geometry Repeat): parse and apply z-axis rotation repeats. Each additional copy
  is rotated by a cumulative multiple of `angle_deg` and assigned incremented tag numbers.

### Report

- **Current distribution table**: `CURRENTS` section appended to CLI report output after the
  feedpoint table. Columns: TAG SEG I_RE I_IM I_MAG I_PHASE.

### CLI

- GE I1=-1 warning updated to describe below-ground wire handling intent.
- GE I1=unknown warnings now include the valid value range hint.

## Unreleased

*(nothing currently queued)*

---

## Previous: 0.1.0

### Solver

- Added NEC `GN` card support for Phase 1 perfect ground (`GN 1`) in Hallen mode.
- Hallen matrix assembly now includes a PEC image-method contribution for `GN 1` decks.
- CLI Hallen runs no longer silently ignore `GN`; ground decks now produce distinct feedpoint impedances.

### Corpus

- Updated `dipole-ground-51seg` golden reference to the new GN-aware Hallen regression value.

### Documentation

- Established mandatory frontmatter contract for every `docs/*.md` file.
- Defined PR automation approach for `last_updated` stamping and frontmatter validation.
- Documented governance, roadmap, and delivery process for docs maintenance.
