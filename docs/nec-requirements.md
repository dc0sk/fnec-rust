---
project: rusty-wire
doc: docs/nec-requirements.md
status: deferred
last_updated: 2026-04-30
---

# NEC Requirements for Complete COMP-001 Tolerance Verification

## Overview

This document specifies the NEC (Numerical Electromagnetic Code) deck and reference data requirements to complete the COMP-001 (resonant wire-length tolerance) and COMP-002 (propagation model tolerance) verification for rusty-wire. This work is tracked as **GAP-011** (NEC reference sweeps) and **GAP-006** (loop/trap-dipole corpus).

**Status**: Minimal baseline in place (free-space resonant dipole); comprehensive validation deferred.

## Reference Integration

All NEC reference data is validated using **fnec-rust** (Hallén MoM solver), which is cross-validated against:
- Python MoM reference implementation (`hallen_reference.py`)
- xnec2c (NEC-2 reference engine)
- 4nec2 (EZNEC-compatible NEC-2 solver)

See `~/git/fnec-rust/corpus/` for fnec-rust's validation framework and proven NEC deck patterns.

## Current Status (April 30, 2026)

### Completed ✅

**Minimal NEC baseline (GAP-011 Phase 1)**:
- 40m band resonant dipole, free space (7.1 MHz)
- NEC deck: `corpus/dipole-40m-freesp.nec`
- Solver: fnec-rust Hallén
- Reference impedance: **62.94 - j69.28 Ω** (free space, half-wave at 7.1 MHz)
- Corpus test: `corpus_resonant_dipole_40m_nec` (baseline validation)

**Note on impedance**: The negative imaginary part (capacitive reactance) indicates the antenna is electrically slightly longer than resonant at 7.1 MHz. This is expected behavior for thin-wire dipoles with the standard half-wave geometry used by rusty-wire.

### In Progress — Phase 2 (External Generation)

**Status**: NEC decks for Phase 2 are being generated in the fnec-rust project corpus and will be imported into rusty-wire corpus upon completion. This approach leverages fnec-rust's proven NEC validation framework and ensures cross-tool consistency.

**Generation Location**: `~/git/fnec-rust/corpus/` (Hallén solver + Python MoM cross-validation)

**Import Timeline**: Phase 2 decks will be committed to rusty-wire corpus/ when fnec-rust generation is complete, then integrated into tests/corpus_validation.rs with CI gates.

### Deferred — Remaining Work

## Phase 2 Completion (GAP-011 Continuation) — To Be Generated in fnec-rust

### 1. Resonant Dipole Variants (High Priority)

All cases at **40m band (7.1 MHz center frequency, half-wave ≈ 20.1 m)**:

#### 1a. Ground-Aware Dipole Cases

| Case | Deck Name | Height | Ground Model | Description | Reference |
|------|-----------|--------|--------------|-------------|-----------|
| Baseline | `dipole-40m-freesp.nec` | — | None | Free space | fnec (Hallén) ✅ |
| GN-1 Perfect | `dipole-40m-ground-perfect.nec` | 10 m | Perfect conducting | Image method | fnec (Hallén + GN1) |
| GN-0 Finite | `dipole-40m-ground-fresnel.nec` | 10 m | Simple Fresnel | Ground reflection coeff | fnec (Hallén + GN0) |
| GN-2 Good | `dipole-40m-ground-good.nec` | 10 m | σ=0.03 S/m | Good soil conductivity | fnec (Hallén + GN2) |
| GN-2 Average | `dipole-40m-ground-avg.nec` | 10 m | σ=0.005 S/m | Average soil | fnec (Hallén + GN2) |
| GN-2 Poor | `dipole-40m-ground-poor.nec` | 10 m | σ=0.001 S/m | Poor soil | fnec (Hallén + GN2) |

**NEC Deck Template (GN-2 example)**:
```
CE Half-wave dipole at 40m (7.1 MHz) over finite-conductivity ground
CE Wire: 20.07 m (λ/2 with ~0.95 velocity factor), height 10 m AGL
GW 1 51 0 0 -10.035 0 0 10.035 0.001
GE 0
GN 2 0 0 0 σ ε_r
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 7.1 0.0
EN
```

**Tolerance Gates** (per tolerance matrix, COMP-001):
- `R_percent_rel`: 1.0% (resonant dipole tight tolerance)
- `X_percent_rel`: 1.0%
- `R_absolute_ohm`: 0.1 (relative tolerance typically wider)
- `X_absolute_ohm`: 0.1

#### 1b. Height-Aware Dipole Cases (10 m AGL, good ground)

