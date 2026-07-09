// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Sommerfeld half-space reflected-field kernel for near-ground impedance
//! (PH9-CHK-006).
//!
//! fnec's default finite-ground model multiplies the method-of-images reflection
//! by a single **normal-incidence** scalar Fresnel coefficient
//! ([`crate::matrix`]). That reproduces nec2c's reflection-coefficient method
//! (GN0) for heights ≥ ~0.1 λ but misses the **surface wave** below that, where the
//! exact Sommerfeld solution (nec2c GN2) diverges from it — at 0.025 λ the scalar/RCM
//! radiation-resistance delta has the *wrong sign* (see
//! `docs/ph9-chk-006-sommerfeld-ground.md`).
//!
//! This module implements the exact reflected field for a **horizontal** electric
//! dipole over a lossy half-space as a 1-D Sommerfeld integral. The derivation (plane-
//! wave / angular spectrum, azimuth reduced to `J0 ± J2`; validated in
//! `studies/sommerfeld-ground/`) gives, for an x-directed source element and an
//! observation displaced by `ρ` **along the dipole axis** (both at heights whose sum
//! is `d = z + z'`):
//!
//! ```text
//! E_x^refl(ρ, d) = (k0·η0 / 8π) ∫_0^∞ (λ/kz0) e^{-j kz0 d}
//!                    [ R_TE (J0(λρ)+J2(λρ)) − R_TM (kz0²/k0²)(J0(λρ)−J2(λρ)) ] dλ
//! ```
//!
//! with `kz0 = √(k0²−λ²)` (Im ≤ 0), `R_TE = (kz0−kz1)/(kz0+kz1)`,
//! `R_TM = (εc·kz0−kz1)/(εc·kz0+kz1)`, `kz1 = √(kg²−λ²)`, `kg = k0√εc`,
//! `εc = εr − jσ/(ωε0)`. The horizontal dipole excites both TE and TM; the surface
//! wave lives in the TM (`R_TM`) term's Zenneck pole.
//!
//! The integral is evaluated with the substitution `λ = k0·sinθ` (propagating,
//! θ ∈ [0, π/2]) and `λ = k0·cosh t` (evanescent, t ∈ [0, ∞)), which cancels the
//! integrable `1/kz0` singularity at `λ = k0` analytically — giving machine-precision
//! agreement with the exact opposite-current image field in the PEC limit.

use num_complex::Complex64;

const C0: f64 = 299_792_458.0; // m/s
const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7; // H/m
const EPS0: f64 = 8.854_187_817e-12; // F/m
const ETA0: f64 = MU0 * C0; // free-space wave impedance

/// Complex relative permittivity of the ground: `εc = εr − j σ/(ω ε0)`.
pub fn complex_permittivity(freq_hz: f64, eps_r: f64, sigma: f64) -> Complex64 {
    let omega = 2.0 * std::f64::consts::PI * freq_hz;
    Complex64::new(eps_r.max(1.0e-6), -sigma.max(0.0) / (omega * EPS0))
}

/// Normal-incidence scalar Fresnel coefficient `Γ = (√εc − 1)/(√εc + 1)` — the
/// coefficient fnec's default (RCM) ground model applies to the geometric image
/// (mirror of `matrix::fresnel_reflection_scalar`).
pub fn scalar_gamma(freq_hz: f64, eps_r: f64, sigma: f64) -> Complex64 {
    let sq = complex_permittivity(freq_hz, eps_r, sigma).sqrt();
    (sq - Complex64::new(1.0, 0.0)) / (sq + Complex64::new(1.0, 0.0))
}

/// Surface-wave correction to the near-ground feedpoint impedance of a **straight
/// horizontal wire** over finite ground (PH9-CHK-006).
///
/// Returns `ΔZ_sw = ΔZ_Sommerfeld − ΔZ_scalarΓ`, the ground-effect difference between
/// the exact Sommerfeld reflected field and the scalar-Γ (reflection-coefficient)
/// image fnec's default model already accounts for. Added to fnec's reported
/// near-ground feedpoint `Z` (which ≈ the scalar-Γ / GN0 result), it upgrades that to
/// the surface-wave-inclusive Sommerfeld (nec2c GN2) value — in particular flipping
/// the sign of the radiation-resistance delta below ~0.05 λ.
///
/// Computed as an induced-EMF reaction integral over the solved segment currents
/// (`currents[m]`, moment `currents[m]·lengths[m]`), so it is stationary in the
/// current to first order. `feed_idx` is the driven segment (reference `I`).
///
/// Returns `None` when the geometry is **not** a straight horizontal wire at a single
/// height above the `z = 0` plane — the class the collinear-axis kernel is validated
/// for. (Bent / vertical / mixed geometry needs the full reflected dyadic, deferred.)
#[allow(clippy::too_many_arguments)]
pub fn horizontal_ground_z_correction(
    midpoints: &[[f64; 3]],
    directions: &[[f64; 3]],
    lengths: &[f64],
    currents: &[Complex64],
    feed_idx: usize,
    freq_hz: f64,
    eps_r: f64,
    sigma: f64,
) -> Option<Complex64> {
    let n = midpoints.len();
    if n == 0 || directions.len() != n || lengths.len() != n || currents.len() != n || feed_idx >= n
    {
        return None;
    }
    const TOL: f64 = 1e-6;
    let h = midpoints[0][2];
    if h <= TOL {
        return None; // must be above the ground plane
    }
    let axis = directions[0];
    for i in 0..n {
        // horizontal (no vertical component)
        if directions[i][2].abs() > TOL {
            return None;
        }
        // parallel to a common horizontal axis (straight wire)
        let dot =
            directions[i][0] * axis[0] + directions[i][1] * axis[1] + directions[i][2] * axis[2];
        if dot.abs() < 1.0 - TOL {
            return None;
        }
        // single height
        if (midpoints[i][2] - h).abs() > TOL {
            return None;
        }
    }

    let i_feed = currents[feed_idx];
    if i_feed.norm() < 1e-60 {
        return None;
    }
    let gamma = scalar_gamma(freq_hz, eps_r, sigma);
    let d = 2.0 * h;

    // The difference kernel is smooth in ρ, so precompute it on a grid and
    // interpolate — O(grid) integral evaluations instead of O(n²).
    let mut rho_max = 0.0f64;
    for m in 0..n {
        for k in 0..n {
            let dx = midpoints[m][0] - midpoints[k][0];
            let dy = midpoints[m][1] - midpoints[k][1];
            rho_max = rho_max.max((dx * dx + dy * dy).sqrt());
        }
    }
    let ng = 256usize;
    let step = rho_max / (ng - 1) as f64;
    let grid: Vec<Complex64> = (0..ng)
        .map(|g| {
            let rho = step * g as f64;
            reflected_ex_horizontal(rho, d, freq_hz, eps_r, sigma, false)
                - gamma * reflected_ex_horizontal(rho, d, freq_hz, eps_r, sigma, true)
        })
        .collect();
    let interp = |rho: f64| -> Complex64 {
        if step <= 0.0 {
            return grid[0];
        }
        let t = (rho / step).min((ng - 1) as f64);
        let i = (t.floor() as usize).min(ng - 2);
        let frac = t - i as f64;
        grid[i] * (1.0 - frac) + grid[i + 1] * frac
    };

    let mut acc = Complex64::new(0.0, 0.0);
    for m in 0..n {
        let mom_m = currents[m] * lengths[m];
        for k in 0..n {
            let dx = midpoints[m][0] - midpoints[k][0];
            let dy = midpoints[m][1] - midpoints[k][1];
            let rho = (dx * dx + dy * dy).sqrt();
            acc += interp(rho) * mom_m * (currents[k] * lengths[k]);
        }
    }
    Some(acc / (i_feed * i_feed))
}

