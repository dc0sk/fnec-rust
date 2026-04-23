---
project: fnec-rust
doc: docs/corpus-validation-strategy.md
status: living
last_updated: 2026-04-23
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

The MVP corpus (Phase 1) includes:
1. Half-wave dipole, free space (the ground truth)
2. Half-wave dipole, over perfect ground (validates image/Sommerfeld treatment)
3. 5-element Yagi (validates multi-wire coupling and array gain)
4. Loaded dipole (validates geometry edge cases and wire-wire coupling)
5. Frequency sweep (validates frequency-domain convergence)
6. Multi-source case (validates multi-driver support)

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

CI runs `cargo test -p nec-cli --test corpus_validation -- --ignored` (the `--ignored` flag limits execution to corpus checks that are explicitly enabled):

```rust
#[test]
#[ignore]
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
cargo test -p nec-cli --test corpus_validation -- --ignored  # Corpus validation (if references available)
```

### GitHub Actions (`.github/workflows/`)

Add a `corpus-validation.yml` workflow that:

1. Runs on every commit to main and PRs
2. Builds/runs the nec-cli test target via Cargo
3. Runs corpus tests: `cargo test -p nec-cli --test corpus_validation -- --ignored`
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

## Adding new corpus cases

To add a new corpus case:

1. Write the NEC deck: `corpus/my-case.nec`
2. Update `corpus/README.md` with case description
3. Add stub to `corpus/reference-results.json` with `null` values and status "Deck created; reference TBD"
4. Run reference capture (manual): `xnec2c --batch -j0 -i corpus/my-case.nec --write-csv ...` (or 4nec2 export when xnec2c is unstable)
5. Update `corpus/reference-results.json` with real values
6. Create integration test: `#[test] #[ignore] fn corpus_validation_my_case() { ... }`
7. Update status in `corpus/README.md`: "Reference captured"
8. Commit together

## Status quo (April 2026)

- ✅ Corpus framework established (`corpus/README.md`, 6 case definitions)
- ✅ MVP corpus decks created (dipole free-space, dipole over ground)
- ✅ Reference results template created (`corpus/reference-results.json`)
- ✅ Validation test scaffolded (`apps/nec-cli/tests/corpus_validation.rs`)
- ⏳ Reference capture in progress (xnec2c where stable; 4nec2 fallback on headless hosts)
- ✅ CI workflow wired (`.github/workflows/corpus-validation.yml`)
- ⏳ Full Phase 1 corpus not complete (Yagi, loaded, frequency sweep, multi-source decks TBD)

## Next steps

**Immediate (Phase 1):**
1. Complete reference captures for all 6 corpus cases via xnec2c
2. Update `corpus/reference-results.json` with real reference data
3. Enable corpus tests (remove `#[ignore]`)
4. Keep `.github/workflows/corpus-validation.yml` green and extend it with per-case summaries
5. Validate that fnec-rust passes all corpus cases within tolerance

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
