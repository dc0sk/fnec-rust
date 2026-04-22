// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::{
    assemble_pocklington_matrix, assemble_z_matrix, build_excitation, build_geometry,
    build_hallen_rhs, scale_excitation_for_pulse_rhs, solve, solve_hallen,
    solve_with_continuity_basis, Segment, ZMatrix,
};
use num_complex::Complex64;
use std::path::PathBuf;
use std::process::ExitCode;

const CONTINUITY_REL_RESIDUAL_MAX: f64 = 1e-3;

fn is_single_linear_chain(segs: &[Segment]) -> bool {
    if segs.is_empty() {
        return false;
    }
    let tag = segs[0].tag;
    for (idx, s) in segs.iter().enumerate() {
        if s.tag != tag {
            return false;
        }
        if s.tag_index as usize != idx + 1 {
            return false;
        }
    }
    true
}

fn l2_norm(v: &[Complex64]) -> f64 {
    v.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt()
}

fn residual_zi_minus_v(z: &ZMatrix, i_vec: &[Complex64], v_vec: &[Complex64]) -> (f64, f64) {
    let n = z.n;
    let mut r = vec![Complex64::new(0.0, 0.0); n];
    for row in 0..n {
        let mut zi = Complex64::new(0.0, 0.0);
        for (col, i_col) in i_vec.iter().enumerate().take(n) {
            zi += z.get(row, col) * *i_col;
        }
        r[row] = zi - v_vec[row];
    }

    let res = l2_norm(&r);
    let denom = l2_norm(v_vec);
    let rel = if denom > 0.0 { res / denom } else { res };
    (res, rel)
}

fn residual_hallen(
    z: &ZMatrix,
    i_vec: &[Complex64],
    c_hom: Complex64,
    cos_vec: &[f64],
    rhs: &[Complex64],
) -> (f64, f64) {
    let n = z.n;
    let mut r = vec![Complex64::new(0.0, 0.0); n];
    for row in 0..n {
        let mut zi = Complex64::new(0.0, 0.0);
        for (col, i_col) in i_vec.iter().enumerate().take(n) {
            zi += z.get(row, col) * *i_col;
        }
        let lhs = zi - c_hom * cos_vec[row];
        r[row] = lhs - rhs[row];
    }

    let res = l2_norm(&r);
    let denom = l2_norm(rhs);
    let rel = if denom > 0.0 { res / denom } else { res };
    (res, rel)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
        eprintln!("Usage: fnec [--solver <pulse|hallen|continuity>] <deck.nec>");
        return ExitCode::from(2);
    }

    let mut solver_mode = "hallen";
    let path_str;
    if args[1] == "--solver" && args.len() >= 4 {
        solver_mode = &args[2];
        path_str = &args[3];
    } else {
        path_str = &args[1];
    }

    let path = PathBuf::from(path_str);

    let input = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let result = match parse(&input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let deck = &result.deck;

    let freq_hz = match deck.cards.iter().find_map(|c| {
        if let Card::Fr(fr) = c {
            Some(fr.frequency_mhz * 1e6)
        } else {
            None
        }
    }) {
        Some(f) => f,
        None => return ExitCode::SUCCESS,
    };

    let segs = match build_geometry(deck) {
        Ok(s) => s,
        Err(_) => return ExitCode::FAILURE,
    };

    let v_vec = match build_excitation(deck, &segs) {
        Ok(v) => v,
        Err(_) => return ExitCode::FAILURE,
    };
    let v_vec_pulse = scale_excitation_for_pulse_rhs(&v_vec, freq_hz);

    let z_mat = match solver_mode {
        "hallen" => assemble_z_matrix(&segs, freq_hz),
        "pulse" | "continuity" => assemble_pocklington_matrix(&segs, freq_hz),
        _ => {
            eprintln!("unknown solver: {}", solver_mode);
            return ExitCode::FAILURE;
        }
    };

    let (i_vec, diag_abs, diag_rel, diag_label) = match solver_mode {
        "hallen" => {
            let hallen_rhs = match build_hallen_rhs(deck, &segs, freq_hz) {
                Ok(h) => h,
                Err(_) => return ExitCode::FAILURE,
            };
            match solve_hallen(&z_mat, &hallen_rhs.rhs, &hallen_rhs.cos_vec) {
                Ok(sol) => {
                    let (a, r) = residual_hallen(
                        &z_mat,
                        &sol.currents,
                        sol.c_hom,
                        &hallen_rhs.cos_vec,
                        &hallen_rhs.rhs,
                    );
                    (sol.currents, a, r, "hallen")
                }
                Err(_) => return ExitCode::FAILURE,
            }
        }
        "pulse" => match solve(&z_mat, &v_vec_pulse) {
            Ok(i) => {
                let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                (i, a, r, "pulse")
            }
            Err(_) => return ExitCode::FAILURE,
        },
        "continuity" => {
            if !is_single_linear_chain(&segs) {
                match solve(&z_mat, &v_vec_pulse) {
                    Ok(i) => {
                        let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                        (i, a, r, "continuity->pulse")
                    }
                    Err(_) => return ExitCode::FAILURE,
                }
            } else {
                match solve_with_continuity_basis(&z_mat, &v_vec_pulse) {
                    Ok(i) => {
                        let (a, r) = residual_zi_minus_v(&z_mat, &i, &v_vec_pulse);
                        if r <= CONTINUITY_REL_RESIDUAL_MAX {
                            (i, a, r, "continuity")
                        } else {
                            eprintln!(
                                "warning: continuity residual {:.3e} > {:.3e}; falling back to pulse",
                                r, CONTINUITY_REL_RESIDUAL_MAX
                            );
                            match solve(&z_mat, &v_vec_pulse) {
                                Ok(i2) => {
                                    let (a2, r2) = residual_zi_minus_v(&z_mat, &i2, &v_vec_pulse);
                                    (i2, a2, r2, "continuity->pulse(residual)")
                                }
                                Err(_) => return ExitCode::FAILURE,
                            }
                        }
                    }
                    Err(_) => return ExitCode::FAILURE,
                }
            }
        }
        _ => {
            eprintln!("unknown solver: {}", solver_mode);
            return ExitCode::FAILURE;
        }
    };

    for (idx, seg) in segs.iter().enumerate() {
        let v = v_vec[idx];
        if v.norm() < 1e-30 {
            continue;
        }
        let i = i_vec[idx];
        let v_source = v * seg.length;
        let z_in = if i.norm() > 1e-60 {
            v_source / i
        } else {
            v_source
        };
        println!(
            "{:<6} {:<6} {:>10.6}{:+.6}j {:>10.6}{:+.6}j {:>10.6}{:+.6}j",
            seg.tag, seg.tag_index, v_source.re, v_source.im, i.re, i.im, z_in.re, z_in.im,
        );
    }

    eprintln!(
        "diag: mode={diag_label} abs_res={:.6e} rel_res={:.6e}",
        diag_abs, diag_rel
    );

    ExitCode::SUCCESS
}
