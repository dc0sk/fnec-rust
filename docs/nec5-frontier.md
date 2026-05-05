---
project: fnec-rust
doc: docs/nec5-frontier.md
status: living
last_updated: 2026-05-05
---

# NEC-5 Accuracy Frontier

## Purpose

This document records the architecture decision for how fnec-rust addresses the
NEC-5-class accuracy frontier: whether to remain wire-only, pursue mixed-potential
surface capability, or take a hybrid route. It also defines the corpus expansion
plan to cover difficult-geometry cases beyond the current PH2N5 matrix.

## Context

NEC-5 extends NEC-4 with:

1. **Mixed-potential EFIE** for general wire-surface junctions (replaces the thin-wire
   kernel for any segment that abuts a surface patch).
2. **Patch (surface) elements** — triangular or quadrilateral RWG basis on arbitrary
   metallic surfaces.
3. **Improved ground formulations** — enhanced Sommerfeld integrals, dielectric
   substrates, and thin-slab approximations.
4. **Extended near-field** and current-density output on surfaces.

The NEC-5 Validation Manual defines scenario classes that expose these capabilities;
PH2-CHK-007 already maps the wire-only classes (`PH2N5-001..010`) to corpus coverage.
Classes PH2N5-009 and PH2N5-010 (surface meshing; monopole on finite box) were
declared **out-of-scope** because they require patch elements not present in
NEC-2/NEC-4 wire-only solvers.

## Decision

**fnec-rust will remain wire-only in its primary solver path.**

Surface meshing (patch elements, RWG basis, wire-surface junctions) will **not** be
implemented in the main `nec_solver` crate.

### Rationale

1. **User base is wire-antenna focused.** The practical use cases driving this project
   are dipoles, Yagis, log-periodics, loops, phased arrays, and loaded antennas —
   all wire-only. No user request for surface meshing has been recorded.

2. **Wire-only already covers the vast majority of NEC-5 validation classes.** Classes
   PH2N5-001 through PH2N5-008 (thin-wire kernel, source models, convergence,
   wires over ground, loops, lumped loads, transmission lines, and PT/NT portability)
   are all covered and CI-gated. Only PH2N5-009 and PH2N5-010 require surfaces.

3. **Surface capability is a multi-year effort.** Full RWG surface meshing plus
   Sommerfeld-integral ground coupling is comparable in scope to building the entire
   current solver from scratch again. The cost-to-benefit ratio for the target user
   base does not justify it.

4. **Embeddability and automation stay the focus.** The architecture goal is to reach
   necpp-style embeddability for wire-antenna research and automation (COMP-012),
   which does not require surface patches.

5. **The hybrid route is the only viable open-ended path.** If surface support is
   ever needed, the correct approach is a **plug-in or external solver bridge** (e.g.,
   calling an existing surface solver and ingesting its results), not embedding full
   RWG in the core. This keeps the `nec_solver` crate focused and testable.

### Wire-only continuation plan

The wire-only path will be deepened across these axes:

| Axis | Target |
|:-----|:-------|
| Kernel accuracy | Pocklington EFIE with exact thin-wire kernel (sinusoidal basis, NEC2-style); sinusoidal mode promoted from experimental to production in PH6-CHK-003 (done). |
| Ground models | Sommerfeld-integral near-field ground (NEC-4-class GN2 near-ground already implemented; buried-wire deferred). |
| Wire-junction accuracy | Multi-wire junctions with correct current continuity enforced at junction nodes (available in Hallen solver; further tightened by continuity basis). |
| Difficult geometries | Corpus expansion to include bent/folded wires, close parallel wires, and loading-stressed geometries (see PH6N5 rows below). |
| Convergence tracking | Explicit convergence-vs-segment-count corpus gates for the reference dipole family. |

### Surface scope boundary (explicit)

The following capabilities are **out of scope for fnec-rust** and will not be
scheduled in any roadmap phase unless the decision is revisited by an explicit
steering document:

- Patch (surface) elements (triangular or quadrilateral RWG)
- Wire-surface junction elements
- Volume-element formulations
- Mixed-potential EFIE on surfaces
- Dielectric slab or substrate Green's functions

