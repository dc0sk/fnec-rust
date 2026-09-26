// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

// Numerical matrix algorithms legitimately use index-based loops.
#![allow(clippy::needless_range_loop)]

//! Dense complex linear solver: LU factorisation with partial pivoting.
//!
//! Solves Z·I = V for the segment current vector I.
//!
//! The implementation is a straightforward in-place Gaussian elimination with
//! partial (row) pivoting.  It is adequate for the segment counts expected in
//! Phase 1 (up to a few hundred segments).  A LAPACK or GPU back-end can
//! replace this path later via the `nec_accel` crate.

use num_complex::Complex64;

use crate::basis::{ContinuityTransform, SinusoidalTransform};
use crate::matrix::ZMatrix;

/// Error returned by the linear solver.
#[derive(Debug, Clone, PartialEq)]
pub enum SolveError {
    /// The matrix is singular (or numerically rank-deficient).
    Singular,
    /// The dimensions of Z and V do not match.
    DimensionMismatch { z_n: usize, v_len: usize },
    /// Hallén vectors do not match matrix dimension.
    HallenDimensionMismatch {
        z_n: usize,
        rhs_len: usize,
        cos_len: usize,
    },
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::Singular => write!(f, "impedance matrix is singular"),
            SolveError::DimensionMismatch { z_n, v_len } => {
                write!(f, "Z is {z_n}×{z_n} but V has length {v_len}")
            }
            SolveError::HallenDimensionMismatch {
                z_n,
                rhs_len,
                cos_len,
            } => write!(
                f,
                "Z is {z_n}×{z_n} but Hallen rhs/cos lengths are {rhs_len}/{cos_len}"
            ),
        }
    }
}

impl std::error::Error for SolveError {}

/// Solve Z·I = V using LU factorisation with partial pivoting.
///
/// Returns the segment current vector I (length N).
pub fn solve(z: &ZMatrix, v: &[Complex64]) -> Result<Vec<Complex64>, SolveError> {
    let n = z.n;
    if v.len() != n {
        return Err(SolveError::DimensionMismatch {
            z_n: n,
            v_len: v.len(),
        });
    }

    let mut a: Vec<Vec<Complex64>> = (0..n)
        .map(|i| (0..n).map(|j| z.get(i, j)).collect())
        .collect();
    let mut b = v.to_vec();

    solve_square_in_place(&mut a, &mut b)
}

/// Solve Z*I = V with a continuity-enforcing basis transform.
///
/// For a single segment chain of length N, segment currents are represented as
/// I = T*a where T is the tip-constrained difference transform with N-1
/// unknowns. The transformed least-squares system is solved as:
///
///   min || Z*T*a - V ||
///
/// using regularized normal equations.
pub fn solve_with_continuity_basis(
    z: &ZMatrix,
    v: &[Complex64],
) -> Result<Vec<Complex64>, SolveError> {
    let n = z.n;
    if v.len() != n {
        return Err(SolveError::DimensionMismatch {
            z_n: n,
            v_len: v.len(),
        });
    }

    if n < 2 {
        return solve(z, v);
    }

    let tr = ContinuityTransform::for_single_chain(n);
    let m = tr.n_basis;

    // A = Z * T, dimensions n x m.
    let mut a = vec![vec![Complex64::new(0.0, 0.0); m]; n];
    for (r, a_row) in a.iter_mut().enumerate().take(n) {
        for (c, a_cell) in a_row.iter_mut().enumerate().take(m) {
            let mut sum = Complex64::new(0.0, 0.0);
            for seg in 0..n {
                let t = tr.t[seg][c];
                if t != 0.0 {
                    sum += z.get(r, seg) * t;
                }
            }
            *a_cell = sum;
        }
    }

    // Normal equations: (A^H A + lambda*I) a = A^H v.
    let mut ata = vec![vec![Complex64::new(0.0, 0.0); m]; m];
    let mut atv = vec![Complex64::new(0.0, 0.0); m];
    for i in 0..m {
        for j in 0..m {
            let mut sum = Complex64::new(0.0, 0.0);
            for row in a.iter().take(n) {
                sum += row[i].conj() * row[j];
            }
            ata[i][j] = sum;
        }
        let mut sum = Complex64::new(0.0, 0.0);
        for (row, vv) in a.iter().zip(v.iter()).take(n) {
            sum += row[i].conj() * *vv;
        }
        atv[i] = sum;
    }

    let lambda = regularization_lambda(&ata, 1e-10, 1e-14);
    for (i, row) in ata.iter_mut().enumerate().take(m) {
        row[i] += Complex64::new(lambda, 0.0);
    }

    let a_sol = solve_square_in_place(&mut ata, &mut atv)?;
    Ok(tr.segment_currents(&a_sol))
}

/// Solve Z*I = V with a sine-tapered continuity basis transform.
///
/// This is an incremental milestone toward NEC2-style sinusoidal basis
/// behavior. It keeps the continuity formulation but applies a sinusoidal
/// edge taper on segment currents.
pub fn solve_with_sinusoidal_basis(
    z: &ZMatrix,
    v: &[Complex64],
) -> Result<Vec<Complex64>, SolveError> {
    let n = z.n;
    if v.len() != n {
        return Err(SolveError::DimensionMismatch {
            z_n: n,
            v_len: v.len(),
        });
    }

    if n < 2 {
        return solve(z, v);
    }

    let tr = SinusoidalTransform::for_single_chain(n);
    let (mut projected_z, mut projected_v) = projected_system(z, v, &tr.t);

    let lambda = regularization_lambda(&projected_z, 1e-10, 1e-14);
    for (i, row) in projected_z.iter_mut().enumerate().take(tr.n_basis) {
        row[i] += Complex64::new(lambda, 0.0);
    }

    let a_sol = solve_square_in_place(&mut projected_z, &mut projected_v)?;
    Ok(tr.segment_currents(&a_sol))
}

fn projected_system(
    z: &ZMatrix,
    v: &[Complex64],
    t: &[Vec<f64>],
) -> (Vec<Vec<Complex64>>, Vec<Complex64>) {
    let n = z.n;
    let m = t.first().map_or(0, Vec::len);

    let mut zt = vec![vec![Complex64::new(0.0, 0.0); m]; n];
    for (r, zt_row) in zt.iter_mut().enumerate().take(n) {
        for (c, zt_cell) in zt_row.iter_mut().enumerate().take(m) {
            let mut sum = Complex64::new(0.0, 0.0);
            for (seg, t_row) in t.iter().enumerate().take(n) {
                let coeff = t_row[c];
                if coeff != 0.0 {
                    sum += z.get(r, seg) * coeff;
                }
            }
            *zt_cell = sum;
        }
    }

    let mut projected_z = vec![vec![Complex64::new(0.0, 0.0); m]; m];
    let mut projected_v = vec![Complex64::new(0.0, 0.0); m];
    for i in 0..m {
        for j in 0..m {
            let mut sum = Complex64::new(0.0, 0.0);
            for row in 0..n {
                let coeff = t[row][i];
                if coeff != 0.0 {
                    sum += coeff * zt[row][j];
                }
            }
            projected_z[i][j] = sum;
        }
        let mut sum = Complex64::new(0.0, 0.0);
        for row in 0..n {
            let coeff = t[row][i];
            if coeff != 0.0 {
                sum += coeff * v[row];
            }
        }
        projected_v[i] = sum;
    }

    (projected_z, projected_v)
}

/// Build a block-diagonal sinusoidal transform for `n` segments split into
/// `wire_endpoints` chains.  Each wire `(first, last)` gets its own
/// `SinusoidalTransform::for_single_chain(n_w)` block placed at the
/// corresponding rows/columns of the global transform.
fn build_block_sinusoidal_transform(n: usize, wire_endpoints: &[(usize, usize)]) -> Vec<Vec<f64>> {
    let m: usize = wire_endpoints
        .iter()
        .map(|&(first, last)| (last - first + 1).saturating_sub(1))
        .sum();
    let mut t = vec![vec![0.0f64; m]; n];
    let mut col_offset = 0;
    for &(first, last) in wire_endpoints {
        let n_w = last - first + 1;
        let tr_w = SinusoidalTransform::for_single_chain(n_w);
        for (local_row, t_row) in tr_w.t.iter().enumerate() {
            let global_row = first + local_row;
            for (local_col, &coeff) in t_row.iter().enumerate() {
                if coeff != 0.0 {
                    t[global_row][col_offset + local_col] = coeff;
                }
            }
        }
        col_offset += tr_w.n_basis;
    }
    t
}

