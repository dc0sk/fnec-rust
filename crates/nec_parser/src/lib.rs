// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Minimal Phase-1 NEC deck parser.
//!
//! Parses the cards required to run a basic dipole simulation:
//! `CM`, `CE`, `GW`, `EX`, `FR`, `RP`, `EN`.
//!
//! Unknown cards produce a [`ParseError::UnknownCard`] but do not stop
//! parsing — callers decide whether to treat them as fatal.

use nec_model::card::{Card, CommentCard, EnCard, ExCard, FrCard, GwCard, RpCard};
use nec_model::deck::NecDeck;

/// A parse error.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// A card mnemonic was encountered that this parser does not recognise.
    UnknownCard { line: usize, mnemonic: String },
    /// A field could not be converted to the expected numeric type.
    BadField {
        line: usize,
        card: String,
        field: usize,
        raw: String,
    },
    /// A card did not have enough fields.
    TooFewFields {
        line: usize,
        card: String,
        need: usize,
        got: usize,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownCard { line, mnemonic } => {
                write!(f, "line {line}: unknown card '{mnemonic}'")
            }
            ParseError::BadField {
                line,
                card,
                field,
                raw,
            } => {
                write!(f, "line {line}: {card} field {field}: cannot parse '{raw}'")
            }
            ParseError::TooFewFields {
                line,
                card,
                need,
                got,
            } => {
                write!(f, "line {line}: {card} needs {need} fields, got {got}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Result of parsing a NEC deck.
///
/// Non-fatal errors (unknown cards) are collected in `errors` while parsing
/// continues.  Fatal errors (bad field values) are returned early via `Err`.
pub struct ParseResult {
    pub deck: NecDeck,
    /// Non-fatal warnings accumulated during parsing.
    pub warnings: Vec<ParseError>,
}

/// Parse a NEC deck from a string.
///
/// Returns `Err` on the first fatal parse error.  Unknown cards are collected
/// as non-fatal warnings and parsing continues.
pub fn parse(input: &str) -> Result<ParseResult, ParseError> {
    let mut deck = NecDeck::new();
    let mut warnings = Vec::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw_line.trim();

        // Skip blank lines and lines that are entirely whitespace.
        if line.is_empty() {
            continue;
        }

        // The mnemonic is the first whitespace-delimited token.
        let (mnemonic, rest) = split_mnemonic(line);

        match mnemonic.to_ascii_uppercase().as_str() {
            "CM" => {
                deck.cards.push(Card::Comment(CommentCard {
                    text: rest.trim().to_string(),
                }));
            }
            "CE" => {
                // CE ends the comment block; any text on the line is still a
                // comment per the NEC convention.
                deck.cards.push(Card::Comment(CommentCard {
                    text: rest.trim().to_string(),
                }));
            }
            "GW" => {
                let fields = parse_fields(rest);
                require_fields(lineno, "GW", &fields, 9)?;
                deck.cards.push(Card::Gw(GwCard {
                    tag: parse_u32(lineno, "GW", 1, &fields[0])?,
                    segments: parse_u32(lineno, "GW", 2, &fields[1])?,
                    start: [
                        parse_f64(lineno, "GW", 3, &fields[2])?,
                        parse_f64(lineno, "GW", 4, &fields[3])?,
                        parse_f64(lineno, "GW", 5, &fields[4])?,
                    ],
                    end: [
                        parse_f64(lineno, "GW", 6, &fields[5])?,
                        parse_f64(lineno, "GW", 7, &fields[6])?,
                        parse_f64(lineno, "GW", 8, &fields[7])?,
                    ],
                    radius: parse_f64(lineno, "GW", 9, &fields[8])?,
                }));
            }
            "EX" => {
                let fields = parse_fields(rest);
                // Standard NEC EX card: I1 I2 I3 I4 F1 F2
                // I4 is required by the spec; F1 (voltage real) and F2 (voltage imag)
                // default to 0.0 if absent.
                require_fields(lineno, "EX", &fields, 4)?;
                let vr = if fields.len() > 4 {
                    parse_f64(lineno, "EX", 5, &fields[4])?
                } else {
                    0.0
                };
                let vi = if fields.len() > 5 {
                    parse_f64(lineno, "EX", 6, &fields[5])?
                } else {
                    0.0
                };
                deck.cards.push(Card::Ex(ExCard {
                    excitation_type: parse_u32(lineno, "EX", 1, &fields[0])?,
                    tag: parse_u32(lineno, "EX", 2, &fields[1])?,
                    segment: parse_u32(lineno, "EX", 3, &fields[2])?,
                    i4: parse_u32(lineno, "EX", 4, &fields[3])?,
                    voltage_real: vr,
                    voltage_imag: vi,
                }));
            }
            "FR" => {
                let fields = parse_fields(rest);
                require_fields(lineno, "FR", &fields, 4)?;
                // NEC2 canonical form has 4 integer fields then floats:
                //   FR I1 I2 I3 I4 F1 F2
                // Many decks (and our own test fixtures) omit the unused I3/I4
                // and use the shorthand:  FR I1 I2 F1 F2
                // Distinguish by testing whether field[2] parses as a float ≥ 1.
                let (freq_idx, step_idx) = if fields.len() >= 6 {
                    (4, 5) // canonical: skip I3/I4
                } else {
                    (2, 3) // shorthand: no I3/I4
                };
                deck.cards.push(Card::Fr(FrCard {
                    step_type: parse_u32(lineno, "FR", 1, &fields[0])?,
                    steps: parse_u32(lineno, "FR", 2, &fields[1])?,
                    frequency_mhz: parse_f64(lineno, "FR", freq_idx + 1, &fields[freq_idx])?,
                    step_mhz: parse_f64(lineno, "FR", step_idx + 1, &fields[step_idx])?,
                }));
            }
            "RP" => {
                let fields = parse_fields(rest);
                require_fields(lineno, "RP", &fields, 7)?;
                deck.cards.push(Card::Rp(RpCard {
                    mode: parse_u32(lineno, "RP", 1, &fields[0])?,
                    n_theta: parse_u32(lineno, "RP", 2, &fields[1])?,
                    n_phi: parse_u32(lineno, "RP", 3, &fields[2])?,
                    theta0: parse_f64(lineno, "RP", 4, &fields[3])?,
                    phi0: parse_f64(lineno, "RP", 5, &fields[4])?,
                    d_theta: parse_f64(lineno, "RP", 6, &fields[5])?,
                    d_phi: parse_f64(lineno, "RP", 7, &fields[6])?,
                }));
            }
            "EN" => {
                deck.cards.push(Card::En(EnCard));
                // Stop at EN per NEC spec.
                break;
            }
            other => {
                warnings.push(ParseError::UnknownCard {
                    line: lineno,
                    mnemonic: other.to_string(),
                });
            }
        }
    }

    Ok(ParseResult { deck, warnings })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn split_mnemonic(line: &str) -> (&str, &str) {
    match line.find(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => (&line[..pos], &line[pos..]),
        None => (line, ""),
    }
}

fn parse_fields(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

fn require_fields(
    lineno: usize,
    card: &str,
    fields: &[&str],
    need: usize,
) -> Result<(), ParseError> {
    if fields.len() < need {
        Err(ParseError::TooFewFields {
            line: lineno,
            card: card.to_string(),
            need,
            got: fields.len(),
        })
    } else {
        Ok(())
    }
}

fn parse_u32(lineno: usize, card: &str, field: usize, s: &str) -> Result<u32, ParseError> {
    // Accept floats like "1.0" that 4nec2 sometimes emits for integer fields.
    s.parse::<f64>()
        .map(|v| v as u32)
        .map_err(|_| ParseError::BadField {
            line: lineno,
            card: card.to_string(),
            field,
            raw: s.to_string(),
        })
}

fn parse_f64(lineno: usize, card: &str, field: usize, s: &str) -> Result<f64, ParseError> {
    s.parse::<f64>().map_err(|_| ParseError::BadField {
        line: lineno,
        card: card.to_string(),
        field,
        raw: s.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, CommentCard, EnCard, ExCard, FrCard, GwCard, RpCard};

    /// Minimal half-wave dipole deck used as golden round-trip fixture.
    /// EX format: I1=type I2=tag I3=seg I4=aux-int F1=vr F2=vi
    const DIPOLE_DECK: &str = "\
CM Half-wave dipole at 28 MHz
CE
GW 1 11 0.0 0.0 -2.677 0.0 0.0 2.677 0.001
EX 0 1 6 0 1.0 0.0
FR 0 1 28.0 0.0
RP 0 37 1 0.0 0.0 5.0 0.0
EN
";

    #[test]
    fn parse_dipole_deck_cards() {
        let result = parse(DIPOLE_DECK).expect("parse must succeed");
        assert!(
            result.warnings.is_empty(),
            "unexpected warnings: {:?}",
            result.warnings
        );

        let cards = &result.deck.cards;
        assert_eq!(cards.len(), 7);

        // CM
        assert_eq!(
            cards[0],
            Card::Comment(CommentCard {
                text: "Half-wave dipole at 28 MHz".into()
            })
        );
        // CE (empty text)
        assert_eq!(cards[1], Card::Comment(CommentCard { text: "".into() }));
        // GW
        assert_eq!(
            cards[2],
            Card::Gw(GwCard {
                tag: 1,
                segments: 11,
                start: [0.0, 0.0, -2.677],
                end: [0.0, 0.0, 2.677],
                radius: 0.001,
            })
        );
        // EX
        assert_eq!(
            cards[3],
            Card::Ex(ExCard {
                excitation_type: 0,
                tag: 1,
                segment: 6,
                i4: 0,
                voltage_real: 1.0,
                voltage_imag: 0.0,
            })
        );
        // FR
        assert_eq!(
            cards[4],
            Card::Fr(FrCard {
                step_type: 0,
                steps: 1,
                frequency_mhz: 28.0,
                step_mhz: 0.0,
            })
        );
        // RP
        assert_eq!(
            cards[5],
            Card::Rp(RpCard {
                mode: 0,
                n_theta: 37,
                n_phi: 1,
                theta0: 0.0,
                phi0: 0.0,
                d_theta: 5.0,
                d_phi: 0.0,
            })
        );
        // EN
        assert_eq!(cards[6], Card::En(EnCard));
    }

    #[test]
    fn unknown_card_is_warning_not_error() {
        let input = "CM test\nXX some unknown card\nEN\n";
        let result = parse(input).expect("should not be a fatal error");
        assert_eq!(result.warnings.len(), 1);
        assert!(matches!(
            &result.warnings[0],
            ParseError::UnknownCard { mnemonic, .. } if mnemonic == "XX"
        ));
    }

    #[test]
    fn too_few_gw_fields_is_error() {
        let input = "GW 1 11 0.0\n";
        assert!(parse(input).is_err());
    }

    #[test]
    fn bad_numeric_field_is_error() {
        let input = "GW 1 11 X 0.0 -2.677 0.0 0.0 2.677 0.001\n";
        assert!(parse(input).is_err());
    }

    #[test]
    fn blank_lines_are_skipped() {
        let input = "\nCM test\n\nEN\n\n";
        let result = parse(input).expect("parse must succeed");
        assert_eq!(result.deck.cards.len(), 2);
    }

    #[test]
    fn stops_at_en() {
        let input = "CM before\nEN\nCM after\n";
        let result = parse(input).expect("parse must succeed");
        // CM after EN must not appear
        assert_eq!(result.deck.cards.len(), 2);
        assert_eq!(result.deck.cards[1], Card::En(EnCard));
    }

    #[test]
    fn ex_i4_field_is_preserved() {
        let input = "EX 5 2 7 3 1.5 -0.25\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        assert_eq!(result.deck.cards.len(), 2);
        assert_eq!(
            result.deck.cards[0],
            Card::Ex(ExCard {
                excitation_type: 5,
                tag: 2,
                segment: 7,
                i4: 3,
                voltage_real: 1.5,
                voltage_imag: -0.25,
            })
        );
    }
}
