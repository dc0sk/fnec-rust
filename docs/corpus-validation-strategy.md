---
project: fnec-rust
doc: docs/corpus-validation-strategy.md
status: living
last_updated: 2026-04-26
---

# Corpus Validation Strategy

## Overview

The golden reference corpus is the primary mechanism for measuring numerical parity with NEC2/NEC4 and validating that fnec-rust is production-ready.

This document describes the corpus philosophy, validation process, and CI integration.

## Corpus philosophy

### Reference-centric validation

All numerical work is validated against a **golden reference corpus** run through an external reference engine (xnec2c preferred; 4nec2 fallback when needed). Internal self-consistency testing is necessary but insufficient; we measure against external truth.

### Tolerance matrix is the contract

Acceptance criteria are defined in `docs/requirements.md` (Numerical compatibility tolerance matrix) per metric:

- Input resistance R: ≤ 0.1% relative or ≤ 0.05 Ω absolute (whichever is wider)
- Input reactance X: ≤ 0.1% relative or ≤ 0.05 Ω absolute (whichever is wider)
- Maximum gain: ≤ 0.05 dB
- Pattern gain per sample: ≤ 0.1 dB
- Segment current magnitude: ≤ 0.1% relative
- Segment current phase: ≤ 0.1 °
- SWR: ≤ 0.01 absolute

**Exceeding any tolerance is a CI failure**, not a warning. Tolerance creep is not acceptable.

### Staged corpus growth

The current Phase 1 corpus includes:
1. Half-wave dipole, free space (the ground truth)
2. Half-wave dipole shifted via GM in-place transform (translation-invariance regression)
3. Half-wave dipole, over perfect ground (validates image/Sommerfeld treatment)
4. 5-element Yagi (validates multi-wire coupling and array gain)
5. Loaded dipole (validates geometry edge cases and wire-wire coupling)
6. Frequency sweep (validates frequency-domain convergence)
7. Multi-source case (validates multi-driver support)
8. Multi-source generated via GR (geometry-repeat equivalence regression)
9. Multi-source generated via GM copy (geometry-move equivalence regression)

As phases progress, the corpus grows to include:
- Complex geometries (helix, spirals, bent wires)
- Advanced ground models (Sommerfeld, buried antennas)
- Near-field patterns and edge diffraction
- Very large and very small segmentation (convergence boundaries)

Each corpus case is defined in `corpus/README.md` with:
- Geometry description and NEC deck
- Expected reference result from xnec2c
- Tolerance gates for this case
- Status (captured, validated, etc.)

## NEC-5 validation coverage matrix (PAR-008)

This matrix maps NEC-5 Validation Manual scenario classes to current fnec-rust corpus coverage.
The intent is not to claim full NEC-5 capability; it is to make coverage explicit, tolerance-gated, and auditable.

| NEC-5 validation class | Manual section/theme | fnec-rust in-scope equivalent | Corpus mapping | Gate metrics | Current status |
|:-----------------------|:---------------------|:-------------------------------|:---------------|:-------------|:---------------|
| Thin-wire kernel behavior | 2.1 Thin-wire Kernel | Hallen thin-wire wire-only behavior at resonance | `dipole-freesp-51seg` | R, X | Covered (external reference captured; CI-gated) |
| Source model behavior | Wire source-model comparisons (Section 2 wire modeling themes) | EX type 0 voltage-source behavior in wire-only decks | `dipole-freesp-51seg`, `multi-source` | R, X per source | Covered for EX type 0 (single-source and multi-source corpus cases) |
| Convergence for dipole antenna | 2.3 Convergence for a Dipole Antenna | Frequency behavior around dipole resonance (segmentation expansion tracked separately) | `frequency-sweep-dipole` | R, X trend across sweep | Covered for FR sweep points (segmentation-variant subcases deferred) |
| Wires over ground | 4.1 Horizontal Wires over Ground | Single wire over ideal/perfect ground in current scope | `dipole-ground-51seg` | R, X (+ optional external R/X gates) | Covered for PEC image-method class; deeper external parity tightening remains ongoing |
| Loop antennas over ground | 4.2 Loop Antennas over Ground | Small-loop/loaded-loop over ground | No current equivalent corpus case | R, X, pattern/gain (future) | Out of scope in current phase |
| Surface meshing and wire-surface junctions | Surface/junction validation themes (wire+patch classes) | Wire-surface coupling and patch meshing | No current equivalent corpus case | Junction current continuity and field behavior (future) | Out of scope in current architecture |
| Monopole on finite box and patch-ground classes | 3.1 Monopole on a Box | Finite conducting surfaces and mixed wire/surface models | No current equivalent corpus case | R, X, pattern/gain vs reference | Out of scope in current architecture |

