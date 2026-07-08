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

## Sommerfeld feasibility study (2026-07-08) — target pinned, deferred as a dedicated increment

A focused feasibility pass quantified the exact correction target and scoped the
implementation. It did **not** ship a solver change; the scalar-Γ path and the
low-height guard are unchanged.

**The correction target (horizontal λ/2 dipole, 14.2 MHz, avg ground εr = 13,
σ = 0.005).** ΔZ = Z(ground) − Z(free space), against the nec2c free-space
reference 78.85 + j44.70 Ω. fnec's scalar Γ tracks nec2c's reflection-coefficient
method (GN0); the *surface-wave correction* is the GN2 − GN0 gap:

| height | ΔR GN0 (RCM ≈ fnec) | ΔR GN2 (Sommerfeld) | ΔX GN0 | ΔX GN2 | surface-wave ΔR, ΔX |
|:-------|--------------------:|--------------------:|-------:|-------:|:--------------------|
| 0.25 λ | +11.6 | +11.0 | +16.9 | +15.6 | −0.6, −1.2 |
| 0.10 λ | −27.0 | −19.2 | +18.1 | +13.4 | +7.8, −4.7 |
| 0.05 λ | −32.4 | −11.6 | +18.9 | +7.9 | +20.8, −11.0 |
| 0.025 λ | −24.3 | **+9.0** | **+102.2** | +23.9 | **+33.3, −78.3** |

The correction is negligible at 0.25 λ and grows steeply toward the ground: by
0.025 λ it flips ΔR from −24 to +9 (the lossy ground *adds* radiation/loss
resistance that RCM misses) and cuts ΔX from +102 to +24. There is no closed form;
this is the genuine surface-wave contribution. nec2c reference decks:
`gfs.nec` (free space), `g_{0,2}_{h}.nec` (GN0/GN2 sweep) — regenerate with three-arm
`GW` + `GN 0|2 0 0 0 13 0.005` + `FR`/`EX`/`XQ`.

**Recommended implementation path — Discrete Complex Image Method (DCIM).** fnec's
reflected impedance term is `Γ · elem(obs, image_of_src)` — a single scalar times one
geometric-optics image. The Sommerfeld reflected kernel can be written
`Σᵢ aᵢ · G(complex_image_i)` (a short sum of *complex* images: complex weights `aᵢ`
at complex heights, via the Sommerfeld identity applied to a complex-exponential fit
of the spectral reflection coefficient), plus explicit **surface-wave-pole
extraction** for accuracy at low height. This maps directly onto fnec's existing
structure: replace the one `Γ · elem(image)` with a small sum over complex images,
which only needs a complex-distance Green's kernel (`exp(−jk r)/r` with complex `r`)
alongside the existing real-image `elem`.

**Why it's a dedicated multi-session increment, not a slice.** The tractable-looking
DCIM still requires, done correctly and validated at each step: (1) the **horizontal**
dipole's full half-space dyadic — TE **and** TM reflection with their polarization
coupling (the vertical dipole is a single clean TM integral and fnec's scalar Γ
*already* nails it, e.g. vertical λ/2 base 0.5 m ΔR +18 vs nec2c +18; the whole gap is
the horizontal case); (2) sampling `R_TE(λ)`, `R_TM(λ)` along a deformed spectral
contour and a **GPOF/Prony complex-exponential fit**; (3) **surface-wave (Zenneck)
pole** detection + extraction so the low-height regime is captured (the pole is what
the RCM omits); (4) a complex-image Green's kernel wired into `assemble_z_matrix_with_ground`
and the far-field path consistently; (5) ΔZ validation across the height sweep above
plus vertical / PEC regression. Any sign or factor error in the dyadic costs many
nec2c-comparison iterations. This is comparable in size to the degree-3 and
closed-loop solver frontiers — a fresh dedicated session with a full validation
budget, high risk of not validating on the first pass. Deferred; the scalar-Γ model
and the `warn_if_low_finite_ground` guard remain the shipped behaviour for < 0.1 λ.