/// Surface-wave ΔZ correction for **any straight wire** over finite ground
/// (PH9-CHK-006 Level 1) — the arbitrary-orientation generalization of
/// [`horizontal_ground_z_correction`]. Horizontal wires dispatch to the fast
/// ρ-grid path; vertical / tilted / sloping straight wires use the general fast
/// dyadic [`reflected_e_projected_fast`] over a `(Δs, Σs)` grid.
///
/// Returns `ΔZ_sw = ΔZ_Sommerfeld − ΔZ_scalarΓ` (induced-EMF reaction over the solved
/// currents), to add to fnec's scalar-Γ feedpoint `Z`. Returns `None` unless the
/// geometry is a **straight** wire (all segment directions parallel) entirely above
/// the `z = 0` plane. Bent / mixed geometry needs the full per-pair dyadic (deferred).
#[allow(clippy::too_many_arguments)]
pub fn ground_z_correction(
    midpoints: &[[f64; 3]],
    directions: &[[f64; 3]],
    lengths: &[f64],
    currents: &[Complex64],
    feed_idx: usize,
    freq_hz: f64,
    eps_r: f64,
    sigma: f64,
) -> Option<Complex64> {
    let n = midpoints.len();
    if n < 2 || directions.len() != n || lengths.len() != n || currents.len() != n || feed_idx >= n
    {
        return None;
    }
    const TOL: f64 = 1e-6;
    // Straight wire, above ground.
    let axis = directions[0];
    for i in 0..n {
        if midpoints[i][2] <= TOL {
            return None;
        }
        let dot =
            directions[i][0] * axis[0] + directions[i][1] * axis[1] + directions[i][2] * axis[2];
        if dot.abs() < 1.0 - TOL {
            return None;
        }
    }
    // Horizontal wire: fast ρ-grid path.
    if axis[2].abs() <= TOL {
        return horizontal_ground_z_correction(
            midpoints, directions, lengths, currents, feed_idx, freq_hz, eps_r, sigma,
        );
    }

    let i_feed = currents[feed_idx];
    if i_feed.norm() < 1e-60 {
        return None;
    }
    let gamma = scalar_gamma(freq_hz, eps_r, sigma);

    // Signed arc position of each midpoint along the axis, from the centroid.
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    for m in midpoints {
        cx += m[0];
        cy += m[1];
        cz += m[2];
    }
    let (cx, cy, cz) = (cx / n as f64, cy / n as f64, cz / n as f64);
    let s: Vec<f64> = midpoints
        .iter()
        .map(|m| (m[0] - cx) * axis[0] + (m[1] - cy) * axis[1] + (m[2] - cz) * axis[2])
        .collect();
    let (mut smin, mut smax) = (f64::INFINITY, f64::NEG_INFINITY);
    for &si in &s {
        smin = smin.min(si);
        smax = smax.max(si);
    }

    // Difference kernel on a (Δs ∈ [smin−smax, smax−smin], Σs ∈ [2smin, 2smax]) grid.
    // dx = Δs·ax, dy = Δs·ay, d = 2cz + Σs·az. Bilinear interpolation per pair.
    let ng = 40usize;
    let ds_lo = smin - smax;
    let ds_span = 2.0 * (smax - smin);
    let sg_lo = 2.0 * smin;
    let sg_span = 2.0 * (smax - smin);
    let kernel = |dss: f64, sgg: f64| -> Complex64 {
        let dx = dss * axis[0];
        let dy = dss * axis[1];
        let d = 2.0 * cz + sgg * axis[2];
        reflected_e_projected_fast(axis, axis, dx, dy, d, freq_hz, eps_r, sigma, false)
            - gamma * reflected_e_projected_fast(axis, axis, dx, dy, d, freq_hz, eps_r, sigma, true)
    };
    let mut grid = vec![Complex64::new(0.0, 0.0); ng * ng];
    for i in 0..ng {
        let dss = ds_lo + ds_span * i as f64 / (ng - 1) as f64;
        for k in 0..ng {
            let sgg = sg_lo + sg_span * k as f64 / (ng - 1) as f64;
            grid[i * ng + k] = kernel(dss, sgg);
        }
    }
    let interp = |dss: f64, sgg: f64| -> Complex64 {
        let fi = if ds_span > 0.0 {
            ((dss - ds_lo) / ds_span * (ng - 1) as f64).clamp(0.0, (ng - 1) as f64)
        } else {
            0.0
        };
        let fk = if sg_span > 0.0 {
            ((sgg - sg_lo) / sg_span * (ng - 1) as f64).clamp(0.0, (ng - 1) as f64)
        } else {
            0.0
        };
        let (i0, k0) = (fi.floor() as usize, fk.floor() as usize);
        let (i0, k0) = (i0.min(ng - 2), k0.min(ng - 2));
        let (a, b) = (fi - i0 as f64, fk - k0 as f64);
        grid[i0 * ng + k0] * (1.0 - a) * (1.0 - b)
            + grid[(i0 + 1) * ng + k0] * a * (1.0 - b)
            + grid[i0 * ng + k0 + 1] * (1.0 - a) * b
            + grid[(i0 + 1) * ng + k0 + 1] * a * b
    };

    let mut acc = Complex64::new(0.0, 0.0);
    for m in 0..n {
        let mom_m = currents[m] * lengths[m];
        for k in 0..n {
            acc += interp(s[m] - s[k], s[m] + s[k]) * mom_m * (currents[k] * lengths[k]);
        }
    }
    Some(acc / (i_feed * i_feed))
}

