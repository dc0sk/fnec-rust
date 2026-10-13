// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-236: `docs/card-support-matrix.md` lists every card in
//! `nec_parser::OTHER_CARDS`, with the same treatment, and no other — so the
//! document cannot say a card is ignored that fnec refuses, or the reverse.

use nec_parser::{OtherCard, OTHER_CARDS};

#[test]
fn the_card_matrix_lists_exactly_the_cards_fnec_does_not_model() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/card-support-matrix.md"),
    )
    .expect("card-support-matrix.md");
    let start = doc
        .find("## Cards fnec does not model")
        .expect("the section exists");
    let section = &doc[start..];
    let section = &section[..section[3..].find("\n## ").map_or(section.len(), |i| i + 3)];
    let mut rows: Vec<(String, String)> = section
        .lines()
        .filter(|l| l.starts_with("| ") && !l.starts_with("| Card"))
        .map(|l| {
            let c: Vec<&str> = l.split('|').map(str::trim).collect();
            (c[1].to_string(), c[2].to_string())
        })
        .collect();
    let mut table: Vec<(String, String)> = OTHER_CARDS
        .iter()
        .map(|(m, k)| {
            let kind = match k {
                OtherCard::Ignored(_) => "Ignored",
                OtherCard::Refused(_) => "Refused",
            };
            (m.to_string(), kind.to_string())
        })
        .collect();
    rows.sort();
    table.sort();
    assert!(rows.len() >= 19, "only {} rows read: {rows:?}", rows.len());
    assert_eq!(rows, table);
}
