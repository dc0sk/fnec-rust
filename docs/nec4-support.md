---
project: fnec-rust
doc: docs/nec4-support.md
status: living
last_updated: 2026-04-28
---

# NEC-4 Support Boundary

This document explicitly defines which NEC-2/NEC-4 cards and features are supported in fnec-rust, which are partially supported (with caveats), and which are deferred to future phases.

**Goal**: Make the scope boundary transparent to users so there are no surprises about what is and isn't available in a given version.

## Support status definitions

| Status | Meaning |
|:-------|:--------|
| **FULL** | Fully implemented and tested. Behavior matches NEC-2/NEC-4 reference. |
| **PARTIAL** | Implemented with limitations (e.g., subset of options, caveats documented). |
| **EXPERIMENTAL** | Implemented but known to diverge from reference or have numerical issues. Not production-ready. |
| **DEFERRED** | Recognized but not yet implemented. Future phases or backlog. |
| **OUT OF SCOPE** | Will not be implemented in fnec-rust (design decision). Reason documented. |

## NEC card support matrix

### Structure cards

| Card | Description | Status | Notes |
|:-----|:------------|:-------|:------|
| CM | Comment | FULL | Parsed and ignored as per spec |
| CE | Comment end | FULL | Parsed and ignored |
| GW | Wire segment | FULL | Straight wire; segments, radius, endpoints fully supported |
| GE | Geometry end | FULL | Parsed; GE I1=1 infers PEC ground when no GN card is present. GE I1=-1 emits a below-ground warning; other unknown values warn with valid range hint; both fall back to free-space. |
| SP | Special segment | OUT OF SCOPE | Complex wire types (Taconite spheres, absorbers). Complex geometry patterns belong in CAD, not NEC deck. Consider import from external tool. |
| GM | Move segments | PARTIAL | Supported subset: rotate (Rx/Ry/Rz) and translate wire tag ranges. `tag_increment=0` modifies in place; `tag_increment>0` appends one transformed copy with incremented tags. Broader NEC GM semantics should be reviewed before claiming full parity. |
| GR | Repeat segments | PARTIAL | Supported subset: repeats existing wires by successive z-axis rotations. Each copy is rotated by a cumulative multiple of `angle_deg` with incremented tag numbers. Broader NEC GR semantics should be reviewed before claiming full parity. |
| GF | Scale segments | DEFERRED | Geometry scaling. Phase 2. |

### Excitation cards

| Card | Description | Status | Notes |
|:-----|:------------|:-------|:------|
| EX type 0 | Voltage source (voltage-driven dipole) | FULL | Supported at any segment, with complex voltage. Primary excitation type. |
| EX type 1 | Current source (magnetic dipole) | PARTIAL | `--solver pulse` now enforces EX type 1 as a driven-segment current source and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still use the staged portability fallback and emit the pending-semantics warning. |
| EX type 2 | Incident plane wave | PARTIAL | Accepted in parser/solver path as a staged portability fallback. Current runtime behavior treats EX type 2 like EX type 0 and emits a warning that incident-plane-wave semantics are pending. |
| EX type 3 | Normalized voltage source | PARTIAL | Accepted in parser/solver path. Default runtime mode treats EX type 3 like EX type 0 and warns on non-default I4; optional CLI mode `--ex3-i4-mode divide-by-i4` enables experimental I4-divisor normalization semantics. |
| EX type 4 | Segment current | PARTIAL | `--solver pulse` now enforces EX type 4 as a driven-segment current source and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still use the staged portability fallback and emit the pending-semantics warning. |
| EX type 5 | Electromagnetic current source (qdsrc) | PARTIAL | `--solver pulse` now enforces EX type 5 as a driven-segment current source and reports the resulting source voltage/impedance. Hallen and other non-pulse paths still use the staged portability fallback and emit the pending-semantics warning. |
| PT | Transmission line source | PARTIAL | Parsed for staged portability. Current runtime behavior emits an explicit deferred-support warning and ignores PT electrical semantics. |
| LD | Load impedance | PARTIAL | Types 0 (series RLC), 1 (parallel RLC), 2 (series RL), 3 (series RC), 4 (series Z), and 5 (distributed conductivity) are implemented. Other load types warn and are ignored. |