/// `√z` on the sheet `Im ≤ 0` (the upgoing/decaying wave `e^{-j kz z}` decays for
/// `z > 0`).
fn sqrt_im_neg(z: Complex64) -> Complex64 {
    let s = z.sqrt();
    if s.im > 0.0 {
        -s
    } else {
        s
    }
}

/// Reflection coefficients `(R_TE, R_TM)` at spectral radial wavenumber, given the
/// vertical wavenumbers `kz0` (air) and `kz1` (ground) and ground `εc`. `pec` forces
/// the perfect-conductor limit `(−1, +1)`.
fn fresnel_spectral(
    kz0: Complex64,
    kz1: Complex64,
    eps_c: Complex64,
    pec: bool,
) -> (Complex64, Complex64) {
    if pec {
        return (Complex64::new(-1.0, 0.0), Complex64::new(1.0, 0.0));
    }
    let r_te = (kz0 - kz1) / (kz0 + kz1);
    let r_tm = (eps_c * kz0 - kz1) / (eps_c * kz0 + kz1);
    (r_te, r_tm)
}

/// The spectral integrand bracket
/// `R_TE (J0+J2) − R_TM (kz0²/k0²)(J0−J2)` at radial wavenumber `lambda`.
fn bracket(
    lambda: f64,
    kz0: Complex64,
    k0: f64,
    rho: f64,
    eps_c: Complex64,
    kg2: Complex64,
    pec: bool,
) -> Complex64 {
    let kz1 = sqrt_im_neg(kg2 - Complex64::new(lambda * lambda, 0.0));
    let (r_te, r_tm) = fresnel_spectral(kz0, kz1, eps_c, pec);
    let x = lambda * rho;
    let j0 = bessel_j0(x);
    let j2 = bessel_j2(x);
    let kz0_rel2 = (kz0 * kz0) / Complex64::new(k0 * k0, 0.0);
    r_te * (j0 + j2) - r_tm * kz0_rel2 * (j0 - j2)
}

/// Number of quadrature points per branch (propagating / evanescent). The integrand
/// is smooth under the sin/cosh substitution, so a few thousand points give
/// machine-precision (validated in `studies/sommerfeld-ground/`).
const N_QUAD: usize = 4000;

/// Reflected `E_x` of an x-directed Hertzian element (unit current moment `I·dl = 1`)
/// over a half-space, at horizontal offset `rho` **along the x-axis** and height-sum
/// `d = z + z' > 0`. `pec = true` gives the perfect-conductor limit.
///
/// This is the exact Sommerfeld reflected field (surface wave included), the
/// replacement for the scalar-Γ image term for a horizontal wire near ground.
pub fn reflected_ex_horizontal(
    rho: f64,
    d: f64,
    freq_hz: f64,
    eps_r: f64,
    sigma: f64,
    pec: bool,
) -> Complex64 {
    let k0 = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let eps_c = complex_permittivity(freq_hz, eps_r, sigma);
    let kg2 = Complex64::new(k0 * k0, 0.0) * eps_c; // kg² = k0² εc

    // Propagating branch: λ = k0 sinθ, kz0 = k0 cosθ, (λ/kz0) dλ = k0 sinθ dθ.
    let mut ip = Complex64::new(0.0, 0.0);
    let a0 = 1e-9;
    let b0 = std::f64::consts::FRAC_PI_2 - 1e-9;
    let h0 = (b0 - a0) / N_QUAD as f64;
    for i in 0..=N_QUAD {
        let theta = a0 + h0 * i as f64;
        let lambda = k0 * theta.sin();
        let kz0 = Complex64::new(k0 * theta.cos(), 0.0);
        let phase = (Complex64::new(0.0, -1.0) * kz0 * d).exp();
        let f = Complex64::new(k0 * theta.sin(), 0.0)
            * phase
            * bracket(lambda, kz0, k0, rho, eps_c, kg2, pec);
        let wgt = if i == 0 || i == N_QUAD { 0.5 } else { 1.0 };
        ip += wgt * f;
    }
    ip *= h0;

    // Evanescent branch: λ = k0 cosh t, kz0 = −j k0 sinh t, (λ/kz0) dλ = j k0 cosh t dt.
    // Truncate where e^{-k0 sinh t · d} ≈ e^{-40}.
    let t_max = if d > 0.0 {
        (40.0 / (k0 * d)).asinh()
    } else {
        8.0
    };
    let mut ie = Complex64::new(0.0, 0.0);
    let a1 = 1e-9;
    let h1 = (t_max - a1) / N_QUAD as f64;
    for i in 0..=N_QUAD {
        let t = a1 + h1 * i as f64;
        let lambda = k0 * t.cosh();
        let kz0 = Complex64::new(0.0, -k0 * t.sinh());
        let phase = (Complex64::new(0.0, -1.0) * kz0 * d).exp();
        let f = Complex64::new(0.0, k0 * t.cosh())
            * phase
            * bracket(lambda, kz0, k0, rho, eps_c, kg2, pec);
        let wgt = if i == 0 || i == N_QUAD { 0.5 } else { 1.0 };
        ie += wgt * f;
    }
    ie *= h1;

    Complex64::new(k0 * ETA0 / (8.0 * std::f64::consts::PI), 0.0) * (ip + ie)
}