/// Build a block-diagonal continuity transform for `n` segments split into
/// `wire_endpoints` chains.
fn build_block_continuity_transform(n: usize, wire_endpoints: &[(usize, usize)]) -> Vec<Vec<f64>> {
    let m: usize = wire_endpoints
        .iter()
        .map(|&(first, last)| (last - first + 1).saturating_sub(1))
        .sum();
    let mut t = vec![vec![0.0f64; m]; n];
    let mut col_offset = 0;
    for &(first, last) in wire_endpoints {
        let n_w = last - first + 1;
        let tr_w = ContinuityTransform::for_single_chain(n_w);
        for (local_row, t_row) in tr_w.t.iter().enumerate() {
            let global_row = first + local_row;
            for (local_col, &coeff) in t_row.iter().enumerate() {
                if coeff != 0.0 {
                    t[global_row][col_offset + local_col] = coeff;
                }
            }
        }
        col_offset += tr_w.n_basis;
    }
    t
}

/// Apply a basis transform T and recover segment currents I = T·a from
/// basis coefficients `a`.
fn basis_to_currents(t: &[Vec<f64>], a: &[Complex64]) -> Vec<Complex64> {
    let n = t.len();
    let mut currents = vec![Complex64::new(0.0, 0.0); n];
    for (row, t_row) in t.iter().enumerate() {
        for (col, &coeff) in t_row.iter().enumerate() {
            if coeff != 0.0 {
                currents[row] += a[col] * coeff;
            }
        }
    }
    currents
}

/// Solve `Z·I = V` using a sinusoidal basis applied independently to each wire
/// chain described by `wire_endpoints`.
///
/// `wire_endpoints` is a slice of `(first, last)` inclusive segment index
/// ranges, one per wire.  Each wire must contain at least two segments; if any
/// wire has fewer the function falls back to `solve(z, v)`.
///
/// The global transform is block-diagonal: wire `k` with `n_k` segments
/// contributes an `n_k × (n_k − 1)` sinusoidal block at offset
/// `(first_k, col_offset_k)`.  Mutual coupling between wires is preserved
/// because the full `Z` matrix is projected rather than solved per wire.
pub fn solve_with_sinusoidal_basis_per_wire(
    z: &ZMatrix,
    v: &[Complex64],
    wire_endpoints: &[(usize, usize)],
) -> Result<Vec<Complex64>, SolveError> {
    let n = z.n;
    if v.len() != n {
        return Err(SolveError::DimensionMismatch {
            z_n: n,
            v_len: v.len(),
        });
    }
    if n < 2 {
        return solve(z, v);
    }
    if wire_endpoints
        .iter()
        .any(|&(first, last)| last < first || last - first < 1)
    {
        return solve(z, v);
    }
    let global_t = build_block_sinusoidal_transform(n, wire_endpoints);
    let m = global_t.first().map_or(0, Vec::len);
    if m == 0 {
        return solve(z, v);
    }
    let (mut proj_z, mut proj_v) = projected_system(z, v, &global_t);
    let lambda = regularization_lambda(&proj_z, 1e-10, 1e-14);
    for i in 0..m {
        proj_z[i][i] += Complex64::new(lambda, 0.0);
    }
    let a_sol = solve_square_in_place(&mut proj_z, &mut proj_v)?;
    Ok(basis_to_currents(&global_t, &a_sol))
}

/// Solve `Z·I = V` using a continuity basis applied independently to each wire
/// chain described by `wire_endpoints`.
///
/// Semantics and fallback rules mirror [`solve_with_sinusoidal_basis_per_wire`].
pub fn solve_with_continuity_basis_per_wire(
    z: &ZMatrix,
    v: &[Complex64],
    wire_endpoints: &[(usize, usize)],
) -> Result<Vec<Complex64>, SolveError> {
    let n = z.n;
    if v.len() != n {
        return Err(SolveError::DimensionMismatch {
            z_n: n,
            v_len: v.len(),
        });
    }
    if n < 2 {
        return solve(z, v);
    }
    if wire_endpoints
        .iter()
        .any(|&(first, last)| last < first || last - first < 1)
    {
        return solve(z, v);
    }
    let global_t = build_block_continuity_transform(n, wire_endpoints);
    let m = global_t.first().map_or(0, Vec::len);
    if m == 0 {
        return solve(z, v);
    }
    let (mut proj_z, mut proj_v) = projected_system(z, v, &global_t);
    let lambda = regularization_lambda(&proj_z, 1e-10, 1e-14);
    for i in 0..m {
        proj_z[i][i] += Complex64::new(lambda, 0.0);
    }
    let a_sol = solve_square_in_place(&mut proj_z, &mut proj_v)?;
    Ok(basis_to_currents(&global_t, &a_sol))
}

/// One linear constraint on the per-segment currents:
/// `val_a · I[col_a] + val_b · I[col_b] = 0`, the second term absent when `col_b`
/// is `None`.
///
/// Primitive on purpose. The GPU resident solve in `nec_accel` consumes exactly
/// these rows and does not depend on this crate, so a named struct here would force
/// a second definition there — and a second copy of a boundary decision is how the
/// original defect survived (FND-156): the GPU built its own end rows.
pub type ConstraintRow = (usize, Option<usize>, f64, f64);

/// The free-end boundary row: the wire current, extrapolated linearly from the end
/// segment's midpoint through its inner neighbour's to the PHYSICAL wire end, is
/// zero.
///
/// **This replaces `I[end] = 0`, which was the defect in FND-156.** A pulse current
/// is sampled at its segment's midpoint, so pinning `I[end] = 0` put the zero half
/// a segment inside the wire at each end — every wire was modelled one segment
/// short. On the corpus half-wave dipole that is 74.24+j13.90 Ω against nec2c's
/// 79.35+j46.22 Ω; with this row it is 78.83+j42.44 Ω. The remaining reactance gap
/// to nec2c at the same segment count is 7.0, 3.8, 2.2 and 1.4 Ω at N = 25, 51, 101
/// and 201 — first-order in 1/N, from the pulse basis itself rather than the end
/// row. (Quadratic extrapolation, weights (15, −10, 3)/8, gives 79.15+j43.93 at
/// N = 51: 1.5 Ω closer, for a third neighbour per end and a three-segment minimum.
/// Not shipped; recorded so it is not later rediscovered as a defect.) The offset
/// had been explained three different ways in three documents, and it was never
/// ablated.
///
/// The weights come from the geometry of the extrapolation. The end segment's
/// midpoint lies `h_end/2` inside the physical end and `(h_end + h_inner)/2` from
/// its neighbour's midpoint, so with `t = h_end / (h_end + h_inner)`:
/// `I(end) = (1 + t)·I[end] − t·I[inner]`. For equal lengths that is the familiar
/// `1.5·I[end] − 0.5·I[inner]`. Callers without segment lengths pass equal ones,
/// which is exact inside a single `GW` (fnec has no `GC` taper) and approximate
/// only where a wire's end segment is the sole segment of its `GW` and its
/// neighbour belongs to another card with a different length.
///
/// `rel_sign` is `sign[end]·sign[inner]` for a conductor path, whose rows constrain
/// the path current rather than each segment's own: the row
/// `(1+t)·sign[e]·I[e] − t·sign[nb]·I[nb] = 0`, divided through by `sign[e]`. It is
/// `+1.0` within a wire. One relative sign rather than two separate ones, because
/// two would invite passing them in the wrong order — a mistake visible only on a
/// reversed one-segment end wire, which
/// `a_reversed_one_segment_end_wire_extrapolates_the_path_current` builds.
///
/// With no inner neighbour (a one-segment wire) it falls back to `I[end] = 0`,
/// which is what every site did before.
pub fn free_end_row(
    end: usize,
    inner: Option<(usize, f64)>,
    h_end: f64,
    h_inner: f64,
) -> ConstraintRow {
    let Some((nb, rel_sign)) = inner else {
        return (end, None, 1.0, 0.0);
    };
    let t = h_end / (h_end + h_inner);
    (end, Some(nb), 1.0 + t, -t * rel_sign)
}

