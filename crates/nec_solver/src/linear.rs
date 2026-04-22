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

use crate::basis::ContinuityTransform;
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

    let lambda = 1e-10;
    for (i, row) in ata.iter_mut().enumerate().take(m) {
        row[i] += Complex64::new(lambda, 0.0);
    }

    let a_sol = solve_square_in_place(&mut ata, &mut atv)?;
    Ok(tr.segment_currents(&a_sol))
}

/// Result of solving Hallén's augmented system.
pub struct HallenSolution {
    /// Solved segment currents.
    pub currents: Vec<Complex64>,
    /// Homogeneous-constant coefficient C.
    pub c_hom: Complex64,
}

/// Solve Hallén's augmented system in least-squares form.
///
/// Unknown vector x = [I_0 .. I_{N-1}, C]^T.
///
/// Overdetermined augmented system:
///   A · I - C·cos_vec = rhs
///   I_0 = 0
///   I_{N-1} = 0

/// Solved via regularized normal equations:
///   (MᴴM + λI) x = Mᴴy
pub fn solve_hallen(
    z: &ZMatrix,
    rhs: &[Complex64],
    cos_vec: &[f64],
) -> Result<HallenSolution, SolveError> {
    let n = z.n;
    if rhs.len() != n || cos_vec.len() != n {
        return Err(SolveError::HallenDimensionMismatch {
            z_n: n,
            rhs_len: rhs.len(),
            cos_len: cos_vec.len(),
        });
    }

    let rows = n + 2;
    let cols = n + 1;
    let mut m = vec![vec![Complex64::new(0.0, 0.0); cols]; rows];
    let mut y = vec![Complex64::new(0.0, 0.0); rows];

    for r in 0..n {
        for c in 0..n {
            m[r][c] = z.get(r, c);
        }
        m[r][n] = Complex64::new(-cos_vec[r], 0.0);
        y[r] = rhs[r];
    }

    // Tip-current constraints.
    m[n][0] = Complex64::new(1.0, 0.0);
    m[n + 1][n - 1] = Complex64::new(1.0, 0.0);

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
    Ok(HallenSolution {
        currents: x[..n].to_vec(),
        c_hom: x[n],
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
    use crate::basis::ContinuityTransform;
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
    fn hallen_dimension_mismatch_returns_error() {
        let z = z_from_2d(vec![
            vec![c(1.0, 0.0), c(0.0, 0.0)],
            vec![c(0.0, 0.0), c(1.0, 0.0)],
        ]);

        let rhs = vec![c(1.0, 0.0)];
        let cos_vec = vec![1.0, 1.0];
        assert!(matches!(
            solve_hallen(&z, &rhs, &cos_vec),
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

        let a = solve_hallen(&z, &rhs_a, &cos_vec).unwrap();
        let b = solve_hallen(&z, &rhs_b, &cos_vec).unwrap();

        let mut diff = 0.0;
        for i in 0..3 {
            diff += (a.currents[i] - b.currents[i]).norm();
        }
        assert!(
            diff > 1e-6,
            "hallén solution should respond to RHS, diff={diff}"
        );
    }
}