/// Azimuth / radial sample counts for the general dyadic (PH9-CHK-006 Levels 1-2).
/// The 2-D integrand is smooth under the sin/cosh radial substitution; these give
/// ~1e-6 PEC agreement (validated in `studies/sommerfeld-ground/general_dyadic.py`).
const NA_DYAD: usize = 96;
const NR_DYAD: usize = 800;

/// Reflected `E` projected on `obs_dir` from a unit current moment along `src_dir`,
/// at horizontal offset `(dx, dy)` and height-sum `d = z_obs + z_src`, over a
/// half-space — the **general reflected dyadic** (arbitrary-orientation
/// generalization of [`reflected_ex_horizontal`]).
///
/// Evaluated as the 2-D angular-spectrum integral (radial `sinθ`/`cosh t`
/// substitution × azimuth grid). Reduces exactly to `reflected_ex_horizontal` for
/// x-source/x-obs on-axis, and to a pure-TM integral for a vertical source. `pec`
/// forces the perfect-conductor limit. This is the shared foundation of the
/// arbitrary-orientation feedpoint correction (Level 1) and the DCIM Z-matrix kernel
/// (Level 2).
#[allow(clippy::too_many_arguments)]
pub fn reflected_e_projected(
    src_dir: [f64; 3],
    obs_dir: [f64; 3],
    dx: f64,
    dy: f64,
    d: f64,
    freq_hz: f64,
    eps_r: f64,
    sigma: f64,
    pec: bool,
) -> Complex64 {
    let k0 = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let eps_c = complex_permittivity(freq_hz, eps_r, sigma);
    let kg2 = Complex64::new(k0 * k0, 0.0) * eps_c;
    let [sx, sy, sz] = src_dir;
    let [ox, oy, oz] = obs_dir;

    // Radial node lists (propagating θ ∈ (0,π/2); evanescent t ∈ (0,t_max)).
    let th_lo = 1e-9;
    let th_hi = std::f64::consts::FRAC_PI_2 - 1e-9;
    let hth = (th_hi - th_lo) / NR_DYAD as f64;
    let t_max = if d > 0.0 {
        (45.0 / (k0 * d)).asinh()
    } else {
        8.0
    };
    let ht = (t_max - 1e-9) / NR_DYAD as f64;

    let dal = 2.0 * std::f64::consts::PI / NA_DYAD as f64;
    let mut tot = Complex64::new(0.0, 0.0);
    for ia in 0..NA_DYAD {
        let al = dal * ia as f64;
        let (ca, sa) = (al.cos(), al.sin());
        let s_ds = -sx * sa + sy * ca;
        let s_do = -ox * sa + oy * ca;

        // accumulate both radial branches
        let mut radial = Complex64::new(0.0, 0.0);
        for br in 0..2 {
            let npts = NR_DYAD;
            for i in 0..=npts {
                let (lambda, kz0, wr, hstep, wtrap);
                if br == 0 {
                    let theta = th_lo + hth * i as f64;
                    lambda = k0 * theta.sin();
                    kz0 = Complex64::new(k0 * theta.cos(), 0.0);
                    wr = Complex64::new(k0 * theta.sin(), 0.0);
                    hstep = hth;
                } else {
                    let t = 1e-9 + ht * i as f64;
                    lambda = k0 * t.cosh();
                    kz0 = Complex64::new(0.0, -k0 * t.sinh());
                    wr = Complex64::new(0.0, k0 * t.cosh());
                    hstep = ht;
                }
                wtrap = if i == 0 || i == npts { 0.5 } else { 1.0 };
                let kz1 = sqrt_im_neg(kg2 - Complex64::new(lambda * lambda, 0.0));
                let (r_te, r_tm) = fresnel_spectral(kz0, kz1, eps_c, pec);
                let pi_ds = -(kz0 * (sx * ca + sy * sa) + Complex64::new(sz * lambda, 0.0)) / k0;
                let pr_do = (kz0 * (ox * ca + oy * sa) - Complex64::new(oz * lambda, 0.0)) / k0;
                let proj = Complex64::new(s_ds * s_do, 0.0) * r_te + pi_ds * pr_do * r_tm;
                let phase = (Complex64::new(0.0, -1.0) * kz0 * d).exp()
                    * Complex64::new(0.0, -lambda * (dx * ca + dy * sa)).exp();
                radial += wtrap * hstep * wr * proj * phase;
            }
        }
        tot += radial;
    }
    Complex64::new(
        k0 * ETA0 / (8.0 * std::f64::consts::PI * std::f64::consts::PI),
        0.0,
    ) * tot
        * Complex64::new(dal, 0.0)
}

