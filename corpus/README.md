---
project: fnec-rust
doc: corpus/README.md
status: living
last_updated: 2026-04-25
---

# Golden Reference Corpus

This directory contains the golden reference test corpus used to validate fnec-rust's numerical accuracy against NEC reference engines (primary: xnec2c, fallback: 4nec2).

Every NEC deck in this corpus is validated against a reference engine and the results are recorded in `corpus/reference-results.json`. CI runs `cargo test -p nec-cli --test corpus_validation` to ensure fnec-rust results remain within the tolerance matrix defined in `docs/requirements.md`.

Optional external-candidate gates can be enabled per case in `tolerance_gates`:
- Impedance candidates: `ExternalR_absolute_ohm`, `ExternalX_absolute_ohm`, `ExternalR_percent_rel`, `ExternalX_percent_rel`
- RP candidates: `ExternalGain_absolute_dB`, `ExternalAxialRatio_absolute`

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

### 1b. `dipole-freesp-gm-inplace-shifted.nec` — Free-space dipole shifted via `GM`

**Purpose**: Validate that the currently supported `GM` in-place transform preserves electrical behavior for a free-space dipole under rigid translation.

**Geometry**:
- Frequency: 14.2 MHz
- Start with the canonical `dipole-freesp-51seg` wire
- Apply `GM 0 1 0 0 0 1.0 0 0 1` to translate the geometry by +1.0 m along x in place
- Feed: Center segment (tag=1, seg=26), 1.0 V excitation
- Ground: None

**Expected results** (current regression gate):
- Same feedpoint impedance as `dipole-freesp-51seg`
- Z_in = 74.242874 + j13.899516 Ω

**Tolerance gates**: Same as `dipole-freesp-51seg`.

**Why this case**: It is a direct corpus-level check that parser + geometry-builder `GM` in-place translation is not only accepted syntactically, but electrically invariant under free-space rigid translation.

### 1c. `dipole-freesp-rp-51seg.nec` — Free-space dipole with `RP` sweep

**Purpose**: Validate that `RP` cards trigger radiation-pattern execution and append a stable pattern table to the report contract.

**Geometry**:
- Frequency: 14.2 MHz
- Wire: same canonical 51-segment half-wave dipole as case 1
- Feed: Center segment (tag=1, seg=26), 1.0 V excitation
- Ground: None
- RP: `RP 0 19 1 0.0 0.0 10.0 0.0` (theta sweep 0..180° in 10° steps at phi=0°)

