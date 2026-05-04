---
project: fnec-rust
doc: docs/utd-feasibility-assessment.md
status: reference
last_updated: 2026-05-04
correction: ADA305743 corrected from "likely NEC-BSC" to Rousseau & Pathak TD-UTD (1996), confirmed via PDF review (all 163 pp. read 2026-05-04)
---

# UTD and Canning Simply Sparse: Feasibility Assessment

**Prepared by**: Claude Sonnet 4.6 (AI assistant)  
**Date**: 2026-05-04  
**Triggered by**: User question on UTD extension value and implementation feasibility  

---

## References reviewed

1. **Lertwiriyaprapa, Pathak & Volakis (2007)** — "A Uniform Geometrical Theory of Diffraction for
   predicting fields of sources near or on thin planar positive/negative material discontinuities",
   *Radio Science* 42(6), doi:10.1029/2007RS003689. Ohio State ElectroScience Laboratory.
   (Full text behind paywall; metadata confirmed via CrossRef.)

2. **DTIC ADA305743** — P. R. Rousseau and P. H. Pathak, *"Time Domain Version of the Uniform
   Geometrical Theory of Diffraction"*, OSU ElectroScience Laboratory Technical Report 721564-3,
   February 1996. ONR-sponsored (Contract N00014-91-J-1013). Full text confirmed via PDF review.
   Develops TD-UTD formulations via the **Analytic Time Transform (ATT)**: transient scattering from
   arbitrarily curved PEC wedges (Ch. 3) and smooth convex PEC surfaces (Ch. 4).

3. **Canning & Rogovin, "A universal matrix solver for integral-equation-based problems" (2003)** and
   follow-on papers — the **Simply Sparse** method (also called Impedance Matrix Localization, IML):
   a basis-transformation technique to sparsify the MoM impedance matrix.

---

## Background: methods compared

### Method of Moments (MoM) — fnec-rust's current solver

MoM is a full-wave integral equation method. It discretizes the Electric Field Integral Equation (EFIE)
or Hallén equation into an N×N dense linear system. Key properties:

- **Accurate at all electrical sizes** — no frequency restriction.
- **Required for antenna quantities**: feedpoint impedance, segment currents, near-field coupling.
- **O(N²) memory, O(N³) direct solve**. At N = 500 this is manageable; at N = 5000 it becomes a wall.
- **Assumes explicit wire/surface discretization** — cannot handle electrically large structures
  without correspondingly large N.

### Uniform Theory of Diffraction (UTD)

UTD is a **high-frequency asymptotic ray method**, a refinement of Keller's Geometrical Theory of
Diffraction (GTD). Fields are modeled as bundles of rays that travel in straight lines and diffract or
reflect at geometric discontinuities (edges, wedges, curved surfaces). Key properties:

- **Efficient for electrically large structures** — no surface mesh needed; complexity scales with
  geometry complexity, not electrical size.
- **Invalid below approximately 2–5 wavelengths** across a structure — pure ray assumptions break down.
- **Cannot compute feedpoint impedance or segment currents** — ray methods give radiated/scattered
  fields, not antenna terminal quantities.
- **Models diffraction at edges, wedges, and surface discontinuities** — exactly the physics that
  Sommerfeld integrals and Fresnel approximations miss for finite ground planes and platform edges.

### Hybrid MoM-UTD

The standard architecture (used in NEC-BSC, FEKO hybrid mode, and others) couples both methods:

- **MoM region**: The antenna wire structure and any nearby small conductors → yields wire currents.
- **UTD region**: Large platform (aircraft, ship, vehicle, building, finite ground plane) → yields
  scattered/diffracted field contributions as additional impedance-matrix entries.
- **Coupling**: MoM wire currents radiate incident fields onto UTD objects; UTD scattered fields appear
  as modified excitation and mutual-impedance entries in the MoM system. The coupled matrix is:

  ```
  [Z_mm + Z_mm^{UTD}] I_m = V_m
  ```

  where `Z_mm^{UTD}` contains the UTD-mediated interaction paths (direct + single-diffracted +
  doubly-diffracted rays from each wire pair).

### Canning's Simply Sparse (Impedance Matrix Localization)

A MoM matrix acceleration technique, **not** a solver replacement:

- Applies a basis transformation `T` so that `Z' = T† Z T` is sparse (many near-zero off-diagonal
  blocks corresponding to spatially distant interaction pairs).