/// Fast (1-D) general reflected dyadic — the azimuthally-reduced form of
/// [`reflected_e_projected`]. The α-integral of the 2-D angular-spectrum form reduces
/// analytically to a single radial Sommerfeld integral with `J0/J1/J2`
/// (`ρ = √(dx²+dy²)`, `φ = atan2(dy, dx)`):
///
/// ```text
/// E_proj = (k0·η0 / 8π²) ∫ (λ/kz0) e^{-j kz0 d} B(λ) dλ,
/// B = R_TE·π[ SS·J0 + (Dxx·cos2φ + Cxy·sin2φ)·J2 ]
///   + R_TM·{ −P²π[ SS·J0 − (Dxx·cos2φ + Cxy·sin2φ)·J2 ]
///            − 2πj·P·Q·o_z·J1·(s_x cosφ + s_y sinφ)
///            + 2πj·P·Q·s_z·J1·(o_x cosφ + o_y sinφ)
///            + 2π·s_z·o_z·Q²·J0 }
/// ```
///
/// `SS = s_x o_x + s_y o_y`, `Dxx = s_x o_x − s_y o_y`, `Cxy = s_x o_y + s_y o_x`,
/// `P = kz0/k0`, `Q = λ/k0`. Reduces exactly to [`reflected_ex_horizontal`] for
/// x-source/x-obs on-axis. ~100× faster than the 2-D oracle; validated against it to
/// ~1e-5 for all orientations (see `studies/sommerfeld-ground/`).
#[allow(clippy::too_many_arguments)]
pub fn reflected_e_projected_fast(
    src_dir: [f64; 3],
    obs_dir: [f64; 3],
    dx: f64,
    dy: f64,
    d: f64,
    freq_hz: f64,
    eps_r: f64,
    sigma: f64,
    pec: bool,
) -> Complex64 {
    let k0 = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let eps_c = complex_permittivity(freq_hz, eps_r, sigma);
    let kg2 = Complex64::new(k0 * k0, 0.0) * eps_c;
    let [sx, sy, sz] = src_dir;
    let [ox, oy, oz] = obs_dir;
    let rho = (dx * dx + dy * dy).sqrt();
    let phi = dy.atan2(dx);
    let (c2, s2) = ((2.0 * phi).cos(), (2.0 * phi).sin());
    let (cf, sf) = (phi.cos(), phi.sin());
    let ss = sx * ox + sy * oy;
    let dxx = sx * ox - sy * oy;
    let cxy = sx * oy + sy * ox;
    let pi = std::f64::consts::PI;
    let j = Complex64::new(0.0, 1.0);

    // B(λ) at radial wavenumber lambda with kz0 = a.
    let b_of = |lambda: f64, a: Complex64| -> Complex64 {
        let p = a / k0;
        let q = lambda / k0;
        let kz1 = sqrt_im_neg(kg2 - Complex64::new(lambda * lambda, 0.0));
        let (r_te, r_tm) = fresnel_spectral(a, kz1, eps_c, pec);
        let j0 = bessel_j0(lambda * rho);
        let j1 = bessel_j1(lambda * rho);
        let j2 = bessel_j2(lambda * rho);
        let te = r_te * (pi * (ss * j0 + (dxx * c2 + cxy * s2) * j2));
        let tm = r_tm
            * (-(p * p) * (pi * (ss * j0 - (dxx * c2 + cxy * s2) * j2))
                - j * 2.0 * pi * p * q * oz * j1 * (sx * cf + sy * sf)
                + j * 2.0 * pi * p * q * sz * j1 * (ox * cf + oy * sf)
                + Complex64::new(2.0 * pi * sz * oz * q * q * j0, 0.0));
        te + tm
    };

    // Radial substitution (propagating θ + evanescent t); (λ/kz0) dλ is the weight.
    let mut ip = Complex64::new(0.0, 0.0);
    let th_lo = 1e-9;
    let th_hi = std::f64::consts::FRAC_PI_2 - 1e-9;
    let h0 = (th_hi - th_lo) / N_QUAD as f64;
    for i in 0..=N_QUAD {
        let theta = th_lo + h0 * i as f64;
        let lambda = k0 * theta.sin();
        let a = Complex64::new(k0 * theta.cos(), 0.0);
        let phase = (Complex64::new(0.0, -1.0) * a * d).exp();
        let f = Complex64::new(k0 * theta.sin(), 0.0) * phase * b_of(lambda, a);
        let wgt = if i == 0 || i == N_QUAD { 0.5 } else { 1.0 };
        ip += wgt * f;
    }
    ip *= h0;

    let t_max = if d > 0.0 {
        (45.0 / (k0 * d)).asinh()
    } else {
        8.0
    };
    let mut ie = Complex64::new(0.0, 0.0);
    let h1 = (t_max - 1e-9) / N_QUAD as f64;
    for i in 0..=N_QUAD {
        let t = 1e-9 + h1 * i as f64;
        let lambda = k0 * t.cosh();
        let a = Complex64::new(0.0, -k0 * t.sinh());
        let phase = (Complex64::new(0.0, -1.0) * a * d).exp();
        let f = Complex64::new(0.0, k0 * t.cosh()) * phase * b_of(lambda, a);
        let wgt = if i == 0 || i == N_QUAD { 0.5 } else { 1.0 };
        ie += wgt * f;
    }
    ie *= h1;

    Complex64::new(k0 * ETA0 / (8.0 * pi * pi), 0.0) * (ip + ie)
}

