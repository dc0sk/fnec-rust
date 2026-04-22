---
project: fnec-rust
doc: docs/design.md
status: living
last_updated: 2026-04-22
---

# Design

## Frontmatter contract

Required keys per file:

- `project: fnec-rust`
- `doc: docs/<name>.md` (exact path)
- `status: living`
- `last_updated: YYYY-MM-DD`

## Validation behavior

Validation script must:

- scan all `docs/*.md`
- assert frontmatter exists as the first block
- verify `doc` equals actual file path
- verify `status` equals `living`
- verify `last_updated` matches date regex
- emit actionable CI errors and non-zero exit on failure

## Stamping behavior

Stamping script must:

- detect changed docs via git diff range
- update only `last_updated` in changed files
- avoid touching unchanged docs
- exit cleanly when no eligible files changed
