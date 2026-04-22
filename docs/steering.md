---
project: fnec-rust
doc: docs/steering.md
status: living
last_updated: 2026-04-22
---

# Steering

## Scope

Steering governs documentation quality and automation policy for this repository.

## Decisions currently in force

1. Use a standard frontmatter schema across all docs.
2. Enforce schema in CI for all pull requests touching docs.
3. Auto-stamp `last_updated` in PRs when docs change.
4. Prefer branch + PR workflow; do not rely on direct writes to protected `main`.
5. Keep automation simple (`bash`, `grep`, `sed`, GitHub Actions) and easy to audit.

## Change control

- Proposed changes are opened via PR.
- CI validation must pass before merge.
- Steering updates this document when policy changes.
