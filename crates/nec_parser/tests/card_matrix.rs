// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `docs/card-support-matrix.md`, the one card table, is held to the parser: it
//! lists every card in `nec_parser::OTHER_CARDS` with the same treatment, and no
//! other (FND-236), and it has a row for every card the parser reads (PH3-CHK-001;
//! that check read a second table in `docs/nec4-support.md` until FND-235, against
//! a hand-written card list that had lost GM, GR, PT, NE and NH).
//!
//! It also holds the parser to `nec_model::card::FIELD_DISPOSITION`, the table of
//! what fnec does with every field position of every card it parses (the guard
//! against FND-280/283/285/298's class: a field accepted and never read): each
//! position is perturbed and the typed cards must change, the deck be refused, or
//! the cards stay as they were with the warning the table names — and every field
//! fnec ignores is listed in the matrix's field-disposition table. The half that
//! needs the solver is `apps/nec-cli/tests/card_field_reach.rs`. A field read
//! wrongly is not caught here; only the nec2c-anchored gates catch that.

use nec_model::card::{
    deck_with, field_name, perturbed_line, Disposition, FieldCase, Probe, FIELD_DISPOSITION,
};
use nec_parser::{parse, OtherCard, ParseError, ParseResult, OTHER_CARDS};

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

// ---------------------------------------------------------------------------
// FIELD_DISPOSITION, held to the parser
// ---------------------------------------------------------------------------

/// The cards the parser types: NEC-2's set less the ones it does not parse and
/// the three that carry no fields.
fn typed_cards() -> Vec<&'static str> {
    NEC2.into_iter()
        .filter(|m| !OTHER_CARDS.iter().any(|(o, _)| o == m))
        .filter(|m| !matches!(*m, "CM" | "CE" | "EN"))
        .collect()
}

/// What a check found: failures, and the tolerated `KnownGap` entries that failed.
#[derive(Default, Debug)]
struct Findings {
    failures: Vec<String>,
    gaps: Vec<String>,
}

/// The card an error names, if it names one.
fn error_card(e: &ParseError) -> Option<&str> {
    match e {
        ParseError::UnknownCard { mnemonic, .. }
        | ParseError::RefusedCard { mnemonic, .. }
        | ParseError::IgnoredCard { mnemonic, .. }
        | ParseError::GeometryAfterGe { mnemonic, .. } => Some(mnemonic),
        ParseError::IgnoredField { card, .. }
        | ParseError::BadField { card, .. }
        | ParseError::UnsupportedValue { card, .. }
        | ParseError::TooFewFields { card, .. } => Some(card),
        ParseError::TaperPairing { .. } => None,
    }
}

fn warns(r: &ParseResult, card: &str, why: &str) -> bool {
    r.warnings.iter().any(|w| {
        matches!(w, ParseError::IgnoredField { card: c, why: text, .. } if c == card && text.contains(why))
    })
}

type Parser<'a> = &'a dyn Fn(&str) -> Result<ParseResult, ParseError>;

