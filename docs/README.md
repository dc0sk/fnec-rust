---
project: fnec-rust
doc: docs/README.md
status: living
last_updated: 2026-08-22
---

# Documentation Overview

This directory captures project decisions and operating guidance for `fnec-rust`.

## Current documentation decisions

- Every `docs/*.md` file must start with standard frontmatter (`project`, `doc`, `status`, `last_updated`).
- `doc` must exactly match the file path.
- `status` is `living` for these active docs.
- `last_updated` uses `YYYY-MM-DD`.
- Documentation hygiene is enforced in PRs by:
  - a stamping workflow that updates `last_updated` on changed docs
  - a validation workflow that checks frontmatter correctness

## Document index

- `docs/project/` — **traceability layer**: end-to-end requirement → design → implementation → tests → results matrix ([docs/project/README.md](project/README.md), [traceability-matrix.md](project/traceability-matrix.md))
- `docs/requirements.md` — functional and non-functional requirements
- `docs/steering.md` — governance and decision ownership
- `docs/roadmap.md` — phased execution plan
- `docs/changelog.md` — change history by date/version
- `docs/releasenotes.md` — externally facing release summaries
- `docs/architecture.md` — docs automation architecture
- `docs/design.md` — implementation-level design details
- `docs/backlog.md` — remaining follow-up tasks
- `docs/sbom.md` — tooling/components inventory for docs automation
- `docs/memories.md` — lessons and operator notes
- `docs/solver-findings.md` — recent MoM kernel findings, experiments, and learnings
- `docs/applied-math.md` — applied electromagnetics/math formulas used by the solver
- `docs/rooftop-basis-plan.md` — next-step plan for continuity-enforcing basis support
- `docs/gui-redesign-plan.md` — phased action plan for the GPU-accelerated 3D GUI workbench (iced + wgpu)
- `docs/gui-guide.md` — user guide for the `nec-gui` 3-D antenna workbench
- `docs/cli-guide.md` — user guide for the `fnec` command-line interface
- `docs/leeson-correction-feasibility.md` — feasibility/design study for the Leeson stepped-diameter correction (BL-IMPR-014)
- `docs/sommerfeld-level2-scope.md` — scoping the real remaining Sommerfeld-Norton Level-2 / DCIM gap (BL-IMPR-015)
- `docs/external/` — **external references** captured for inspiration/cross-validation (not dependencies): [pymininec.md](external/pymininec.md) plus source PDFs (MININEC/UTD reports)

The curated list above covers the primary living docs. Beyond them, `docs/` also
holds **per-checklist design records** (`docs/ph*-chk-*.md`, `docs/gui-*-plan.md`)
and the **traceability layer** under `docs/project/`; these are indexed by ID
through `docs/project/` rather than re-listed here, and all `docs/**/*.md` carry
validated frontmatter (`scripts/validate-docs-frontmatter.sh`).