- Solves `Z' I' = V'` using sparse factorization (no fill-in variant exists for the IML case).
- Achieves sub-O(N²) storage and faster iterative convergence for large N.
- Orthogonal to UTD — it works entirely within the MoM wire framework.

---

## Assessment: can UTD add advantage to fnec-rust?

### Short answer

**UTD cannot replace MoM** for any of fnec-rust's core antenna modeling tasks, and is not applicable to
the primary use case (isolated wire antennas at HF/VHF/UHF). However, **hybrid MoM-UTD would add
genuine new capability** for a class of problems that fnec-rust cannot currently address at all: antennas
on or near electrically large platforms (vehicles, ships, buildings) and antennas over finite ground
planes with edge diffraction.

The specific AGU paper (Lertwiriyaprapa et al.) extends UTD to material discontinuities (including
negative-index materials). This is a more specialized contribution relevant only if fnec-rust ever targets
metamaterial antenna or antenna-on-radome problems.

### Where UTD hybrid would add real value

| Problem class | Current fnec-rust capability | What hybrid MoM-UTD adds |
|:--------------|:-----------------------------|:------------------------|
| Antenna over infinite PEC ground | Image method (GN=1) — exact | Nothing; image method is already exact |
| Antenna over infinite finite-conductivity ground | Fresnel approximation (GN0/GN2) | More accurate Sommerfeld + surface wave treatment |
| Antenna over **finite** ground plane (radials with edge) | Not supported — Fresnel assumes infinite extent | UTD models finite-ground edge diffraction correctly |
| Antenna on a metallic vehicle/platform | Not supported — MoM would require meshing entire platform | UTD handles platform as large scatterer; only wire antenna needs MoM |
| Antenna near a building corner/edge | Not supported | UTD wedge diffraction coefficient directly applicable |
| Antenna near a material-coated surface | Not supported | Lertwiriyaprapa et al. UTD coefficient applicable |

### Where UTD does NOT help fnec-rust

| Case | Reason UTD is not applicable |
|:-----|:-----------------------------|
| Isolated dipoles, Yagis, loops, verticals | Structure dimensions ≈ 1–10λ; UTD accuracy requires >> 5λ across the diffracting object |
| Feedpoint impedance computation | UTD provides far/scattered fields, not terminal quantities — MoM must always compute impedance |
| Segment current distribution | Ray methods give no segment-level current resolution — MoM required |
| Near-field coupling between closely spaced wires | UTD far-field assumption breaks down in the near-field zone |
| Sub-wavelength antenna elements (loading, matching) | LD loads, TL networks — entirely within MoM domain, UTD irrelevant |

### On the Lertwiriyaprapa et al. paper specifically

The closed-form UTD coefficients for thin planar material discontinuities are a **building block** for
future hybrid modeling, not a standalone capability. Their direct relevance to fnec-rust's current roadmap
is low, but they would become relevant if:

- Antenna-on-radome (dielectric-coated platform surface) modeling is targeted.
- Antenna near a frequency-selective surface (FSS) or absorber panel is targeted.
- Any negative-index or metamaterial environment is in scope.

These are not in the current roadmap. The paper should be filed as a reference for a hypothetical Phase 6
material-boundary extension.

### On the DTIC ADA305743 report (Rousseau & Pathak 1996 — TD-UTD)

ADA305743 is **not** a hybrid MoM-UTD code manual. It is a foundational theoretical report extending
frequency-domain UTD into the **time domain** using the Analytic Time Transform (ATT). Its scope:

- **TD-UTD for curved PEC wedges** (Ch. 3): ATT applied to Kouyoumjian–Pathak edge diffraction
  coefficients; slope diffraction included; handles arbitrary wedge curvature via local plane-wave
  decomposition.
- **TD-UTD for smooth convex surfaces** (Ch. 4): creeping-wave (Franz) modes in the time domain via
  ATT inversion of the surface diffraction series.
- **Analytic Time Transform (ATT)**: a one-sided transform that produces complex-analytic time
  functions from frequency-domain UTD fields; avoids the non-causal artifacts that arise from naive
  Fourier inversion of UTD expressions with their GO jump discontinuities.

**Relevance to fnec-rust**: TD-UTD targets transient/pulsed excitation (radar signatures, UWB,
EMP/HEMP analysis). fnec-rust is a frequency-domain CW antenna modeling tool — TD-UTD does not
apply to its core use cases. This document is a more specialized reference than the frequency-domain
Kouyoumjian–Pathak UTD, and less immediately applicable to a hybrid MoM-UTD antenna solver than
originally assessed.

Potential future relevance:

- If fnec-rust ever adds broadband sweep → time-domain conversion (e.g., IFFT of complex impedance
  and pattern sweeps), the ATT mathematical machinery is directly applicable.
- For any impulsive or transient near-field scattering analysis, TD-UTD would be the correct tool.

For the hybrid MoM-UTD architecture, the foundational frequency-domain references remain
Kouyoumjian & Pathak (1974) and the NEC-BSC documentation (a separate document not yet located).

---

### ADA305743: algorithm reference for future implementors

The following is a detailed technical summary for any future work that requires implementing or
extending the TD-UTD formulations. The full PDF (163 pp.) has been read and summarized here to
the level of implementable equations. Equation numbers refer to the original report.

#### Chapter 2 — The Analytic Time Transform (ATT)

The ATT is defined in two equivalent forms. For Im t > α:

```
A_ω form:  F̊(t) = (1/π) ∫₀^∞ F̃(ω) u(ω) e^{jωt} dω          (2.6)
A_t form:  F̊(t) = (j/π) ∫_{-∞}^∞ F(τ)/(t − τ) dτ            (2.10)
```

The key identity connecting ATT to the one-sided Laplace transform is (eq 2.13):

```
ATT[F̃(ω)] = (1/π) · L_x[F̃(x)]|_{s=−jt}
```

This means every Laplace transform table entry can be reused directly for ATT computations by
substituting s = −jt. At real time (Im t = 0): `F̊(t) = F(t) + j H{F}(t)` — the analytic signal.

**Critical asymptotic series warning** (§2.4): The ATT cannot be applied term-by-term to a
high-frequency asymptotic power series expansion. Each term diverges as a distribution; the
correct procedure uses the full Bleistein–Handelsman framework (ref [31] in the report:
Bleistein & Handelsman, *Asymptotic Expansions of Integrals*, Dover 1986). Ignoring this leads
to non-causal early-time results (see Ch. 5 discussion below).

**Excitation pulse fitting for closed-form convolution**: Any finite-energy time pulse F₀(t)
supported on [0, a] can be fitted as (eq 2.68):

```
F₀(t) ≈ (1/π) Σ_{n=1}^N Re[A_n α'_n / (t − α''_n)² + α'_n²]
```

Coefficients found by least squares (overdetermined system [C][A] = [B] solved via SVD or Cholesky,
Appendix A). With this representation, convolution with any TD-UTD impulse response is performed
in closed form (eq 2.70 / 3.80–3.81). Spacing: α''_n = (n−1)a/(N−1); width:
α' = 0.75(a/(N−1)) to 1.0(a/(N−1)).

**ATT and wavelets**: The second time derivative of the ATT (eq 2.11) is a continuous wavelet
transform (CWT) — see Kaiser [56]. This opens a time-frequency or time-scale decomposition
perspective on TD-UTD field analysis.

---

#### Chapter 3 — TD-UTD for Arbitrarily Curved PEC Wedge

Total impulse response (eq 3.9 / 5.1):

```
Ė^UTD_I(t) = Ė^i_I(t) U_i + Ė^r_I(t) U_r + Ė^d_I(t) + Ė^{sd}_I(t)
```

where U_i, U_r are spatial unit step functions (1 on lit side, 0 on shadow side).

**First-order dyadic diffraction coefficient** (eqs 3.40–3.41):

```
Ḋ_{s,h}(t) = −1/(2n√(2π) sin β₀) Σ_{m=1}^4 K^{s,h}_m F̊(x_m, t)
```

where K^{s,h}_m are cotangent functions of wedge angle and observation/incidence angles (eqs 3.20–3.23),
x_m are distance parameters related to UTD L-parameters (eqs 3.24–3.31), and F̊(x_m, t) is the
closed-form ATT of the edge transition function (eqs 3.37–3.38):

```
F̊(x_m, t) = −j√(−x_m/π) / [√(−jt)(√(−jt) + e^{−jπ/4}√(−x_m/c))]
```

This is computable in O(1) for each (x_m, t) pair.

**Slope diffraction**: Two versions of the higher-order dyadic slope diffraction coefficient
(eq 3.56–3.65) are derived:

- **Hwang's version** (compact, better late-time accuracy): slope transition function ATT is (C.16):
  ```
  F̊_s(x_m, t) = √(c/π) · 2x_m e^{−jπ/4} / (√(−jt) + e^{−jπ/4}√(−x_m/c))
  ```
  This has a closed-form real part at real time: F_s(x_m, t) = (2x_m√c/√π) √t/(t + x_m/c) u(t), which
  peaks at t = x_m/c (arrival of the slope diffracted wavefront).

