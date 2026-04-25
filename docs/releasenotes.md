---
project: fnec-rust
doc: docs/releasenotes.md
status: living
last_updated: 2026-05-01
---

# Release Notes

## 0.2.0

### Solver

- **Multi-wire Hallen fix**: three correlated bugs corrected — passive wires now receive zero RHS,
  each wire uses its own arc-length coordinate for the cos(k·s) term, and each wire gets an
  independent homogeneous constant C_w with its own endpoint constraints. This makes Yagi and
  multi-source antenna analysis correct.
- Corpus validation passing for yagi-5elm-51seg and multi-source decks.

### Parser / Geometry

- **GM card** (Geometry Move): parse and apply rotate + translate transformations to wire tag ranges.
  When `tag_increment == 0` wires are modified in place; when > 0 new copies are appended with
  incremented tag numbers.
- **GR card** (Geometry Repeat): parse and apply z-axis rotation repeats. Each additional copy
  is rotated by a cumulative multiple of `angle_deg` and assigned incremented tag numbers.

### Report

- **Current distribution table**: `CURRENTS` section appended to CLI report output after the
  feedpoint table. Columns: TAG SEG I_RE I_IM I_MAG I_PHASE.

### CLI

- GE I1=-1 warning updated to describe below-ground wire handling intent.
- GE I1=unknown warnings now include the valid value range hint.

## Unreleased

*(nothing currently queued)*

---

## Previous: 0.1.0

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
