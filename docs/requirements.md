---
project: fnec-rust
doc: docs/requirements.md
status: living
last_updated: 2026-04-22
---

# Requirements

## Functional requirements

- **FR-001**: Every markdown file under `docs/` must include YAML frontmatter.
- **FR-002**: Frontmatter must contain `project: fnec-rust`.
- **FR-003**: Frontmatter `doc` must exactly equal the file path (example: `docs/requirements.md`).
- **FR-004**: Frontmatter must contain `status: living`.
- **FR-005**: Frontmatter must contain `last_updated` in `YYYY-MM-DD` format.
- **FR-006**: PR automation must stamp `last_updated` for changed docs based on git diff.
- **FR-007**: PR automation must validate all docs frontmatter and fail on violations.
- **FR-008**: Automation must work with a protected `main` flow by committing to PR branches.

## Non-functional requirements

- **NFR-001**: Checks must run in GitHub Actions on pull requests.
- **NFR-002**: Validation output must provide actionable file-level errors.
- **NFR-003**: Stamping must be idempotent (no-op when no doc changes exist).
- **NFR-004**: Process should remain mobile-friendly; fallback instructions must not require direct `main` pushes.

## Acceptance criteria

- [ ] All `docs/*.md` files include valid frontmatter with correct `doc` path.
- [ ] Validation workflow fails when frontmatter is missing or malformed.
- [ ] Stamping workflow updates `last_updated` for changed docs and commits only when needed.
- [ ] Documentation clearly explains governance, roadmap, changelog, and release-note usage.