- **Veruttipong's version** (better early-time accuracy, used near incidence/reflection shadow boundaries):
  additional transition function F̊_{vs}(x_m,t) with a logarithmic closed form (C.42):
  ```
  F̊_{vs}(x_m, t) = 2c√(−x_m/π) [ln|e^{jπ/4}√(−jct) + √(−x_m)| / √(−x_m) + j arg(e^{jπ/4}√(−jct) + √(−x_m)) − jπ/2]
  ```

**Validation** (§3.5): Parabolic strip and dipole-on-wedge examples. TD-UTD vs MoM+FFT:
"almost indistinguishable." TD-UTD vs Felsen's exact closed-form solution: excellent agreement.

---

#### Chapter 4 — TD-UTD for Smooth Convex PEC Surfaces

Total impulse response (eq 5.2):

```
Ė^UTD_I(t) = Ė^i_I(t) + Ė^{gr}_I(t)    (lit region)
            = Ė^d_I(t)                    (shadow region)
```

**TD-UTD generalized reflected field** (eq 4.52):

```
Ė^{gr}_I(t) = E⁰ · [Ṙ_s(τ_r) ê_⊥ ê_⊥ + Ṙ_h(τ_r) ê||^i ê||^r] A_r(s^r) A_i(s^i)
```

**TD-UTD surface diffracted field** (eq 4.53):

```
Ė^d_I(t) = E⁰ · [Ḋ_s(τ_d) b̂₁ b̂₂ + Ḋ_h(τ_d) n̂₁ n̂₂] A_d(s^d) A_i(s^i)
```

Both coefficients Ṙ_{s,h} and Ḋ_{s,h} depend on the function F̊^P_{s,h}(Ξ, t) (eqs 4.48–4.49),
which is not available in closed form and requires the numerical algorithm of Appendix E.

The Ξ parameter encodes the source-observer geometry through a surface geodesic integral:

```
Ξ = ∫_{Q₁}^{Q₂} M(l') / ρ_g(l') dl',    M(l) = (ρ_g(l) / 2c)^{1/3}
```

where ρ_g(l) is the local surface radius of curvature along the geodesic path.

**Validity condition**: Im(t) > 0.002|Ξ³| (or Im(t) > 0.002|Ξ^L|³ for lit region).

**Validation** (§4.6): 2-D cylinder, r = 1 m; bistatic angles ψ = 0° to 175°; both TE_z and TM_z
polarizations. TD-UTD vs eigenfunction reference (transformed to time domain via IFFT): excellent
agreement at all angles including deep shadow (ψ = 175°).

---

#### Appendix D — Algorithm for F̊_{cw}(α, t)

The time-domain creeping wave function is defined as (D.1):

```
F̊_{cw}(α, t) = (1/π) ∫₀^∞ (jω)^{−5/6} e^{−α(jω)^{1/3}} e^{jωt} dω,   Im t ≥ 0, α ≥ 0
```

It satisfies the differential equation ∂³/∂α³ F̊_{cw} + ∂/∂t F̊_{cw} = 0 (D.2). Scale change:
F̊_{cw}(bα, t) = (1/b^{5/2}) F̊_{cw}(α, t/b³) (D.10). At α = 0:
F̊_{cw}(0, t) = Γ(5/6) e^{−jπ/12} / [π(−jt)^{5/6}] (D.3).

**Three-regime algorithm**:

| Time regime | Method | Condition |
|:------------|:-------|:----------|
| Very small t | Early-time series (D.43) — 3 terms of (D.12) | \|t\| < 0.021(α³/40) |
| Intermediate t | Numerical algorithm (D.42) | 0.021(α³/40) ≤ \|t\| ≤ 15α³ |
| Large t | Late-time series (D.44) — 3 terms of (D.14) | \|t\| > 15α³ |

Early-time series (semi-convergent, asymptotic for small |t/α³|):
```
F̊_{cw}(α, t) ≈ −3j Γ(5/2) / (παˆ5/2) [1 + 39.375(t/α³) + (3·5·7···15)/(3·27)(t/α³)²]    (D.43)
```

Late-time series (convergent for t ≠ 0):
```
F̊_{cw}(α, t) ≈ (e^{jπ/3}/πt^{5/6})[Γ(5/6) − Γ(7/6) e^{jπ/3} α/t^{1/3} + Γ(9/6) e^{j2π/3} α²/(2t^{2/3})]  (D.44)
```