/// The constraint rows [`solve_hallen`] imposes, in the order it imposes them: each
/// wire's free ends (a wire end that is a junction endpoint gets no free-end row),
/// then the junction continuity rows `I[a] + sign·I[b] = 0`.
///
/// Public so the GPU resident solve can take the rows it enforces from here rather
/// than re-deriving them from the same inputs — which it used to do, and which is
/// how a CPU-only fix to the end condition would have left `--exec gpu` answering
/// the old, one-segment-short model.
pub fn hallen_constraint_rows(
    wire_endpoints: &[(usize, usize)],
    junction_constraints: &[(usize, usize, f64)],
) -> Vec<ConstraintRow> {
    let junction_endpoint_set: std::collections::HashSet<usize> = junction_constraints
        .iter()
        .flat_map(|&(a, b, _)| [a, b])
        .collect();
    let mut rows = Vec::new();
    for &(first, last) in wire_endpoints {
        let inner = |nb: usize| (last > first).then_some((nb, 1.0));
        if !junction_endpoint_set.contains(&first) {
            rows.push(free_end_row(first, inner(first + 1), 1.0, 1.0));
        }
        if !junction_endpoint_set.contains(&last) {
            rows.push(free_end_row(last, inner(last.wrapping_sub(1)), 1.0, 1.0));
        }
    }
    for &(a, b, sign) in junction_constraints {
        rows.push((a, Some(b), 1.0, sign));
    }
    rows
}

/// Write each constraint row into `m`, starting at row `first_row`, over the given
/// column mapping: `col(i)` yields the matrix entries for the current on segment
/// `i`. The pulse solvers map a segment to its own column; the sinusoidal basis
/// maps it to that segment's row of the basis transform.
fn write_constraint_rows(
    m: &mut [Vec<Complex64>],
    first_row: usize,
    rows: &[ConstraintRow],
    mut col: impl FnMut(usize, f64, &mut [Complex64]),
) {
    for (crow, &(a, b, va, vb)) in (first_row..).zip(rows) {
        col(a, va, &mut m[crow]);
        if let Some(b) = b {
            col(b, vb, &mut m[crow]);
        }
    }
}

/// Solve Hallén's augmented integral equation using a sinusoidal (Galerkin)
/// basis for the segment currents.
///
/// This is the NEC2-style accurate path for `--solver sinusoidal`.
/// Instead of solving for raw segment currents `I` directly, we write
/// `I = T · a` where `T` is the block-diagonal **global-sine** basis transform
/// and `a` is the vector of basis coefficients.  For each wire of length `L`
/// with `n_w` segments, the transform entry is:
///
/// ```text
/// T[j][k] = sin((k+1) · π · pos_j)   k = 0..n_w-2
/// ```
///
/// where `pos_j = (j + 0.5) / n_w` is the normalised midpoint position of
/// segment `j` on that wire (0 < pos < 1).  These are the proper global
/// sinusoidal expansion functions — each one spans the whole wire and
/// automatically satisfies the endpoint condition I(0)=I(L)=0.
///
/// The Galerkin-projected system is assembled directly as `A = T^T Z T` and
/// `b = T^T · rhs`, then the endpoint/junction boundary-condition rows are
/// appended, and the overdetermined `(m + constraint_rows) × (m + W)` system
/// is solved via normal equations.  Using the Galerkin projection preserves
/// the complex-symmetric structure of the Hallén kernel, yielding results that
/// converge to the Hallén pulse-basis solution as `m → N`.
///
/// Arguments mirror [`solve_hallen`]; see that function for details on
/// `wire_endpoints` and `junction_constraints`.
pub fn solve_hallen_sinusoidal_basis(
    z: &ZMatrix,
    rhs: &[Complex64],
    cos_vec: &[f64],
    wire_endpoints: &[(usize, usize)],
    junction_constraints: &[(usize, usize, f64)],
) -> Result<HallenSolution, SolveError> {
    let n = z.n;
    if rhs.len() != n || cos_vec.len() != n {
        return Err(SolveError::HallenDimensionMismatch {
            z_n: n,
            rhs_len: rhs.len(),
            cos_len: cos_vec.len(),
        });
    }

    // Resolve effective wire endpoint list.
    let fallback_endpoints;
    let endpoints: &[(usize, usize)] = if wire_endpoints.is_empty() || n == 0 {
        fallback_endpoints = if n > 0 { vec![(0usize, n - 1)] } else { vec![] };
        &fallback_endpoints
    } else {
        wire_endpoints
    };

    // If any wire has fewer than 2 segments, fall back to standard Hallén.
    if endpoints.iter().any(|&(first, last)| last <= first) {
        return solve_hallen(z, rhs, cos_vec, wire_endpoints, junction_constraints);
    }

    let w = endpoints.len(); // number of wires (= number of homogeneous constants)

    // Build the global-sine basis transform T (n × m) as a block-diagonal matrix.
    // For wire k with n_k segments:
    //   T[first_k + j][col_offset + p] = sin((p+1) · π · (j+0.5) / n_k)
    let m: usize = endpoints
        .iter()
        .map(|&(first, last)| (last - first + 1).saturating_sub(1))
        .sum();
    if m == 0 {
        return solve_hallen(z, rhs, cos_vec, wire_endpoints, junction_constraints);
    }

    let mut global_t = vec![vec![0.0f64; m]; n];
    let mut col_offset = 0usize;
    for &(first, last) in endpoints.iter() {
        let n_w = last - first + 1;
        let m_w = n_w - 1;
        for local_row in 0..n_w {
            let pos = (local_row as f64 + 0.5) / (n_w as f64); // 0 < pos < 1
            for p in 0..m_w {
                let coeff = std::f64::consts::PI * ((p + 1) as f64) * pos;
                global_t[first + local_row][col_offset + p] = coeff.sin();
            }
        }
        col_offset += m_w;
    }

    // Determine which wire each segment belongs to.
    let mut seg_wire = vec![0usize; n];
    for (wi, &(first, last)) in endpoints.iter().enumerate() {
        for sw in seg_wire.iter_mut().take(last + 1).skip(first) {
            *sw = wi;
        }
    }

    // The same boundary rows `solve_hallen` imposes, expressed below in the sine
    // basis. The end rows are NOT redundant with the basis: the sine modes vanish
    // at the physical ends, but Galerkin testing with them yields one row fewer than
    // the unknowns per wire, and these rows are what make the system determined —
    // dropping them returns thousands of ohms. So they are extrapolated to the
    // physical end like every other variant's, not removed (FND-156).
    let crows = hallen_constraint_rows(endpoints, junction_constraints);
    let constraint_rows = crows.len();

    // --- Galerkin projection ---
    // Step 1: ZT = Z @ T  (n × m, complex).
    let mut zt = vec![vec![Complex64::new(0.0, 0.0); m]; n];
    for r in 0..n {
        for c in 0..m {
            let mut sum = Complex64::new(0.0, 0.0);
            for (j, t_row) in global_t.iter().enumerate() {
                let coeff = t_row[c];
                if coeff != 0.0 {
                    sum += z.get(r, j) * coeff;
                }
            }
            zt[r][c] = sum;
        }
    }

    // Step 2: A = T^T @ ZT  (m × m, complex).
    let mut a_mat = vec![vec![Complex64::new(0.0, 0.0); m]; m];
    for i in 0..m {
        for j in 0..m {
            let mut sum = Complex64::new(0.0, 0.0);
            for r in 0..n {
                sum += global_t[r][i] * zt[r][j];
            }
            a_mat[i][j] = sum;
        }
    }

    // Step 3: b = T^T @ rhs  (m).
    let mut b_proj = vec![Complex64::new(0.0, 0.0); m];
    for i in 0..m {
        let mut sum = Complex64::new(0.0, 0.0);
        for r in 0..n {
            sum += global_t[r][i] * rhs[r];
        }
        b_proj[i] = sum;
    }

    // Step 4: For each wire k, cos_proj[k] = T^T (indicator_k · cos_vec)  (m).
    // The C_k column in the projected system is -cos_proj[k].
    let mut cos_projs = vec![vec![0.0f64; m]; w];
    for r in 0..n {
        let k = seg_wire[r];
        let cv = cos_vec[r];
        for i in 0..m {
            cos_projs[k][i] += global_t[r][i] * cv;
        }
    }

    // Assemble the projected + constrained system:
    //   rows: m (Galerkin) + constraint_rows
    //   cols: m + w  (basis coefficients + one C per wire)
    let rows = m + constraint_rows;
    let cols = m + w;
    let mut mat = vec![vec![Complex64::new(0.0, 0.0); cols]; rows];
    let mut y_vec = vec![Complex64::new(0.0, 0.0); rows];

    // Galerkin rows.
    for i in 0..m {
        for j in 0..m {
            mat[i][j] = a_mat[i][j];
        }
        for k in 0..w {
            mat[i][m + k] = Complex64::new(-cos_projs[k][i], 0.0);
        }
        y_vec[i] = b_proj[i];
    }

    // Endpoint and junction constraints: a row on segment currents becomes, in the
    // sine basis, the same combination of those segments' rows of `global_t`.
    write_constraint_rows(&mut mat, m, &crows, |seg, v, row| {
        for (c, t) in global_t[seg].iter().enumerate().take(m) {
            row[c] += Complex64::new(v * t, 0.0);
        }
    });

    // Solve the (m + constraint_rows) × (m + w) system via normal equations.
    let mut ata = vec![vec![Complex64::new(0.0, 0.0); cols]; cols];
    let mut aty = vec![Complex64::new(0.0, 0.0); cols];
    for i in 0..cols {
        for j in 0..cols {
            let mut sum = Complex64::new(0.0, 0.0);
            for r in 0..rows {
                sum += mat[r][i].conj() * mat[r][j];
            }
            ata[i][j] = sum;
        }
        let mut sum = Complex64::new(0.0, 0.0);
        for r in 0..rows {
            sum += mat[r][i].conj() * y_vec[r];
        }
        aty[i] = sum;
    }
    let lambda = 1e-8;
    for i in 0..cols {
        ata[i][i] += Complex64::new(lambda, 0.0);
    }

    let x = solve_square_in_place(&mut ata, &mut aty)?;
    let a_basis = &x[..m];
    let c_hom_per_wire = x[m..].to_vec();
    let currents = basis_to_currents(&global_t, a_basis);

    Ok(HallenSolution {
        currents,
        c_hom: c_hom_per_wire
            .first()
            .copied()
            .unwrap_or(Complex64::new(0.0, 0.0)),
        c_hom_per_wire,
    })
}

