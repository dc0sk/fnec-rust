// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

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
///   I_{wire_k_first} = 0  (for each wire k)
///   I_{wire_k_last}  = 0  (for each wire k)
///
/// `wire_endpoints` is a slice of `(first_seg_idx, last_seg_idx)` per wire.
/// An empty slice falls back to constraining only the global first and last segment,
/// which is correct for single-wire decks.
///
/// Solved via regularized normal equations:
///   (MᴴM + λI) x = Mᴴy
pub fn solve_hallen(
    z: &ZMatrix,
    rhs: &[Complex64],
    cos_vec: &[f64],
    wire_endpoints: &[(usize, usize)],
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
    let endpoints: &[(usize, usize)];
    let fallback_endpoints;
    if wire_endpoints.is_empty() || n == 0 {
        fallback_endpoints = if n > 0 { vec![(0usize, n - 1)] } else { vec![] };
        endpoints = &fallback_endpoints;
    } else {
        endpoints = wire_endpoints;
    }

    let w = endpoints.len();
    let rows = n + 2 * w;
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

    for (w, &(first, last)) in endpoints.iter().enumerate() {
        m[n + 2 * w][first] = Complex64::new(1.0, 0.0);
        m[n + 2 * w + 1][last] = Complex64::new(1.0, 0.0);
    }

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

fn solve_square_in_place(
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
            solve_hallen(&z, &rhs, &cos_vec, &[]),
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

        let a = solve_hallen(&z, &rhs_a, &cos_vec, &[]).unwrap();
        let b = solve_hallen(&z, &rhs_b, &cos_vec, &[]).unwrap();

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
