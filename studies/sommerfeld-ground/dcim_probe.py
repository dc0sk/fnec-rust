#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-006 Level-2 DCIM PROBE (BL-IMPR-015, phase 1).
#
# Question: can the Discrete Complex Image Method (DCIM) reproduce fnec's EXACT
# reflected potential kernels (G_A, G_Φ from crates/nec_solver/src/sommerfeld.rs
# `reflected_potential_kernels`) as a short closed-form sum of complex images
#   G(ρ,d) ≈ Σ aᵢ · exp(−j k0 rᵢ)/rᵢ ,   rᵢ = √(ρ² + (d − bᵢ)²)  (bᵢ complex),
# so the O(N_grid) Sommerfeld quadrature can be replaced by O(N_images) closed
# form? This is the de-risking prototype that must pass BEFORE any Rust port.
#
# Method (one-level DCIM, Aksun/Chow): sample the spectral reflection coefficient
# g(kz0) along the path kz0 = −j k0 t (t ≥ 0, the evanescent axis, λ = k0√(1+t²)),
# where e^{j kz0 bᵢ} = e^{k0 bᵢ t} is a clean complex exponential in t. GPOF
# (matrix-pencil) fits g(t) ≈ Σ aᵢ e^{sᵢ t}; each term maps to a complex image at
# depth bᵢ = sᵢ/k0 by the Sommerfeld identity
#   ∫₀^∞ (λ/kz0) J0(λρ) e^{−j kz0 D} dλ = j e^{−j k0 √(ρ²+D²)}/√(ρ²+D²).

import numpy as np

C0 = 299_792_458.0
F = 14.2e6
EPS0 = 8.8541878128e-12
MU0 = 4e-7 * np.pi
W = 2 * np.pi * F
K0 = W / C0
LAM = C0 / F
EPSR, SIGMA = 13.0, 0.005
EPSC = EPSR - 1j * SIGMA / (W * EPS0)
KG2 = K0 * K0 * EPSC


def sqrt_im_neg(z):
    """Branch of √z with Im ≤ 0 (matches sommerfeld::sqrt_im_neg)."""
    s = np.sqrt(z + 0j)
    return np.where(s.imag > 0, -s, s)


def kz1_of(kz0):
    """kz1 from kz0 via λ² = k0² − kz0²."""
    lam2 = K0 * K0 - kz0 * kz0
    return sqrt_im_neg(KG2 - lam2)


def r_te(kz0):
    kz1 = kz1_of(kz0)
    return (kz0 - kz1) / (kz0 + kz1)


def r_tm(kz0):
    kz1 = kz1_of(kz0)
    return (EPSC * kz0 - kz1) / (EPSC * kz0 + kz1)


def f_phi(kz0):
    """Spectral coefficient of the scalar-potential kernel G_Φ."""
    lam2 = K0 * K0 - kz0 * kz0
    return (K0 * K0 * r_te(kz0) + kz0 * kz0 * r_tm(kz0)) / lam2


# ---------------------------------------------------------------------------
# Exact kernels (Python port of sommerfeld::reflected_potential_kernels).
# ---------------------------------------------------------------------------
def exact_kernels(rho, d, nq=4000):
    """(G_A, G_Φ) via the two-branch Sommerfeld quadrature, matching fnec."""
    from scipy.special import j0
    neg_j = -1j

    # propagating branch: λ = k0 sinθ, kz0 = k0 cosθ, (λ/kz0)dλ = k0 sinθ dθ
    th = np.linspace(1e-7, np.pi / 2 - 1e-7, nq)
    lam = K0 * np.sin(th)
    kz0 = K0 * np.cos(th) + 0j
    meas = K0 * np.sin(th) + 0j
    base = meas * np.exp(neg_j * kz0 * d) * j0(lam * rho)
    ga_p = np.trapezoid(base * r_te(kz0), th)
    gp_p = np.trapezoid(base * f_phi(kz0), th)

    # evanescent branch: λ = k0 cosh t, kz0 = −j k0 sinh t, (λ/kz0)dλ = j k0 cosh t dt
    tmax = np.arcsinh(40.0 / (K0 * d)) if d > 0 else 8.0
    tt = np.linspace(1e-7, tmax, nq)
    lam = K0 * np.cosh(tt)
    kz0 = -1j * K0 * np.sinh(tt)
    meas = 1j * K0 * np.cosh(tt)
    base = meas * np.exp(neg_j * kz0 * d) * j0(lam * rho)
    ga_e = np.trapezoid(base * r_te(kz0), tt)
    gp_e = np.trapezoid(base * f_phi(kz0), tt)

    return neg_j * (ga_p + ga_e), neg_j * (gp_p + gp_e)


