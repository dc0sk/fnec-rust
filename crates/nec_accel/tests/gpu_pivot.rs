// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-254: the GPU solve's pivot search, on matrices that need it.
//!
//! No antenna's matrix exercises the pivot search: a Hallén matrix's largest
//! entry in a column is on its diagonal, so a search that looked at the diagonal
//! alone — or at one lane's rows — gave the same bits. The FND-193 fix moved the
//! search's reduction from workgroup memory to per-lane partials, and a sabotage
//! that took lane 0's candidate as the pivot passed every GPU test, and three
//! byte-identical before/after reports could not have told either. Here the
//! matrix is chosen: each column's largest entry is off the diagonal, the diagonal
//! is zero, and in the larger system it sits in a lane other than the diagonal's.
//! The solve is the production one (`solve_hallen_gpu_resident`'s augmented
//! system, column scaling, LU, refinement and residual gate) on a given Z; the
//! reference is f64 Gaussian elimination with partial pivoting on the same system.
//!
//! Skips vacuously when no wgpu adapter is available.

use nec_accel::gpu_test::gpu_attempt;
use nec_accel::{solve_hallen_gpu_resident_with_z_for_test, GpuSolveDeclined};
use num_complex::Complex64;

/// `n × n` with the column's largest entry `shift` rows down, `Z[(c + shift) mod
/// n][c] = 10 + j`, zeros in every row the diagonal's lane scans (`r ≡ c` mod 64,
/// the diagonal included) and small distinct entries elsewhere. A search confined
/// to the diagonal's lane sees only zeros.
fn shifted(n: usize, shift: usize) -> Vec<Complex64> {
    let mut z = vec![Complex64::new(0.0, 0.0); n * n];
    for r in 0..n {
        for c in 0..n {
            z[r * n + c] = if r == (c + shift) % n {
                Complex64::new(10.0, 1.0)
            } else if (r as i64 - c as i64).rem_euclid(64) == 0 {
                Complex64::new(0.0, 0.0)
            } else {
                Complex64::new(
                    0.01 * ((r * 7 + c * 3) % 11) as f64,
                    0.01 * ((r + 2 * c) % 5) as f64,
                )
            };
        }
    }
    z
}

/// The augmented system the resident solve forms for one wire with no sine
/// column: `[Z | −cos; constraint]`, the constraint `x[0] + 0.5·x[n] = 0`.
fn augmented(z: &[Complex64], cos: &[f64], n: usize) -> Vec<Vec<Complex64>> {
    let s = n + 1;
    let mut m = vec![vec![Complex64::new(0.0, 0.0); s]; s];
    for r in 0..n {
        for c in 0..n {
            m[r][c] = z[r * n + c];
        }
        m[r][n] = Complex64::new(-cos[r], 0.0);
    }
    m[n][0] = Complex64::new(1.0, 0.0);
    m[n][n] = Complex64::new(0.5, 0.0);
    m
}

/// f64 Gaussian elimination with partial pivoting.
fn solve_f64(mut m: Vec<Vec<Complex64>>, mut y: Vec<Complex64>) -> Vec<Complex64> {
    let s = y.len();
    for col in 0..s {
        let p = (col..s)
            .max_by(|&a, &b| m[a][col].norm().total_cmp(&m[b][col].norm()))
            .expect("rows");
        m.swap(col, p);
        y.swap(col, p);
        let pivot = m[col].clone();
        for row in col + 1..s {
            let f = m[row][col] / pivot[col];
            for (k, v) in pivot.iter().enumerate().skip(col) {
                m[row][k] -= f * v;
            }
            let v = y[col];
            y[row] -= f * v;
        }
    }
    let mut x = vec![Complex64::new(0.0, 0.0); s];
    for row in (0..s).rev() {
        let mut acc = y[row];
        for k in row + 1..s {
            acc -= m[row][k] * x[k];
        }
        x[row] = acc / m[row][row];
    }
    x
}

fn check(n: usize, shift: usize) {
    let what = format!("pivot, n = {n}, shift {shift}");
    let z = shifted(n, shift);
    let cos: Vec<f64> = (0..n).map(|r| 0.3 + 0.1 * (r % 7) as f64).collect();
    let sin = vec![0.0; n];
    let rhs: Vec<Complex64> = (0..n)
        .map(|r| Complex64::new(1.0 + 0.1 * (r % 3) as f64, 0.05 * (r % 4) as f64))
        .collect();
    let constraints = [(0usize, Some(n), 1.0, 0.5)];
    let attempt = gpu_attempt(&what, || {
        match pollster::block_on(solve_hallen_gpu_resident_with_z_for_test(
            &z,
            &rhs,
            &cos,
            &sin,
            &[(0, n - 1)],
            &[false],
            &constraints,
        )) {
            Err(e @ (GpuSolveDeclined::NoAdapter | GpuSolveDeclined::DeviceFailed)) => Err(e),
            Err(other) => panic!("{what}: the device solve declined: {other:?}"),
            ok => ok,
        }
    });
    let gpu = match attempt {
        Ok(x) => x,
        Err(GpuSolveDeclined::NoAdapter) => {
            eprintln!("{what}: no wgpu adapter — skipped");
            return;
        }
        Err(e) => panic!("{what}: no GPU is expected, yet the solve declined with {e:?}"),
    };
    let mut y = rhs.clone();
    y.push(Complex64::new(0.0, 0.0));
    let cpu = solve_f64(augmented(&z, &cos, n), y);
    assert_eq!(gpu.len(), cpu.len(), "{what}");
    let scale = cpu.iter().map(|v| v.norm()).fold(0.0, f64::max);
    for (i, (g, c)) in gpu.iter().zip(&cpu).enumerate() {
        assert!(
            (g - c).norm() < 1e-4 * scale,
            "{what}: x[{i}] = {g:.6} on the device, {c:.6} in f64"
        );
    }
}

/// Fewer rows than lanes: lane `col` sees only the diagonal, which is zero, so a
/// search that took lane 0's candidate — or the diagonal — divides by zero.
#[test]
fn the_pivot_is_the_columns_largest_entry_not_its_diagonal() {
    check(8, 7);
}

/// More rows than lanes: column 0's largest entry is in row 5, which only lane 5
/// scans; lane 0 sees rows 0 and 64, both zero. The pivot must come from the
/// reduction across lanes.
#[test]
fn the_pivot_search_reduces_across_lanes() {
    check(100, 5);
}
