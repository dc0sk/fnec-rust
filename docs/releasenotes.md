---
project: fnec-rust
doc: docs/releasenotes.md
status: living
last_updated: 2026-05-04
---

# Release Notes

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
