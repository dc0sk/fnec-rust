---
project: fnec-rust
doc: docs/applied-math.md
status: living
last_updated: 2026-04-24
---

# Applied Math Reference

## Overview

This document provides the theoretical foundation for fnec-rust's antenna solver. The solver uses the **Method of Moments (MoM)** to discretize Maxwell's equations, specifically the Electric Field Integral Equation (EFIE) or Hallén equation for thin-wire antennas.

**Key concepts:**
- **Integral equation**: The field scattered by a wire is expressed as a convolution of the unknown current with the free-space Green's function.
- **Discretization**: Wires are divided into segments, and current is approximated using basis functions (pulse, rooftop, or sinusoidal).
- **Point matching (collocation)**: The integral equation is enforced at test points (one per segment), producing a linear system $Z \cdot I = v$.
- **Matrix assembly**: The impedance matrix $Z$ contains entries computed via Green's function integrals; the RHS $v$ encodes excitation and boundary conditions.
- **Solution**: After solving for segment currents, feedpoint impedance is computed as $Z_{\mathrm{in}} = V_{\mathrm{source}} / I_{\mathrm{source}}$.

Formulations differ in:
- **EFIE vs. Hallén**: EFIE is a second-kind equation; Hallén is first-kind with homogeneous solutions.
- **Basis functions**: Pulse basis is simple but ill-conditioned; continuity-enforcing bases (rooftop, sinusoidal) improve convergence.
- **Numerical solution**: Direct methods (QR, SVD) are preferred over normal equations for stability.

See individual sections below for mathematical details on each formulation and practical solver notes.

## Symbols

- f: frequency [Hz]
- omega = 2*pi*f
  $$\omega = 2\pi f$$
- c0: speed of light in vacuum
- mu0, eps0: free-space constants
- eta0 = sqrt(mu0/eps0) and k = omega/c0 = 2*pi/lambda
  $$\eta_0 = \sqrt{\frac{\mu_0}{\varepsilon_0}}, \qquad k = \frac{\omega}{c_0} = \frac{2\pi}{\lambda}$$