**Expected results** (current regression gate):
- Same feedpoint impedance as `dipole-freesp-51seg`
- Z_in = 74.242874 + j13.899516 Ω
- Pattern table present with 19 points (`RADIATION_PATTERN`, `N_POINTS 19`)
- Numeric pattern samples locked in corpus validation across 7 theta points (`0°, 30°, 60°, 90°, 120°, 150°, 180°` at `φ=0°`):
  - θ = 0°, φ = 0° → `GAIN_DB=-999.99`, `GAIN_V_DB=-999.99`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`
  - θ = 30°, φ = 0° → `GAIN_DB=-5.4220`, `GAIN_V_DB=-5.4220`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`
  - θ = 60°, φ = 0° → `GAIN_DB=0.3910`, `GAIN_V_DB=0.3910`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`
  - θ = 90°, φ = 0° → `GAIN_DB=2.1483`, `GAIN_V_DB=2.1483`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`
  - θ = 120°, φ = 0° → `GAIN_DB=0.3910`, `GAIN_V_DB=0.3910`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`
  - θ = 150°, φ = 0° → `GAIN_DB=-5.4220`, `GAIN_V_DB=-5.4220`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`
  - θ = 180°, φ = 0° → `GAIN_DB=-999.99`, `GAIN_V_DB=-999.99`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`

**Tolerance gates**:
- Same as `dipole-freesp-51seg` for impedance
- Pattern gain fields: ≤ 0.05 dB absolute on stored `GAIN_DB`, `GAIN_V_DB`, and `GAIN_H_DB` values
- Axial ratio: ≤ 0.0001 absolute on stored `AXIAL_RATIO` values
- External RP candidate gates: optional `ExternalGain_absolute_dB` / `ExternalAxialRatio_absolute` keys can additionally CI-gate `external_reference_candidate.pattern_samples` when present

**Why this case**: It locks RP execution into corpus and report-contract testing without adding new solver-option surface area.

### 1d. `dipole-xaxis-rp-grid-51seg.nec` — X-axis dipole with theta/phi RP grid

**Purpose**: Validate that the RP path handles true multi-phi coverage on a geometry whose pattern is not invariant across the sampled azimuth cuts.

**Geometry**:
- Frequency: 14.2 MHz
- Wire: same canonical 51-segment half-wave dipole length as case 1, but rotated onto the x-axis
- Feed: Center segment (tag=1, seg=26), 1.0 V excitation
- Ground: None
- RP: `RP 0 5 4 0.0 0.0 45.0 90.0` (theta points `0°, 45°, 90°, 135°, 180°`; phi points `0°, 90°, 180°, 270°`)

**Expected results** (current regression gate):
- Same feedpoint impedance as `dipole-freesp-51seg`
- Z_in = 74.242874 + j13.899516 Ω
- Pattern table present with 20 points (`RADIATION_PATTERN`, `N_POINTS 20`)
- Numeric pattern samples locked in corpus validation across representative theta/phi combinations, including:
  - `θ=0°, φ=0°` → `GAIN_DB=2.1485`, `GAIN_V_DB=2.1485`, `GAIN_H_DB=-999.99`, `AXIAL_RATIO=0.0`
  - `θ=90°, φ=0°` → deep null (`GAIN_DB=-999.99`)
  - `θ=90°, φ=90°` → `GAIN_DB=2.1485`, `GAIN_V_DB=-999.99`, `GAIN_H_DB=2.1485`, `AXIAL_RATIO=0.0`

**Tolerance gates**:
- Same as `dipole-freesp-51seg` for impedance
- Pattern gain fields: ≤ 0.05 dB absolute on stored `GAIN_DB`, `GAIN_V_DB`, and `GAIN_H_DB` values
- Axial ratio: ≤ 0.0001 absolute on stored `AXIAL_RATIO` values
- External RP candidate gates: optional `ExternalGain_absolute_dB` / `ExternalAxialRatio_absolute` keys can additionally CI-gate `external_reference_candidate.pattern_samples` when present

**Why this case**: It proves the RP regression path across multiple phi cuts on a non-z-axis geometry, which is a stronger check than the azimuth-invariant baseline dipole.

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
- External impedance candidate gate (enabled in corpus JSON): `ExternalR_absolute_ohm=10.0`, `ExternalX_absolute_ohm=30.0`

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
- External impedance candidate gate (enabled in corpus JSON): `ExternalR_absolute_ohm=15.0`, `ExternalX_absolute_ohm=50.0`
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

### 7. `multi-source-gr-180.nec` — Dipole array generated via `GR`

**Purpose**: Validate that `GR` geometry expansion produces the same electrical result as an equivalent handwritten multi-wire deck.

**Geometry**:
- Frequency: 14.2 MHz
- Start with one vertical half-wave dipole centered at x = +0.5 m
- `GR 1 1 180.0` generates one additional copy by rotating 180 degrees about z, placing the second dipole at x = -0.5 m
- Both dipoles are center-fed at 1.0 V
- Ground: None

**Expected results** (current regression gate):
- Same feedpoint impedances as `multi-source.nec`
- Source 1: 152.352342 + j31.560296 Ω
- Source 2: 152.352339 + j31.560296 Ω

**Tolerance gates**: Same as `multi-source.nec`.

**Why this case**: It is a direct corpus-level check that parser + geometry-builder `GR` support is not only syntactically accepted, but electrically equivalent to an already validated explicit geometry.

### 8. `multi-source-gm-copy.nec` — Dipole array generated via `GM`

**Purpose**: Validate that the currently supported `GM` translated-copy subset produces the same electrical result as an equivalent handwritten multi-wire deck.

**Geometry**:
- Frequency: 14.2 MHz
- Start with one vertical half-wave dipole centered at x = 0 m
- `GM 1 1 0 0 0 1.0 0 0 1` appends one translated copy at x = +1.0 m with tag increment 1
- Both dipoles are center-fed at 1.0 V
- Ground: None

**Expected results** (current regression gate):
- Same feedpoint impedances as `multi-source.nec`
- Source 1: 152.352342 + j31.560296 Ω
- Source 2: 152.352339 + j31.560296 Ω

**Tolerance gates**: Same as `multi-source.nec`.

**Why this case**: It locks the currently implemented `GM` behavior into corpus validation and makes the supported subset explicit: one in-place transform or one appended transformed copy, not full unqualified NEC GM parity.

## Corpus metadata

| Case | Deck file | Segments | Wires | Sources | Ground | Reference Z_in (Ω) |
|:-----|:----------|:---------|:------|:--------|:-------|:------------------|
| 1 | dipole-freesp-51seg.nec | 51 | 1 | 1 | None | 74.24 + j13.90 |
| 1b | dipole-freesp-gm-inplace-shifted.nec | 51 | 1 | 1 | None | 74.24 + j13.90 |
| 1c | dipole-freesp-rp-51seg.nec | 51 | 1 | 1 | None | 74.24 + j13.90 |
| 1d | dipole-xaxis-rp-grid-51seg.nec | 51 | 1 | 1 | None | 74.24 + j13.90 |
| 2 | dipole-ground-51seg.nec | 51 | 1 | 1 | Perfect | 81.91 + j16.42 |
| 3 | yagi-5elm-51seg.nec | 51 | 5 | 1 | None | [TBD] |
| 4 | dipole-loaded.nec | ≈51 | 2 | 1 | None | [TBD] |
| 5 | frequency-sweep-dipole.nec | 51 | 1 | 1 (5× freq) | None | [TBD] × 5 |
| 6 | multi-source.nec | 51 | 2 | 2 | None | [TBD] × 2 |
| 7 | multi-source-gr-180.nec | 51 | 2 | 2 | None | 152.35 + j31.56 × 2 |
| 8 | multi-source-gm-copy.nec | 51 | 2 | 2 | None | 152.35 + j31.56 × 2 |

**Total**: 11 benchmark families, ≈19 individual frequency/source points.

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

# Optional: record the same number as an external_reference_candidate
scripts/import-reference-impedance.py \
  --case dipole-ground-51seg \
  --target external \
  --real 63.12 --imag -18.45 \
  --source "4nec2 (Wine 9.x)"
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
      "R_percent_rel": 0.1,
      "X_percent_rel": 0.1,
      "R_absolute_ohm": 0.05,
      "X_absolute_ohm": 0.05
    }
  },
  ...
}
```

## CI validation

On each commit, `cargo test -p nec-cli --test corpus_validation` runs fnec against corpus decks with captured references and compares results against `corpus/reference-results.json`. Any result exceeding the tolerance gate is a **CI failure** (not a warning).

## Status

- [x] 9 corpus deck families are present, including GM/GR equivalence regressions.
- [x] `corpus/reference-results.json` is populated with active regression values and tolerance gates.
- [x] Validation test suite is active (`apps/nec-cli/tests/corpus_validation.rs`) and CI workflow is wired (`.github/workflows/corpus-validation.yml`).
- [x] Active corpus validation currently passes in CI/local runs (with documented skips where references are intentionally absent).
- [ ] External-reference parity capture remains incomplete for several cases (notably loaded and some pattern/gain-oriented classes).
- [ ] Full Phase 1→2 parity gate remains open until external-reference coverage and deferred scope items are closed.