fn regularization_lambda(a: &[Vec<Complex64>], rel_scale: f64, floor: f64) -> f64 {
    if a.is_empty() {
        return floor;
    }

    let n = a.len();
    let mut diag_sum = 0.0;
    for (i, row) in a.iter().enumerate().take(n) {
        if i < row.len() {
            diag_sum += row[i].norm();
        }
    }

    let diag_avg = diag_sum / (n as f64);
    (diag_avg * rel_scale).max(floor)
}

/// Result of solving Hallén's augmented system.
pub struct HallenSolution {
    /// Solved segment currents.
    pub currents: Vec<Complex64>,
    /// Homogeneous-constant coefficients per wire.
    ///
    /// Length is the number of wires constrained in the Hallen solve.
    pub c_hom_per_wire: Vec<Complex64>,
    /// Homogeneous-constant coefficient of the first wire (compat field).
    pub c_hom: Complex64,
}

/// Solve Hallén's augmented system in least-squares form.
///
/// Unknown vector x = [I_0 .. I_{N-1}, C_0 .. C_{W-1}]^T.
///
/// Overdetermined augmented system:
///   A · I - C_w(row)·cos_vec = rhs
///   I[seg] = 0  (for each free wire endpoint not in a junction)
///   I[seg_a] + sign·I[seg_b] = 0  (for each junction)
///
/// `wire_endpoints` is a slice of `(first_seg_idx, last_seg_idx)` per wire.
/// An empty slice falls back to constraining only the global first and last segment,
/// which is correct for single-wire decks.
///
/// `junction_constraints` is a slice of `(seg_a, seg_b, sign)` where each entry
/// encodes a current-continuity constraint `I[seg_a] + sign * I[seg_b] = 0` at a
/// geometric wire junction. Wire endpoint indices that appear in at least one
/// junction constraint will NOT receive a free-end row; free endpoints (not in any
/// junction) receive one — the current extrapolated to the PHYSICAL wire end is
/// zero ([`free_end_row`]). Until FND-156 this was `I = 0` at the end segment's
/// midpoint, which modelled every wire one segment short.
/// Pass an empty slice for the single-wire or collinear-multi-wire case.
///
/// Solved via regularized normal equations:
///   (MᴴM + λI) x = Mᴴy
pub fn solve_hallen(
    z: &ZMatrix,
    rhs: &[Complex64],
    cos_vec: &[f64],
    wire_endpoints: &[(usize, usize)],
    junction_constraints: &[(usize, usize, f64)],
) -> Result<HallenSolution, SolveError> {
    let n = z.n;
    if rhs.len() != n || cos_vec.len() != n {
        return Err(SolveError::HallenDimensionMismatch {
            z_n: n,
            rhs_len: rhs.len(),
            cos_len: cos_vec.len(),
        });
    }

    // Build the endpoint constraint list: per-wire if supplied, else global endpoints.
    let fallback_endpoints;
    let endpoints: &[(usize, usize)] = if wire_endpoints.is_empty() || n == 0 {
        fallback_endpoints = if n > 0 { vec![(0usize, n - 1)] } else { vec![] };
        &fallback_endpoints
    } else {
        wire_endpoints
    };

    // The boundary rows — a free-end row per wire end that is not a junction
    // endpoint, then one continuity row per junction — built by the one function the
    // GPU resident solve also takes them from (FND-156).
    let crows = hallen_constraint_rows(endpoints, junction_constraints);
    let constraint_rows = crows.len();

    let w = endpoints.len();
    let rows = n + constraint_rows;
    let cols = n + w;
    let mut m = vec![vec![Complex64::new(0.0, 0.0); cols]; rows];
    let mut y = vec![Complex64::new(0.0, 0.0); rows];

    // Determine wire index for each segment row from endpoint ranges.
    let mut row_wire = vec![0usize; n];
    for (wi, &(first, last)) in endpoints.iter().enumerate() {
        for rw in row_wire.iter_mut().take(last + 1).skip(first) {
            *rw = wi;
        }
    }

    for r in 0..n {
        for c in 0..n {
            m[r][c] = z.get(r, c);
        }
        let c_col = n + row_wire[r];
        m[r][c_col] = Complex64::new(-cos_vec[r], 0.0);
        y[r] = rhs[r];
    }

    write_constraint_rows(&mut m, n, &crows, |seg, v, row| {
        row[seg] += Complex64::new(v, 0.0);
    });

    // Normal equations with light Tikhonov regularization.
    let mut ata = vec![vec![Complex64::new(0.0, 0.0); cols]; cols];
    let mut aty = vec![Complex64::new(0.0, 0.0); cols];
    for i in 0..cols {
        for j in 0..cols {
            let mut sum = Complex64::new(0.0, 0.0);
            for r in 0..rows {
                sum += m[r][i].conj() * m[r][j];
            }
            ata[i][j] = sum;
        }
        let mut sum = Complex64::new(0.0, 0.0);
        for r in 0..rows {
            sum += m[r][i].conj() * y[r];
        }
        aty[i] = sum;
    }

    let lambda = 1e-8;
    for i in 0..cols {
        ata[i][i] += Complex64::new(lambda, 0.0);
    }

    let x = solve_square_in_place(&mut ata, &mut aty)?;
    let c_hom_per_wire = x[n..].to_vec();
    Ok(HallenSolution {
        currents: x[..n].to_vec(),
        c_hom: c_hom_per_wire
            .first()
            .copied()
            .unwrap_or(Complex64::new(0.0, 0.0)),
        c_hom_per_wire,
    })
}

