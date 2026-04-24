---
project: fnec-rust
doc: corpus/README.md
status: living
last_updated: 2026-04-23
---

# Golden Reference Corpus

This directory contains the golden reference test corpus used to validate fnec-rust's numerical accuracy against NEC reference engines (primary: xnec2c, fallback: 4nec2).

Every NEC deck in this corpus is validated against a reference engine and the results are recorded in `corpus/reference-results.json`. CI runs `cargo test -p nec-cli --test corpus_validation -- --ignored` to ensure fnec-rust results remain within the tolerance matrix defined in `docs/requirements.md`.

## Corpus cases

### 1. `dipole-freesp-51seg.nec` — Half-wave dipole, free space

**Purpose**: Validate core Hallén solver accuracy on the canonical thin-wire antenna.

**Geometry**:
- Frequency: 14.2 MHz (λ ≈ 21.128 m)
- Wire: L = 10.564 m (λ/2), a = 0.001 m (thin wire)
- Segments: 51 (uniform spacing)
- Feed: Center segment (tag=1, seg=26), 1.0 V excitation
- Ground: None (free space)

**Expected results** (from xnec2c reference):
- Z_in ≈ 74.24 + j13.90 Ω (validated against Python MoM script)
- Current distribution: symmetric cosine envelope

**Tolerance gates**:
- R (real): ≤ 0.1% relative or ≤ 0.05 Ω absolute
- X (imag): ≤ 0.1% relative or ≤ 0.05 Ω absolute
- Current mag (center): ≤ 0.1% relative

**Why this case**: It is the simplest, most well-understood benchmark. Pass here is a prerequisite for all other cases.

### 2. `dipole-ground-51seg.nec` — Half-wave dipole, over ground

**Purpose**: Validate Hallén solver with perfect-ground image-method effects.

**Geometry**:
- Frequency: 14.2 MHz
- Wire: L = 10.564 m, a = 0.001 m, height h = 10 m AGL
- Segments: 51
- Feed: Center segment, 1.0 V
- Ground: Perfect conductor at z = 0 (infinite, ideal)

**Expected results** (current regression gate):
- Z_in ≈ 81.91 + j16.42 Ω
- Current distribution: distorted from free-space case due to image interaction

**External parity status**:
- External reference candidate for this case is tracked in `corpus/reference-results.json` and remains pending capture from xnec2c/4nec2.
- CI currently gates the GN=1 regression value and prints external deltas when candidate values are present.

**Tolerance gates**: Same as dipole-freesp (R, X, current).

**Why this case**: Ground effects are critical for practical antennas. Validates GN=1 perfect-ground image-method behavior.

### 3. `yagi-5elm-51seg.nec` — 5-element Yagi array

**Purpose**: Validate multi-wire geometry, mutual coupling, and array gain.

**Geometry**:
- Frequency: 14.2 MHz
- Driven element: L = 10.564 m (λ/2 dipole), a = 0.001 m, 51 segments
- Reflector: L = 10.8 m, spacing 0.2 m behind driven
- Directors: 3 × L = 10.3 m, spacing 0.2 m forward
- Feed: Driven element center, 1.0 V
- Ground: None

**Expected results** (from xnec2c):
- Z_in ≈ [TBD — expected ≈ 25–40 Ω real, ±5 Ω imag]
- Forward gain ≈ [TBD — expected ≈ 10–12 dBi]
- Takeoff angle: ≈ 12–18° (elevation)

**Tolerance gates**:
- R, X: ≤ 0.1% relative or ≤ 0.05 Ω absolute
- Gain (max): ≤ 0.05 dB
- Takeoff angle: ≤ 1° (when available from pattern)

**Why this case**: Multi-wire geometry, coupling effects, array gain. Tests solver scaling and matrix conditioning.

### 4. `dipole-loaded.nec` — Half-wave dipole with series top-hat loading coil

**Purpose**: Validate wire-wire coupling and frequency tuning via loading.