### Frequency and output cards

| Card | Description | Status | Notes |
|:-----|:------------|:-------|:------|
| FR | Frequency specification | FULL | Single frequency (NP=0, NF≥1). Frequency step (NF>1) supported. Step count respected. |
| RP | Radiation pattern request | PARTIAL | Parsed and executed in the CLI report path. `RADIATION_PATTERN` section is emitted with THETA/PHI and gain columns. Scope is currently text-report output only (no JSON/CSV/plot export, no near-field `XQ`). |
| XQ | Near/far field request | DEFERRED | Near-field analysis. Phase 2+. |

### Ground cards

| Card | Description | Status | Notes |
|:-----|:------------|:-------|:------|
| GN | Ground definition | PARTIAL | Type 1 (perfect conductor, z=0 PEC image method) supported: $$\text{GN type }1 \Rightarrow \text{PEC image method at } z=0$$. Other types currently emit a runtime warning and fall back to free-space. Finite-conductivity Sommerfeld/Norton variants are DEFERRED (Phase 2+). |
| EN | End of input | FULL | Terminates deck parse. |

### Advanced/specialized cards

| Card | Description | Status | Notes |
|:-----|:------------|:-------|:------|
| CP | Control program | OUT OF SCOPE | Procedural looping / iteration. Belongs in user scripts or CAD tool. |
| SY | Symbol definition | OUT OF SCOPE | Parametric expressions in deck. Use pre-processing / template tool instead. |
| TL | Transmission line (network) | PARTIAL | Initial executable subset: lossless TL (`type=0`, `NSEG>=0`, with `NSEG=0` treated as a single-section shorthand) contributes a 2-port impedance stamp to the matrix. Endpoint `segment=0` is mapped to the tag center segment with a warning; for even segment counts, the lower center segment is chosen deterministically. Unsupported TL variants emit runtime warnings and are ignored. |
| NT | Network definition | PARTIAL | Parsed for staged portability. Current runtime behavior emits an explicit deferred-support warning and ignores NT electrical semantics. |
| CH | Characteristic impedance | DEFERRED | Wire impedance tagging. Phase 2. |
| MA | Matériel (material) definition | DEFERRED | Lossy wire materials (copper, aluminum, etc.). Phase 2. |

### Control/metadata cards (NEC-4)

| Card | Description | Status | Notes |
|:-----|:------------|:-------|:------|
| NM | Program control (NEC-4) | DEFERRED | Version/control flags. Phase 2. |
| NE | Program end (NEC-4) | DEFERRED | Extension to EN. Phase 2. |

## Source type support matrix

| Source Type | Status | Notes |
|:------------|:-------|:------|
| Voltage excitation (EX 0) | FULL | Complex voltage at any segment. Primary production mode. |
| Current excitation (magnetic dipole) | PARTIAL | `--solver pulse` now implements first slices for EX 1 and EX 5 using driven-segment current constraints. Hallen and other non-pulse paths still map both to EX 0 with explicit runtime warnings while broader semantics remain deferred. |
| Plane wave incidence | PARTIAL | EX 2 is accepted as a staged portability fallback, currently mapped to EX 0 behavior with an explicit runtime warning; full scattering semantics remain deferred. |
| Segment-current excitation | PARTIAL | `--solver pulse` now implements a first EX 4 segment-current slice using a driven-segment current constraint. Hallen and other non-pulse paths still map EX 4 to EX 0 with an explicit runtime warning while broader semantics remain deferred. |
| Multi-port sources | PARTIAL | PT and NT are parsed for staged portability with explicit deferred-support warnings; electrical semantics remain deferred. |

## Solver mode support

