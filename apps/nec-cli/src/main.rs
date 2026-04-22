// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::{
    assemble_z_matrix, build_excitation, build_geometry, build_hallen_rhs, solve, solve_hallen,
    solve_with_continuity_basis,
};
use std::path::PathBuf;
use std::process::ExitCode;

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

    let z_mat = assemble_z_matrix(&segs, freq_hz);

    let i_vec = match solver_mode {
        "hallen" => {
            let hallen_rhs = match build_hallen_rhs(deck, &segs, freq_hz) {
                Ok(h) => h,
                Err(_) => return ExitCode::FAILURE,
            };
            match solve_hallen(&z_mat, &hallen_rhs.rhs, &hallen_rhs.cos_vec) {
                Ok(sol) => sol.currents,
                Err(_) => return ExitCode::FAILURE,
            }
        }
        "pulse" => match solve(&z_mat, &v_vec) {
            Ok(i) => i,
            Err(_) => return ExitCode::FAILURE,
        },
        "continuity" => match solve_with_continuity_basis(&z_mat, &v_vec) {
            Ok(i) => i,
            Err(_) => return ExitCode::FAILURE,
        },
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

    ExitCode::SUCCESS
}