/// Hallén solve over **conductor paths** — the general-junction delta-gap solver
/// (PH9-CHK-002).
///
/// This generalizes [`solve_hallen`] from contiguous single wires to arbitrary
/// degree-2 conductor chains (bends, start-to-start / end-to-end splits). The
/// difference is entirely in how the homogeneous term and the free-end boundary
/// condition are addressed:
///
/// - `path_of_seg[m]` assigns each segment to a logical conductor path; all
///   segments on a path **share one** homogeneous constant `C`. There is one `C`
///   column per distinct path.
/// - `free_end_rows` are the free-end boundary rows (see [`free_end_row`]) — the two
///   physical free ends of each open chain, *not* every `GW` endpoint. Interior
///   degree-2 junctions receive no constraint (the current flows through them
///   continuously, exactly as inside a single wire).
///
/// The caller must pass a `cos_vec` and `rhs` already built with the path sign and
/// signed arc-length convention (see [`build_hallen_rhs`]): the current on
/// segment `m` in its own direction is `sign[m]·I_path(s_m)`, so
/// `cos_vec[m] = sign[m]·cos(k·s_m)` and the source term carries the same sign.
///
/// Solved via regularized normal equations, mirroring [`solve_hallen`].
///
/// [`build_hallen_rhs`]: crate::build_hallen_rhs
pub fn solve_hallen_paths(
    z: &ZMatrix,
    rhs: &[Complex64],
    cos_vec: &[f64],
    path_of_seg: &[usize],
    free_end_rows: &[ConstraintRow],
) -> Result<HallenSolution, SolveError> {
    let n = z.n;
    if rhs.len() != n || cos_vec.len() != n || path_of_seg.len() != n {
        return Err(SolveError::HallenDimensionMismatch {
            z_n: n,
            rhs_len: rhs.len(),
            cos_len: cos_vec.len(),
        });
    }

    let num_paths = path_of_seg.iter().copied().max().map_or(0, |m| m + 1);
    let constraint_rows = free_end_rows.len();
    let rows = n + constraint_rows;
    let cols = n + num_paths;
    let mut m = vec![vec![Complex64::new(0.0, 0.0); cols]; rows];
    let mut y = vec![Complex64::new(0.0, 0.0); rows];

    for r in 0..n {
        for c in 0..n {
            m[r][c] = z.get(r, c);
        }
        let c_col = n + path_of_seg[r];
        m[r][c_col] = Complex64::new(-cos_vec[r], 0.0);
        y[r] = rhs[r];
    }

    // Free-end rows (the open-chain terminals only), extrapolated to the physical
    // end along the path — built by `path_end_rows`, which knows the path's
    // traversal order, signs and segment lengths (FND-156).
    write_constraint_rows(&mut m, n, free_end_rows, |seg, v, row| {
        row[seg] += Complex64::new(v, 0.0);
    });

    // Normal equations with light Tikhonov regularization (mirrors solve_hallen).
    let mut ata = vec![vec![Complex64::new(0.0, 0.0); cols]; cols];
    let mut aty = vec![Complex64::new(0.0, 0.0); cols];
    for i in 0..cols {
        for j in 0..cols {
            let mut sum = Complex64::new(0.0, 0.0);
            for r in 0..rows {
                sum += m[r][i].conj() * m[r][j];
            }
            ata[i][j] = sum;
        }
        let mut sum = Complex64::new(0.0, 0.0);
        for r in 0..rows {
            sum += m[r][i].conj() * y[r];
        }
        aty[i] = sum;
    }
    let lambda = 1e-8;
    for i in 0..cols {
        ata[i][i] += Complex64::new(lambda, 0.0);
    }

    let x = solve_square_in_place(&mut ata, &mut aty)?;
    let c_hom_per_wire = x[n..].to_vec();
    Ok(HallenSolution {
        currents: x[..n].to_vec(),
        c_hom: c_hom_per_wire
            .first()
            .copied()
            .unwrap_or(Complex64::new(0.0, 0.0)),
        c_hom_per_wire,
    })
}

/// Hallén solve for a **distributed** (asymmetric) excitation such as an
/// incident plane wave.
///
/// Unlike [`solve_hallen`] — which carries a single `cos(k·s)` homogeneous
/// constant per wire, sufficient for the *symmetric* delta-gap source — this
/// path carries **both** homogeneous constants per wire (`cos` and `sin`), the
/// two degrees of freedom classical Hallén needs to satisfy `I = 0` at both wire
/// endpoints for a general asymmetric current. The delta-gap `solve_hallen`
/// path is intentionally left unchanged so the validated corpus is unaffected.
///
/// The per-segment row is `Z·I − C_cos·cos(k·s) − C_sin·sin(k·s) = rhs`, plus a
/// free-end row at each wire end ([`free_end_row`]: zero current at the physical
/// end, not at the end segment's midpoint). The system is square
/// (`N + 2·wires` unknowns and equations) and solved via regularized normal
/// equations, mirroring [`solve_hallen`]. Returns the segment currents.
pub fn solve_hallen_planewave(
    z: &ZMatrix,
    rhs: &[Complex64],
    cos_vec: &[f64],
    sin_vec: &[f64],
    wire_endpoints: &[(usize, usize)],
) -> Result<Vec<Complex64>, SolveError> {
    let n = z.n;
    if rhs.len() != n || cos_vec.len() != n || sin_vec.len() != n {
        return Err(SolveError::HallenDimensionMismatch {
            z_n: n,
            rhs_len: rhs.len(),
            cos_len: cos_vec.len().min(sin_vec.len()),
        });
    }

    let fallback_endpoints;
    let endpoints: &[(usize, usize)] = if wire_endpoints.is_empty() || n == 0 {
        fallback_endpoints = if n > 0 { vec![(0usize, n - 1)] } else { vec![] };
        &fallback_endpoints
    } else {
        wire_endpoints
    };

    let w = endpoints.len();
    // Two free-end rows per wire (no junctions on this path — the builder refuses
    // them), built by the same function every other variant uses. The plane-wave
    // sites need this as much as the driven ones and are harder to catch: the
    // reciprocity gates compare pattern SHAPES, and a one-segment-short wire barely
    // moves a normalised pattern, so a fix that missed this site would pass them
    // all (FND-156).
    let crows = hallen_constraint_rows(endpoints, &[]);
    let constraint_rows = crows.len();
    let rows = n + constraint_rows;
    // Two homogeneous constants (cos, sin) per wire.
    let cols = n + 2 * w;
    let mut m = vec![vec![Complex64::new(0.0, 0.0); cols]; rows];
    let mut y = vec![Complex64::new(0.0, 0.0); rows];

    let mut row_wire = vec![0usize; n];
    for (wi, &(first, last)) in endpoints.iter().enumerate() {
        for rw in row_wire.iter_mut().take(last + 1).skip(first) {
            *rw = wi;
        }
    }

    for r in 0..n {
        for c in 0..n {
            m[r][c] = z.get(r, c);
        }
        let wi = row_wire[r];
        m[r][n + 2 * wi] = Complex64::new(-cos_vec[r], 0.0);
        m[r][n + 2 * wi + 1] = Complex64::new(-sin_vec[r], 0.0);
        y[r] = rhs[r];
    }

    write_constraint_rows(&mut m, n, &crows, |seg, v, row| {
        row[seg] += Complex64::new(v, 0.0);
    });

    // Regularized normal equations (mirrors solve_hallen).
    let mut ata = vec![vec![Complex64::new(0.0, 0.0); cols]; cols];
    let mut aty = vec![Complex64::new(0.0, 0.0); cols];
    for i in 0..cols {
        for j in 0..cols {
            let mut sum = Complex64::new(0.0, 0.0);
            for r in 0..rows {
                sum += m[r][i].conj() * m[r][j];
            }
            ata[i][j] = sum;
        }
        let mut sum = Complex64::new(0.0, 0.0);
        for r in 0..rows {
            sum += m[r][i].conj() * y[r];
        }
        aty[i] = sum;
    }
    let lambda = 1e-8;
    for i in 0..cols {
        ata[i][i] += Complex64::new(lambda, 0.0);
    }

    let x = solve_square_in_place(&mut ata, &mut aty)?;
    Ok(x[..n].to_vec())
}