**Numerical algorithm** (for intermediate t): change variables z = (jω)^{1/3}, then y = z(3t/α)^{1/2},
rotate contour to steepest descent path, split integral at x_m = (3/(2|Ω|))^{1/3} where Ω = (α³/3t)^{1/2}.
The main sub-integral İ₁ reduces to (D.34):

```
İ₁(t) = (Bx_m)^{5/2} ∫₀¹ g(u) e^{−|Ω|ABx_m u} du,   g(u) = u^{3/2} e^{−B³u³/2},   B = 2.7
```

where g(u) is approximated by M = 10 exponential terms using the extended Prony method (Tables D.1 and D.2
in the report, ERR = 2.937e-9), giving the closed-form result (D.42):

```
İ₁(t) ≈ (Bx_m)^{5/2} Σ_{m=1}^{10} g_m (1 − exp(−b_m − |Ω|ABx_m)) / (b_m + |Ω|ABx_m)
```

The Prony parameters from Tables D.1 and D.2 are fixed constants (independent of α, t) that must
be hard-coded as lookup tables in any implementation.

---

#### Appendix E — Algorithm for F̊^P_{s,h}(Ξ, t)

The surface diffraction special function is defined as (E.1):

```
F̊^P_{s,h}(Ξ, t) = ATT[P̃_{s,h}(ω^{1/3} Ξ) / ω^{1/6}]
```

where P̃_{s,h} are Pekeris caret functions (Fock-type functions). Scale property: same as F̊_{cw}.

**Four-regime algorithm** (Ξ > 0: shadow; Ξ < 0: lit):

| Region | Time regime | Method | Condition |
|:-------|:------------|:-------|:----------|
| Shadow (Ξ > 0) | Early time | Creeping wave mode series (E.5): N_s=50, N_h=20 Airy zeros + tail correction (E.33) | \|t/Ξ³\| < 1 |
| Shadow (Ξ > 0) | Late time | Inverse power series (E.17)+(E.18), 50 terms, Pekeris coefficients Table E.2 | \|t/Ξ³\| ≥ 1 |
| Lit (Ξ < 0) | Late time | Same inverse power series (E.17)+(E.18), 50 terms | \|t/Ξ³\| ≥ 0.15 |
| Lit (Ξ < 0) | Early time | Approximate early time (E.25), N=14 terms, Tables E.3 (soft) or E.4 (hard) | \|t/Ξ³\| < 0.15 |

Shadow early time — creeping wave mode series:
```
F̊^P_{s,h}(Ξ, t) ≈ { −(1/√π) Σ_{n=1}^{Ns} F̊_{cw}(Ξ q_n, t) / [2 Ai'(−q_n)]²  (soft)
                    { −(1/√π) Σ_{n=1}^{Nh} F̊_{cw}(Ξ q̄_n, t) / [2 q̄_n Ai(−q̄_n)]²  (hard)
```

where q_n are zeros of Ai(−q_n) = 0 and q̄_n are zeros of Ai'(−q̄_n) = 0 (Table E.1: q₁=2.338,
q₂=4.088, q₃=5.521, q₄=6.787; q̄₁=1.019, q̄₂=3.248, q̄₃=4.820, q̄₄=6.163). For n ≥ 51 the tail
correction is:

```
S_{N₀} ≈ −j 1.786878×10⁻³ / Ξ^{5/2}    (for soft polarization; hard tail is negligible)
```

Airy zero asymptotic formulas (accurate to ≥ 5 significant figures for n ≥ 51):
```
q_n  ≈ f[3π(4n−1)/8],    f(z) ≈ z^{2/3}(1 + (5/48)z⁻² − ...)    (E.10)
q̄_n ≈ g[3π(4n−1)/3],    g(z) ≈ z^{2/3}(1 − (7/48)z⁻² − ...)    (E.11)
```

Shadow/lit late time — inverse power series:
```
F̊^P_{s,h}(Ξ, t) = F̊_{p,q}(Ξ, t) − e^{−jπ/4} / (2πΞ(−jt)^{1/2})
F̊_{p,q}(Ξ, t) = (e^{−jπ/12}/π(−jt)^{5/6}) Σ_{n=0}^∞ {ρ_n / σ_n} Γ(n/3+5/6) e^{jnπ/6} Ξ^n / (n!(−jt)^{n/3})
```

Pekeris coefficients {ρ_n, σ_n} are tabulated in Table E.2 of the report (n = 0 to 49; the series
exhibits every-third-term zeros at n = 23, 26, 29, 32, ... for the σ_n sequence). Accurate to ≥ 3
significant digits for |t/(Ξ³)| > 0.15.

