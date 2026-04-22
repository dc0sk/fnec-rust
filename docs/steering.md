---
project: fnec-rust
doc: docs/steering.md
status: living
last_updated: 2026-04-22
---

# Steering

## Scope

Steering governs development process, documentation quality, and automation policy for this repository.

## Decisions currently in force

1. Use a standard frontmatter schema across all docs.
2. Enforce schema in CI for all pull requests touching docs.
3. Auto-stamp `last_updated` in PRs when docs change.
4. Keep `main` protected at all times; never commit or push directly to `main`, and use feature branches plus pull requests only.
5. Keep automation simple (`bash`, `grep`, `sed`, GitHub Actions) and easy to audit.

## Pre-commit hooks (required, enforced locally via git hooks or cargo-husky)

All developers must have these hooks active before committing:

| Hook | Command | Fail behaviour |
|:-----|:--------|:--------------|
| Format check | `cargo fmt --all -- --check` | Block commit; developer must run `cargo fmt --all` to fix |
| Test suite | `cargo test --workspace` | Block commit; all tests must pass |

Setup: hooks are installed via a workspace-level tool (e.g. `cargo-husky` or a project `Makefile` target).  
The setup method must be documented in the top-level README so new contributors activate them on first clone.

## Pre-push hooks (required)

All developers must have these hooks active before pushing:

| Hook | Command | Fail behaviour |
|:-----|:--------|:--------------|
| Security audit | `cargo audit` | Block push; address or explicitly acknowledge all advisories |

## Version bump process (required whenever the version number in Cargo.toml changes)

A version bump is not complete until all of the following are done in the same PR:

1. Update `version` field in the workspace `Cargo.toml`.
2. Regenerate the machine-readable SBOM in SPDX format: `cargo sbom --output-format spdx-json > SBOM.spdx.json`
3. Update `docs/changelog.md`: move all items from the `## Unreleased` section into a new dated release section `## X.Y.Z — YYYY-MM-DD`.
4. Update `docs/releasenotes.md`: write a curated, user-facing summary for the new version.
5. Update any version references in `docs/sbom.md` for internal project crates.
6. Update version badge or header in the top-level `README.md` if present.

Tools required: `cargo-sbom` must be installed (`cargo install cargo-sbom`).  
`SBOM.spdx.json` is committed to the repository root and updated on every version bump only.

CI must fail if a version bump PR does not touch `docs/changelog.md`.

## Change control

- Proposed changes are opened via PR.
- CI validation must pass before merge.
- Steering updates this document when policy changes.
