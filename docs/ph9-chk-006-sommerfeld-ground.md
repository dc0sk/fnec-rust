---
project: fnec-rust
doc: docs/ph9-chk-006-sommerfeld-ground.md
status: living
last_updated: 2026-07-08
---

# PH9-CHK-006: accurate near-ground impedance

## Status

**Foundational correctness fix landed (2026-07-08): the ground-image current
direction was sign-inverted, making *every* near-ground feedpoint impedance wrong.**
Fixing it brings finite-ground and PEC impedance to the correct-signed ground effect,
validated against nec2c. The further refinements the checklist names
(angle/polarization-dependent Fresnel; the Sommerfeld/Norton surface wave) are now on
a correct foundation and remain the bounded, deferred frontier.

## What was wrong

fnec has two independent image paths:

- **Far field** — `farfield.rs::pec_image_farfield`, used for the radiation pattern.
- **Impedance** — `matrix.rs::image_segment`, the method-of-images reflection term in
  the Hallén Z matrix.

The far-field path used the correct PEC image current `(−Jx, −Jy, +Jz)` (Balanis
Table 4-1: horizontal components reverse, vertical keeps sign). The Z-matrix path used
`(Jx, Jy, −Jz)` — the **exact negation** — so the reflected contribution entered the
impedance with the wrong sign. Because the two paths are separate, the *pattern* over
ground validated (PH8-CHK-006 / PH9-CHK-003, gain to 0.06 dB) while the *impedance*
was silently wrong. No prior test caught it: the ground-impedance references were fnec
self-regressions that had pinned the buggy values, and the one external (nec2c) gate
sat just below fnec's systematic reactance offset and passed by luck.

Symptom: a horizontal λ/2 dipole 0.1 λ over average ground reported 92 − j48 Ω where
nec2c gives ≈52 + j63 Ω — the radiation resistance *rose* over ground instead of
dropping. The fix makes `image_segment` return `(−Jx, −Jy, +Jz)`, matching the
far-field image.

## Validation (nec2c, 14.2 MHz, avg ground εr = 13, σ = 0.005)

fnec's Hallén operator carries a documented ~32 Ω systematic reactance offset vs
nec2c (present in free space: fs X 13.9 vs 46.2), so absolute parity is not the gate.
The physical, offset-cancelling quantity is the **ground-induced delta**
`ΔZ = Z(ground) − Z(free space)`. Across four geometries the fix flips ΔR from the
wrong sign to nec2c's sign, and the magnitudes agree well:

| geometry | ΔR before (fnec) | ΔR after (fnec) | ΔR nec2c |
|:---------|-----------------:|----------------:|---------:|
| vertical λ/2, 0.47 λ AGL, GN0 | +3.9 ❌ | −1.4 | −2.9 |
| vertical λ/2, base 0.5 m, GN2 | −4.8 ❌ | **+18.0** | **+18.0** |
| vertical λ/2, 0.47 λ AGL, PEC | +7.7 ❌ | −0.4 | −4.6 |
| horizontal λ/2, 0.1 λ AGL, GN0 | +25 ❌ | **−26** | **−27** |

The near-ground cases — where the effect is large — agree to ≈1 %. The high cases
(effect only a few Ω) agree in sign and order; their residual is the scalar-Γ model
(below), not the sign. External resistance parity for the PEC case tightened from
≈7 Ω to **0.93 Ω**. Gate: `crates/nec_solver/tests/ground_impedance.rs` (two
opposite-sign geometries) plus the refreshed corpus/`ground_diagnostics` regressions.

## Boundary — what is and is not modelled

| class | status |
|:------|:-------|
| PEC ground impedance | **correct-signed image (this fix)** |
| finite ground (GN0/GN2) impedance — scalar reflection | **correct-signed; scalar normal-incidence Γ** |
| angle- & polarization-dependent Fresnel (nec2c GN0 RCM) | deferred |
| Sommerfeld/Norton surface wave (nec2c GN2 exact) | deferred |
| buried wire | deferred → fail-fast (unchanged) |

The finite-ground reflection still multiplies the (now correctly-signed) image by a
single **normal-incidence** scalar Fresnel coefficient
`Γ = (√εc − 1)/(√εc + 1)`. This is why the small high-antenna deltas are under-scaled
(e.g. GN0 vertical ΔR −1.4 vs nec2c −2.9). The next PH9-CHK-006 increments, now on a
correct foundation:

1. **Angle- & polarization-dependent Fresnel (RCM)** — evaluate `Γ_v(θ)` / `Γ_h(θ)`
   per image ray and decompose by polarization, matching nec2c's GN0. Well-defined,
   validatable via the same ΔZ method.
2. **Sommerfeld/Norton surface wave** — the exact half-space correction (nec2c GN2),
   accurate for antennas very close to ground. The hardest slice; research-grade
   Sommerfeld-integral evaluation.

GN2 currently aliases the scalar-Γ path (documented in `card-support-matrix.md`); it
is *not* the true Sommerfeld method yet.