| Height | Deck Name | Expected Impedance Delta | Rusty-wire Factor |
|--------|-----------|-------------------------|--------------------|
| 7 m | `dipole-7m-ground-good.nec` | ~-5 to -10 Ω real | 0.78 skip-distance factor |
| 10 m | `dipole-10m-ground-good.nec` | Reference (~75 Ω) | 1.0 (baseline) |
| 12 m | `dipole-12m-ground-good.nec` | ~+5 to +10 Ω real | 1.12 skip-distance factor |

**Integration Point**: Rusty-wire currently does not adjust resonant length with height. These NEC cases will quantify that approximation's accuracy and inform whether height-aware resonant length correction is needed.

### 2. Inverted-V Antenna (Phase 2)

**Baseline case at 40m**:

| Aspect | Specification | Notes |
|--------|---------------|-------|
| Frequency | 7.1 MHz | 40m band |
| Geometry | Apex angle 90° | Two 45° legs, apex height 12 m, legs to ground |
| Leg length | ~14.2 m each | Maintains resonance vs dipole |
| Ground | Good soil (GN-2) | σ=0.03 S/m |
| Deck name | `inverted-v-40m-90deg.nec` | |

**NEC Deck Template**:
```
CE Inverted-V at 40m (90° apex angle)
CE Apex at (0, 0, 12 m), legs @ ±45° to ground
GW 1 26 10 10 12 0 0 0 0.001
GW 2 26 -10 -10 12 0 0 0 0.001
GE 0
GN 2 0 0 0 0.03 10
EX 0 1 13 0 1.0 0.0
FR 0 1 0 0 7.1 0.0
EN
```

**Expected vs Dipole**:
- Inverted-V impedance typically: ~50-60 Ω (resistive), low reactance
- Tolerance gates: ±2% (wider than dipole due to complex geometry)

### 3. EFHW (End-Fed Half-Wave) Antenna (Phase 2)

**Baseline case at 40m**:

| Aspect | Specification |
|--------|---------------|
| Frequency | 7.1 MHz |
| Total length | ~20.1 m (resonant) |
| Feed point | 1% from one end (off-center feed) |
| Ground | Good soil (GN-2) |
| Deck name | `efhw-40m.nec` |

**NEC Deck Template**:
```
CE EFHW at 40m (1% offset from end)
CE Single wire, 20.1 m length, end-fed
GW 1 51 0.2 0 0 20.3 0 0 0.001
GE 0
GN 2 0 0 0 0.03 10
EX 0 1 1 0 1.0 0.0
FR 0 1 0 0 7.1 0.0
EN
```

**Expected Impedance**:
- Typically high impedance at end (1000+ Ω) due to end-fed geometry
- Rusty-wire recommends 1:49 or 1:56 transformer to match 50 Ω feedline
- Tolerance: ±3% (geometry-dependent, empirical estimate)

## Phase 3 Deferral (GAP-006)

### Loop Antenna (Square, Full-Wave)

**40m band, free space + ground variants**:

| Case | Dimensions | Deck Name | Status |
|------|-----------|-----------|--------|
| Square loop, fs | ~20 m side | `loop-40m-square-freesp.nec` | Phase 3 |
| Square loop, gnd | ~20 m side, 5 m height | `loop-40m-square-gnd.nec` | Phase 3 |

**Note**: Loop resonances are multiple: full-wave, 3/2-wave, etc. Each has distinct impedance. Start with full-wave (fundamental).

### Trap Dipole (Multi-Band)

**40m + 20m simultaneous operation**:

| Aspect | Specification |
|--------|---------------|
| Design | 40m dipole with L-C trap at center for 20m isolation |
| Total length | ~21 m (40m resonant) |
| Trap resonance | 14.2 MHz (20m band center) |
| Deck name | `trap-dipole-40m-20m.nec` |

**NEC Deck Structure**:
- Main element: ~21 m dipole
- Lumped L-C trap segments at center (challenging in NEC; may require capacitive/inductive wire loading via LD card)
- Expected impedance: 40m resonant (~73 Ω), 20m SWR degraded due to trap

## Execution Plan

### Tools & Environment

**Primary solver**: fnec-rust (available at `~/git/fnec-rust`)
```bash
cd ~/git/fnec-rust && cargo build --release
./target/release/fnec <deck.nec>
```

**Fallback validation**: xnec2c or 4nec2 (external reference engines)

### Procedure for Each NEC Case

1. **Create NEC deck** in `~/git/rusty-wire/corpus/`
   - Follow template format above
   - Name: `{antenna}-{band}-{variant}.nec`

2. **Run fnec solver**
   ```bash
   fnec corpus/dipole-40m-ground-good.nec > temp-output.txt
   grep "FEEDPOINT\|FREQ_MHZ\|Z_RE\|Z_IM" temp-output.txt
   ```

3. **Record feedpoint impedance** (R, X at center frequency)

4. **Cross-validate** with xnec2c (if available) or 4nec2
   - Compare Z within ±1% tolerance
   - Document reference engine version

