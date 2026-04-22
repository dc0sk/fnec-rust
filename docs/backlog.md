---
project: fnec-rust
doc: docs/backlog.md
status: living
last_updated: 2026-04-22
---

# Backlog

- [ ] Implement `scripts/stamp-docs.sh` with `--from-git-diff` support.
- [ ] Implement `scripts/validate-docs-frontmatter.sh` for strict checks.
- [ ] Add `.github/workflows/docs-last-updated-pr.yml`.
- [ ] Add `.github/workflows/docs-validate.yml`.
- [ ] Add troubleshooting note for mobile approval-dialog limitations in contributor guidance.
- [ ] **Sinusoidal-basis EFIE (NEC2-style Pocklington fix)**: The current pulse/continuity solver modes use a pulse-basis Pocklington EFIE that is known to diverge from the physical solution for thin-wire antennas as the segment count increases. NEC2 uses sinusoidal (piecewise-sinusoidal) basis functions via `tbf`/`sbf`/`trio` which eliminate this divergence. Implementing the same sinusoidal-basis matrix assembly would make pulse/continuity modes accurate. Until then, these modes are marked experimental in the CLI. Reference: xnec2c `calculations.c`, NEC2 Theory of Operation (Burke & Poggio 1981).