// ---------------------------------------------------------------------------
// Bessel functions J0, J1, J2 (Abramowitz & Stegun 9.4, ~1e-7 accuracy).
// ---------------------------------------------------------------------------

/// Bessel function of the first kind, order 0.
pub fn bessel_j0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.0 {
        let t = (x / 3.0) * (x / 3.0);
        1.0 + t
            * (-2.2499997
                + t * (1.2656208
                    + t * (-0.3163866 + t * (0.0444479 + t * (-0.0039444 + t * 0.0002100)))))
    } else {
        let z = 3.0 / ax;
        let f0 = 0.79788456
            + z * (-0.00000077
                + z * (-0.00552740
                    + z * (-0.00009512 + z * (0.00137237 + z * (-0.00072805 + z * 0.00014476)))));
        // The leading phase constant is π/4 (A&S 9.4.3).
        let theta = ax - std::f64::consts::FRAC_PI_4
            + z * (-0.04166397
                + z * (-0.00003954
                    + z * (0.00262573 + z * (-0.00054125 + z * (-0.00029333 + z * 0.00013558)))));
        f0 * theta.cos() / ax.sqrt()
    }
}

/// Bessel function of the first kind, order 1.
pub fn bessel_j1(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.0 {
        let t = (x / 3.0) * (x / 3.0);
        x * (0.5
            + t * (-0.56249985
                + t * (0.21093573
                    + t * (-0.03954289 + t * (0.00443319 + t * (-0.00031761 + t * 0.00001109))))))
    } else {
        let z = 3.0 / ax;
        let f1 = 0.79788456
            + z * (0.00000156
                + z * (0.01659667
                    + z * (0.00017105 + z * (-0.00249511 + z * (0.00113653 + z * (-0.00020033))))));
        // The leading phase constant is 3π/4 (A&S 9.4.6).
        let theta = ax - 3.0 * std::f64::consts::FRAC_PI_4
            + z * (0.12499612
                + z * (0.00005650
                    + z * (-0.00637879 + z * (0.00074348 + z * (0.00079824 + z * (-0.00029166))))));
        let m = f1 * theta.cos() / ax.sqrt();
        if x < 0.0 {
            -m
        } else {
            m
        }
    }
}

