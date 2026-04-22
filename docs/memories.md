---
project: fnec-rust
doc: docs/memories.md
status: living
last_updated: 2026-04-23
---

# Memories

## Working notes from current decisions

- Mobile approval dialogs can be unreliable; contributors may need browser fallback.
- Branch + PR flow is the safe default under protected `main`.
- Keeping docs automation small and shell-based improves maintainability.
- Frontmatter consistency is now treated as a first-class quality gate.
- Pocklington with pulse basis shows strong conditioning/convergence issues for dipole feedpoint impedance at modest segment counts.
- Hallen implementation attempts are highly sensitive to RHS/constraint conventions and can produce non-physical impedances if not formulated carefully.
- For iterative experiments, project-local temporary files should live under a gitignored workspace temp folder to reduce approval churn.
- Separating matrix assembly by solver mode is necessary for correctness hygiene but alone did not shift dipole feedpoint convergence; RHS/test-function scaling remains a primary suspect.