### Coverage interpretation rules

- A row is considered covered for PAR-008 scope when at least one reproducible corpus case is mapped and enforced with explicit tolerance gates in CI.
- External-reference depth is tracked separately in each case status and remains a quality dimension for parity tightening, not a blocker for matrix mapping completeness.
- Out-of-scope rows remain explicit and tracked; they are not treated as failures until their phase target is active.

### Out-of-scope rationale (current phase)

- Surface meshing, wire-surface junctions, and finite box/patch classes are out of scope because current solver architecture is wire-focused and does not implement NEC-5 mixed wire/surface capability.
- Loop-over-ground parity is deferred until advanced ground and loop-specific corpus cases are added in Phase 2 expansion work.
- Ground-case regression is now modeled for GN=1 (PEC image method), but external-reference parity evidence for this class is still incomplete.

### Entry/exit criteria for PAR-008 completion

- Matrix rows above remain synchronized with corpus cases and `corpus/reference-results.json` status.
- Each in-scope row must have at least one reproducible corpus mapping with explicit tolerance gates bound to `docs/requirements.md` metrics.
- Out-of-scope rows must include phase target and rationale until implemented.

### PAR-008 completion status (current)

- Scenario-class mapping is complete for current wire-solver architecture scope.
- In-scope mapped classes have reproducible corpus coverage and explicit tolerance gates.
- Out-of-scope NEC-5 classes are documented with rationale and retained in roadmap/backlog for later phases.
- PAR-008 is therefore considered complete as a coverage-matrix deliverable; deeper external-reference parity work continues under class-specific parity items.

## Validation workflow

### Step 1: Reference capture (manual, one-time per case)

For each corpus case:

1. Write the NEC deck file (e.g., `corpus/dipole-freesp-51seg.nec`)
2. Run through reference engine:
   ```bash
  xnec2c --batch -j0 -i corpus/dipole-freesp-51seg.nec --write-csv .tmp-work/dipole-freesp.csv
   ```
  If xnec2c hangs in headless Linux (known with some 4.4.x builds), run 4nec2 under Wine/VM and export equivalent impedance/report data.
3. Extract key results (impedance, gain, pattern samples, currents)
4. Record in `corpus/reference-results.json` (manual edit or helper script):
   ```bash
   scripts/import-reference-impedance.py \
     --case dipole-ground-51seg \
     --real 63.12 --imag -18.45 \
     --source "4nec2 (Wine 9.x)" \
     --status "Reference captured via 4nec2/Wine"
   ```

   For full runs, use batch import:
   ```bash
   scripts/import-reference-impedance.py --batch-file .tmp-work/reference-import.json
   ```

   JSON shape remains:
   ```json
   {
     "dipole-freesp-51seg": {
       "feedpoint_impedance": {
         "real_ohm": 74.24,
         "imag_ohm": 13.90
       },
       "tolerance_gates": {
         "R_percent_rel": 0.1,
         "X_percent_rel": 0.1,
         "R_absolute_ohm": 0.05,
         "X_absolute_ohm": 0.05
       }
     }
   }
   ```
5. Update `corpus/README.md` status to "Reference captured"

### Step 2: Integration test (automatic in CI)

CI runs `cargo test -p nec-cli --test corpus_validation`:

```rust
#[test]
fn corpus_validation_dipole_freesp() {
    // Run fnec-rust: fnec --solver hallen corpus/dipole-freesp-51seg.nec
    // Extract impedance from output: 74.242874+13.899516j
    // Compare: |74.24 - 74.24| <= 0.05 Ω ✓, |13.90 - 13.90| <= 0.05 Ω ✓
    // Assert pass or fail
}
```

Each test:
1. Runs fnec-rust on the corpus deck with the specified solver
2. Parses the feedpoint impedance (and other metrics if available)
3. Checks against tolerance gates from `corpus/reference-results.json`
4. Fails the entire CI gate if any tolerance is exceeded

### Step 3: Phase gate

Before Phase 1 → Phase 2 transition:
- All corpus cases must have references captured
- All validation tests must pass
- Documented blocker resolution: **BLK-003** (4nec2 report format contract locked; golden corpus results validated within tolerance)