/// Hallén solve for a **distributed** excitation over **conductor paths** — the
/// general-junction receive solver (PH9-CHK-002).
///
/// This is to [`solve_hallen_planewave`] what [`solve_hallen_paths`] is to
/// [`solve_hallen`]: it generalizes the asymmetric two-DOF (cos/sin) plane-wave
/// solve from contiguous single wires to arbitrary degree-2 conductor chains
/// (bends, start-to-start / end-to-end splits), so a *receiving* bent or connected
/// antenna solves on one continuous current path across each junction.
///
/// The classical Hallén homogeneous solution on a continuous conductor is
/// `C_cos·cos(k·s) + C_sin·sin(k·s)` in the path arc-length `s`; both DOF are
/// needed because a distributed incident field induces a general asymmetric
/// current. Each path therefore carries **two** homogeneous constants and gets the
/// free-end boundary condition at its **two free ends only** — interior degree-2
/// junctions flow continuously, exactly as inside a single wire.
///
/// - `path_of_seg[m]` assigns each segment to a logical conductor path; all
///   segments on a path share the same two `C` columns (`n + 2·p` and `n + 2·p+1`).
/// - `free_end_rows` are the free-end boundary rows (see [`free_end_row`]) — the two
///   physical free ends of each open chain (two per path).
///
/// The caller must pass `cos_vec`, `sin_vec` and `rhs` already built with the path
/// sign and signed-arc-length convention (see [`crate::build_planewave_hallen_paths`]):
/// the current on segment `m` in its own NEC direction is `sign[m]·I_path(s_m)`, so
/// `cos_vec[m] = sign[m]·cos(k·s_m)`, `sin_vec[m] = sign[m]·sin(k·s_m)`, and the
/// forcing carries the same sign. For a single straight wire (`sign = +1`,
/// arc-length = the wire axis) this reduces exactly to [`solve_hallen_planewave`].
///
/// Solved via regularized normal equations, mirroring [`solve_hallen_planewave`].
/// Returns the segment currents.
pub fn solve_hallen_planewave_paths(
    z: &ZMatrix,
    rhs: &[Complex64],
    cos_vec: &[f64],
    sin_vec: &[f64],
    path_of_seg: &[usize],
    free_end_rows: &[ConstraintRow],
) -> Result<Vec<Complex64>, SolveError> {
    let n = z.n;
    if rhs.len() != n || cos_vec.len() != n || sin_vec.len() != n || path_of_seg.len() != n {
        return Err(SolveError::HallenDimensionMismatch {
            z_n: n,
            rhs_len: rhs.len(),
            cos_len: cos_vec.len().min(sin_vec.len()),
        });
    }

    let num_paths = path_of_seg.iter().copied().max().map_or(0, |m| m + 1);
    // Two homogeneous constants (cos, sin) per path.
    let cols = n + 2 * num_paths;
    // One free-end row per path terminal (two per open-chain path).
    let constraint_rows = free_end_rows.len();
    let rows = n + constraint_rows;
    let mut m = vec![vec![Complex64::new(0.0, 0.0); cols]; rows];
    let mut y = vec![Complex64::new(0.0, 0.0); rows];

    for r in 0..n {
        for c in 0..n {
            m[r][c] = z.get(r, c);
        }
        let p = path_of_seg[r];
        m[r][n + 2 * p] = Complex64::new(-cos_vec[r], 0.0);
        m[r][n + 2 * p + 1] = Complex64::new(-sin_vec[r], 0.0);
        y[r] = rhs[r];
    }

    // Free-end rows (the open-chain terminals only), extrapolated to the physical
    // end along the path — built by `path_end_rows`, which knows the path's
    // traversal order, signs and segment lengths (FND-156).
    write_constraint_rows(&mut m, n, free_end_rows, |seg, v, row| {
        row[seg] += Complex64::new(v, 0.0);
    });

    // Regularized normal equations (mirrors solve_hallen_planewave).
    let mut ata = vec![vec![Complex64::new(0.0, 0.0); cols]; cols];
    let mut aty = vec![Complex64::new(0.0, 0.0); cols];
    for i in 0..cols {
        for j in 0..cols {
            let mut sum = Complex64::new(0.0, 0.0);
            for r in 0..rows {
                sum += m[r][i].conj() * m[r][j];
            }
            ata[i][j] = sum;
        }
        let mut sum = Complex64::new(0.0, 0.0);
        for r in 0..rows {
            sum += m[r][i].conj() * y[r];
        }
        aty[i] = sum;
    }
    let lambda = 1e-8;
    for i in 0..cols {
        ata[i][i] += Complex64::new(lambda, 0.0);
    }

    let x = solve_square_in_place(&mut ata, &mut aty)?;
    Ok(x[..n].to_vec())
}

/// Result of a current-source Hallén solve (PH8-CHK-001).
#[derive(Debug, Clone)]
pub struct CurrentSourceSolution {
    /// Solved segment currents (the source segment carries the forced current).
    pub currents: Vec<Complex64>,
    /// Port voltage `V` that sustains the forced current — the dual of the
    /// delta-gap source. The feedpoint impedance is `V / I₀`.
    pub port_voltage: Complex64,
}