5. **Add to corpus/reference-results.json**
   ```json
   "dipole-40m-ground-good": {
     "deck_file": "dipole-40m-ground-good.nec",
     "description": "Half-wave dipole at 40m (7.1 MHz), good soil",
     "frequency_mhz": 7.1,
     "segments": 51,
     "wires": 1,
     "sources": 1,
     "ground": "GN-2 good soil (σ=0.03 S/m)",
     "feedpoint_impedance": { "real_ohm": XX.XX, "imag_ohm": YY.YY },
     "tolerance_gates": {
       "R_percent_rel": 0.01,
       "X_percent_rel": 0.01,
       "R_absolute_ohm": 0.1,
       "X_absolute_ohm": 0.1
     },
     "reference_source": "fnec-rust Hallén solver, cross-validated with xnec2c"
   }
   ```

6. **Enable corpus test** in `tests/corpus_validation.rs`
   ```rust
   #[test]
   fn corpus_resonant_dipole_40m_ground_good() {
       // Validate against reference impedance
       // Tolerance check: R ±1%, X ±1%
   }
   ```

7. **Update tolerance matrix** in `docs/requirements.md`
   - Mark COMP-001 rows as "CI-gated" once NEC cases are integrated
   - Update "Resonant length" tolerance rows with validated phase status

## Corpus Reference Structure

Example entry in `corpus/reference-results.json`:

```json
{
  "schema_version": "1.0",
  "last_updated": "2026-04-30",
  "reference_engine": "fnec-rust Hallén solver",
  "reference_engine_version": "fnec 0.2.0",
  "cases": {
    "dipole-40m-freesp": {
      "deck_file": "dipole-40m-freesp.nec",
      "description": "Half-wave dipole at 40m (7.1 MHz), free space",
      "frequency_mhz": 7.1,
      "segments": 51,
      "wires": 1,
      "sources": 1,
      "ground": "none",
      "feedpoint_impedance": {
        "real_ohm": 62.94,
        "imag_ohm": -69.28
      },
      "tolerance_gates": {
        "R_percent_rel": 0.01,
        "X_percent_rel": 0.01,
        "R_absolute_ohm": 0.1,
        "X_absolute_ohm": 0.1
      },
      "current_samples": [
        {
          "wire_id": 1,
          "segment_id": 26,
          "amplitude_db": ...,
          "phase_deg": ...
        }
      ],
      "reference_source": "fnec-rust (Hallén, validated against Python MoM)"
    }
  }
}
```

## Estimated Effort

| Phase | Case Count | Effort | Timeline |
|-------|-----------|--------|----------|
| Minimal (✅ done) | 1 dipole free space | 0.5 h | 2026-04-30 |
| Phase 2 part A | 6 dipole variants (ground + height) | 2 h | TBD |
| Phase 2 part B | 2 inverted-V cases | 1 h | TBD |
| Phase 2 part C | 2 EFHW cases | 1 h | TBD |
| Phase 3 | 4 loop/trap cases | 2 h | TBD |
| **Total** | **15+ cases** | **6.5 h** | **TBD** |

## Integration with Tolerance Matrix

Once NEC cases are complete, update [docs/requirements.md](requirements.md):

- COMP-001 status: "deferred" → "CI-gated"
- Tolerance matrix rows for resonant antennas: add "COMP-001 NEC-gated" marker
- Traceability: `COMP-001 | tests/corpus_validation.rs | covered (NEC-gated)`

## References

- [fnec-rust corpus README](https://github.com/scienceopen/fnec-rust/tree/main/corpus)
- [fnec-rust Hallén solver docs](https://github.com/scienceopen/fnec-rust/blob/main/crates/nec-solver/src/hallenMoM.rs)
- [NEC-2 standard deck format](https://www.nec2.org/)
- [COMP-001 tolerance definition](requirements.md#comp-001)
- [Corpus validation pattern](corpus-guide.md)

## Notes

1. **Impedance sign convention**: fnec-rust reports capacitive reactance as negative (e.g., -69.28 Ω). This is standard in antenna engineering. Verify sign consistency across reference engines.

2. **Frequency accuracy**: All decks specify frequency to 0.1 MHz precision. Ensure consistent frequency across test invocations.

3. **Segment resolution**: 51 segments is a proven balance between accuracy and performance. Avoid finer segmentation unless NEC convergence requires it.

4. **GN card variants**: NEC-2 supports GN 0, 1, 2. fnec-rust currently supports GN 1 (perfect) and GN 2 (finite) reliably. Verify GN 0 support status before using.

5. **Future extensions**: Once EFHW transformer recommendations are part of rusty-wire's advise mode, consider adding NEC decks for loaded dipoles (LD card) and Yagi arrays (multi-wire cases).