/// Perturb every position of every row and check the parser does what the row
/// says.
fn check_fields(rows: &[FieldCase], parse: Parser) -> Findings {
    let mut out = Findings::default();
    for row in rows {
        let at = format!("{} [{}]", row.card, row.case);
        if row.deck.matches("{card}").count() != 1 || !row.line.starts_with(row.card) {
            out.failures.push(format!(
                "{at}: the deck must hold `{{card}}` once, the line start with the card"
            ));
            continue;
        }
        let base = match parse(&deck_with(row, row.line)) {
            Ok(b) => b,
            Err(e) => {
                out.failures
                    .push(format!("{at}: the baseline deck does not parse: {e}"));
                continue;
            }
        };
        let tokens: Vec<&str> = row.line.split_whitespace().collect();
        for (i, probes) in row.fields.iter().enumerate() {
            let pos = i + 1;
            let name = field_name(row.card, pos);
            if probes.is_empty() {
                out.failures.push(format!(
                    "{at} {name}: no probe, so the position is not accounted for"
                ));
            }
            for Probe { value, disposition } in probes.iter().copied() {
                let here = format!("{at} {name} = {value} ({disposition:?})");
                let written = tokens.get(pos).copied().unwrap_or("0");
                if written == value {
                    out.failures.push(format!(
                        "{here}: the probe is the baseline's value, so it probes nothing"
                    ));
                    continue;
                }
                let line = perturbed_line(row.line, pos, value);
                let got = parse(&deck_with(row, &line));
                let cards_differ = |r: &ParseResult| r.deck.cards != base.deck.cards;
                let problem: Option<String> = match (disposition, &got) {
                    (Disposition::Refused, Err(e)) if error_card(e) == Some(row.card) => None,
                    (Disposition::Refused, Err(e)) => {
                        Some(format!("refused, but not naming {}: {e}", row.card))
                    }
                    (Disposition::Refused, Ok(_)) => {
                        Some("parsed; the table says it is refused".to_string())
                    }
                    (_, Err(e)) => Some(format!("refused: {e}")),
                    (Disposition::Consumed | Disposition::KnownGap(_), Ok(r)) => (!cards_differ(r))
                        .then(|| "the typed cards are unchanged: the parser drops it".to_string()),
                    (Disposition::Ignored(why), Ok(r)) => {
                        if cards_differ(r) {
                            Some("the typed cards change, so it is not ignored".to_string())
                        } else if !warns(r, row.card, why) {
                            Some(format!(
                                "no {} warning carrying {why:?}: {:?}",
                                row.card, r.warnings
                            ))
                        } else {
                            None
                        }
                    }
                    (Disposition::Silent(_) | Disposition::Unused, Ok(r)) => {
                        if cards_differ(r) {
                            Some("the typed cards change, so it is not ignored".to_string())
                        } else if r.warnings != base.warnings {
                            Some(format!(
                                "warns, where the table says it is silent: {:?}",
                                r.warnings
                            ))
                        } else {
                            None
                        }
                    }
                    (Disposition::Kept { why, warned }, Ok(r)) => {
                        if !cards_differ(r) {
                            Some(
                                "the typed cards are unchanged; the table says the value is kept"
                                    .to_string(),
                            )
                        } else if warned && !warns(r, row.card, why) {
                            Some(format!(
                                "no {} warning carrying {why:?}: {:?}",
                                row.card, r.warnings
                            ))
                        } else if !warned && r.warnings != base.warnings {
                            Some(format!(
                                "warns, where the table says it is silent: {:?}",
                                r.warnings
                            ))
                        } else {
                            None
                        }
                    }
                };
                if let Some(problem) = problem {
                    let msg = format!("{here}: {problem}\n    {line}");
                    if matches!(disposition, Disposition::KnownGap(_)) {
                        out.gaps.push(msg);
                    } else {
                        out.failures.push(msg);
                    }
                }
            }
        }
    }
    out
}

/// The field-disposition table's rows as `card-support-matrix.md` must list
/// them: every ignored value, grouped by card, case, treatment and reason.
fn expected_doc_rows(rows: &[FieldCase]) -> Vec<String> {
    // (card, case, treatment, why) -> the fields, in position order.
    type Key<'a> = (&'a str, &'a str, &'a str, &'a str);
    let mut groups: Vec<(Key, Vec<String>)> = Vec::new();
    for row in rows {
        for (i, probes) in row.fields.iter().enumerate() {
            for probe in probes.iter() {
                let (treatment, why) = match probe.disposition {
                    Disposition::Ignored(why) | Disposition::Kept { why, warned: true } => {
                        ("ignored, with a warning", why)
                    }
                    Disposition::Silent(why) | Disposition::Kept { why, warned: false } => {
                        ("ignored", why)
                    }
                    _ => continue,
                };
                let key = (row.card, row.case, treatment, why);
                let name = field_name(row.card, i + 1);
                match groups.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, names)) if names.contains(&name) => {}
                    Some((_, names)) => names.push(name),
                    None => groups.push((key, vec![name])),
                }
            }
        }
    }
    groups
        .into_iter()
        .map(|((card, case, treatment, why), names)| {
            format!(
                "| {card} | {case} | {} | {treatment} | {why} |",
                names.join(", ")
            )
        })
        .collect()
}

const FIELD_SECTION: &str = "### Fields fnec reads and does not use";

fn check_doc(rows: &[FieldCase], doc: &str) -> Vec<String> {
    let Some(start) = doc.find(FIELD_SECTION) else {
        return vec![format!(
            "card-support-matrix.md has no `{FIELD_SECTION}` section"
        )];
    };
    let section = &doc[start + FIELD_SECTION.len()..];
    let section = &section[..section.find("\n#").unwrap_or(section.len())];
    let listed: Vec<String> = section
        .lines()
        .filter(|l| l.starts_with("| ") && !l.starts_with("| Card "))
        .map(|l| {
            l.split('|')
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(" | ")
                .trim()
                .to_string()
        })
        .collect();
    let norm = |l: &str| {
        l.split('|')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" | ")
            .trim()
            .to_string()
    };
    let expected: Vec<String> = expected_doc_rows(rows);
    let mut out = Vec::new();
    for e in &expected {
        if !listed.contains(&norm(e)) {
            out.push(format!("card-support-matrix.md lacks the row\n    {e}"));
        }
    }
    for l in &listed {
        if !expected.iter().any(|e| norm(e) == *l) {
            out.push(format!(
                "card-support-matrix.md lists a row FIELD_DISPOSITION does not hold\n    {l}"
            ));
        }
    }
    out
}

