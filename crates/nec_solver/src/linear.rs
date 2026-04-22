// SPDX-License-Identifier: GPL-2.0-only
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

use crate::matrix::ZMatrix;

/// Error returned by the linear solver.
#[derive(Debug, Clone, PartialEq)]
pub enum SolveError {
    /// The matrix is singular (or numerically rank-deficient).
    Singular,
    /// The dimensions of Z and V do not match.
    DimensionMismatch { z_n: usize, v_len: usize },
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::Singular => write!(f, "impedance matrix is singular"),
            SolveError::DimensionMismatch { z_n, v_len } => {
                write!(f, "Z is {z_n}×{z_n} but V has length {v_len}")
            }
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

    // Build a mutable augmented matrix [Z | v] (row-major).
    let mut a: Vec<Vec<Complex64>> = (0..n)
        .map(|i| {
            let mut row: Vec<Complex64> = (0..n).map(|j| z.get(i, j)).collect();
            row.push(v[i]);
            row
        })
        .collect();

    // Gaussian elimination with partial pivoting.
    for col in 0..n {
        // Find pivot row (max |a[row][col]|).
        let pivot = (col..n)
            .max_by(|&r1, &r2| {
                a[r1][col]
                    .norm()
                    .partial_cmp(&a[r2][col].norm())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        if a[pivot][col].norm() < 1e-30 {
            return Err(SolveError::Singular);
        }

        a.swap(col, pivot);

        let diag = a[col][col];
        // Eliminate rows below.
        for row in (col + 1)..n {
            let factor = a[row][col] / diag;
            for k in col..=n {
                // =n because of the augmented column
                let sub = factor * a[col][k];
                a[row][k] -= sub;
            }
        }
    }

    // Back-substitution.
    let mut x = vec![Complex64::new(0.0, 0.0); n];
    for i in (0..n).rev() {
        let mut sum = a[i][n]; // RHS
        for j in (i + 1)..n {
            sum -= a[i][j] * x[j];
        }
        x[i] = sum / a[i][i];
    }

    Ok(x)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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
}