If a contributor wishes to pursue surface capability, the correct path is a
`nec_surface` crate that implements an external solver bridge, calling an existing
validated surface solver (e.g., openEMS, FEKO, Method-of-Moments via an open
interface) and converting results to fnec-rust's report format. That crate would
not touch `nec_solver`.

## Corpus expansion plan — PH6N5 cases

Three new difficult-geometry corpus cases are defined below to push wire-only
accuracy beyond the baseline dipole family. These cases expose current-distribution
sensitivity, inter-element coupling, and segment-aspect-ratio stress.

### PH6N5-001: Bent-wire (V-dipole) feedpoint impedance

**Physics target:** Bent wire where the two arms form a 120° included angle. Tests
whether the Hallen solver correctly handles non-collinear two-segment wires at a
junction node; specifically, that the cos(k|z|) homogeneous-term projection along
each arm's own axis gives physically correct current distribution at the bend.

**Geometry:** Two 5.282 m arms at 120° included angle (each tilted 30° from
vertical), common feed at the apex, 14.2 MHz, free space, 51 segments per arm,
wire radius 0.001 m.

**Reference value:** nec2c — to be captured during fixture creation.

**Corpus case ID:** `vdipole-bent-120deg-51seg`

**Status:** pending fixture + capture

### PH6N5-002: Close parallel dipoles (coupling stress)

**Physics target:** Two parallel half-wave dipoles separated by λ/20 (≈ 1.06 m at
14.2 MHz), driven in phase. Tests near-field inter-element coupling accuracy and
whether the Z-matrix off-diagonal terms between the two wires are correct.

**Geometry:** Two 10.564 m dipoles, 1.06 m apart (x-separation), both z-aligned,
centre-fed, same frequency 14.2 MHz, free space, 51 segments each.

**Reference value:** nec2c on both feedpoint impedances — to be captured during
fixture creation.

**Corpus case ID:** `coupled-parallel-dipoles-51seg`

**Status:** pending fixture + capture

### PH6N5-003: Loaded Yagi with element coupling (stress geometry)

**Physics target:** 3-element Yagi (reflector, driver, director) with a lumped
inductive load on the reflector. Tests simultaneous coupling between three collinear
but distinct-tag wires, correct EX routing to the driver only, and LD application
on a passive element.

**Geometry:** Standard 3-element Yagi optimised for 14.2 MHz (reflector 10.9 m,
driver 10.1 m, director 9.6 m), 21 segments each, 0.001 m radius; LD type 0
inductive load (XL = 150 Ω) at centre of reflector.

**Reference value:** nec2c driver feedpoint impedance and reflector centre current
— to be captured during fixture creation.

**Corpus case ID:** `yagi-3elm-loaded-reflector-21seg`

**Status:** pending fixture + capture

## Traceability — PH6-CHK-002 matrix rows

These rows will be appended to `docs/corpus-validation-strategy.md` under a new
`PH6-CHK-002 traceability matrix` section once the corpus fixtures are created.

| Row ID | NEC-5 validation class | Status | Corpus case IDs |
|:-------|:------------------------|:-------|:----------------|
| PH6N5-001 | Non-collinear wire junction (bent/V-dipole) | pending | `vdipole-bent-120deg-51seg` |
| PH6N5-002 | Near-field inter-element coupling (close parallel dipoles) | pending | `coupled-parallel-dipoles-51seg` |
| PH6N5-003 | Loaded passive element in multi-wire Yagi | pending | `yagi-3elm-loaded-reflector-21seg` |

## Review

This decision should be reviewed if:

- A funded use case requires wire-surface junctions (e.g., an antenna mounted on a
  metallic ground plane modelled as a patch mesh).
- An open-source RWG solver becomes available under a GPL-3.0-compatible licence
  and the bridge cost is low.
- The NEC-5 source code is released under an open licence that makes direct
  implementation practical.

Any reversal of the wire-only decision must produce a revised version of this
document with explicit rationale and a phased implementation plan before any
surface code is written.