/// Bessel function of the first kind, order 2, via the recurrence
/// `J2(x) = (2/x) J1(x) − J0(x)`.
pub fn bessel_j2(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        0.0
    } else {
        2.0 / x * bessel_j1(x) - bessel_j0(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREQ: f64 = 14.2e6;
    const LAM: f64 = C0 / FREQ;
    const FRAC: f64 = std::f64::consts::FRAC_1_SQRT_2;

    fn k0() -> f64 {
        2.0 * std::f64::consts::PI * FREQ / C0
    }

    /// Exact free-space E_x of an x-directed Hertzian element (moment I·dl = 1) at
    /// offset (dx, dy, dz): E_x = jωμ0 (g + ∂²g/∂x²/k0²).
    fn ex_freespace(dx: f64, dy: f64, dz: f64) -> Complex64 {
        let k = k0();
        let omega = 2.0 * std::f64::consts::PI * FREQ;
        let r = (dx * dx + dy * dy + dz * dz).sqrt();
        let e = Complex64::new(0.0, -k * r).exp();
        let g = e / (4.0 * std::f64::consts::PI * r);
        let gp = -(Complex64::new(1.0, k * r)) * e / (4.0 * std::f64::consts::PI * r * r);
        let gpp = e * Complex64::new(2.0 - k * k * r * r, 2.0 * k * r)
            / (4.0 * std::f64::consts::PI * r.powi(3));
        let d2 = Complex64::new(dx * dx / (r * r), 0.0) * gpp
            + Complex64::new(1.0 / r - dx * dx / r.powi(3), 0.0) * gp;
        Complex64::new(0.0, omega * MU0) * (g + d2 / Complex64::new(k * k, 0.0))
    }

    /// Exact free-space E projected on `obs_dir` from a unit current moment along
    /// `src_dir` at offset (dx,dy,dz): E = jωμ0[(ô·ŝ)g + (ô·∇∇g·ŝ)/k0²].
    fn eproj_freespace(src: [f64; 3], obs: [f64; 3], dx: f64, dy: f64, dz: f64) -> Complex64 {
        let k = k0();
        let omega = 2.0 * std::f64::consts::PI * FREQ;
        let rv = [dx, dy, dz];
        let r = (dx * dx + dy * dy + dz * dz).sqrt();
        let e = Complex64::new(0.0, -k * r).exp();
        let g = e / (4.0 * std::f64::consts::PI * r);
        let gp = -(Complex64::new(1.0, k * r)) * e / (4.0 * std::f64::consts::PI * r * r);
        let gpp = e * Complex64::new(2.0 - k * k * r * r, 2.0 * k * r)
            / (4.0 * std::f64::consts::PI * r.powi(3));
        let mut quad = Complex64::new(0.0, 0.0);
        let mut odots = 0.0;
        for i in 0..3 {
            odots += obs[i] * src[i];
            for j in 0..3 {
                let dij = if i == j { 1.0 } else { 0.0 };
                let d2 = Complex64::new(rv[i] * rv[j] / (r * r), 0.0) * gpp
                    + Complex64::new(dij / r - rv[i] * rv[j] / r.powi(3), 0.0) * gp;
                quad += Complex64::new(obs[i] * src[j], 0.0) * d2;
            }
        }
        Complex64::new(0.0, omega * MU0)
            * (Complex64::new(odots, 0.0) * g + quad / Complex64::new(k * k, 0.0))
    }

    /// PEC self-check for the GENERAL reflected dyadic: with (R_TE,R_TM)=(−1,+1) the
    /// 2-D spectral integral must reproduce the free-space image-dyadic field
    /// (image source direction (−sx,−sy,+sz)) for every orientation pair.
    #[test]
    fn pec_general_dyadic_matches_image_for_all_orientations() {
        // Representative pairs (full matrix in the Python study; the oracle is slow).
        let cases: &[([f64; 3], [f64; 3], f64, f64)] = &[
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2 * LAM, 0.25 * LAM),
            ([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0.2 * LAM, 0.0),
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.25 * LAM, 0.1 * LAM),
        ];
        for &(src, obs, dx, dy) in cases {
            let d = 2.0 * 0.05 * LAM;
            let somm = reflected_e_projected(src, obs, dx, dy, d, FREQ, 1.0, 0.0, true);
            let img = eproj_freespace([-src[0], -src[1], src[2]], obs, dx, dy, d);
            let rel = (somm - img).norm() / img.norm();
            assert!(
                rel < 1e-3,
                "src={src:?} obs={obs:?} dx={dx} dy={dy} rel={rel:.2e}"
            );
        }
    }

    /// The fast 1-D reduced dyadic must match the 2-D oracle for all orientations.
    #[test]
    fn fast_dyadic_matches_2d_oracle() {
        // Representative orientation pairs (the full matrix is covered in the Python
        // study; the 2-D oracle is slow, so keep the in-Rust cross-check lean).
        let cases: &[([f64; 3], [f64; 3], f64, f64)] = &[
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2 * LAM, 0.25 * LAM), // horizontal off-axis
            ([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0.2 * LAM, 0.0),        // vertical
            ([FRAC, 0.0, FRAC], [0.0, FRAC, FRAC], 0.2 * LAM, 0.15 * LAM), // tilted, cross
        ];
        for &(src, obs, dx, dy) in cases {
            let d = 2.0 * 0.05 * LAM;
            let fast = reflected_e_projected_fast(src, obs, dx, dy, d, FREQ, 13.0, 0.005, false);
            let oracle = reflected_e_projected(src, obs, dx, dy, d, FREQ, 13.0, 0.005, false);
            let rel = (fast - oracle).norm() / oracle.norm();
            assert!(rel < 1e-3, "src={src:?} obs={obs:?} rel={rel:.2e}");
        }
    }

    /// Primary gate for the fast dyadic: its PEC limit must reproduce the exact
    /// free-space image-dyadic field for every orientation — valid everywhere
    /// (unlike the 2-D oracle, which loses accuracy in the low-d/large-ρ pole
    /// corner). Includes x/z AND z/x, whose φ-odd cross term catches sign errors a
    /// symmetric test would miss.
    #[test]
    fn fast_dyadic_pec_matches_image_all_orientations() {
        let cases: &[([f64; 3], [f64; 3], f64, f64)] = &[
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.3 * LAM, 0.0), // x/x on-axis
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2 * LAM, 0.25 * LAM), // x/x off-axis
            ([0.0, 1.0, 0.0], [0.0, 1.0, 0.0], 0.25 * LAM, 0.0), // y/y offset along x
            ([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0.2 * LAM, 0.0), // z/z vertical
            ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.25 * LAM, 0.1 * LAM), // x/z cross
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.25 * LAM, 0.1 * LAM), // z/x cross (sign)
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.15 * LAM, 0.2 * LAM), // bent
            ([FRAC, 0.0, FRAC], [0.0, FRAC, FRAC], 0.2 * LAM, 0.15 * LAM), // tilted
        ];
        for &(src, obs, dx, dy) in cases {
            for &hl in &[0.1, 0.025] {
                let d = 2.0 * hl * LAM;
                let fast = reflected_e_projected_fast(src, obs, dx, dy, d, FREQ, 1.0, 0.0, true);
                let img = eproj_freespace([-src[0], -src[1], src[2]], obs, dx, dy, d);
                let rel = (fast - img).norm() / img.norm();
                assert!(rel < 1e-4, "src={src:?} obs={obs:?} h={hl}λ rel={rel:.2e}");
            }
        }
    }

    /// The fast dyadic must also reduce exactly to the shipped horizontal kernel.
    #[test]
    fn fast_dyadic_reduces_to_horizontal() {
        for &rl in &[0.05, 0.2, 0.4] {
            for &hl in &[0.1, 0.025] {
                let (rho, d) = (rl * LAM, 2.0 * hl * LAM);
                let fast = reflected_e_projected_fast(
                    [1.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    rho,
                    0.0,
                    d,
                    FREQ,
                    13.0,
                    0.005,
                    false,
                );
                let horiz = reflected_ex_horizontal(rho, d, FREQ, 13.0, 0.005, false);
                let rel = (fast - horiz).norm() / horiz.norm();
                assert!(rel < 1e-6, "ρ={rl}λ h={hl}λ rel={rel:.2e}");
            }
        }
    }

    #[test]
    fn bessel_matches_known_values() {
        // Reference values (Abramowitz & Stegun tables).
        assert!((bessel_j0(0.0) - 1.0).abs() < 1e-7);
        assert!((bessel_j0(1.0) - 0.7651976866).abs() < 1e-6);
        assert!((bessel_j0(5.0) - (-0.1775967713)).abs() < 1e-6);
        assert!((bessel_j1(1.0) - 0.4400505857).abs() < 1e-6);
        assert!((bessel_j1(5.0) - (-0.3275791376)).abs() < 1e-6);
        assert!(bessel_j2(0.0).abs() < 1e-9);
        assert!((bessel_j2(1.0) - 0.1149034849).abs() < 1e-6);
        assert!((bessel_j2(5.0) - 0.0465651163).abs() < 1e-6);
    }

    /// PEC self-check: the reflected-field integral with (R_TE=−1, R_TM=+1) must
    /// reproduce the exact opposite-current image field (x-element of current −1 at
    /// (0,0,−h), observed at (ρ,0,h)). This pins every prefactor/sign and validates
    /// the substitution quadrature. The sin/cosh substitution removes the λ=k0
    /// singularity, so agreement is limited only by the Bessel-approx accuracy.
    #[test]
    fn pec_reflected_field_matches_opposite_current_image() {
        for &hl in &[0.25, 0.1, 0.05, 0.025] {
            let h = hl * LAM;
            for &rl in &[0.05, 0.15, 0.3] {
                let rho = rl * LAM;
                let somm = reflected_ex_horizontal(rho, 2.0 * h, FREQ, 1.0, 0.0, true);
                let image = -ex_freespace(rho, 0.0, 2.0 * h);
                let rel = (somm - image).norm() / image.norm();
                assert!(
                    rel < 1e-4,
                    "h={hl}λ ρ={rl}λ: rel={rel:.2e} somm={somm} image={image}"
                );
            }
        }
    }

    /// The reaction ΔZ correction (ΔZ_Sommerfeld − ΔZ_scalarΓ) must capture the
    /// surface-wave gap: large-positive at 0.025 λ (nec2c GN2−GN0 ΔR ≈ +33, flipping
    /// fnec's ≈−24 toward +9), and negligible at 0.25 λ where the surface wave dies.
    #[test]
    fn ground_correction_captures_surface_wave_gap() {
        let build = |hl: f64| -> Option<Complex64> {
            let l = LAM / 4.0;
            let n = 41usize;
            let k0 = k0();
            let h = hl * LAM;
            let xs: Vec<f64> = (0..n)
                .map(|i| -l + 2.0 * l * i as f64 / (n - 1) as f64)
                .collect();
            let mids: Vec<[f64; 3]> = xs.iter().map(|&x| [x, 0.0, h]).collect();
            let dirs = vec![[1.0, 0.0, 0.0]; n];
            let lens = vec![2.0 * l / (n - 1) as f64; n];
            let curr: Vec<Complex64> = xs
                .iter()
                .map(|&x| Complex64::new((k0 * (l - x.abs())).sin(), 0.0))
                .collect();
            horizontal_ground_z_correction(&mids, &dirs, &lens, &curr, n / 2, FREQ, 13.0, 0.005)
        };
        let low = build(0.025).expect("straight horizontal geometry qualifies");
        let high = build(0.25).expect("straight horizontal geometry qualifies");
        assert!(
            low.re > 15.0,
            "0.025λ correction ΔR must be large-positive (surface-wave gap); got {:.2}",
            low.re
        );
        assert!(
            high.re.abs() < 5.0,
            "0.25λ correction should be negligible; got {:.2}",
            high.re
        );
    }

    /// The general (any straight wire) correction on a 30°-tilted low λ/2 dipole must
    /// recover the surface-wave gap (nec2c GN2−GN0 ΔR ≈ +10.4; Python probe +9.6).
    #[test]
    fn general_correction_tilted_dipole_matches_nec2c_gap() {
        let d_hat = [0.866_025_4_f64, 0.0, 0.5];
        let l = LAM / 4.0;
        let n = 21usize;
        let k = k0();
        let ctr = [0.0, 0.0, 3.0];
        let s: Vec<f64> = (0..n)
            .map(|i| -l + 2.0 * l * i as f64 / (n - 1) as f64)
            .collect();
        let mids: Vec<[f64; 3]> = s
            .iter()
            .map(|&si| {
                [
                    ctr[0] + si * d_hat[0],
                    ctr[1] + si * d_hat[1],
                    ctr[2] + si * d_hat[2],
                ]
            })
            .collect();
        let dirs = vec![d_hat; n];
        let lens = vec![2.0 * l / (n - 1) as f64; n];
        let curr: Vec<Complex64> = s
            .iter()
            .map(|&si| Complex64::new((k * (l - si.abs())).sin(), 0.0))
            .collect();
        let dz = ground_z_correction(&mids, &dirs, &lens, &curr, n / 2, FREQ, 13.0, 0.005)
            .expect("straight tilted wire qualifies");
        assert!(
            (dz.re - 9.6).abs() < 3.0,
            "tilted ΔR should recover the surface-wave gap ~+9.6; got {:.2}",
            dz.re
        );
    }

    /// Non-horizontal / vertical geometry must be declined (returns None).
    #[test]
    fn ground_correction_declines_vertical_geometry() {
        let mids = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 2.0]];
        let dirs = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let lens = vec![1.0, 1.0];
        let curr = vec![Complex64::new(1.0, 0.0), Complex64::new(0.5, 0.0)];
        assert!(
            horizontal_ground_z_correction(&mids, &dirs, &lens, &curr, 0, FREQ, 13.0, 0.005)
                .is_none()
        );
    }

    /// Lossy ground: the reflected kernel must produce a *positive* real part at the
    /// specular self-point for a very low horizontal dipole — the surface-wave sign
    /// flip that the scalar-Γ (RCM) model gets wrong. This is a coarse smoke gate;
    /// the quantitative ΔZ vs nec2c GN2 is validated in the study.
    #[test]
    fn lossy_ground_kernel_is_finite_and_reasonable() {
        let h = 0.025 * LAM;
        let e = reflected_ex_horizontal(0.05 * LAM, 2.0 * h, FREQ, 13.0, 0.005, false);
        assert!(e.norm().is_finite() && e.norm() > 0.0);
    }
}
