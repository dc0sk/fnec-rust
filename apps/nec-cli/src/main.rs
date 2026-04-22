// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_model::card::Card;
use nec_parser::parse;
use nec_solver::{assemble_z_matrix, build_excitation, build_geometry, solve};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("fnec {}", env!("CARGO_PKG_VERSION"));
        eprintln!("Usage: fnec <deck.nec>");
        return ExitCode::from(2);
    }

    let path = PathBuf::from(&args[1]);

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

    for w in &result.warnings {
        eprintln!("warning: {w}");
    }

    let deck = &result.deck;

    println!("Deck: {}", path.display());
    println!("  Cards parsed : {}", deck.cards.len());

    // Print a brief inventory of card types.
    let mut n_comment = 0u32;
    let mut n_gw = 0u32;
    let mut n_ex = 0u32;
    let mut n_fr = 0u32;
    let mut n_rp = 0u32;
    let mut n_en = 0u32;

    for card in &deck.cards {
        match card {
            Card::Comment(_) => n_comment += 1,
            Card::Gw(_) => n_gw += 1,
            Card::Ex(_) => n_ex += 1,
            Card::Fr(_) => n_fr += 1,
            Card::Rp(_) => n_rp += 1,
            Card::En(_) => n_en += 1,
        }
    }

    if n_comment > 0 {
        println!("  CM/CE        : {n_comment}");
    }
    if n_gw > 0 {
        println!("  GW wires     : {n_gw}");
    }
    if n_ex > 0 {
        println!("  EX sources   : {n_ex}");
    }
    if n_fr > 0 {
        println!("  FR freq      : {n_fr}");
    }
    if n_rp > 0 {
        println!("  RP requests  : {n_rp}");
    }
    if n_en > 0 {
        println!("  EN           : {n_en}");
    }
    if !result.warnings.is_empty() {
        println!("  Warnings     : {}", result.warnings.len());
    }

    // ------------------------------------------------------------------
    // End-to-end solve (requires GW + EX + FR cards).
    // ------------------------------------------------------------------

    // Extract the first FR frequency; skip the solve if none present.
    let freq_hz = match deck.cards.iter().find_map(|c| {
        if let Card::Fr(fr) = c {
            Some(fr.frequency_mhz * 1e6)
        } else {
            None
        }
    }) {
        Some(f) => f,
        None => {
            println!("\n[No FR card — skipping impedance computation]");
            return ExitCode::SUCCESS;
        }
    };

    // Skip if there are no EX sources.
    if n_ex == 0 {
        println!("\n[No EX card — skipping impedance computation]");
        return ExitCode::SUCCESS;
    }

    // Build geometry.
    let segs = match build_geometry(deck) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error (geometry): {e}");
            return ExitCode::FAILURE;
        }
    };

    // Build excitation vector.
    let v_vec = match build_excitation(deck, &segs) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error (excitation): {e}");
            return ExitCode::FAILURE;
        }
    };

    // Assemble impedance matrix.
    println!(
        "\nAssembling {n}×{n} impedance matrix at {f:.3} MHz …",
        n = segs.len(),
        f = freq_hz / 1e6
    );
    let z_mat = assemble_z_matrix(&segs, freq_hz);

    // Solve Z·I = V.
    let i_vec = match solve(&z_mat, &v_vec) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error (solver): {e}");
            return ExitCode::FAILURE;
        }
    };

    // Print feedpoint results for every driven segment.
    println!("\nFeedpoint Results:");
    println!(
        "{:<6} {:<6} {:>20} {:>20} {:>18}",
        "Tag", "Seg", "V (V)", "I (A)", "Z_in (Ω)"
    );
    println!("{}", "-".repeat(76));

    let mut any_driven = false;
    for (idx, seg) in segs.iter().enumerate() {
        let v = v_vec[idx];
        if v.norm() < 1e-30 {
            continue;
        }
        any_driven = true;
        let i = i_vec[idx];
        let z_in = if i.norm() > 1e-60 { v / i } else { v };
        println!(
            "{:<6} {:<6} {:>10.4}{:+.4}j {:>10.6}{:+.6}j {:>8.3}{:+.3}j",
            seg.tag, seg.tag_index, v.re, v.im, i.re, i.im, z_in.re, z_in.im,
        );
    }

    if !any_driven {
        println!("  (no driven segments found)");
    }

    ExitCode::SUCCESS
}