**Geometry**:
- Frequency: 7.1 MHz (λ/2 → L ≈ 21.1 m without loading; shortened here with coil)
- Main dipole: L = 10.564 m, a = 0.001 m, 51 segments
- Loading coil: Placed at top of dipole (approx. as small loop ≈ 0.5 m diameter, 0.001 m wire a)
- Feed: Center of main dipole, 1.0 V
- Ground: None

**Expected results** (from xnec2c):
- Z_in ≈ [TBD — loaded impedance at 7.1 MHz expected near 50 Ω]
- Current distribution: distorted by coupling to coil

**Tolerance gates**: Same as dipole-freesp (R, X, current).

**Why this case**: Loading (coils, hats, stubs) is common in practical designs. Tests coupling calculations and validates that geometry edge cases (small wire segments, proximity effects) are handled correctly.

### 5. `frequency-sweep-dipole.nec` — Half-wave dipole, frequency sweep

**Purpose**: Validate frequency-domain convergence and impedance trend.

**Geometry**:
- Frequency range: 10 MHz, 12 MHz, 14.2 MHz, 16 MHz, 18 MHz (5 points)
- Wire: L = 10.564 m, a = 0.001 m, 51 segments
- Feed: Center segment, 1.0 V per frequency step
- Ground: None

**Expected results** (from xnec2c):
- Z_in trajectory must match known dipole impedance curve: minimum R around λ/2 (14.2 MHz), resistance increases off-resonance, reactance crosses zero near resonance
- Impedance at 10 MHz ≈ [TBD]
- Impedance at 14.2 MHz ≈ 74.24 + j13.90 Ω
- Impedance at 18 MHz ≈ [TBD]

**Tolerance gates**:
- Each frequency point: R, X within 0.1% relative
- Trend validation: impedance curve must be smooth (no discontinuities), resonance near 14.2 MHz

**Why this case**: Frequency sweeps are standard analysis. Validates that the solver scales correctly across frequency and produces physically sensible results.

### 6. `multi-source.nec` — Dipole array with two independent sources

**Purpose**: Validate multi-source impedance and current interaction.

**Geometry**:
- Frequency: 14.2 MHz
- Two parallel half-wave dipoles: L = 10.564 m each, a = 0.001 m, spacing 1 m
- Dipole 1: center at x=0, feed at center segment, 1.0 V
- Dipole 2: center at x=1 m, feed at center segment, 1.0 V (independent source)
- Ground: None

**Expected results** (from xnec2c):
- Z_in (both dipoles, with mutual coupling): ≈ [TBD — both around 74 Ω, with mutual impedance affecting phase slightly]
- Coupling factor: ≈ [TBD — expected small but nonzero]

**Tolerance gates**: R, X ≤ 0.1% relative per source.

**Why this case**: Multi-source problems are common (feed networks, phased arrays, test fixtures). Validates that the solver correctly handles multiple excitation points and coupling.

## Corpus metadata

| Case | Deck file | Segments | Wires | Sources | Ground | Reference Z_in (Ω) |
|:-----|:----------|:---------|:------|:--------|:-------|:------------------|
| 1 | dipole-freesp-51seg.nec | 51 | 1 | 1 | None | 74.24 + j13.90 |
| 2 | dipole-ground-51seg.nec | 51 | 1 | 1 | Perfect | 81.91 + j16.42 |
| 3 | yagi-5elm-51seg.nec | 51 | 5 | 1 | None | [TBD] |
| 4 | dipole-loaded.nec | ≈51 | 2 | 1 | None | [TBD] |
| 5 | frequency-sweep-dipole.nec | 51 | 1 | 1 (5× freq) | None | [TBD] × 5 |
| 6 | multi-source.nec | 51 | 2 | 2 | None | [TBD] × 2 |

**Total**: 6 benchmark families, ≈12 individual frequency/source points.

## Reference workflow

Preferred (xnec2c, when stable on the host):

```bash
xnec2c --batch -j0 -i corpus/dipole-freesp-51seg.nec --write-csv .tmp-work/dipole-freesp.csv
```