| Mode | Status | Notes |
|:-----|:-------|:------|
| Hallén (augmented integral equation) | FULL | Validated: $$Z_{\mathrm{in}} \approx 74.24 + j\,13.90\,\Omega$$ vs Python reference. Production-ready. |
| Pocklington pulse basis | EXPERIMENTAL | Known divergence for thin-wire antennas. Do not use. Fixed by sinusoidal basis (Phase 2). |
| Pocklington continuity basis | EXPERIMENTAL | Rooftop basis transform. Same divergence issue. Phase 2. |

## Ground model support

| Model | Status | Notes |
|:-----|:-------|:------|
| Free space (no ground) | FULL | Baseline. No coupling to ground plane. |
| Perfect conductor (infinite, ideal) | PARTIAL | Implemented via image method. Phase 1. Sommerfeld corrections (Phase 2) for accuracy near ground. |
| Finite conductivity (Sommerfeld) | DEFERRED | Includes earth losses, frequency-dependent coupling. Phase 2. |
| Layered earth | DEFERRED | Multi-layer soil models. Phase 3. |
| Seawater effects | DEFERRED | Conductive media. Phase 3. |

## PAR-002 scoped ground parity plan

PAR-002 focuses on moving beyond PEC-only ground by adding a finite-conductivity near-ground subset that is externally validated and CI-gated.

Planned PAR-002 scope:

1. Add an explicit finite-conductivity GN subset in runtime behavior (while keeping unsupported GN variants as warning+fallback paths).
2. Add near-ground corpus fixtures that isolate finite-conductivity effects from unrelated feature changes.
3. Add tolerance-gated external-reference comparisons for those fixtures in `corpus/reference-results.json` and `apps/nec-cli/tests/corpus_validation.rs`.
4. Keep existing GN type 1 PEC behavior unchanged and regression-protected while finite-ground support expands.

PAR-002 non-goals for this slice:

1. Full layered-earth modeling.
2. Seawater-specific propagation models.
3. Broad near-field feature parity.

PAR-002 completion evidence (document-level):

1. `docs/corpus-validation-strategy.md` contains a finite-ground capture workflow and acceptance checklist.
2. Backlog PAR-002 entry includes concrete progress notes tied to implemented corpus and validation artifacts.
3. This support matrix remains synchronized with the exact GN subset actually implemented.

## Output format support

| Format | Status | Notes |
|:-----|:-------|:------|
| Text (4nec2-like) | FULL | Impedance per source, segment currents, residual diagnostics. Defined in Phase 1. |
| JSON | DEFERRED | Structured output for automation. Phase 2. |
| CSV | DEFERRED | Spreadsheet compatibility. Phase 2. |
| Plot data (pattern, current) | DEFERRED | gnuplot, matplotlib data. Phase 2. |

## Numerical feature support

| Feature | Status | Notes |
|:-----|:-------|:------|
| Complex impedance (R + jX) | FULL | All computations in complex domain. |
| Frequency sweep | FULL | Multiple frequencies in single deck (FR card with NF > 1). |
| Multi-source (multiple EX cards) | PARTIAL | Multiple `EX 0` sources on **distinct tags** are solved and reported as one feedpoint row per excited segment. Two sources on the **same tag** are rejected with a clear error in the Hallén path (unsupported; pulse path handles it without this restriction). Advanced multi-port/network features remain deferred to `PT`/`NT`. |
| Segment current calculation | FULL | Complex current per segment, phase and magnitude. |
| Feedpoint impedance | FULL | Computed via: $$Z_{\mathrm{in}} = \frac{V_{\mathrm{source}}}{I_{\mathrm{source}}} = R + jX$$ at driven segment. |
| Gain computation | PARTIAL | RP-driven far-field gain table is implemented in text report output (`GAIN_DB`, `GAIN_V_DB`, `GAIN_H_DB`). Structured exports and additional parity metrics remain deferred. |
| Radiation pattern | PARTIAL | `RP` cards now produce a `RADIATION_PATTERN` section in CLI text output. Near-field and external plot/export workflows remain deferred. |

## Phase progression

