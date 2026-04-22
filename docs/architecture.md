---
project: fnec-rust
doc: docs/architecture.md
status: living
last_updated: 2026-04-22
---

# Architecture

## Docs quality architecture

1. Author updates docs on a feature branch.
2. PR triggers docs workflows.
3. Validation job checks frontmatter schema and path correctness.
4. Stamping job updates `last_updated` for changed docs.
5. Stamping job commits only when changes exist.
6. Protected `main` receives changes through normal PR merge.

## Key constraints

- No direct pushes to protected `main`.
- CI errors must be explicit and file-scoped.
- Automation must remain deterministic and auditable.