pub(crate) fn solve_square_in_place(
    a: &mut [Vec<Complex64>],
    b: &mut [Complex64],
) -> Result<Vec<Complex64>, SolveError> {
    let n = a.len();
    if b.len() != n {
        return Err(SolveError::DimensionMismatch {
            z_n: n,
            v_len: b.len(),
        });
    }

    // Build augmented matrix [A | b].
    let mut aug: Vec<Vec<Complex64>> = (0..n)
        .map(|i| {
            let mut row = a[i].clone();
            row.push(b[i]);
            row
        })
        .collect();

    // Gaussian elimination with partial pivoting.
    for col in 0..n {
        // Find pivot row (max |a[row][col]|).
        let pivot = (col..n)
            .max_by(|&r1, &r2| {
                aug[r1][col]
                    .norm()
                    .partial_cmp(&aug[r2][col].norm())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        if aug[pivot][col].norm() < 1e-30 {
            return Err(SolveError::Singular);
        }

        aug.swap(col, pivot);

        let diag = aug[col][col];
        // Eliminate rows below.
        for row in (col + 1)..n {
            let factor = aug[row][col] / diag;
            for k in col..=n {
                // =n because of the augmented column
                let sub = factor * aug[col][k];
                aug[row][k] -= sub;
            }
        }
    }

    // Back-substitution.
    let mut x = vec![Complex64::new(0.0, 0.0); n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n]; // RHS
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        x[i] = sum / aug[i][i];
    }

    Ok(x)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basis::{ContinuityTransform, SinusoidalTransform};
    use crate::matrix::ZMatrix;
    use num_complex::Complex64;

    /// The row's value on a current: what the solver forces to zero.
    fn row_value(row: ConstraintRow, i: &[f64]) -> f64 {
        let (a, b, va, vb) = row;
        va * i[a] + b.map_or(0.0, |b| vb * i[b])
    }

    /// FND-156: the row must vanish on a current that is zero at the PHYSICAL
    /// tip and linear in arc length, whatever the two segment lengths. The old
    /// `I[end] = 0` fails this for every current that is nonzero at the end
    /// segment's midpoint, which is every current that is zero at the tip.
    #[test]
    fn a_free_end_row_vanishes_on_a_current_that_is_zero_at_the_tip() {
        for (h_end, h_inner) in [(1.0, 1.0), (0.3, 0.7), (2.0, 0.5)] {
            // Arc length from the tip to each midpoint, scaled by an arbitrary slope.
            let i = [3.7 * h_end / 2.0, 3.7 * (h_end + h_inner / 2.0)];
            let row = free_end_row(0, Some((1, 1.0)), h_end, h_inner);
            assert!(
                row_value(row, &i).abs() < 1e-12,
                "h = ({h_end}, {h_inner}): row {row:?} leaves {} on a tip-zero current",
                row_value(row, &i)
            );
            let old: ConstraintRow = (0, None, 1.0, 0.0);
            assert!(
                row_value(old, &i).abs() > 0.1,
                "the old row must fail this, or the test cannot tell them apart"
            );
        }
    }

    #[test]
    fn equal_segments_give_the_one_and_a_half_minus_a_half_row() {
        assert_eq!(
            free_end_row(7, Some((6, 1.0)), 0.2, 0.2),
            (7, Some(6), 1.5, -0.5)
        );
    }

    /// A reversed neighbour flips the inner weight's sign and nothing else.
    #[test]
    fn a_relative_sign_flips_only_the_inner_weight() {
        let (a, b, va, vb) = free_end_row(3, Some((4, -1.0)), 0.3, 0.7);
        assert_eq!((a, b), (3, Some(4)));
        assert!(
            (va - 1.3).abs() < 1e-12 && (vb - 0.3).abs() < 1e-12,
            "{va} {vb}"
        );
    }

    /// A one-segment wire has no inner neighbour to extrapolate through.
    #[test]
    fn a_one_segment_end_falls_back_to_a_zero_current() {
        assert_eq!(free_end_row(2, None, 0.5, 0.5), (2, None, 1.0, 0.0));
    }

    /// Rows only at ends that are not junctions, each extrapolating INWARD, and
    /// the junction rows after them unchanged.
    #[test]
    fn constraint_rows_extrapolate_inward_and_skip_junction_ends() {
        // Wire 0 = segs 0..=4, wire 1 = segs 5..=9 joined 4↔5, wire 2 = seg 10 alone.
        let rows = hallen_constraint_rows(&[(0, 4), (5, 9), (10, 10)], &[(4, 5, 1.0)]);
        assert_eq!(
            rows,
            vec![
                (0, Some(1), 1.5, -0.5),
                (9, Some(8), 1.5, -0.5),
                (10, None, 1.0, 0.0),
                (10, None, 1.0, 0.0),
                (4, Some(5), 1.0, 1.0),
            ]
        );
    }

    fn c(re: f64, im: f64) -> Complex64 {
        Complex64::new(re, im)
    }

    /// Helper: build ZMatrix from a 2D vec of Complex64.
    fn z_from_2d(rows: Vec<Vec<Complex64>>) -> ZMatrix {
        let n = rows.len();
        let mut z = ZMatrix::new(n);
        for (i, row) in rows.iter().enumerate() {
            for (j, &val) in row.iter().enumerate() {
                z.set_test(i, j, val);
            }
        }
        z
    }

    /// 1×1 trivial: Z=[2+0j], V=[4+0j] → I=[2+0j]
    #[test]
    fn solve_1x1() {
        let z = z_from_2d(vec![vec![c(2.0, 0.0)]]);
        let v = vec![c(4.0, 0.0)];
        let i = solve(&z, &v).unwrap();
        assert!((i[0] - c(2.0, 0.0)).norm() < 1e-12);
    }

    /// 2×2 real: [[3,1],[1,2]]·[1,1]=[4,3]
    #[test]
    fn solve_2x2_real() {
        let z = z_from_2d(vec![
            vec![c(3.0, 0.0), c(1.0, 0.0)],
            vec![c(1.0, 0.0), c(2.0, 0.0)],
        ]);
        let v = vec![c(4.0, 0.0), c(3.0, 0.0)];
        let i = solve(&z, &v).unwrap();
        assert!((i[0] - c(1.0, 0.0)).norm() < 1e-12, "i[0]={}", i[0]);
        assert!((i[1] - c(1.0, 0.0)).norm() < 1e-12, "i[1]={}", i[1]);
    }

    /// 2×2 complex
    #[test]
    fn solve_2x2_complex() {
        // Z = [[1+j, 0],[0, 2+0j]], V=[1+j, 4] → I=[1, 2]
        let z = z_from_2d(vec![
            vec![c(1.0, 1.0), c(0.0, 0.0)],
            vec![c(0.0, 0.0), c(2.0, 0.0)],
        ]);
        let v = vec![c(1.0, 1.0), c(4.0, 0.0)];
        let i = solve(&z, &v).unwrap();
        assert!((i[0] - c(1.0, 0.0)).norm() < 1e-12, "i[0]={}", i[0]);
        assert!((i[1] - c(2.0, 0.0)).norm() < 1e-12, "i[1]={}", i[1]);
    }

    /// Singular matrix returns SolveError::Singular.
    #[test]
    fn singular_matrix_returns_error() {
        let z = z_from_2d(vec![
            vec![c(1.0, 0.0), c(2.0, 0.0)],
            vec![c(2.0, 0.0), c(4.0, 0.0)], // row 2 = 2 × row 1
        ]);
        let v = vec![c(1.0, 0.0), c(2.0, 0.0)];
        assert!(matches!(solve(&z, &v), Err(SolveError::Singular)));
    }

    /// Dimension mismatch.
    #[test]
    fn dimension_mismatch_returns_error() {
        let z = z_from_2d(vec![
            vec![c(1.0, 0.0), c(0.0, 0.0)],
            vec![c(0.0, 0.0), c(1.0, 0.0)],
        ]);
        let v = vec![c(1.0, 0.0)]; // wrong length
        assert!(matches!(
            solve(&z, &v),
            Err(SolveError::DimensionMismatch { z_n: 2, v_len: 1 })
        ));
    }

    /// Verify Z·I ≈ V for a 3×3 random-looking system.
    #[test]
    fn residual_3x3() {
        let z = z_from_2d(vec![
            vec![c(4.0, 1.0), c(1.0, 0.0), c(0.0, -1.0)],
            vec![c(1.0, 0.0), c(5.0, 2.0), c(1.0, 0.0)],
            vec![c(0.0, -1.0), c(1.0, 0.0), c(3.0, 1.0)],
        ]);
        let v = vec![c(2.0, 1.0), c(3.0, -1.0), c(1.0, 0.0)];
        let i = solve(&z, &v).unwrap();

        // Check residual: ||Z·I - V|| < tol
        for row in 0..3 {
            let mut zi: Complex64 = c(0.0, 0.0);
            for col in 0..3 {
                zi += z.get(row, col) * i[col];
            }
            let err = (zi - v[row]).norm();
            assert!(err < 1e-10, "row {row}: residual {err}");
        }
    }

    #[test]
    fn continuity_basis_identity_reconstructs_column_space_vector() {
        let n = 4;
        let mut z = ZMatrix::new(n);
        for i in 0..n {
            for j in 0..n {
                z.set_test(i, j, if i == j { c(1.0, 0.0) } else { c(0.0, 0.0) });
            }
        }

        let tr = ContinuityTransform::for_single_chain(n);
        let a = vec![c(1.0, 0.0), c(2.0, 0.0), c(3.0, 0.0)];
        let v = tr.segment_currents(&a);

        let i = solve_with_continuity_basis(&z, &v).unwrap();
        for k in 0..n {
            assert!((i[k] - v[k]).norm() < 1e-8, "k={k}, i={}, v={}", i[k], v[k]);
        }
    }

    #[test]
    fn sinusoidal_basis_identity_reconstructs_column_space_vector() {
        let n = 5;
        let mut z = ZMatrix::new(n);
        for i in 0..n {
            for j in 0..n {
                z.set_test(i, j, if i == j { c(1.0, 0.0) } else { c(0.0, 0.0) });
            }
        }

        let tr = SinusoidalTransform::for_single_chain(n);
        let a = vec![c(1.0, 0.0), c(0.5, 0.0), c(-0.25, 0.0), c(0.75, 0.0)];
        let v = tr.segment_currents(&a);

        let i = solve_with_sinusoidal_basis(&z, &v).unwrap();
        for k in 0..n {
            assert!((i[k] - v[k]).norm() < 1e-8, "k={k}, i={}, v={}", i[k], v[k]);
        }
    }

    #[test]
    fn hallen_dimension_mismatch_returns_error() {
        let z = z_from_2d(vec![
            vec![c(1.0, 0.0), c(0.0, 0.0)],
            vec![c(0.0, 0.0), c(1.0, 0.0)],
        ]);

        let rhs = vec![c(1.0, 0.0)];
        let cos_vec = vec![1.0, 1.0];
        assert!(matches!(
            solve_hallen(&z, &rhs, &cos_vec, &[], &[]),
            Err(SolveError::HallenDimensionMismatch {
                z_n: 2,
                rhs_len: 1,
                cos_len: 2
            })
        ));
    }

    #[test]
    fn hallen_solution_changes_with_rhs() {
        let z = z_from_2d(vec![
            vec![c(2.0, 0.0), c(0.3, 0.0), c(0.1, 0.0)],
            vec![c(0.3, 0.0), c(2.0, 0.0), c(0.3, 0.0)],
            vec![c(0.1, 0.0), c(0.3, 0.0), c(2.0, 0.0)],
        ]);
        let cos_vec = vec![1.0, 0.9, 0.7];

        let rhs_a = vec![c(0.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)];
        let rhs_b = vec![c(0.0, -0.1), c(0.0, -0.2), c(0.0, -0.1)];

        let a = solve_hallen(&z, &rhs_a, &cos_vec, &[], &[]).unwrap();
        let b = solve_hallen(&z, &rhs_b, &cos_vec, &[], &[]).unwrap();

        let mut diff = 0.0;
        for i in 0..3 {
            diff += (a.currents[i] - b.currents[i]).norm();
        }
        assert!(
            diff > 1e-6,
            "hallén solution should respond to RHS, diff={diff}"
        );
    }

    // ── per-wire block-transform tests ───────────────────────────────────────

    /// Single wire chain: per-wire sinusoidal should give the same result as
    /// the original single-chain solve.
    #[test]
    fn sinusoidal_per_wire_single_chain_matches_original() {
        let n = 5usize;
        let mut z = ZMatrix::new(n);
        for i in 0..n {
            for j in 0..n {
                let diag = if i == j { c(10.0, 0.0) } else { c(0.3, 0.0) };
                z.set_test(i, j, diag);
            }
        }
        let v: Vec<Complex64> = (0..n).map(|i| c(i as f64 + 1.0, 0.0)).collect();
        let wire_ep = vec![(0usize, n - 1)];

        let result_single = solve_with_sinusoidal_basis(&z, &v).unwrap();
        let result_per = solve_with_sinusoidal_basis_per_wire(&z, &v, &wire_ep).unwrap();

        for k in 0..n {
            assert!(
                (result_single[k] - result_per[k]).norm() < 1e-10,
                "k={k}: single={} per_wire={}",
                result_single[k],
                result_per[k]
            );
        }
    }

    /// Single wire chain: per-wire continuity should give the same result as
    /// the original single-chain solve.
    #[test]
    fn continuity_per_wire_single_chain_matches_original() {
        let n = 4usize;
        let mut z = ZMatrix::new(n);
        for i in 0..n {
            for j in 0..n {
                let val = if i == j { c(8.0, 0.0) } else { c(0.5, 0.0) };
                z.set_test(i, j, val);
            }
        }
        let v: Vec<Complex64> = (0..n).map(|i| c(1.0 + i as f64, 0.5)).collect();
        let wire_ep = vec![(0usize, n - 1)];

        let result_single = solve_with_continuity_basis(&z, &v).unwrap();
        let result_per = solve_with_continuity_basis_per_wire(&z, &v, &wire_ep).unwrap();

        for k in 0..n {
            assert!(
                (result_single[k] - result_per[k]).norm() < 1e-10,
                "k={k}: single={} per_wire={}",
                result_single[k],
                result_per[k]
            );
        }
    }

    /// Two-wire block-diagonal: segments 0-2 form wire A, segments 3-5 form
    /// wire B.  Block-diagonal Z (no cross-wire coupling) so each wire is
    /// independent.  The per-wire result must match solving each wire alone.
    #[test]
    fn sinusoidal_per_wire_two_uncoupled_wires() {
        let na = 3usize;
        let nb = 3usize;
        let n = na + nb;

        let mut z_full = ZMatrix::new(n);
        for i in 0..n {
            for j in 0..n {
                let same_block = (i < na) == (j < na);
                let val = if i == j {
                    c(10.0, 0.0)
                } else if same_block {
                    c(0.5, 0.0)
                } else {
                    c(0.0, 0.0)
                };
                z_full.set_test(i, j, val);
            }
        }
        let v_full: Vec<Complex64> = (0..n).map(|i| c(i as f64 + 1.0, 0.0)).collect();
        let wire_ep = vec![(0usize, na - 1), (na, n - 1)];

        let result_full = solve_with_sinusoidal_basis_per_wire(&z_full, &v_full, &wire_ep).unwrap();

        // Solve wire A alone.
        let mut z_a = ZMatrix::new(na);
        for i in 0..na {
            for j in 0..na {
                z_a.set_test(i, j, z_full.get(i, j));
            }
        }
        let v_a: Vec<Complex64> = v_full[..na].to_vec();
        let result_a = solve_with_sinusoidal_basis(&z_a, &v_a).unwrap();

        // Solve wire B alone.
        let mut z_b = ZMatrix::new(nb);
        for i in 0..nb {
            for j in 0..nb {
                z_b.set_test(i, j, z_full.get(na + i, na + j));
            }
        }
        let v_b: Vec<Complex64> = v_full[na..].to_vec();
        let result_b = solve_with_sinusoidal_basis(&z_b, &v_b).unwrap();

        for k in 0..na {
            assert!(
                (result_full[k] - result_a[k]).norm() < 1e-8,
                "wire A k={k}: full={} alone={}",
                result_full[k],
                result_a[k]
            );
        }
        for k in 0..nb {
            assert!(
                (result_full[na + k] - result_b[k]).norm() < 1e-8,
                "wire B k={k}: full={} alone={}",
                result_full[na + k],
                result_b[k]
            );
        }
    }

    /// When a wire has only 1 segment (n_w < 2), per-wire functions fall back
    /// to plain `solve` rather than panicking or returning wrong results.
    #[test]
    fn sinusoidal_per_wire_fallback_on_1seg_wire() {
        let n = 3usize;
        let mut z = ZMatrix::new(n);
        for i in 0..n {
            z.set_test(i, i, c(5.0, 0.0));
        }
        let v = vec![c(1.0, 0.0), c(2.0, 0.0), c(3.0, 0.0)];
        // Wire B has only 1 segment → fallback to plain solve.
        let wire_ep = vec![(0usize, 1), (2, 2)];
        let result = solve_with_sinusoidal_basis_per_wire(&z, &v, &wire_ep).unwrap();
        assert_eq!(result.len(), n);
        // Check result matches plain solve (diagonal system → I[k] = V[k]/Z[k]).
        for k in 0..n {
            let expected = v[k] / c(5.0, 0.0);
            assert!(
                (result[k] - expected).norm() < 1e-10,
                "k={k}: got={} expected={}",
                result[k],
                expected
            );
        }
    }
}
