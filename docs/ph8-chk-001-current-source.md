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

## Staged delivery

1. **Solve core (this increment)** — `solve_hallen_current_source` +
   `build_current_source_shape`, impedance-consistency validated.
2. **CLI wiring** — route EX type-4 decks on `--solver hallen` to the solve,
   report `Z = V/i0` in `FEEDPOINTS`; flip the `dipole-ex4` corpus/ex_cards
   contracts from "rejected" to the accept-path.
3. **Breadth** — multi-wire geometry; interaction with loads/TL.