Lit early-time alternate representation (E.25, centered at t = (−Ξ)³/12 — the GO arrival time):
```
F̊^P_{s,h}(Ξ, t) ≈ ±√(−Ξ)/2 · δ̊[t − (−Ξ)³/12] + (1/(−Ξ)^{5/2}) Σ_{n=1}^{14} B^{s,h}_n (−jt/(−Ξ)³)^{14−n}
```

Coefficients {B^s_n} (soft, Table E.3) and {B^h_n} (hard, Table E.4) are given to full double
precision. This expansion fails violently as t → 0 (the pole moves toward the origin) and loses
accuracy when Im(t/(−Ξ)³) < 0.002; in that limit the GO delta-function term dominates anyway.

---

#### Chapter 5 — Key insights for implementors

1. **Early-time results are not causal in the usual sense.** The "early time" in TD-UTD refers to
   scattering from a localized portion of the scatterer; the time origin is shifted to that
   scattering event, not to the global "turn-on" time. Inverse Laplace transforms would incorrectly
   enforce one-sided (causal) time functions. ATT produces two-sided analytic time functions, which
   is the correct mathematical object for TD-UTD fields.

2. **The F̊^P_{s,h} algorithm in Appendix E works as long as Im(t) > 0.002|Ξ³|.** For computing
   a true impulse response at Im(t) = 0, only the dominant GO delta-function term in the lit region
   is reliable; the remainder term in (E.25) is not accurate in this limit.

3. **Authors note the Appendix E algorithm prioritizes accuracy over efficiency.** Future work
   could replace the series/mode approach with rational function approximations for speed.

---

## Assessment: large-N MoM acceleration options

The current dense O(N²) matrix assembly and O(N³) direct solve are practical up to approximately
N = 2000 segments on a developer workstation and N = 500 on Raspberry Pi class hardware. Beyond
those thresholds, memory and time become user-visible constraints. Three acceleration strategies are
compared here.

### Adaptive Cross Approximation (ACA)

**What it is**: ACA is a purely algebraic method for compressing the MoM impedance matrix Z. For
two groups of segments that are well-separated in space, the off-diagonal block Z_{IJ} (rows from
group I, columns from group J) has low numerical rank — because the Green's function kernel
e^{-jkr}/r is smooth for segment pairs far apart. ACA finds the factorization:

```
Z_{IJ} ≈ U · V^T      (rank r << min(|I|, |J|))
```

using an iterative pivot-selection procedure: it samples rows and columns of Z_{IJ} adaptively,
stopping when the approximation error falls below a tolerance. No physics knowledge is required —
ACA treats Z as a black-box matrix.

**Why it fits fnec-rust's MoM solver**:

- The dense Z assembly code in `nec_solver` already produces the correct matrix; ACA operates on
  top of it without changing the physics model.
- ACA compression can be added as a post-assembly step: assemble Z densely (existing path), then
  apply hierarchical ACA to compress far-field blocks. Near-field blocks (close segment pairs)
  are kept dense.
- The compressed representation (H-matrix) supports fast matrix-vector products O(N log N),
  enabling iterative solvers (GMRES, BiCGSTAB) to replace direct LU factorization for large N.
- Correctness is easy to verify: compress at different tolerances and compare against the
  uncompressed solution for the same deck.

**Expected scaling**:

| N segments | Dense Z memory | ACA-compressed | Direct LU time | GMRES + ACA time |
|:----------:|:-------------:|:--------------:|:--------------:|:----------------:|
| 500 | 4 MB | ~1 MB | < 1 s | negligible |
| 2 000 | 64 MB | ~8 MB | ~1 s | ~0.5 s |
| 5 000 | 400 MB | ~20 MB | ~30 s | ~5 s |
| 20 000 | 6.4 GB (unusable) | ~80 MB | not feasible | ~30 s |

(Estimates assume complex128; ACA compression ratio ~20× for typical thin-wire geometries.)

**Implementation prerequisites**: dense Z assembly must be validated and corpus-gated (Phase 1/2).
ACA itself is a Phase 5 addition to `nec_accel` or a new `nec_solver::compress` module.

**Recommended Rust crate**: no mature Rust ACA crate exists at the time of writing; implementation
from scratch is ~1 000 LoC for the core pivot loop and H-matrix arithmetic. Alternatively,
`ndarray` + a BLAS backend can be used for the rank-r factorization steps.

---

