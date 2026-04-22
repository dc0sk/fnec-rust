---
project: fnec-rust
doc: docs/README.md
status: living
last_updated: 2026-04-22
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
