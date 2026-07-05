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
| **single 52-seg `GW` wire** (same physical dipole) | ~centre (seg 27) | **74.41 + j14.52** | ✅ target |
| collinear chain, **merged endpoint block, no junction** | at the junction | −76 − j1900 | ❌ (rules out the grouping) |

`--solver pulse` / `continuity` / `sinusoidal` all fail these too (different wrong
numbers, same conclusion).

## Root cause (verified by controlled experiment)

Hypotheses were formed and then **tested**; two were falsified:

- **Rejected A** (source-at-junction-endpoint only): feeding *away* from the
  junction on a two-wire dipole is also wrong.
- **Rejected B** (collinear-specific): a 15°-*bent* two-wire dipole fails
  identically.
- **Rejected C** (the wire grouping / current-continuity constraint): solving the
  collinear chain with the two wires **merged into one endpoint block and no
  junction constraint** still gives garbage (−76 − j1900), so the
  `I[a] + sign·I[b] = 0` machinery is *not* the cause.

**Confirmed mechanism.** The distinguishing experiment: the identical physical
dipole modeled as a **single 52-segment `GW` wire** solves correctly
(74.41 + j14.52 Ω), while the two-wire chain does not — even with merged grouping.
The only remaining difference is the **Hallén homogeneous solution**. fnec builds
its along-wire coordinate for `cos_vec` (the `cos(k·s)` homogeneous basis)
**per `GW` wire**, so `s` resets to 0 at every wire start, and it assigns an
**independent homogeneous constant `C_k` per wire** (`linear.rs:556`). Across a
junction the homogeneous basis is therefore *discontinuous* (a phase reset in
`cos(k·s)`) and its constant is uncoupled. For a single physical conductor spanning
a junction this is simply the wrong basis, and the least-squares solve returns a
self-consistent but unphysical current (negative resistance). The error is largest
where the junction current is largest (centre feeds), which is why low-current
loading junctions (`dipole-loaded`'s top-hat) look fine.

This is a formulation gap in the Hallén homogeneous solution, not the
current-continuity constraint and not a tolerance issue — it reproduces exactly
across solvers and mesh densities.

## Practical impact

Real centre-fed bent/branched antennas — inverted-V, folded dipole, T/gamma
matches, delta loops — put a **high-current junction at or near the feed**, so
their feedpoint impedance is currently unreliable. Antennas whose junctions are
end-loading or parasitic (top-hats, Yagi directors/reflectors as separate wires)
are unaffected.

## Fix plan (redirected by the confirmed mechanism)

The fix must make the **Hallén homogeneous solution continuous across junctions** —
not add a current/charge constraint (the current-continuity machinery is already
present and is not the cause).

1. **Junction-continuous homogeneous basis (collinear first).** Compute the
   `cos(k·s)` along-wire coordinate `s` with a *path-continuous* arc length across
   connected wires, and share one homogeneous constant per *connected component*
   rather than per `GW` wire. For a collinear chain this is exactly the single-wire
   basis and should recover 74 + j14 Ω — directly validatable. This is the
   smallest correct step and touches `build_hallen_rhs` (`cos_vec`) and the
   per-wire `C_k` assignment in `solve_hallen`.
2. **General junction basis functions (NEC-standard).** For non-collinear branches
   (bends, T/Y, stepped), basis functions spanning each junction node with the
   source as an interior excitation. Largest effort; the general answer.

Validate every increment against: the **single-wire reference** (split straight
dipole must recover 74.41 + j14.52 Ω), **nec2c** on an inverted-V, and the
already-passing **`dipole-loaded`** (must not regress). The key regression risk is
`cos_vec`/`C_k` changes leaking into single-wire and low-current-junction solves —
gate those explicitly.

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