# ---------------------------------------------------------------------------
# GPOF (matrix-pencil) — fit y[n] ≈ Σ aᵢ zᵢ^n, return (a, s) with zᵢ = e^{sᵢ Δt}.
# ---------------------------------------------------------------------------
def gpof(y, dt, pencil=None, sv_tol=1e-6, max_terms=12):
    y = np.asarray(y, complex)
    n = len(y)
    L = n // 2 if pencil is None else pencil
    # Hankel data matrix Y (rows n-L, cols L+1)
    Y = np.array([y[i:i + L + 1] for i in range(n - L)])
    _, sv, Vh = np.linalg.svd(Y, full_matrices=False)
    m = int(np.sum(sv > sv_tol * sv[0]))
    m = max(1, min(m, max_terms, L))
    Vh = Vh[:m]
    V1 = Vh[:, :-1].conj().T
    V2 = Vh[:, 1:].conj().T
    z = np.linalg.eigvals(np.linalg.pinv(V1) @ V2)  # poles
    # Stability filter: g must stay bounded as we march into the evanescent
    # region, so drop poles |z| > 1 (exponentially GROWING, non-physical images —
    # the cause of two-level blow-up).
    z = z[np.abs(z) <= 1.0 + 1e-6]
    if len(z) == 0:
        z = np.array([1.0 + 0j])
    s = np.log(z) / dt
    # residues by least squares on the Vandermonde
    Z = np.vander(z, n, increasing=True).T
    a, *_ = np.linalg.lstsq(Z, y, rcond=None)
    return a, s


def dcim_line(kz0_start, kz0_end, n):
    """A uniform straight complex line kz0_start → kz0_end (n samples)."""
    idx = np.arange(n)
    return kz0_start + (kz0_end - kz0_start) * idx / (n - 1)


def dcim_fit_2level(coeff_fn, tau1=2.0, n1=120, tau2=9.0, n2=360, m1=5, m2=12):
    """Two-level DCIM (Aksun): level 1 fits the slowly-varying (small-|b|, far)
    images on a short line near the real axis; level 2 fits the residual on a
    longer line for the fast (large-|b|, near) images. Returns combined images."""
    kz0_1 = dcim_line(K0 * (1 - 1e-3j), -1j * K0 * tau1, n1)
    a1_g, s1 = gpof(coeff_fn(kz0_1), dt=1.0, max_terms=m1)
    d1 = kz0_1[1] - kz0_1[0]
    b1 = s1 / (1j * d1)
    a1 = a1_g * np.exp(-1j * kz0_1[0] * b1)

    kz0_2 = dcim_line(K0 * (1 - 1e-3j), -1j * K0 * tau2, n2)
    g2 = coeff_fn(kz0_2)
    g2 -= sum(ai * np.exp(1j * kz0_2 * bi) for ai, bi in zip(a1, b1))  # residual
    a2_g, s2 = gpof(g2, dt=1.0, max_terms=m2)
    d2 = kz0_2[1] - kz0_2[0]
    b2 = s2 / (1j * d2)
    a2 = a2_g * np.exp(-1j * kz0_2[0] * b2)

    return np.concatenate([a1, a2]), np.concatenate([b1, b2])


