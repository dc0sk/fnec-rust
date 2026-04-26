---
project: fnec-rust
doc: docs/changelog.md
status: living
last_updated: 2026-05-01
---

# Changelog

All notable documentation process changes are recorded here.

## Unreleased

### Added

- RP card execution is now wired into the CLI report path.
- Text reports now include a `RADIATION_PATTERN` section when one or more `RP` cards are present.
- Added corpus regression deck `corpus/dipole-freesp-rp-51seg.nec` and contract coverage for pattern-table rendering.

### Changed

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
