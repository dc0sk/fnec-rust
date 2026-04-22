---
project: fnec-rust
doc: docs/changelog.md
status: living
last_updated: 2026-04-23
---

# Changelog

All notable documentation process changes are recorded here.

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
- Added NEC2 reference-inspired pulse RHS wavelength normalization path (`1/(dl*lambda)`) and validation notes.
