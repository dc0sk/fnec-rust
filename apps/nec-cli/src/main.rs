// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

use nec_model::card::Card;
use nec_parser::parse;
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

    println!("Deck: {}", path.display());
    println!("  Cards parsed : {}", result.deck.cards.len());

    // Print a brief inventory of card types.
    let mut n_comment = 0u32;
    let mut n_gw = 0u32;
    let mut n_ex = 0u32;
    let mut n_fr = 0u32;
    let mut n_rp = 0u32;
    let mut n_en = 0u32;

    for card in &result.deck.cards {
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

    ExitCode::SUCCESS
}
