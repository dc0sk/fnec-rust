---
project: fnec-rust
doc: docs/ph9-chk-002-junction-feed-diagnosis.md
status: living
last_updated: 2026-07-04
---

# PH9-CHK-002: multi-wire junction accuracy — root-cause diagnosis & fix plan

## Status

**Diagnosed and scoped; fix deferred to a dedicated effort.** PH9-CHK-005 made the
junction-*fed* case warn; this document records what was empirically established
about *why* junctioned multi-wire geometry is mis-solved, so the reformulation can
be implemented deliberately. The investigation corrected an initial mis-hypothesis
(below) — everything here is measured, not assumed.

## What was measured (14.2 MHz, `--solver hallen`, all values Ω)

| Geometry | Feed | fnec Z | Physical? |
|:---------|:-----|:-------|:----------|
| single-wire λ/2 dipole | centre (seg 26/51) | 74.24 + j13.90 | ✅ reference |
| single-wire λ/2 dipole | off-centre (seg 13/51) | 155.2 + j17.3 | ✅ |
| `dipole-loaded` (5 wires: dipole + top-hat) | centre of the main wire | 12.39 − j918.1 | ✅ (junction at the **low-current tip**) |
| λ/2 dipole split into 2 collinear wires | at the centre junction | −34.5 − j1447 | ❌ negative R |
| collinear end-to-start chain | at the centre junction | −34.5 − j1447 | ❌ (identical) |
| 2-wire dipole bent 15° | off-centre (seg 13), away from bend | −0.04 − j887 | ❌ |
| 2-wire dipole bent 15° | at the apex junction | −34.5 − j1447 | ❌ |

`--solver pulse` / `continuity` / `sinusoidal` all fail these too (different wrong
numbers, same conclusion).

## Corrected root cause

- **Rejected hypothesis A** (source-at-junction-endpoint): feeding *away* from the
  junction on a two-wire-dipole is also wrong, so it is not only the source.
- **Rejected hypothesis B** (collinear junctions specifically): a 15°-*bent*
  two-wire dipole fails identically, so it is not collinearity.
- **Supported cause**: fnec's junction handling enforces **current continuity**
  (`I[seg_a] + sign·I[seg_b] = 0`) but not **charge / current-derivative
  continuity** across the node. That approximation is fine when the junction
  carries *low current* — `dipole-loaded`'s top-hat sits at the dipole's tip
  (a current node) and solves correctly — but it is grossly wrong when a
  *high-current* junction is required, e.g. a dipole **split or bent at its centre
  feed**, where the current is at its maximum. The error scales with the junction
  current, which is why low-current loading junctions work and centre junctions
  do not.

This is a formulation gap (it reproduces exactly across solvers and mesh
densities), not a tolerance issue.

## Practical impact

Real centre-fed bent/branched antennas — inverted-V, folded dipole, T/gamma
matches, delta loops — put a **high-current junction at or near the feed**, so
their feedpoint impedance is currently unreliable. Antennas whose junctions are
end-loading or parasitic (top-hats, Yagi directors/reflectors as separate wires)
are unaffected.

## Fix plan

1. **Junction basis functions (NEC-standard, general).** Basis functions spanning
   each junction node enforce *both* current and charge continuity, with a source
   at the node handled as an interior excitation of the spanning function. Correct
   for all cases (centre-fed bends, T/Y, stepped). Largest effort; the right
   long-term answer.
2. **Charge-continuity constraint (incremental).** Add the missing derivative
   condition at each junction to the existing continuity system before attempting
   the full basis rework — may recover high-current junctions at lower risk.
   Validate against nec2c on an inverted-V and against the single-wire reference
   for a split straight dipole.

Validate every increment against: the single-wire reference (split straight dipole
must recover 74 + j14 Ω), nec2c on an inverted-V, and the already-passing
`dipole-loaded` (must not regress).

## Guardrail in place

`nec-cli::solve_session::warn_if_feedpoint_at_junction` (PH9-CHK-005) warns on a
junction-*fed* feedpoint. It is deliberately conservative: it does not warn on a
valid low-current loading junction (e.g. `dipole-loaded`), but consequently does
not catch the rarer "fed away from a high-current junction" case. Broadening it
reliably needs the current-magnitude information the fix itself will produce.

## Reproduction

```
# split straight dipole, fed at the centre junction — must become 74 + j14 when fixed
GW 1 26 0 0 0  0 0  5.282  0.001
GW 2 26 0 0 0  0 0 -5.282  0.001
EX 0 1 1 0 1.0 0.0
FR 0 1 0 0 14.2 0.0
```