## CI integration

### Local pre-commit

```bash
cargo test                  # Unit tests
cargo test -p nec-cli --test corpus_validation  # Corpus validation
```

### GitHub Actions (`.github/workflows/`)

`corpus-validation.yml` workflow behavior:

1. Runs on every commit to main and PRs
2. Builds/runs the nec-cli test target via Cargo
3. Runs corpus tests: `cargo test -p nec-cli --test corpus_validation`
4. Reports per-case tolerance status (summary table in PR comment)
5. Fails the CI gate if any case exceeds tolerance

Example PR comment:

```
## Corpus Validation Results

| Case | Status | R (Ω) | X (Ω) | Tolerance | Pass |
|:-----|:-------|:------|:------|:----------|:-----|
| dipole-freesp | ✓ | 74.24 | 13.90 | ±0.05 | PASS |
| dipole-ground | ⏳ | — | — | — | SKIPPED (ref TBD) |
| yagi-5elm | ⏳ | — | — | — | SKIPPED (ref TBD) |

**Overall**: 1 pass, 2 skipped, 0 failures ✓
```

## Host tooling dependencies

The following external tools are required for the reference-capture and validation workflow used in this repository:

| Tool | Required | Purpose |
|:-----|:--------:|:--------|
| `gh` (GitHub CLI) | Yes | PR/issue automation, milestone/label management, and workflow integration from terminal runs |
| `jq` | Yes | JSON inspection and extraction in terminal workflows (corpus status, reference field queries) |
| `wine` | Conditional | Run Windows NEC engines (4nec2/NEC2 binaries) when native xnec2c batch execution is unstable |
| `xnec2c` | Preferred | Primary external NEC2 reference engine for golden-reference capture |
| 4nec2 + NEC2 executable (`nec2dxs500.exe`/equivalent) | Fallback | External reference capture path when xnec2c is unavailable or unstable on host |
| `pdftotext` | Conditional | Extract text from NEC-5 Validation Manual for planning and traceability work |

Notes:
- zsh command autocorrect prompts (for example, suggesting `jaq` when `jq` is missing) indicate the originally requested tool is not installed.
- Project workflow assumes `jq` for scripts/commands unless explicitly stated otherwise.

## Adding new corpus cases

To add a new corpus case:

1. Write the NEC deck: `corpus/my-case.nec`
2. Update `corpus/README.md` with case description
3. Add stub to `corpus/reference-results.json` with `null` values and status "Deck created; reference TBD"
4. Run reference capture (manual): `xnec2c --batch -j0 -i corpus/my-case.nec --write-csv ...` (or 4nec2 export when xnec2c is unstable)
5. Update `corpus/reference-results.json` with real values
6. Ensure the new case is represented in `corpus/reference-results.json` (the harness is table-driven and reads from the JSON table)
7. Update status in `corpus/README.md`: "Reference captured"
8. Commit together

## Status quo (April 2026)

- ✅ Corpus framework established (`corpus/README.md`, 9 active case families)
- ✅ Regression references and tolerance gates maintained in `corpus/reference-results.json`
- ✅ Validation test is active and table-driven (`apps/nec-cli/tests/corpus_validation.rs`)
- ✅ CI workflow wired (`.github/workflows/corpus-validation.yml`)
- ⏳ External-reference parity capture is still in progress for some classes/cases
- ⏳ Full Phase 1→2 gate remains open pending external-reference closure and deferred scope items

## Next steps

**Immediate (Phase 1):**
1. Complete external-reference captures for remaining in-scope cases (xnec2c preferred, 4nec2 fallback)
2. Keep `corpus/reference-results.json` synchronized with case additions and tolerance-gate intent
3. Keep `.github/workflows/corpus-validation.yml` green and extend per-case summaries where useful
4. Maintain corpus pass status while unresolved/deferred cases remain explicitly documented

**Later (Phase 2):**
1. Expand corpus with complex geometries
2. Add near-field validation cases
3. Add convergence boundary tests (very large/small segmentation)
4. Document corpus sensitivity and interpretation guidelines

## References

- `corpus/README.md` — Corpus case definitions and geometry descriptions
- `corpus/reference-results.json` — Reference results and tolerance gates
- `apps/nec-cli/tests/corpus_validation.rs` — Integration test implementation
- `docs/requirements.md` — Tolerance matrix and numerical compatibility policy
- `docs/roadmap.md` — Phase gates and blocker definitions
