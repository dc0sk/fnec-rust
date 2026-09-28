---
project: fnec-rust
doc: docs/hallen-bends.md
status: living
last_updated: 2026-09-28
---

# Hallén on bent conductors (FND-162)

Hallén's rows hold the **tangential** vector potential of each segment and
integrate `(∂²_s + k²) A_s = −jωμε V δ` along a straight run. A bent conductor
breaks that in two ways. The homogeneous term cannot be one function across a
corner. And at the corner the transverse divergence `∇⊥·A⊥` contributed by the
other, non-parallel section (Mei 1965's curved-wire term) is missing.

Since 2026-09-27 (stage 1b) the driven Hallén solve (delta gap and current
source) models both, and since 2026-09-28 the plane-wave receive solve too:

- **Per-section homogeneous terms.** Each straight section of a conductor path
  carries its own `(C, D)`.
- **Two rows at every bend.** Path-current continuity (both sides extrapolated to
  the node), and equal scalar potential, `∂A_s/∂s` equal on both sides.
- **The corner term.** On each section the missing part of `A_s` is
  `Q(s) = −∫_{s₀}^{s} F(s″) cos k(s − s″) ds″`, referenced at one node of the
  section. It enters the matrix; `crates/nec_solver/src/corner.rs` holds `f_n`
  and the graded quadrature. A section referenced at its other node carries its
  integral into that bend's potential row.

The formulation was worked out by the design review (Fable). It was prototyped in
numpy and gated before any Rust: with the term off, the prototype reproduced the
code's stage-1 numbers to 0.0005 %. The Rust then matched the prototype to every
printed digit.

## Measured (14.2 MHz)

| deck | nec2c 1.3.1 | before (bend rows only) | now |
|---|---|---|---|
| 90° L, 21 per arm | 60.537 − j118.240 | 84.649 − j50.199 | 59.409 − j125.053 (5.2 %) |
| 90° L, 41 per arm | 60.385 − j117.700 | 85.272 − j46.716 | 59.754 − j121.716 (3.1 %) |
| inverted-V fed ¼ up an arm, 21 | 3578.6 − j1727.0 | 2261.5 − j2484.1 | 3243.7 − j1869.9 (9.2 %) |
| inverted-V fed ¼ up an arm, 41 | 3572.5 − j959.3 | 2664.5 − j2120.9 | 3438.2 − j1088.1 (5.0 %) |
| inverted-V fed at the apex, 21 / 41 / 81 | 86.8 + j197.2 / 87.5 + j198.1 / 88.1 + j199.0 | 0.7 % / 3.0 % / 6.5 %, diverging | 4.0 % / 3.0 % / 2.5 %, converging |
| two-bend U, 11 / 21 per wire | 14.39 − j219.22 / 14.31 − j218.18 | — | 14.16 − j221.40 / 14.18 − j219.57 (≈ 1 %) |
| Z shape, 11 / 21 per wire | 45.41 − j565.43 / 44.62 − j559.29 | — | 43.30 − j567.62 / 43.42 − j560.83 (≈ 1 %) |
| steep split-V (corpus) | 268.56 + j452.26 | 344.58 + j530.33 | 270.43 + j443.20 (1.8 %) |

The apex-fed inverted-V is the case to remember. Without the corner term it read
0.7 % from nec2c at 21 segments, and the error *grew* under refinement: two errors
cancelling at one mesh. Every deck here is gated at two meshes
(`crates/nec_solver/tests/bend_corner_nec2c.rs`), with the error required to
shrink. Removing the corner term fails all three gates: the U at 88 %, the L at
57 %, the inverted-V at 40 %.

## Stage 1 on its own measured no gain

Per-section `(C, D)` plus the two bend rows, *without* the corner term, left the
L at 84.6 − j50.2 against nec2c's 60.5 − j118.2. That result refuted bend rows
without the term, not the design. The review had predicted it: the omitted term is
the same order as the jump the rows fix.

## The receive solve (2026-09-28)

The receive solve had been kept on one `(C, D)` per path, on the belief that its
forcing "jumps at a bend, which the bend rows do not carry". The forcing does not
jump. It is the incident field's tangential part convolved with `sin(k|s − s′|)`,
which is a sum of delta-gap right-hand sides, one per segment. Each of those is
smooth at every node, so the bend rows written for a feed hold for it unchanged.
A unit test in `planewave.rs` pins that identity to 1e-12. Routed through the same
section layout, bend rows and corner term, the receive currents against nec2c
(|ΔI| / peak |I|, at the middle of each run and both segments at the bend):

| deck | per path, before | now, coarse → fine mesh |
|---|---|---|
| 90° L, θ = 45°, 21 / 41 per arm | 106 % / 122 % | 6.3 % → 3.6 % |
| the same L lit edge-on (one arm sees no field) | 106 % / 121 % | 5.6 % → 3.3 % |
| two-bend U, 11 / 21 per wire | 41 % / 51 % | 1.9 % → 1.1 % |
| inverted-V, θ = 60°, 21 / 41 per arm | 8.6 % / 9.1 % | 1.7 % → 1.1 % |

A straight wire is unchanged (5.6 %, the pulse basis's own error). Sections
without the corner term gain nothing: the L stays at 121 % and the U reaches
647 %. Gates: `crates/nec_solver/tests/bend_receive_nec2c.rs`, and the absolute
reciprocity level in `planewave_junction.rs`. That level is `|I_sc|²/G =
R·λ²/(π·η₀·|Z|²)` from the transmit solve, met to 0.09 %. The per-path receive
misses it by 8 %, while the angle spread the test used to check was 0.18 % for
both.

The receive-pattern sweep builds the layout and corner term once per geometry
(`plan_hallen_planewave`). Rebuilt per direction, a 37 × 73 pattern on a 41-per-arm
L took 264 s. It now takes 10 s; the per-path solve took 6 s.

## Still open (FND-162)

- **A straight run only one segment long.** Its single row cannot fix two
  constants, so the whole deck falls back to one term per path and its bends go
  unmodelled: about 26 % off in the currents on a U whose middle wire is one
  segment. Hallén warns on exactly these decks, driven or receive, through the same
  predicate the layout uses (`hallen_leaves_a_bend_unmodelled`).
- **Degree-3+ junctions and closed loops.** These are later stages of the same
  design (Kirchhoff's law and equal potential at a node of degree d; the loop's
  closure). They are warned about and pointed to `--solver mpie`.
- **Perpendicular wires that share no node.** They do not couple, because
  Hallén's tangential kernel gives them exactly zero mutual terms. The same `F`
  term restores the coupling, but on the plain per-wire rows, which this stage
  does not touch.
- **Receive over ground** is FND-170, not a bend problem: the receive forcing has
  no ground-reflected wave, so even a straight dipole over perfect ground is about
  55 % off.