### Fast Multipole Method (FMM)

**What it is**: FMM is a physics-based hierarchical method. Segments are organized into an
oct-tree. Far-field interactions between well-separated groups are computed via multipole
expansions (outgoing) and local expansions (incoming). The matrix is never assembled explicitly;
instead, the matrix-vector product Z·I is evaluated directly in O(N log N) or O(N) time,
enabling large-scale iterative solvers.

**Complexity comparison**:

| Method | Memory | Matrix-vector product | Direct solve |
|:-------|:------:|:---------------------:|:------------:|
| Dense | O(N²) | O(N²) per iteration | O(N³) LU |
| ACA / H-matrix | O(N log N) | O(N log N) | O(N log² N) |
| FMM | O(N) | O(N log N) or O(N) | iterative only |

**Why harder than ACA**:

- Requires an explicit multipole expansion of the Green's function for 3D EM (vector spherical
  harmonics, Gegenbauer series, or the MLFMA translation operators). This is physics-specific
  code — not algebraic.
- Requires an iterative solver (GMRES is standard for EFIE; BiCGSTAB is an alternative). Direct
  factorization is not available.
- Convergence of GMRES on the EFIE/Hallén system can be slow without a preconditioner; adding a
  suitable preconditioner (diagonal, near-field ILU, or sparse approximate inverse) is additional
  engineering work.
- The implementation effort is 5–10× larger than ACA.

**Recommended path**: implement ACA first (algebraic, directly verifiable, lower risk), validate
against the full corpus, then evaluate FMM only if N > 5 000 becomes a practical need. ACA covers
the realistic problem sizes for fnec-rust's target hardware (workstation, RPi 4/5) through Phase 5.

---

### Canning's Simply Sparse (Impedance Matrix Localization)

The Simply Sparse / IML method applies a basis transformation T so that Z' = T† Z T is sparse,
then solves Z' I' = V' using sparse factorization.

**Assessment**: less suitable than ACA or FMM for fnec-rust:

1. **Not urgent at current problem sizes**: the corpus uses N ≈ 51–500 segments; the scaling
   wall is not yet a practical constraint.
2. **Harder to verify**: the basis transformation introduces a mapping between physical wire
   currents and transformed basis currents; subtle accuracy issues are harder to diagnose than
   in ACA (where the uncompressed dense result is always available for comparison).
3. **Less well-tested in practice**: ACA and FMM have been independently validated in multiple
   MoM codes (FEKO, Altair Feko, openEMS-ACA patches, MLFMA research codes); Canning IML has
   fewer independent implementations.
4. **No fill-free sparse factorization**: the "no fill-in" property holds only approximately
   for realistic geometries; in practice, the sparse solver still fills significantly.

**Recommendation**: evaluate alongside ACA at Phase 5, but do not prioritize over ACA. If ACA
delivers the required scaling, IML provides no additional benefit worth the implementation cost.

---

### Scaling threshold summary

| Hardware | Dense solve ceiling | ACA recommended from | FMM needed from |
|:---------|:-------------------:|:--------------------:|:---------------:|
| Developer workstation (64 GB RAM) | N ≈ 2 000 | N > 2 000 | N > 20 000 |
| Raspberry Pi 4 (4 GB RAM) | N ≈ 500 | N > 500 | N > 5 000 |
| Raspberry Pi 5 (8 GB RAM) | N ≈ 700 | N > 700 | N > 7 000 |

These are fnec-rust's planning thresholds for Phase 5 large-problem work.

---

## Architecture implications if hybrid MoM-UTD is pursued

### What must be added (Phase 5+)

1. **UTD geometry engine**: Representation of large-body objects as canonical shapes (flat polygons for
   wedge edges, cylinders for curved surfaces). Input could be a new deck card class (e.g. `SP` for
   scattering patch — currently marked OUT OF SCOPE but could be re-scoped for UTD canonical shapes).

2. **UTD diffraction coefficient library**: At minimum, Kouyoumjian-Pathak PEC wedge coefficients. Add
   Lertwiriyaprapa et al. material-boundary coefficients if material surfaces are in scope.

3. **Ray-tracing engine**: For each wire-segment pair, enumerate all significant ray paths (direct, singly
   diffracted at each edge, doubly diffracted). Spatial acceleration (BVH or grid) required for
   platforms with many edges.

4. **Impedance matrix modification**: Add `Z_mm^{UTD}` contributions into the existing Hallén/Pocklington
   matrix assembly. This is the narrowest integration point — the solver itself does not change.

