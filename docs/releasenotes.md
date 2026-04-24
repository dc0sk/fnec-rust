---
project: fnec-rust
doc: docs/releasenotes.md
status: living
last_updated: 2026-04-24
---

# Release Notes

## Unreleased

### Solver

- Added NEC `GN` card support for Phase 1 perfect ground (`GN 1`) in Hallen mode.
- Hallen matrix assembly now includes a PEC image-method contribution for `GN 1` decks.
- CLI Hallen runs no longer silently ignore `GN`; ground decks now produce distinct feedpoint impedances.

### Corpus

- Updated `dipole-ground-51seg` golden reference to the new GN-aware Hallen regression value.

### Documentation

- Established mandatory frontmatter contract for every `docs/*.md` file.
- Defined PR automation approach for `last_updated` stamping and frontmatter validation.
- Documented governance, roadmap, and delivery process for docs maintenance.
