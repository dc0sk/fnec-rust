---
project: fnec-rust
doc: docs/ph9-chk-005-junction-feed-guardrail.md
status: living
last_updated: 2026-07-04
---

# PH9-CHK-005: difficult-geometry accuracy — the junction-fed limitation

## Requirement / change

Roadmap `PH9-CHK-005` (PRT-008/009/010): a difficult-geometry accuracy program.
The concrete, high-value finding that emerged: **feeding a segment that sits at a
wire junction produces an unphysical feedpoint impedance in fnec**, silently. This
increment characterizes and *guards* that limitation.

## The finding (demonstrated)

A straight half-wave dipole was split into two collinear `GW` wires joined at the
origin and fed at that junction:

```
GW 1 26 0 0 0  0 0  5.282  0.001
GW 2 26 0 0 0  0 0 -5.282  0.001
EX 0 1 1 0 1.0 0.0        ← feed at the junction segment
```

This is **physically identical** to the reference single-wire dipole
(74.24 + j13.90 Ω), yet fnec reports **−34.49 − j1447 Ω** — a *negative* resistance,
which is impossible for a passive antenna.

### Why

At a wire junction the feed current splits across the joined wires (Kirchhoff). The
driven segment carries only part of the total feed current, so the per-segment
`Z = V/I` is not the true feedpoint impedance and can go unphysical. fnec's Hallén
path enforces junction *continuity* (for the currents) but does not yet model a
*source at a junction* correctly — that is the accurate-junction-fed-impedance work
scoped as **PH9-CHK-002**. (The same limitation makes bent/junction-fed antennas
like the inverted-V unreliable, and is why the initial difficult-geometry corpus
candidates were withdrawn — they were all junction-fed.)

## The guardrail

`nec-cli::solve_session::warn_if_feedpoint_at_junction` — when a voltage/current
source drives a segment whose node is a wire junction (`detect_wire_junctions`),
the CLI emits:

> warning: feedpoint at tag T segment S is on a wire junction; the feed current
> splits across the joined wires, so the reported impedance (V/I on one segment) is
> not accurate and may be unphysical (junction-fed impedance is deferred — see
> PH9-CHK-002)

The result is still printed (the currents away from the feed are still meaningful),
but the impedance is no longer silently presented as trustworthy. A feed *away*
from a junction, or a single-wire geometry, does not warn. (Note: a junction at a
*high-current* region can still be mis-solved even when fed elsewhere — see the
PH9-CHK-002 diagnosis; the junction-*fed* warning is a deliberately conservative
signal, not a complete junction-accuracy check.)

## Complementary post-solve check: negative resistance

The junction-*fed* warning is a pre-solve check on the feed location, so it misses
a genuinely mis-solved junction geometry that happens to be fed *away* from the
junction (e.g. a bent dipole fed mid-arm still yields a nonsense impedance).
`warn_if_negative_resistance` closes that gap after the solve: **a passive antenna
cannot have a negative input resistance**, so a negative `Re(Z)` on the Hallén path
is a reliable, general signal that the result is unphysical (in practice a
junctioned-geometry limitation — see PH9-CHK-002). It is scoped to `--solver hallen`
because the pulse current-source path has documented negative-`R` corpus values.
Together the two checks cover both junction-fed and fed-away failure modes without
false-warning on any valid geometry (all passing corpus/reference cases on the
Hallén path have `Re(Z) > 0`).

## Validation (`apps/nec-cli/tests/junction_feedpoint.rs`)

- **Junction-fed warns** — the split-dipole-fed-at-junction deck warns and names
  PH9-CHK-002.
- **Fed away from junction does not warn** — the same two wires fed on segment 13
  (mid-wire) produce no junction warning.
- **Single-wire does not warn** — the ordinary dipole produces no junction warning.

## Test results

`cargo test --workspace`: **571 passed**, 0 failed (was 568; +3 junction tests);
clippy clean. No existing corpus case feeds at a junction, so none newly warns.

## Follow-on

Accurate junction-fed feedpoint impedance (summing the split current at the
junction, or a proper junction source model) is **PH9-CHK-002**. This increment
makes the limitation visible; PH9-CHK-002 will remove it.