| Phase | Cumulative Support |
|:------|:------------------|
| Phase 1 (current) | GW, partial GM/GR, EX type 0, staged EX types 1/2/4/5 and EX3 mode-gated normalization path, FR, RP report-path support, GE, GN type 1 (PEC), LD types 0/1/2/3/4/5, TL subset (`type=0`, `NSEG>=0`, segment-0 center mapping), PT/NT staged parsing, Hallén solver, free space + perfect ground, text output, complex impedance, frequency sweep, multi-source reporting |
| Phase 2 | Add: GN finite-conductivity models (Sommerfeld), remaining LD load types, more advanced ground, EX types 1–4 (magnetic dipole, plane wave, normalized, multi-port), JSON/CSV output, sinusoidal Pocklington basis, GF (geometry scaling), richer pattern/gain parity |
| Phase 3 | Add: TL/NT (transmission lines), seawater effects, near-field analysis, advanced ground layering, plugin system integration |
| Phase 4+ | Additional NEC-4 specialty features as demanded |

## Compatibility with xnec2c vs 4nec2

fnec-rust is **4nec2-first**. The parser and solver primarily target 4nec2 compatibility.

| Tool | Dialect support |
|:-----|:----------------|
| 4nec2 | Primary (default) |
| xnec2c | Secondary (auto-detect or via flag). Subset of features from xnec2c accepted where they overlap with 4nec2; xnec2c-only features (e.g., custom Lua scripts) are out of scope. |
| NEC2 (FORTRAN reference) | Implicit (xnec2c is the working reference). |
| NEC4 (commercial) | Incremental adoption; Phase-gated. No guarantee of 100% parity. |

## Parsing robustness

| Aspect | Status |
|:-------|:-------|
| Unknown cards | FULL | Parsed as warnings; does not halt. User sees diagnostic. |
| Malformed cards | FULL | Validation errors printed; parse halts with clear message. |
| Numeric field errors | FULL | Type/bounds checking; fails with diagnostic. |
| Blank lines and comments | FULL | Gracefully skipped. |

## Known limitations and caveats

### Phase 1 known issues

1. **Pulse/continuity solver modes diverge** for thin-wire antennas. Use Hallén mode (`--solver hallen`) for production work. These modes are marked EXPERIMENTAL with a runtime warning.

2. **Perfect-ground scope only**: `GN 1` is implemented (PEC image method at z=0). Other GN variants currently warn and fall back to free-space. Finite-conductivity Sommerfeld/Norton models remain deferred to Phase 2+.

3. **Text output only**: No JSON, CSV, or plot data export yet. Scripting to post-process text output is the current workaround.

4. **Pattern/gain scope is partial**: `RP` cards are executed and rendered in text output, but JSON/CSV export and near-field parity remain deferred.

5. **No near fields**: Current output is limited to feedpoint impedance, segment currents, and residual diagnostics. Near-field analysis is deferred to Phase 2.

6. **Single frequency per run**: Frequency sweep works (multiple points), but each frequency is a separate line in output. Batch sweeps need external scripting or await Phase 2+ reporting improvements.

## Acceptance criteria for Phase 1 → Phase 2 gate

- [ ] This document (`docs/nec4-support.md`) is written and ratified.
- [ ] All FULL cards have integration test coverage.
- [ ] All PARTIAL cards have explicit documentation of limitations in this file.
- [ ] All DEFERRED cards have planned Phase assignment and backlog entry.
- [ ] Golden corpus passes all tolerance gates (BLK-001, BLK-003).
- [ ] Known limitations (`corpus/README.md`) align with this support matrix.
- [ ] User-facing error messages direct users to this document for unsupported features.

## References

- **NEC-2 Theory of Operation**: Burke & Poggio, LLNL 1981 (FORTRAN reference implementation)
- **xnec2c source**: https://github.com/KJ7LNW/xnec2c (working C reference)
- **4nec2 documentation**: https://www.4nec2.net/ (user-facing reference)
- `docs/requirements.md` — Tolerance matrix and numerical compatibility policy
- `docs/roadmap.md` — Phase definitions and deliverables
- `corpus/README.md` — Golden reference corpus cases and validation