5. **Validation corpus**: Platform-effect reference cases in `corpus/`. FEKO (hybrid mode) or NEC-BSC
   can generate ground-truth outputs for hybrid MoM-UTD validation.

### What does NOT change

- The Hallén MoM solver and matrix assembly.
- The parser (except possibly new geometry card syntax).
- The report contract.
- The corpus validation framework.

This clean separation means hybrid MoM-UTD can be implemented as an **optional layer** that modifies the
matrix fill step, with zero impact on the existing solver paths when UTD objects are absent.

---

## Recommendation summary

| Item | Assessment | Priority | Suggested phase |
|:-----|:-----------|:--------:|:----------------|
| Hybrid MoM-UTD for finite ground planes | High value — closes a real capability gap (finite ground, radials) | Medium | Phase 4–5 |
| Hybrid MoM-UTD for antenna-on-platform | High value — addresses a common professional use case (vehicles, ships) | Medium | Phase 5 |
| Lertwiriyaprapa et al. material-boundary UTD | Specialized — relevant for radome/metamaterial antenna work only | Low | Phase 5+ or later |
| DTIC ADA305743 (Rousseau & Pathak — TD-UTD) | Time-domain UTD; relevant only for transient/pulsed analysis or future broadband sweep → TD conversion | Reference | Phase 5+ or later |
| **Adaptive Cross Approximation (ACA)** | **Preferred first MoM acceleration step — algebraic, verifiable, ~1000 LoC core** | **Medium** | **Phase 5** |
| **Fast Multipole Method (FMM)** | **Gold standard for N > 20 000; higher implementation cost — pursue after ACA** | **Low** | **Phase 5+** |
| Canning Simply Sparse (IML) | Applicable to MoM scaling but harder to verify and less proven than ACA | Low | Phase 5 (evaluate alongside ACA, deprioritize if ACA suffices) |

### Concrete next step (pre-Phase 5)

Before Phase 5 GPU work begins, add a single planning document or section to `docs/architecture.md`
that:

1. Names hybrid MoM-UTD as a Phase 5 roadmap candidate for finite-ground and antenna-on-platform use cases.
2. Identifies the impedance matrix modification point (`assemble_z_matrix_with_ground` or a new
   `assemble_z_matrix_with_utd_bodies`) as the integration boundary.
3. References NEC-BSC (architecture TBD — accession not yet located), Kouyoumjian & Pathak (1974), and Lertwiriyaprapa et al. (2007) as
   the foundational UTD literature.
4. Names FMM and ACA as the preferred candidates for large-N MoM matrix acceleration over Canning IML.
5. Defines the minimum corpus case for hybrid validation: a thin-wire dipole next to a flat PEC plate,
   with a FEKO reference output.

This is a one-hour documentation task that preserves the architectural decision record without committing
implementation effort before Phase 1/2 are solid.

---

## References

- R. G. Kouyoumjian and P. H. Pathak, "A uniform geometrical theory of diffraction for an edge in a
  perfectly conducting surface," *Proc. IEEE*, vol. 62, no. 11, pp. 1448–1461, Nov. 1974.
- T. Lertwiriyaprapa, P. H. Pathak, and J. L. Volakis, "A Uniform Geometrical Theory of Diffraction for
  predicting fields of sources near or on thin planar positive/negative material discontinuities,"
  *Radio Science*, vol. 42, no. 6, 2007. doi:10.1029/2007RS003689.
- P. R. Rousseau and P. H. Pathak, "Time Domain Version of the Uniform Geometrical Theory of
  Diffraction," OSU ElectroScience Laboratory Technical Report 721564-3, February 1996.
  DTIC accession ADA305743. (Develops TD-UTD via Analytic Time Transform for curved PEC wedge and
  smooth convex PEC surface scattering; ONR Contract N00014-91-J-1013.)
- F. X. Canning and K. Rogovin, "A universal matrix solver for integral-equation-based problems," *IEEE
  Antennas Propagat. Mag.*, vol. 45, no. 1, pp. 19–26, Feb. 2003.
- F. X. Canning, "Inversion of sparse MoM matrices without fill in," 2007.
- J. M. Rius et al., "Multilevel matrix decomposition algorithm for analysis of electrically large
  electromagnetic problems in 3-D," *Microwave Opt. Technol. Lett.*, 2000. (FMM reference for comparison.)
- M. Bebendorf, "Approximation of boundary element matrices," *Numerische Mathematik*, vol. 86, pp.
  565–589, 2000. (ACA reference for comparison.)
