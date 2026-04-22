---
project: fnec-rust
doc: docs/sbom.md
status: living
last_updated: 2026-04-22
---

# SBOM (Documentation Automation Scope)

## Components

- GitHub Actions workflows
- Bash runtime (`bash`)
- Standard Unix text tools (`grep`, `sed`, `awk` if needed)
- Git CLI for diff detection and branch commits

## Notes

- No additional third-party runtime dependencies are required for the docs automation design.
- Actions should use pinned major versions (`actions/checkout@v4`) as baseline.
