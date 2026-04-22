---
project: fnec-rust
doc: docs/backlog.md
status: living
last_updated: 2026-04-22
---

# fnec-rust — Backlog (Idea Pool)

This document is an uncommitted idea pool: feature ideas, experiments, nice-to-haves, and research topics.
Items move from here into `docs/requirements.md` (as requirements) and then into `docs/roadmap.md` (as scheduled work).

## Triage Rules
- Use labels: `P0` (must), `P1` (should), `P2` (nice), `P3` (research)
- If an item becomes a requirement, assign a stable ID in `requirements.md`.
- If an item is planned, reference it from `roadmap.md`.

## Ideas
| ID | Priority | Area | Title | Notes | Links |
|---:|:--------:|:-----|:------|:------|:------|
| B-001 | P0 | Solver | NEC2 core solver in Rust | MoM pipeline, verified vs references | |
| B-002 | P0 | Parsing | 4nec2-tolerant NEC card parser | Robust parsing of “real-world” decks | |
| B-003 | P1 | GPU | Optional GPU acceleration | Runtime selection; start with postprocessing or matrix fill | |
| B-004 | P1 | Formats | Markdown project import/export | Literate project format with embedded `.nec` | |
| B-005 | P2 | UI | TUI (ratatui) | Batch runs, progress, quick plots | |
| B-006 | P1 | UI | GUI (iced) | Project workspace, plots, run cache | |

## Parking Lot (Unsorted)
-