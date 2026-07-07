---
project: fnec-rust
doc: docs/ph9-chk-006-sommerfeld-ground.md
status: living
last_updated: 2026-07-08
---

# PH9-CHK-006: accurate near-ground impedance

## Status

**PH9-CHK-006 acceptance criteria met (2026-07-08).** Two increments:

1. **Correctness fix** — the ground-image current direction was sign-inverted, making
   *every* near-ground feedpoint impedance wrong (opposite-signed ground effect).
   Fixed and validated against nec2c via the ground-induced ΔZ.
2. **Boundary + guard** — a height sweep vs nec2c shows fnec's finite-ground impedance
   is genuinely accurate (≈ Sommerfeld) for heights ≥ ~0.2 λ and gated there;
   below ~0.1 λ the reflection-coefficient model breaks down (the surface wave
   dominates) and `warn_if_low_finite_ground` guards it. Boundary documented below.

This satisfies the checklist: an accurate near-ground class passes a nec2c tolerance
gate, out-of-scope (low-height / buried) classes are guarded / fail fast, and the
boundary is documented. The genuinely-hard remaining work — the **Sommerfeld/Norton
surface wave** for < 0.1 λ accuracy — stays deferred (angle-dependent Fresnel RCM is
*not* worth a slice; see below).

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

## Accuracy vs height, and why RCM is *not* the next slice

A height sweep of a horizontal λ/2 dipole over average ground (14.2 MHz),
comparing fnec's scalar-Γ ΔR against nec2c's **reflection-coefficient method
(GN0)** and its **exact Sommerfeld solution (GN2)**:

| height | fnec ΔR | nec2c GN0 (RCM) | nec2c GN2 (truth) |
|:-------|--------:|----------------:|------------------:|
| 0.25 λ | +9.9 | +11.6 | +11.0 |
| 0.10 λ | −25.9 | −27.1 | −19.2 |
| 0.05 λ | −36.8 | −32.4 | −11.6 |
| 0.025 λ | −40.4 | −24.4 | **+8.8** |

Two conclusions drive the scope of the remaining work:

1. **fnec's scalar Γ already tracks nec2c's RCM (GN0)** at practical heights
   (≥ 0.1 λ), and there RCM ≈ Sommerfeld. So implementing the full angle- &
   polarization-dependent Fresnel RCM would largely *reproduce fnec's current
   behaviour* — **low value**. (The scalar over-shoots RCM only below ~0.05 λ.)
2. The real accuracy gap is **RCM → Sommerfeld**, which only opens below ~0.1 λ and
   there it is severe: at 0.025 λ the reflection-coefficient ΔR is −24 Ω while the
   Sommerfeld truth is **+9 Ω** — a *sign* disagreement. Closing it requires the
   surface-wave integral, not a better Fresnel coefficient.

So fnec's finite-ground impedance is **genuinely accurate (≈ Sommerfeld) for
heights ≥ ~0.2 λ** and degrades below, becoming unreliable under ~0.1 λ. This is
gated (`ground_impedance.rs::horizontal_dipole_quarter_wave_high_matches_sommerfeld`,
ΔR +9.9 vs Sommerfeld +11.0) and **guarded**: `warn_if_low_finite_ground`
(`solve_session.rs`) warns when the lowest conductor point is below 0.1 λ over
`SimpleFiniteGround` that the near-ground impedance is a reflection-coefficient
approximation with no surface wave.

## Boundary — what is and is not modelled

| class | status |
|:------|:-------|
| PEC ground impedance | **correct-signed image (2026-07-08 fix)** |
| finite ground impedance, height ≥ ~0.2 λ | **accurate (≈ Sommerfeld), gated vs nec2c GN2** |
| finite ground impedance, height < 0.1 λ | approximate → **guarded (low-height warning)** |
| angle- & polarization-dependent Fresnel (nec2c GN0 RCM) | deferred — **low value** (fnec ≈ RCM already) |
| Sommerfeld/Norton surface wave (nec2c GN2 exact) | deferred — the real < 0.1 λ accuracy slice; hardest |
| buried wire | deferred → fail-fast (unchanged) |

The finite-ground reflection still multiplies the (now correctly-signed) image by a
single **normal-incidence** scalar Fresnel coefficient `Γ = (√εc − 1)/(√εc + 1)`.
The genuinely valuable — and genuinely hard — remaining increment is the
**Sommerfeld/Norton surface wave** (nec2c GN2), which is what makes low-antenna
impedance correct; angle-dependent Fresnel RCM (nec2c GN0) is *not* worth a slice on
its own because fnec already reproduces it where it matters. GN2 currently aliases
the scalar-Γ path (documented in `card-support-matrix.md`); it is *not* the true
Sommerfeld method yet.