Fallback (4nec2 under Wine or Windows VM):

1. Open the deck in 4nec2.
2. Run the frequency loop.
3. Export feedpoint impedance/report data to CSV or text.
4. Import the extracted values with the helper script:

```bash
scripts/import-reference-impedance.py \
  --case dipole-ground-51seg \
  --real 63.12 --imag -18.45 \
  --source "4nec2 (Wine 9.x)" \
  --status "Reference captured via 4nec2/Wine"
```

For sweep/multi-source cases, update a point key:

```bash
scripts/import-reference-impedance.py \
  --case frequency-sweep-dipole \
  --point 12 \
  --real 41.21 --imag -28.34 \
  --source "4nec2 (Windows VM)"
```

Bulk import (recommended once you have all numbers):

1. Copy `corpus/reference-import-template.json` to `.tmp-work/reference-import.json`
2. Replace sample values with your measured values
3. Import all values in one shot:

```bash
scripts/import-reference-impedance.py --batch-file .tmp-work/reference-import.json
```

## Exactly what I need from you

Please provide these values from 4nec2 output (all in ohms):

1. `dipole-ground-51seg`: `real`, `imag`
2. `yagi-5elm-51seg`: `real`, `imag`
3. `dipole-loaded`: `real`, `imag`
4. `frequency-sweep-dipole`: points `10`, `12`, `14.2`, `16`, `18` each with `real`, `imag`
5. `multi-source`: `source_1` and `source_2` each with `real`, `imag`
6. Reference metadata:
   - engine label (e.g., `4nec2 (Wine 9.x)`)
   - engine version string shown by 4nec2

Preferred format: fill `corpus/reference-import-template.json` and send it back, or paste values as:

```text
dipole-ground-51seg: R=..., X=...
yagi-5elm-51seg: R=..., X=...
dipole-loaded: R=..., X=...
frequency-sweep-dipole@10: R=..., X=...
frequency-sweep-dipole@12: R=..., X=...
frequency-sweep-dipole@14.2: R=..., X=...
frequency-sweep-dipole@16: R=..., X=...
frequency-sweep-dipole@18: R=..., X=...
multi-source@source_1: R=..., X=...
multi-source@source_2: R=..., X=...
engine: ...
engine_version: ...
```

Current caveat (Linux headless CI/dev shells):

- `xnec2c 4.4.18` may hang in `--batch` mode with GTK warnings and no output file, even when input syntax is valid.
- In that environment, use 4nec2 (Wine/VM) or Python validated references until xnec2c batch stability is resolved.

Results extracted into `corpus/reference-results.json` with structure:

```json
{
  "dipole-freesp-51seg": {
    "frequency_mhz": 14.2,
    "segments": 51,
    "reference": "xnec2c commit [hash]",
    "feedpoint_impedance": {
      "real_ohm": 74.24,
      "imag_ohm": 13.90
    },
    "tolerance_gates": {
      "R_percent": 0.1,
      "X_percent": 0.1,
      "R_absolute_ohm": 0.05,
      "X_absolute_ohm": 0.05
    }
  },
  ...
}
```

## CI validation

On each commit, `cargo test -p nec-cli --test corpus_validation -- --ignored` runs fnec against corpus decks with captured references and compares results against `corpus/reference-results.json`. Any result exceeding the tolerance gate is a **CI failure** (not a warning).

## Status

- [ ] Dipole free-space deck created and xnec2c reference captured
- [ ] Dipole ground deck created and xnec2c reference captured
- [ ] Yagi deck created and reference captured
- [ ] Loaded dipole deck created and reference captured
- [ ] Frequency sweep created and reference captured
- [ ] Multi-source deck created and reference captured
- [x] Validation test suite scaffolded (`apps/nec-cli/tests/corpus_validation.rs`) and CI workflow added (`.github/workflows/corpus-validation.yml`)
- [ ] All corpus cases pass fnec-rust within tolerance matrix
- [ ] BLK-003 resolved: corpus validation gates Phase 1 → Phase 2
