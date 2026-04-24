---
project: fnec-rust
doc: docs/applied-math.md
status: living
last_updated: 2026-04-24
---

# Applied Math Reference

## Symbols

- f: frequency [Hz]
- omega = 2*pi*f
- c0: speed of light in vacuum
- mu0, eps0: free-space constants
- eta0 = sqrt(mu0/eps0)
- k = omega/c0 = 2*pi/lambda
- G(R) = exp(-j*k*R)/R

## Thin-wire EFIE (axial component)

For wire current I(z') and test point z:

E_inc(z) + E_scat(z) = 0

With standard potential form:

E_scat = -j*omega*A_t - grad_t(Phi)

For a filamentary z-directed wire,

A_z(z) = (mu0/(4*pi)) * integral I(z') * G(R) dz'

and scalar-potential contribution from charge (via continuity equation) introduces derivative terms in Pocklington form.

## Pocklington operator form

A common axial operator form is:

E_scat(z) = -(j*omega*mu0/(4*pi)) * integral I(z') * [ k^2 + d^2/dz^2 ] G(R) dz'

After integration by parts (piecewise basis), endpoint derivative terms appear.

## Hallen form

Hallen recasts the wire equation into a first-kind integral equation with homogeneous terms, commonly represented as:

integral I(z') * G(R) dz' = F(z) + C1*cos(k*z) + C2*sin(k*z)

For symmetric center-fed dipoles, symmetry often removes one homogeneous component (implementation-dependent convention).

## Segment discretization notes

Given segment length dl and center-fed voltage source V0:

- Field-like RHS convention (point matching):
  rhs[m] may be represented in V/m units.
- Feedpoint reporting must remain:
  Z_in = V_source / I_source.

If internal rhs stores V0/dl at the driven segment, then:

V_source = (V0/dl) * dl

## Basis continuity and conditioning

Pulse basis (piecewise-constant current) allows per-segment jumps and can require strong cancellation of large terms. This often degrades conditioning in thin-wire EFIE systems.

Continuity-enforcing basis sets (rooftop/triangular or sinusoidal-like segment coupling) reduce non-physical junction behavior and generally improve practical convergence of feedpoint impedance.

## Useful matrix relationships

Given a pulse-basis segment-current vector I_seg and a continuity basis vector a with transformation T:

I_seg = T * a

Then original system Z * I_seg = v becomes:

Z * T * a = v

A normal-equation solve (or QR/SVD preferred) can be formed as:

(T^H * Z^H * Z * T) * a = T^H * Z^H * v

Recover segment currents by I_seg = T*a after solve.

## Numerical remarks

- Reduced-kernel self terms usually require singularity subtraction:
  G(R_eff) = [G(R_eff)-1/R_eff] + 1/R_eff,
  with analytic integral for 1/R_eff part.
- Direct normal equations amplify conditioning issues; QR/SVD is typically more robust for overdetermined augmented systems.
- Constraint enforcement (tip current, symmetry, feed normalization) should be explicit and unit-tested.

## KaTeX Formula Equivalents

$$
\omega = 2\pi f
$$

$$
\eta_0 = \sqrt{\frac{\mu_0}{\varepsilon_0}}, \qquad
k = \frac{\omega}{c_0} = \frac{2\pi}{\lambda}
$$

$$
G(R) = \frac{e^{-jkR}}{R}
$$

$$
E_{\mathrm{inc}}(z) + E_{\mathrm{scat}}(z) = 0
$$

$$
E_{\mathrm{scat}} = -j\omega A_t - \nabla_t \Phi
$$

$$
A_z(z) = \frac{\mu_0}{4\pi}\int I(z')\,G(R)\,dz'
$$

$$
E_{\mathrm{scat}}(z) = -\frac{j\omega\mu_0}{4\pi}\int I(z')\left[k^2 + \frac{d^2}{dz^2}\right]G(R)\,dz'
$$

$$
\int I(z')\,G(R)\,dz' = F(z) + C_1\cos(kz) + C_2\sin(kz)
$$

$$
Z_{\mathrm{in}} = \frac{V_{\mathrm{source}}}{I_{\mathrm{source}}}, \qquad
V_{\mathrm{source}} = \left(\frac{V_0}{dl}\right)dl
$$

$$
I_{\mathrm{seg}} = Ta, \qquad
ZTa = v, \qquad
\left(T^H Z^H Z T\right)a = T^H Z^H v
$$

$$
G(R_{\mathrm{eff}})=\left[G(R_{\mathrm{eff}})-\frac{1}{R_{\mathrm{eff}}}\right]+\frac{1}{R_{\mathrm{eff}}}
$$