fn report(f: &Findings) {
    for g in &f.gaps {
        eprintln!("KNOWN GAP (tolerated, listed until decided): {g}");
    }
    assert!(
        f.failures.is_empty(),
        "{} failure(s):\n{}",
        f.failures.len(),
        f.failures.join("\n")
    );
}

#[test]
fn every_card_the_parser_types_has_field_dispositions() {
    let mut table: Vec<&str> = FIELD_DISPOSITION.iter().map(|r| r.card).collect();
    table.sort_unstable();
    table.dedup();
    let mut typed = typed_cards();
    typed.sort_unstable();
    assert_eq!(
        table, typed,
        "FIELD_DISPOSITION's cards against the parser's"
    );
    assert_eq!(typed.len(), 19, "{typed:?}");
}

#[test]
fn every_field_position_of_every_card_is_accounted_for() {
    let f = check_fields(FIELD_DISPOSITION, &parse);
    report(&f);
    // A floor against a checker that silently checks nothing.
    let probes: usize = FIELD_DISPOSITION
        .iter()
        .flat_map(|r| r.fields.iter())
        .map(|p| p.len())
        .sum();
    assert!(probes >= 12 * FIELD_DISPOSITION.len(), "{probes}");
}

#[test]
fn the_matrix_lists_every_field_fnec_ignores() {
    let problems = check_doc(FIELD_DISPOSITION, &matrix());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(expected_doc_rows(FIELD_DISPOSITION).len() >= 10);
}

// ---- the guard's standing self-test: each sabotage must make it fail ----

fn rows_of(card: &str) -> Vec<FieldCase> {
    FIELD_DISPOSITION
        .iter()
        .filter(|r| r.card == card)
        .copied()
        .collect()
}

#[test]
fn self_test_a_parser_that_drops_gn_i2_fails_the_guard() {
    // The parser reading GN as if I2 were never written: FND-283's defect.
    let drop_i2 = |deck: &str| -> Result<ParseResult, ParseError> {
        let deck: String = deck
            .lines()
            .map(|l| match l.strip_prefix("GN ") {
                Some(rest) => {
                    let mut t: Vec<&str> = rest.split_whitespace().collect();
                    if t.len() > 1 {
                        t[1] = "0";
                    }
                    format!("GN {}\n", t.join(" "))
                }
                None => format!("{l}\n"),
            })
            .collect();
        parse(&deck)
    };
    let f = check_fields(&rows_of("GN"), &drop_i2);
    assert!(
        f.failures
            .iter()
            .any(|m| m.starts_with("GN [I1 = 0, I2 = 0] I2 = 16 (Refused)")),
        "{:#?}",
        f.failures
    );
}

#[test]
fn self_test_an_ignored_field_without_its_matrix_row_fails_the_guard() {
    let doc = matrix();
    let row = expected_doc_rows(FIELD_DISPOSITION)
        .into_iter()
        .find(|r| r.starts_with("| GN | I1 = 0, I2 = 0 | I3"))
        .expect("GN I3's row");
    let without: String = doc
        .lines()
        .filter(|l| {
            l.split('|').map(str::trim).collect::<Vec<_>>()
                != row.split('|').map(str::trim).collect::<Vec<_>>()
        })
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(without.len(), doc.len(), "the row is in the matrix");
    let problems = check_doc(FIELD_DISPOSITION, &without);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("lacks the row") && p.contains("| GN | I1 = 0, I2 = 0 | I3")),
        "{problems:#?}"
    );
}

#[test]
fn self_test_a_consumed_field_marked_ignored_fails_the_guard() {
    let mut rows = rows_of("LD");
    rows[0].fields[1] = &[Probe {
        value: "2",
        disposition: Disposition::Ignored("the tag"),
    }];
    let f = check_fields(&rows[..1], &parse);
    assert!(
        f.failures
            .iter()
            .any(|m| m.starts_with("LD [I1 = 0] I2 = 2") && m.contains("not ignored")),
        "{:#?}",
        f.failures
    );
}

#[test]
fn self_test_a_position_without_a_probe_fails_the_guard() {
    let mut rows = rows_of("FR");
    rows[0].fields[2] = &[];
    let f = check_fields(&rows, &parse);
    assert!(
        f.failures
            .iter()
            .any(|m| m.contains("FR [any] I3: no probe")),
        "{:#?}",
        f.failures
    );
}

#[test]
fn self_test_a_probe_equal_to_the_baseline_fails_the_guard() {
    let mut rows = rows_of("FR");
    rows[0].fields[4] = &[Probe {
        value: "29.9",
        disposition: Disposition::Consumed,
    }];
    let f = check_fields(&rows, &parse);
    assert!(
        f.failures.iter().any(|m| m.contains("probes nothing")),
        "{:#?}",
        f.failures
    );
}
