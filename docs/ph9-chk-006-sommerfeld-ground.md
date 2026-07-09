---
project: fnec-rust
doc: docs/ph9-chk-006-sommerfeld-ground.md
status: living
last_updated: 2026-07-09
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
surface wave** for < 0.1 λ accuracy — has since been **physics-validated in a probe
(2026-07-09, reproduces nec2c GN2 incl. the low-height sign flip; see "Sommerfeld
feasibility study" below and `studies/sommerfeld-ground/`)** but not yet shipped as a
solver change; angle-dependent Fresnel RCM remains *not* worth a slice.

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
| Sommerfeld/Norton surface wave (nec2c GN2 exact) — straight horizontal wire | **implemented + wired** as an opt-in solver: `fnec --ground-solver sommerfeld` (default `rcm`); reproduces nec2c GN2 incl. the low-height sign flip (~13 %) |
| Sommerfeld surface wave — straight vertical / tilted / sloping wire | **implemented** (Level 1, general reflected dyadic); adds the surface-wave gap correctly, though absolute GN2 match is looser off-horizontal (bounded by fnec's scalar-Γ baseline) |
| Sommerfeld surface wave — bent / mixed geometry, and currents/patterns | deferred; `--ground-solver sommerfeld` declines bent geometry (keeps RCM). Correct currents/patterns near ground = Level 2 (kernel in the Z-matrix via DCIM) |
| buried wire | deferred → fail-fast (unchanged) |

The finite-ground reflection still multiplies the (now correctly-signed) image by a
single **normal-incidence** scalar Fresnel coefficient `Γ = (√εc − 1)/(√εc + 1)`.
The genuinely valuable — and genuinely hard — remaining increment is the
**Sommerfeld/Norton surface wave** (nec2c GN2), which is what makes low-antenna
impedance correct; angle-dependent Fresnel RCM (nec2c GN0) is *not* worth a slice on
its own because fnec already reproduces it where it matters. GN2 currently aliases
the scalar-Γ path (documented in `card-support-matrix.md`); it is *not* the true
Sommerfeld method yet.

## Sommerfeld feasibility study (2026-07-08/09) — target pinned **and physics validated**; production is a de-risked increment

A focused feasibility pass quantified the exact correction target **and then proved,
numerically against nec2c, that a direct Sommerfeld-integral reflected kernel
reproduces the GN2 near-ground impedance — including the low-height sign flip.** It
did **not** ship a solver change; the scalar-Γ path and the low-height guard are
unchanged. The validated prototype lives in `studies/sommerfeld-ground/`.

**Feasibility VALIDATED (2026-07-09).** A Python probe
(`studies/sommerfeld-ground/horizontal_dipole_sommerfeld.py`) implements the reflected
`E_x` for a horizontal dipole as a 1-D Sommerfeld integral (plane-wave-spectrum
derivation, azimuth reduced to `J0±J2`; independently cross-checked against a
Michalski–Zheng mixed-potential derivation by a second reviewer), validated in three
stages: (1) a **PEC field self-check** — with `R_TE=−1, R_TM=+1` the integral
reproduces the exact opposite-current image field to a few % (pins every
prefactor/sign); (2) a **PEC ΔZ pipeline** matching nec2c GN1 to ~7–8 % via an
induced-EMF reaction integral; (3) the **GN2 goal** — the lossy Sommerfeld ΔR tracks
nec2c GN2 across the height sweep and **reproduces the surface-wave sign flip at
0.025 λ** (probe +10.8 vs GN2 truth +9.0, where the reflection-coefficient method
gives a wrong-signed −24.3). The ~20 % residual at the lowest height is the assumed
sinusoidal current, which fnec's actual solved current would tighten (reaction ΔZ is
stationary in the current to first order). The reflected kernel is:

```
E_x^refl(ρ,d) = (ωμ0/8π) ∫_0^∞ (λ/kz0) e^{-j kz0 d}
                  [ R_TE (J0(λρ)+J2(λρ)) − R_TM (kz0²/k0²)(J0(λρ)−J2(λρ)) ] dλ
```

`d = z+z'`, `kz0=√(k0²−λ²)` (Im ≤ 0); equivalently `E_s^r ∝ k0²(ŝ·ŝ')·S{R_TE} +
(ŝ·∇)(ŝ'·∇')·S{(k0²R_TE+kz0²R_TM)/kρ²}` — the surface wave lives in the second
(charge/TM) kernel's Zenneck pole. See the study README for the full result table and
the two production routes (reaction ΔZ correction first; DCIM for speed/generality).

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

**Production landed (2026-07-09) — `fnec --ground-solver sommerfeld`.** Two increments:

1. **Kernel** — `crates/nec_solver/src/sommerfeld.rs` (`reflected_ex_horizontal`),
   Bessel J0/J1/J2 (A&S) + the sin/cosh substitution quadrature. Gated by a
   **machine-precision PEC self-check** and an **end-to-end nec2c GN2 gate**
   (`crates/nec_solver/tests/sommerfeld_ground.rs`).
2. **Wiring** — a new opt-in ground solver `--ground-solver <rcm|sommerfeld>` (default
   `rcm` = the unchanged scalar-Γ behaviour). When `sommerfeld` is selected over
   finite ground and the geometry is a **straight horizontal wire**,
   `horizontal_ground_z_correction` adds the surface-wave reaction ΔZ
   (`ΔZ_Sommerfeld − ΔZ_scalarΓ`, over fnec's solved currents) to the reported
   feedpoint `Z`. Non-horizontal/bent/mixed geometry is silently declined (keeps RCM).

**Measured end-to-end** (horizontal λ/2 dipole 0.025 λ over εr=13/σ=0.005; ΔR vs
fnec free space 67.2 Ω): `--ground-solver rcm` → 26.8 Ω (**ΔR −40**, the wrong-signed
RCM result); `--ground-solver sommerfeld` → 77.2 Ω (**ΔR +10.1**, matching nec2c GN2
ΔR +9.0 to ~13 %). The additive correction is self-consistent because fnec's solved
current drives *both* the scalar-Γ baseline it subtracts and the Sommerfeld term it
adds. CLI gate: `apps/nec-cli/tests/sommerfeld_ground_cli.rs`.

**What remains for production — now de-risked.** The hardest and riskiest step (the
**horizontal** dipole's half-space reflected kernel with correct TE+TM coupling, and
its validation vs nec2c) is **done** — see the validated study and the Rust kernel above. The vertical
dipole is a single clean TM integral fnec's scalar Γ already nails (e.g. vertical
λ/2 base 0.5 m ΔR +18 vs nec2c +18); the whole gap was the horizontal case, and its
kernel is now pinned. Remaining production work, each still worth validating: (1) a
**robust λ-quadrature in Rust** — contour deformation past the integrable branch
point at λ=k0 and **surface-wave (Zenneck) pole** extraction (the pole in the R_TM /
charge kernel is what carries the low-height correction); (2) wiring the **reaction
ΔZ correction** `ΔZ_sw = −(1/I0²)∬ I[E^r_Somm − E^r_scalarΓ]I` onto the existing
conductor-path currents (no Hallén-solver surgery — the recommended first increment);
(3) optionally **DCIM** (GPOF/Prony complex-exponential fit → `Σ aᵢ·G(complex_imageᵢ)`)
for speed/generality, slotting into `assemble_z_matrix_with_ground` with a
complex-distance Green's kernel; (4) the vertical-horizontal coupling entry for mixed
decks (projects to zero on purely horizontal geometry, defer). Unlike the degree-3
and closed-loop frontiers (whose prototypes did *not* validate), the Sommerfeld
physics **is** now validated end-to-end against nec2c — so the remaining work is a
robust-numerics-and-wiring increment, not a research gamble. Until it ships, the
scalar-Γ model and the `warn_if_low_finite_ground` guard remain the shipped
behaviour for < 0.1 λ.

## Generalization roadmap (Levels 1 & 2)

What shipped (call it **Level 0**) corrects **one quantity** (the feedpoint
impedance) for **one geometry class** (a straight horizontal wire). Two axes remain:
*geometry orientation* and *which quantity is corrected*. The kernel is already
general in ε_r, σ, frequency, height, and separation; the shared foundation both
levels need is the **full reflected half-space dyadic**.

### The shared foundation: the general reflected dyadic

The Level-0 kernel is the `φ = 0`, x-source/x-obs slice of the reflected E-field
dyadic. The general element — reflected `E` projected on observation direction `d̂_o`
at horizontal offset `(ΔX, ΔY)` and height-sum `d`, per unit current moment along
source direction `d̂_s` — is the same plane-wave-spectrum integral with the general
projections:

```text
E_proj = (k0·η0 / 8π²) ∬ (1/kz0) e^{-j kz0 d} e^{-j(kx ΔX + ky ΔY)}
           [ (d̂_s·ŝ)(d̂_o·ŝ) R_TE + (d̂_s·p̂_i)(d̂_o·p̂_r) R_TM ] dkx dky
```

with `ŝ = (−sinα, cosα, 0)` (TE), `p̂_i`/`p̂_r` the incident/reflected TM unit
vectors (`α` = spectral azimuth). This reduces **exactly** to the validated
`J0 ± J2` form for x-source/x-obs on-axis, and to a pure-`R_TM` integral for a
vertical (z) source (which fnec's scalar Γ already handles — a built-in cross-check).
Evaluate it as a **2-D integral** (radial `sinθ`/`cosh t` substitution × azimuth
grid): a low-risk direct extension of Level 0, and the **oracle** against which the
Level-2 closed form is checked.

**Level 1 LANDED (2026-07-09) — `--ground-solver sommerfeld` now corrects any straight wire.**
The 1-D azimuthal reduction was derived and validated: the α-integral of the 2-D
dyadic reduces to a single radial `J0/J1/J2` Sommerfeld integral —
`sommerfeld::reflected_e_projected_fast` — ~100× faster than the oracle, matching it
to ~1e-5 for all orientations and reducing exactly to `reflected_ex_horizontal` for
the x/x slice (`sommerfeld.rs::fast_dyadic_*` tests). `ground_z_correction` uses it
over a `(Δs, Σs)` grid to correct **horizontal, vertical, or tilted straight wires**
(the CLI dispatches horizontal to the fast ρ-grid path; bent/mixed geometry is still
declined and keeps the scalar-Γ result). The reflected dyadic is also kept as a
validated 2-D oracle (`reflected_e_projected` + `pec_general_dyadic_*`).

**Accuracy caveat (honest).** The correction adds the *surface-wave gap*
(`ΔZ_Sommerfeld − ΔZ_scalarΓ`) correctly for any straight wire — e.g. a 30°-tilted
low λ/2 dipole: the reaction correction is +9.2 Ω, matching the nec2c GN2−GN0 gap
+10.4. But it is added to fnec's scalar-Γ **baseline**, and the additive scheme is
exact only when fnec's RCM ground effect equals the induced-EMF RCM (true for
horizontal, where the corrected ΔR lands within ~13 % of nec2c GN2). Off-horizontal,
fnec's scalar Γ is a poorer RCM approximation and its solved current carries a
larger 2-nd-order reaction error, so the *absolute* GN2 match is looser (tilted:
corrected ΔR +1.3 vs GN2 +8.1 — a clear improvement over RCM's −7.9, but not exact).
Closing that residual is Level 2 (Sommerfeld current, not just feed Z).

### Level 1 — arbitrary orientation, feedpoint ΔZ (post-solve)

Generalize `horizontal_ground_z_correction` to any wire geometry over finite ground
(bent, vertical, inverted-V, mixed), still as a stationary post-solve reaction ΔZ on
fnec's solved currents:
`ΔZ_sw = (1/I_feed²) Σ_{m,n} [E^r_Somm(d̂_m,d̂_n,offset,d) − Γ·E^r_PEC(…)]·(I_m ℓ_m)(I_n ℓ_n)`.

- **Kernel:** the 2-D reflected dyadic above (all orientation pairs, arbitrary offset azimuth).
- **Gates:** (a) **PEC self-check** for every orientation pair — the 2-D dyadic with
  `(R_TE,R_TM)=(−1,+1)` must equal the free-space image-dyadic field (`p̂_img =
  (−sx,−sy,+sz)`) to machine precision; (b) **reduces exactly** to the Level-0 φ=0
  form; (c) **vertical λ/2** dipole ΔZ vs nec2c GN2 (must agree with the scalar-Γ
  result that already matches); (d) a **bent (inverted-V / L)** dipole over ground ΔZ
  vs nec2c GN2; (e) default `rcm` byte-unchanged.
- **Scope / non-goals:** corrects feedpoint `Z` only — currents, pattern, gain still
  use scalar Γ (that is Level 2). The azimuthal-analytic (`J0/J1/J2`) reduction is
  **not** required here (the 2-D form suffices for a once-per-solve correction); it is
  derived and validated in Level 2 where DCIM needs it.
- **Effort:** moderate — the physics is the 2-D extension; the work is validation.

### Level 2 — Sommerfeld ground *in the Z-matrix* (correct currents & patterns)

Replace the scalar-Γ reflected term in `assemble_z_matrix_with_ground`
(`Γ·elem(obs, image)`) with the exact Sommerfeld reflected coupling and **re-solve**,
so the ground enters the currents themselves — making **currents, mutual coupling,
arrays, patterns, gain, and efficiency** correct near ground, not just the feed Z.

- **Method — DCIM (Discrete Complex Image Method):** the 2-D per-element integral is
  too slow for an N² matrix fill, so fit the spectral reflection kernels as sums of
  complex exponentials `Σ aᵢ e^{-j kz0 bᵢ}` (GPOF/Prony along a deformed contour), with
  the **Zenneck pole extracted explicitly**. Each exponential maps via the Sommerfeld
  identity to a **complex image**: `aᵢ · G(R_complex)`, `R_complex = √(ρ² + (d+jbᵢ)²)`.
  Wire a complex-distance Green's kernel alongside the existing real-image `elem`.
- **Architectural risk (the crux):** fnec's Hallén operator eliminates the scalar
  potential, but the surface wave lives in the **charge/TM** kernel `G_Φ =
  S{(k0²R_TE + kz0²R_TM)/kρ²}`, which does not fit the axial-A Hallén row. Two routes:
  (i) add a mixed-potential E-field ground term to the Hallén Z (as NEC-2 does — the
  cleaner physics, more surgery); or (ii) a **Born/iterative** scheme that re-solves
  with the Level-1 reaction correction folded in (reuses Level 1, avoids matrix
  surgery, needs convergence proof). Decide by prototyping both against the oracle.
- **Gates:** DCIM kernel vs the Level-1 2-D oracle (<1 % over the `(ρ,d)` domain
  including the pole-dominated low-`d`/large-`ρ` corner); low horizontal **and**
  vertical dipole **currents + pattern + feed Z** vs nec2c GN2; strict regression on
  free-space / PEC / high-ground (must be unchanged).
- **Effort:** large — DCIM fitting robustness and the Hallén-vs-mixed-potential
  architecture are the real risks; stage it (kernel-in-Z for a single wire first).

**Order:** Level 1 first (small, de-risks the dyadic and ships arbitrary-orientation
feed-Z), then Level 2 (the high-value currents/patterns fix) on top of the validated
dyadic oracle.
