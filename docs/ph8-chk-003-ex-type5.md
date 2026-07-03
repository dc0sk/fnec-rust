---
project: fnec-rust
doc: docs/ph8-chk-003-ex-type5.md
status: living
last_updated: 2026-07-03
---

# PH8-CHK-003: EX type 5 (voltage source, current-slope discontinuity)

## Requirement / change

Roadmap `PH8-CHK-003` (CP-003, PRT-002): implement EX type 5 runtime semantics so
real 4nec2 decks that use it run instead of failing fast.

## What EX type 5 is

In NEC2, EX type 5 is a **voltage source** applied by the *current-slope
discontinuity* method — an alternative numerical model to type 0's *applied-field*
(delta-gap) method. Both impress a voltage at the source segment; they differ only
in how the source region is discretized.

## Decision: model type 5 as a voltage source via the applied-field method

fnec's Hallén formulation implements the **applied-field** voltage source (type 0).
EX type 5 is therefore modelled as a voltage source through the **same** path, so
its feedpoint impedance equals type 0's.

**Honesty note.** NEC's separate current-slope-discontinuity numerics are *not*
reproduced: on the reference dipole, `nec2c` gives type 0 = 79.35 + j46.22 Ω and
type 5 = 85.01 + j49.22 Ω — a ~6 % difference from the source-region model. fnec
gives the type-0 value for both. This is within fnec's general Hallén-vs-NEC
accuracy envelope (the operator itself differs from `nec2c` by more than this on
many geometries — see `docs/ph8-chk-002-plane-wave-excitation.md`) and is the
right trade for **deck portability** (CP-003): a type-5 deck now runs and gives a
sound voltage-source impedance, rather than failing. The residual current-slope
refinement is a documented limitation.

## Implementation

- `nec_model::card`: `ExcitationKind::is_voltage_source()` = `VoltageSource`
  (type 0) or `VoltageSourceCurrentSlope` (type 5).
- `nec_solver::excitation`: `build_excitation` / `build_hallen_rhs` accept both
  voltage-source types (were type-0-only). Unknown types (≥ 6) still error.
- Type 5 flows through the existing feedpoint/report path unchanged; it solves on
  both `--solver hallen` and `--solver pulse` (like type 0).

## Validation

- **`ex_cards.rs`** — EX type 5 feedpoint impedance equals type 0's to < 1e-3 Ω
  on the reference dipole (`--solver hallen`); type 5 also solves under
  `--solver pulse`.
- **Corpus** — `dipole-ex5-freesp-51seg` now solves (74.23 + j13.9, == type 0);
  `dipole-ex5-pulse-current-freesp-51seg` solves under pulse (−345.6 − j988.0,
  == type 0 pulse). Both had their "is not yet supported" contracts removed.
- The PAR-003 checklist no longer lists the EX5 pulse case as a must-error case
  (only EX1, which is a plane wave rejected under pulse).

## Test results

`cargo test --workspace`: **557 passed**, 0 failed; clippy clean.
`docs/card-support-matrix.md` EX type 5 → **Partial**.

## Remaining

The current-slope-discontinuity source model (for exact NEC type-5 parity) is a
documented non-goal for the Hallén formulation.
