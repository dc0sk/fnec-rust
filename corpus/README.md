---
project: fnec-rust
doc: corpus/README.md
status: living
last_updated: 2026-08-31
---

# Golden Reference Corpus

**This corpus is primarily a regression gate, not an external-validation gate.** Most cases pin fnec's own output so it cannot change silently; a minority are additionally gated against an independent NEC engine. The distinction matters, so it is stated first and the numbers are counted rather than described (FND-110).

Of the 50 cases in `corpus/reference-results.json`:

<!-- CORPUS-TIER-COUNTS: checked by `the_readme_tier_counts_match_the_reference_data` -->

| Tier | Cases | What the gate proves |
|:-----|------:|:---------------------|
| Self-pinned regression only | 41 | the answer has not changed since it was pinned |
| Additionally gated against an external engine | 9 | the answer also agrees with an independent solver, within a stated absolute tolerance |
| **Total** | **50** | |

Of the 41 self-pinned rows, four are pinned to a value derived independently of fnec rather than to fnec's own output: `dipole-freesp-51seg` against a Python MoM script, and three `LD` rows re-derived against analytic values after FND-122. The rest record what the code produced.

These three counts are **derived from `reference-results.json` and checked**, not maintained by hand — a stale count here is the same failure as the stale claim this section replaced.

The external engines actually used are **nec2c 1.3.1** and **NEC2DXS500 via Wine** — not xnec2c, which an earlier version of this file named as the primary reference. xnec2c is not used by anything: it hangs headless in CI. 4nec2 is not used either.

The external tolerances are wide on purpose and are not a defect being hidden: fnec's Hallén formulation differs from nec2c systematically — a documented ~32 Ω reactance offset is present in free space too — so an absolute-X gate must exceed it. The tight gates are the self-pinned regression; the external numbers say the answer is the right *shape*.

**What this corpus therefore does and does not establish.** It establishes that fnec's results are stable and that a subset agrees with an independent implementation. It does **not** establish that every deck here has been checked against an external engine — 40 of them have not, and a self-pinned row is only as good as the code that produced it. Both criticals in the 2026-08-28 audit (FND-121, FND-122) shipped past this corpus for exactly that reason: fixtures had been chosen where the two candidate answers coincide.

`external_reference_candidate` is the key the validator reads. A case that declares an `External*` tolerance gate must carry one, and `every_external_gate_is_backed_by_a_readable_external_reference` fails the build otherwise — a gate stored under any other key is never evaluated, which is how one row advertised a nec2c parity check that had never run (FND-138).

CI runs `cargo test -p nec-cli --test corpus_validation` to hold every case to the tolerance matrix defined in `docs/requirements.md`.

**Not every `.nec` file here is a corpus case.** 56 decks are on disk against 46 named in `reference-results.json`; the rest are fixtures owned by named tests, and two are gated by nothing at all (FND-139). `corpus_deck_sanity.rs` checks that every deck in the directory at least carries a `GE` card, which is a syntax check and not a numerical one.

## Validation Framework

The corpus validation test (`apps/nec-cli/tests/corpus_validation.rs`) supports comprehensive accuracy gating across multiple output domains:

### 1. **Feedpoint Impedance** (all cases)
- Real and imaginary parts with absolute and relative tolerance gates
- Multi-source impedance validation (per-source gates for multi-wire scenarios)
- Frequency-sweep impedance validation (per-frequency gates)
- **Tolerance gates**: `R_absolute_ohm`, `X_absolute_ohm`, `R_percent_rel`, `X_percent_rel`

### 2. **Radiation Pattern Samples** (RP-enabled cases)
- Gain (linear, vertical, horizontal), axial ratio at specified (θ, φ) points
- Automatic extraction from fnec `RADIATION_PATTERN` section
- Comparison against reference `pattern_samples` array in `reference-results.json`
- **Tolerance gates**: `Gain_absolute_dB` (for gain fields), `AxialRatio_absolute`
- **External candidate gates** (optional): `ExternalGain_absolute_dB`, `ExternalAxialRatio_absolute`

### 3. **Current Distribution** (multi-wire and multi-segment cases — infrastructure in place)
- Segment current magnitude and phase at specified (wire_id, segment_id) points
- Automatic extraction from fnec CURRENTS section
- Support for multi-wire scenarios (Yagi arrays, parallel dipoles, etc.)
- **Reference data**: `current_samples` array (wire_id, segment_id, amplitude_db, phase_deg, description)
- **Tolerance gates**: `Current_amplitude_dB`, `Current_phase_deg` (values TBD per case)
- **Status**: Infrastructure implemented; reference values to be captured via measurement

### 4. **External Reference Candidates** (optional, per-case gating)
- Parallel validation against external solvers (nec2c, 4nec2, NEC2DXS500)
- Absolute and relative tolerance gates for impedance deltas
- Optional pattern-sample comparison when external RP data available
- **Tolerance gates**: `ExternalR_absolute_ohm`, `ExternalX_absolute_ohm`, `ExternalR_percent_rel`, `ExternalX_percent_rel`

## Tolerance Matrix

