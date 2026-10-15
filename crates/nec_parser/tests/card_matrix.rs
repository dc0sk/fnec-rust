// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `docs/card-support-matrix.md`, the one card table, is held to the parser: it
//! lists every card in `nec_parser::OTHER_CARDS` with the same treatment, and no
//! other (FND-236), and it has a row for every card the parser reads (PH3-CHK-001;
//! that check read a second table in `docs/nec4-support.md` until FND-235, against
//! a hand-written card list that had lost GM, GR, PT, NE and NH).

use nec_parser::{parse, OtherCard, ParseError, OTHER_CARDS};

fn matrix() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/card-support-matrix.md"),
    )
    .expect("card-support-matrix.md")
}

/// NEC-2's card set (NEC-2 User's Guide, Part III), xnec2c's PL and 4nec2's SY.
const NEC2: [&str; 36] = [
    "CM", "CE", "GA", "GC", "GD", "GE", "GF", "GH", "GM", "GR", "GS", "GW", "GX", "SC", "SM", "SP",
    "CP", "EK", "EN", "EX", "FR", "GN", "KH", "LD", "NE", "NH", "NT", "NX", "PQ", "PT", "RP", "TL",
    "WG", "XQ", "PL", "SY",
];

#[test]
fn every_card_the_parser_reads_has_a_matrix_row() {
    let doc = matrix();
    let parsed: Vec<&str> = NEC2
        .into_iter()
        .filter(|m| !OTHER_CARDS.iter().any(|(o, _)| o == m))
        .collect();
    // The parser reads each of them: none is unknown, ignored or refused.
    for m in &parsed {
        let r = parse(&format!("{m}\n"));
        assert!(
            !matches!(
                r,
                Err(ParseError::UnknownCard { .. } | ParseError::RefusedCard { .. })
            ) && !matches!(&r, Ok(p) if p.warnings.iter().any(|w| matches!(w, ParseError::IgnoredCard { .. }))),
            "{m} is neither parsed nor in OTHER_CARDS"
        );
    }
    assert!(parsed.len() >= 17, "{parsed:?}");
    let first_cells: Vec<String> = doc
        .lines()
        .filter(|l| l.starts_with("| "))
        .map(|l| l.split('|').nth(1).unwrap_or("").to_string())
        .collect();
    for m in parsed {
        assert!(
            first_cells.iter().any(|c| c
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .any(|w| w == m)),
            "card-support-matrix.md has no row for {m}, which the parser reads"
        );
    }
}

#[test]
fn the_card_matrix_lists_exactly_the_cards_fnec_does_not_model() {
    let doc = matrix();
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
    // A floor against a reader that silently reads nothing; the equality below is
    // the check. It drops as cards become modelled (PH10-CHK-002: GS 19 -> 18,
    // GX -> 17, GA/GH -> 15, GC -> 14).
    assert!(rows.len() >= 14, "only {} rows read: {rows:?}", rows.len());
    assert_eq!(rows, table);
}
