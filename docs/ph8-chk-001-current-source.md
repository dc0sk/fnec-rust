---
project: fnec-rust
doc: docs/ph8-chk-001-current-source.md
status: living
last_updated: 2026-07-02
---

# PH8-CHK-001: current-source excitation (NEC2 EX type 4)

## Requirement / change

Roadmap `PH8-CHK-001` (CP-003, PRT-002): drive a specified segment with a fixed
**current** and report the resulting feedpoint voltage / impedance, on the Hallén
solver path. Under the NEC2 EX-type alignment (PH8-CHK-002 foundation) the current
source is **EX type 4** (the roadmap's original "type 1" predates the alignment).

## Design

A current source is the exact **dual** of the delta-gap voltage source. The
voltage-source Hallén equation is

```
Z·I − C·cos(k·s) = g·V         (endpoints: I = 0)
```

where `g` is the unit-voltage source shape (`build_hallen_rhs` with `V = 1`) and
`V` is a **known** driving voltage. For a current source the roles swap: the
current at the source segment is **known** (`I[src] = i0`) and the port voltage
`V` is **unknown**. So `V` becomes an extra solve column and `I[src] = i0` an
extra constraint:

- rows `0..N`:  `Z·I − C·cos − g·V = 0`
- endpoint rows: `I = 0` at each wire end
- source row:    `I[src] = i0`

Solving yields the segment currents and the port voltage `V`; the feedpoint
impedance is `Z = V / i0`.

The endpoint and source rows are **exact constraints**, not least-squares
observations, so they are weighted heavily (`1e6`) in the normal-equations solve.
This pins the forced current exactly and enforces zero end-current; without it the
single inhomogeneous constraint that sets the whole solution scale is slightly
under-satisfied and the impedance drifts ~0.3 %.

Implementation:
- `nec_solver::build_current_source_shape` — synthesizes a `V = 1` delta-gap at
  the source segment and returns `g` + `cos_vec` (reusing `build_hallen_rhs`).
- `nec_solver::solve_hallen_current_source` — the augmented solve above; returns
  `CurrentSourceSolution { currents, port_voltage }`.

Supported class: a single straight wire (one source shape, one homogeneous
constant). This is the solve core; CLI wiring + the EX type-4 report path are a
follow-on increment (mirroring the plane-wave staging).

## Validation

Internal **impedance-consistency** gate — no external reference needed. The port
impedance `Z = V/I` is a property of the antenna, independent of how it is driven,
so the current-source feedpoint impedance must equal the voltage-source impedance
at the same feed. `crates/nec_solver/tests/current_source.rs`:

1. **Center-fed λ/2 dipole (51 seg)** — `Z(current) = 74.228 + j13.897` vs
   `Z(voltage) = 74.243 + j13.900`; rel **2×10⁻⁴**. Forced feed current exactly
   `1.0`.
2. **Linearity** — doubling `i0` doubles all currents and the port voltage,
   impedance unchanged (rel < 1×10⁻⁹).
3. **Off-center feed (seg 18)** — consistency holds (rel ~9×10⁻⁴).

Gate tolerance 5×10⁻³ (≈5× margin over the measured ~0.02–0.09 % agreement; the
residual is the numerical difference between the two augmented-system
formulations — the current-source path enforces `I=0` at the ends exactly, which
`solve_hallen` leaves as a soft least-squares constraint).

## Test results

`cargo test --workspace`: **547 passed**, 0 failed (was 544; +3 current-source
tests); clippy clean. The shared voltage-source `solve_hallen` path is untouched.

## Increment 2 — CLI wiring (2026-07-02)

Current-source decks are now user-runnable end to end.

- **Routing** (`nec-cli::solve_session`): the Hallén path detects a current-source
  EX card (`deck_has_current_source`) and routes the single-straight-wire class
  through `solve_current_source_hallen` (`build_current_source_shape` +
  `solve_hallen_current_source`). Diag label `hallen-current-source`.
- **Report**: `build_feedpoint_rows` uses the solved port voltage for a
  current-source card, so `FEEDPOINTS` shows `V=V_port`, `I=i0`, `Z=V/i0`.
- **Delta-gap builders** (`build_excitation`, `build_hallen_rhs`) now treat the
  current source as *not a delta-gap source* (skip); type 5 still errors.
- **Fail-fast kept**: multi-wire current source and non-Hallén solvers fail fast
  ("multi-wire current source is not yet supported" / "requires --solver hallen").

**Contract updates**: `ex_cards.rs`/`parser_warnings.rs` type-4 tests flipped from
"rejected" to the accept-path. Corpus `dipole-ex4-freesp-51seg` now **solves** and
is validated against the dipole feedpoint impedance (74.23+j13.9 — the same value
the current-source path is internally consistent with); the pulse variant's error
contract became "requires --solver hallen".

Manual CLI check: `EX 4 1 26 0 1.0 0.0` on `--solver hallen` →
`FEEDPOINTS: V=74.228+j13.897, I=1.0, Z=74.228+j13.897`.

Results: `cargo test --workspace` **547 passed**, 0 failed; clippy clean.
`docs/card-support-matrix.md` EX type 4 → **Partial**.

## Staged delivery

1. **Solve core** (#260) — `solve_hallen_current_source` +
   `build_current_source_shape`, impedance-consistency validated.
2. **CLI wiring** (this increment) — routing + `FEEDPOINTS` report + corpus/test
   accept-path flip.
3. **Breadth** — multi-wire geometry; interaction with loads/TL.