- G(R) = exp(-j*k*R)/R (free-space Green's function)
  $$G(R) = \frac{e^{-jkR}}{R}$$

## Thin-wire EFIE (axial component)

For wire current I(z') and test point z, the boundary condition is:

E_inc(z) + E_scat(z) = 0
$$E_{\mathrm{inc}}(z) + E_{\mathrm{scat}}(z) = 0$$

With standard potential form:

E_scat = -j*omega*A_t - grad_t(Phi)
$$E_{\mathrm{scat}} = -j\omega A_t - \nabla_t \Phi$$

For a filamentary z-directed wire:

A_z(z) = (mu0/(4*pi)) * integral I(z') * G(R) dz'
$$A_z(z) = \frac{\mu_0}{4\pi}\int I(z')\,G(R)\,dz'$$

and scalar-potential contribution from charge (via continuity equation) introduces derivative terms in Pocklington form.

## Pocklington operator form

A common axial operator form is:

E_scat(z) = -(j*omega*mu0/(4*pi)) * integral I(z') * [ k^2 + d^2/dz^2 ] G(R) dz'
$$E_{\mathrm{scat}}(z) = -\frac{j\omega\mu_0}{4\pi}\int I(z')\left[k^2 + \frac{d^2}{dz^2}\right]G(R)\,dz'$$

After integration by parts (piecewise basis), endpoint derivative terms appear.

## Hallen form

Hallen recasts the wire equation into a first-kind integral equation with homogeneous terms, commonly represented as:

integral I(z') * G(R) dz' = F(z) + C1*cos(k*z) + C2*sin(k*z)
$$\int I(z')\,G(R)\,dz' = F(z) + C_1\cos(kz) + C_2\sin(kz)$$

For symmetric center-fed dipoles, symmetry often removes one homogeneous component (implementation-dependent convention).

## Segment discretization notes

Given segment length dl and center-fed voltage source V0:

- Field-like RHS convention (point matching):
  rhs[m] may be represented in V/m units.
- Feedpoint reporting must remain:
  Z_in = V_source / I_source
  $$Z_{\mathrm{in}} = \frac{V_{\mathrm{source}}}{I_{\mathrm{source}}}$$

If internal rhs stores V0/dl at the driven segment, then:

V_source = (V0/dl) * dl
$$V_{\mathrm{source}} = \left(\frac{V_0}{dl}\right)dl$$

## Basis continuity and conditioning

Pulse basis (piecewise-constant current) allows per-segment jumps and can require strong cancellation of large terms. This often degrades conditioning in thin-wire EFIE systems.

Continuity-enforcing basis sets (rooftop/triangular or sinusoidal-like segment coupling) reduce non-physical junction behavior and generally improve practical convergence of feedpoint impedance.

## Useful matrix relationships

Given a pulse-basis segment-current vector I_seg and a continuity basis vector a with transformation T:

I_seg = T * a
$$I_{\mathrm{seg}} = Ta$$

Then original system Z * I_seg = v becomes:

Z * T * a = v
$$ZTa = v$$

A normal-equation solve (or QR/SVD preferred) can be formed as:

(T^H * Z^H * Z * T) * a = T^H * Z^H * v
$$\left(T^H Z^H Z T\right)a = T^H Z^H v$$

Recover segment currents by I_seg = T*a after solve.

## Numerical remarks

- Reduced-kernel self terms usually require singularity subtraction:
  G(R_eff) = [G(R_eff)-1/R_eff] + 1/R_eff
  $$G(R_{\mathrm{eff}})=\left[G(R_{\mathrm{eff}})-\frac{1}{R_{\mathrm{eff}}}\right]+\frac{1}{R_{\mathrm{eff}}}$$
  with analytic integral for 1/R_eff part.
- Direct normal equations amplify conditioning issues; QR/SVD is typically more robust for overdetermined augmented systems.
- Constraint enforcement (tip current, symmetry, feed normalization) should be explicit and unit-tested.

## Experimental solver residual budgets

The fallback-capable experimental modes use explicit residual budgets as safety rails, not as parity claims.

- `continuity`: relative L2 residual budget is `1e-3` (`CONTINUITY_REL_RESIDUAL_MAX` in `apps/nec-cli/src/main.rs`). If the continuity-basis solve exceeds this threshold, the CLI emits a warning and falls back to the pulse solution path.
- `sinusoidal`: relative L2 residual budget is `1e-2` (`SINUSOIDAL_REL_RESIDUAL_MAX` in `apps/nec-cli/src/main.rs`). If the projected sinusoidal-basis solve exceeds this threshold on a topology that otherwise passes the A4 topology gate, the CLI emits a warning and falls back to Hallen.
- `sin_rel_res` in CLI diagnostics records the pre-fallback sinusoidal residual so automation can distinguish a successful sinusoidal solve from a guarded fallback.

These thresholds are intentionally stricter than "solver did not crash" but looser than the compatibility tolerance matrix. They exist to prevent experimental modes from silently returning numerically poor results while a more stable fallback exists.

## Scoped finite-ground approximation (GN0/GN2)

The current GN0/GN2 implementation is a scoped finite-ground approximation, not a full Sommerfeld/Norton ground solver.

- Runtime model: Hallen image contribution is scaled by a complex Fresnel-style reflection factor derived from `EPSE` and `SIG`.
- Current validation scope: above-ground wire cases that are explicitly locked in corpus CI (`dipole-gn0-fresnel-51seg`, `dipole-gn2-deferred`, and `dipole-gn2-near-ground-51seg`).
- Current non-goals: buried conductors (`z < 0`), loop/patch/surface classes, and broad claims of NEC-4-class accuracy outside the contracted corpus cases.

Known limitations of this approximation class:

- very low-height conductors where full Sommerfeld current distribution and surface-wave effects dominate
- strongly conductive or highly frequency-sensitive soil cases beyond the current regression set
- geometries whose ground interaction is not well captured by a single image/reflection-factor correction

Promotion path:

1. Keep expanding externally captured near-ground and finite-conductivity corpus fixtures.
2. Document the validity envelope of the Fresnel-style approximation against those fixtures.
3. Replace or complement the current approximation with a fuller Sommerfeld/Norton implementation when Phase 2 ground scope requires it.