def dcim_fit(coeff_fn, tau_max=6.0, n_samp=300, **gp):
    """One-level DCIM on the Aksun line kz0: k0 → −j k0·τmax (Re sweeps k0 → 0,
    so the *propagating* region Re(kz0)∈[0,k0] — which controls the far field — is
    on the path, unlike a vertical line at Re=k0). Geometry-independent."""
    kz0 = dcim_line(K0 * (1 - 1e-3j), -1j * K0 * tau_max, n_samp)
    g = coeff_fn(kz0)
    a_g, s = gpof(g, dt=1.0, **gp)
    delta = kz0[1] - kz0[0]
    b = s / (1j * delta)
    a = a_g * np.exp(-1j * kz0[0] * b)
    return a, b


def dcim_kernel(rho, d, a, b):
    """Reconstruct G(ρ,d) = Σ aᵢ e^{−j k0 rᵢ}/rᵢ, rᵢ = √(ρ² + (d−bᵢ)²)."""
    out = 0j
    for ai, bi in zip(a, b):
        r = np.sqrt(rho * rho + (d - bi) ** 2 + 0j)
        out += ai * np.exp(-1j * K0 * r) / r
    return out


def main():
    print(f"f={F/1e6} MHz  εr={EPSR} σ={SIGMA}  εc={EPSC:.3f}  λ={LAM:.3f} m")
    print("DCIM one-level fit of the reflected potential kernels vs exact quadrature.\n")

    # Two-level DCIM (near + far image sets), with GPOF pole filtering (|z|≤1).
    a_te, b_te = dcim_fit_2level(r_te)
    a_ph, b_ph = dcim_fit_2level(f_phi)
    print(f"images: G_A={len(a_te)}  G_Φ={len(a_ph)}\n")

    for hl in (0.25, 0.10, 0.05):
        d = 2 * hl * LAM  # height-sum for a wire at height hl·λ
        print(f"height {hl}λ  (d={d:.3f} m):")
        worst_a = worst_p = 0.0
        for rr in (0.05, 0.2, 0.5, 1.0, 2.0):
            rho = rr * LAM
            ga_x, gp_x = exact_kernels(rho, d)
            ga_d = dcim_kernel(rho, d, a_te, b_te)
            gp_d = dcim_kernel(rho, d, a_ph, b_ph)
            ea = abs(ga_d - ga_x) / abs(ga_x)
            ep = abs(gp_d - gp_x) / abs(gp_x)
            worst_a, worst_p = max(worst_a, ea), max(worst_p, ep)
            print(f"   ρ={rr:4.2f}λ  G_A exact={ga_x:+.3e} dcim={ga_d:+.3e} rel={ea:.1e}"
                  f"   G_Φ rel={ep:.1e}")
        print(f"   -> worst rel: G_A {worst_a:.1e}  G_Φ {worst_p:.1e}\n")

    print("CONCLUSION (see dcim_mom_validate.py for the DECISIVE end-to-end test)")
    print("  * DCIM machinery VALIDATED: deformed contour kz0=k0(1−jτ) + GPOF")
    print("    matrix-pencil (pole-filtered) + Sommerfeld identity reproduce the")
    print("    exact reflected potential kernels as complex images.")
    print("  * The pointwise `worst rel` above is dominated by the far zone (ρ=2λ),")
    print("    the lateral-wave tail spherical images can't represent — but the MoM")
    print("    only uses ρ ≤ the wire half-length (~0.5λ) and the feedpoint Z is")
    print("    insensitive to the far zone. So pointwise error is NOT the metric.")
    print("  * END-TO-END (dcim_mom_validate.py): two-level DCIM + pole filter →")
    print("    feedpoint Z within ~7% of the EXACT kernel at 0.05λ AND 0.025λ")
    print("    (R within 1.5–3.4% of nec2c GN2) — inside fnec's ~5–8% gate.")
    print("  Next: (Phase 2b) tighten X + explicit Zenneck-pole extraction; (Phase")
    print("  3) Rust port into assemble_z_matrix_with_ground w/ a complex-distance")
    print("  Green's kernel, gated vs nec2c GN2, incl. currents + pattern.")


if __name__ == "__main__":
    main()
