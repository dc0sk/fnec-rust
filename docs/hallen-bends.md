---
project: fnec-rust
doc: docs/hallen-bends.md
status: living
last_updated: 2026-09-27
---

# Hallén on bent conductors (FND-162)

Hallén's conductor-path basis has no condition at a bend. A bent conductor
therefore solves to a wrong answer, and it did so with no warning. Since
2026-09-27, every frontend's Hallén path warns
(`validate::bent_conductor_warning`) and points to `--solver mpie`, which models
the bend. This note records what was measured, what was tried, and what remains.

## Measured (14.2 MHz, arms of 21 and 41 segments)

| deck | nec2c | Hallén | MPIE |
|---|---|---|---|
| inverted-V, fed next to the apex (21) | 86.786 + j197.230 | 87.710 + j197.997 | 84.765 + j193.431 |
| inverted-V, fed next to the apex (41) | 87.501 + j198.130 | 85.312 + j193.519 | 85.830 + j194.788 |
| inverted-V, fed ¼ up an arm (21) | 3578.6 − j1727.0 | 2311.3 − j2544.0 | 3682.1 − j2112.2 |
| inverted-V, fed ¼ up an arm (41) | 3572.5 − j959.3 | 2747.6 − j2168.6 | 3678.6 − j1113.1 |
| 90° L, fed mid-arm (21) | 60.537 − j118.240 | 84.965 − j50.208 | 62.672 − j123.367 |
| 90° L, fed mid-arm (41) | 60.385 − j117.700 | 85.589 − j46.700 | 61.471 − j120.347 |

Hallén is close with the feed at the apex and far off away from it; MPIE tracks
nec2c throughout. (Before FND-167 the end-to-start spelling of the first deck
solved to −20.9 − j1274.6; that was a routing bug, not this limitation.)

## Tried: stage 1 of the reviewed design — no gain

The design (reviewed by Fable, 2026-09-27, "sound with changes") gives each
straight section of a path its own homogeneous `(C, D)` and closes every bend
with two rows. One row makes the path current continuous (both sides
extrapolated to the node). The other sets equal scalar potential,
`∂A_s/∂s` equal on both sides, where the delta-gap source term cancels.

It was built and measured, and it did not help:

| deck | nec2c | before | stage 1 |
|---|---|---|---|
| inverted-V, apex (21) | 86.786 + j197.230 | 87.710 + j197.997 | 85.850 + j196.161 |
| inverted-V, apex (41) | 87.501 + j198.130 | 85.312 + j193.519 | 84.312 + j192.528 |
| inverted-V, ¼ up (21) | 3578.6 − j1727.0 | 2311.3 − j2544.0 | 2261.5 − j2484.1 |
| L (21) | 60.537 − j118.240 | 84.965 − j50.208 | 84.649 − j50.199 |
| L (41) | 60.385 − j117.700 | 85.589 − j46.700 | 85.272 − j46.716 |

The review predicted why this could happen. Equal `∂A_s/∂s` is equal potential
only when every section's vector potential is tangential to it. At a corner the
transverse divergence `∇⊥·A⊥` from the other section adds a term `F_a` (Mei
1965's curved-wire term), and that term is the same order as the jump the rows
fix.

**This refutes bend rows *without* that term.** It does not refute the design.
The code is on the branch `feat/hallen-bend-nodes` (commit 1a92c8d), unmerged.

## Remaining: stage 1b (parked)

Add the corner term:

- `F_a(node)` to each bend's potential row;
- `Q_a(s) = −∫_node^s F_a cos k(s − s″) ds″` to each collocation row, built from
  `∂G` integrals between non-parallel section pairs.

Gate it on the decks above at two meshes (convergence direction, not one number).
The same term is what would make perpendicular wires couple: Hallén's
tangential kernel gives them exactly zero mutual terms. Degree-3 junctions and
loops are later stages of the same design. The multi-day cost is why this is
parked while MPIE answers these decks.