Default tolerance gates (overridable per case in `reference-results.json`):

| Domain | Gate Name | Default | Notes |
|--------|-----------|---------|-------|
| Feedpoint R | `R_absolute_ohm` | 0.05 Ω | Minimum acceptable; relative floor applied |
| Feedpoint X | `X_absolute_ohm` | 0.05 Ω | Minimum acceptable; relative floor applied |
| Feedpoint R | `R_percent_rel` | 0.1% | Relative tolerance relative to reference |
| Feedpoint X | `X_percent_rel` | 0.1% | Relative tolerance relative to reference |
| RP Gain | `Gain_absolute_dB` | 0.05 dB | Applies to all gain fields (linear, V, H) |
| RP Axial Ratio | `AxialRatio_absolute` | 0.0001 | Dimensionless (unitless) |
| Current Mag | `Current_amplitude_dB` | 0.1 dB | Per-segment current magnitude variance |
| Current Phase | `Current_phase_deg` | 2.0° | Per-segment current phase variance |

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

**Expected results** (from a Python MoM script, not from xnec2c — see this case's `reference_source`):
- Z_in ≈ 74.24 + j13.90 Ω
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
- Apply `GM 0 0 0 0 0 1.0 0 0 0` to translate the geometry by +1.0 m along x in place (NRPT=0 is NEC's in-place move; the card was `GM 0 1 ... 1` until FND-119, which NEC reads as one same-tag COPY — nec2c gives 102 segments for it, not 51, so the deck did not test what this entry claims)
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
- This case IS externally gated: `external_reference_candidate` carries nec2c 1.3.1 values and CI enforces `ExternalR_absolute_ohm` / `ExternalX_absolute_ohm` against them. (This line previously said capture was pending from xnec2c/4nec2, which had not been true since the nec2c values were captured.)
- CI currently gates the GN=1 regression value and prints external deltas when candidate values are present.

**Tolerance gates**: Same as dipole-freesp (R, X, current).
- External impedance candidate gate (enabled in corpus JSON): `ExternalR_absolute_ohm=10.0`, `ExternalX_absolute_ohm=30.0`

**Why this case**: Ground effects are critical for practical antennas. Validates GN=1 perfect-ground image-method behavior.

### 2b. `dipole-gn2-near-ground-51seg.nec` — Low dipole over finite-conductivity ground

**Purpose**: Validate the currently supported low above-ground `GN 2` near-ground class separately from buried-wire guardrails.

**Geometry**:
- Frequency: 14.2 MHz
- Wire: same canonical 51-segment half-wave dipole length as case 1, lowered so the wire spans `z=0.5 m` to `z=11.064 m`
- Feed: Center segment, 1.0 V
- Ground: `GN 2` with `EPSE=13.0`, `SIG=0.005 S/m`

**Expected results** (current regression gate):
- Z_in = 69.436745 + j16.705598 Ω
- No deferred-ground warning
- No buried-wire guardrail error

**Tolerance gates**: Same as the canonical impedance gates (`R_absolute_ohm`, `X_absolute_ohm`, `R_percent_rel`, `X_percent_rel`).

**Why this case**: It closes the PH2-CHK-002 supported-path gap by proving the active-ground runtime still accepts low above-ground geometry while buried active-ground geometry remains explicitly blocked.

### 3. `yagi-5elm-51seg.nec` — 5-element Yagi array

**Purpose**: Validate multi-wire geometry, mutual coupling, and array gain.

**Geometry**:
- Frequency: 14.2 MHz
- Driven element: L = 10.564 m (λ/2 dipole), a = 0.001 m, 51 segments
- Reflector: L = 10.8 m, spacing 0.2 m behind driven
- Directors: 3 × L = 10.3 m, spacing 0.2 m forward
- Feed: Driven element center, 1.0 V
- Ground: None

**Expected results** (expected shape; never captured from any engine — the gate is the self-pinned regression):
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

**Expected results** (expected shape; never captured from any engine — the gate is the self-pinned regression):
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

**Expected results** (expected shape; never captured from any engine — the gate is the self-pinned regression):
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

**Expected results** (expected shape; never captured from any engine — the gate is the self-pinned regression):
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

**Why this case**: It locks `GM` translated-copy behaviour into corpus validation. Note it cannot discriminate the FND-119 defect on its own: with `NRPT = 1` the old last-tag reading and the NEC reading happen to agree. `dipole-gm-nrpt2-freesp.nec` and `crates/nec_solver/tests/gm_nec2c.rs` cover the cases that separate them.

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

What was actually used for the nine externally gated cases: **nec2c 1.3.1** on Arch Linux, on a copy of the deck with `XQ` inserted, and **NEC2DXS500** under Wine.

```bash
nec2c -i /tmp/deck-with-xq.nec -o /tmp/deck.out
```

xnec2c is listed below for completeness and is **not** the route to use — it hangs in headless CI, which is why nothing in this repo depends on it:

```bash
xnec2c --batch -j0 -i corpus/dipole-freesp-51seg.nec --write-csv .tmp-work/dipole-freesp.csv
```

4nec2 (under Wine or a Windows VM) is not used by any current case:

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
