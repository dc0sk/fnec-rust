// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Minimal Phase-1 NEC deck parser.
//!
//! Parses the cards required to run a basic dipole simulation:
//! `CM`, `CE`, `GW`, `GE`, `GN`, `EX`, `FR`, `RP`, `EN`.
//!
//! Extended support for:
//! - `LD` — Lumped and distributed loads (types 0, 4, 5)
//! - `TL` — Transmission-line connections (lossless and lossy models)
//! - `PT` — Preserved for staged portability (runtime semantics deferred)
//! - `NT` — Preserved for staged portability (runtime semantics deferred)
//!
//! Unknown cards produce a [`ParseError::UnknownCard`] but do not stop
//! parsing — callers decide whether to treat them as fatal.

use nec_model::card::{
    Card, CommentCard, EnCard, ExCard, FrCard, GeCard, GmCard, GnCard, GrCard, GwCard, LdCard,
    NtCard, PtCard, RpCard, TlCard,
};
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
            "GE" => {
                let fields = parse_fields(rest);
                let ground_reflection_flag = if fields.is_empty() {
                    0
                } else {
                    parse_i32(lineno, "GE", 1, &fields[0])?
                };
                deck.cards.push(Card::Ge(GeCard {
                    ground_reflection_flag,
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
            "GN" => {
                let fields = parse_fields(rest);
                require_fields(lineno, "GN", &fields, 1)?;
                deck.cards.push(Card::Gn(GnCard {
                    ground_type: parse_i32(lineno, "GN", 1, &fields[0])?,
                }));
            }
            "LD" => {
                // LD I1 I2 I3 I4 F1 F2 F3
                // I1: load type; I2: tag; I3: first seg; I4: last seg
                // F1..F3: load parameters (type-dependent)
                let fields = parse_fields(rest);
                require_fields(lineno, "LD", &fields, 4)?;
                let f1 = if fields.len() > 4 {
                    parse_f64(lineno, "LD", 5, &fields[4])?
                } else {
                    0.0
                };
                let f2 = if fields.len() > 5 {
                    parse_f64(lineno, "LD", 6, &fields[5])?
                } else {
                    0.0
                };
                let f3 = if fields.len() > 6 {
                    parse_f64(lineno, "LD", 7, &fields[6])?
                } else {
                    0.0
                };
                deck.cards.push(Card::Ld(LdCard {
                    load_type: parse_i32(lineno, "LD", 1, &fields[0])?,
                    tag: parse_u32(lineno, "LD", 2, &fields[1])?,
                    seg_first: parse_u32(lineno, "LD", 3, &fields[2])?,
                    seg_last: parse_u32(lineno, "LD", 4, &fields[3])?,
                    f1,
                    f2,
                    f3,
                }));
            }
            "TL" => {
                // TL I1 I2 I3 I4 I5 I6 F1 F2 F3
                // I1: tag of first segment
                // I2: segment number of first segment (0 = all)
                // I3: tag of second segment
                // I4: segment number of second segment (0 = all)
                // I5: number of transmission-line segments
                // I6: transmission-line type (0 = lossless, non-zero = lossy)
                // F1: characteristic impedance (Ω)
                // F2: transmission-line length (m)
                // F3: angle (°) or velocity factor
                let fields = parse_fields(rest);
                require_fields(lineno, "TL", &fields, 6)?;
                let z0 = if fields.len() > 6 {
                    parse_f64(lineno, "TL", 7, &fields[6])?
                } else {
                    50.0 // Default 50 Ω if omitted
                };
                let length = if fields.len() > 7 {
                    parse_f64(lineno, "TL", 8, &fields[7])?
                } else {
                    0.0
                };
                let f3 = if fields.len() > 8 {
                    parse_f64(lineno, "TL", 9, &fields[8])?
                } else {
                    1.0 // Default velocity factor 1.0 (lossless) if omitted
                };
                deck.cards.push(Card::Tl(TlCard {
                    tag1: parse_u32(lineno, "TL", 1, &fields[0])?,
                    segment1: parse_u32(lineno, "TL", 2, &fields[1])?,
                    tag2: parse_u32(lineno, "TL", 3, &fields[2])?,
                    segment2: parse_u32(lineno, "TL", 4, &fields[3])?,
                    num_segments: parse_u32(lineno, "TL", 5, &fields[4])?,
                    tl_type: parse_u32(lineno, "TL", 6, &fields[5])?,
                    z0,
                    length,
                    f3,
                }));
            }
            "PT" => {
                // PT cards are preserved for staged portability.
                // Runtime semantics are currently deferred and handled by CLI warnings.
                let fields = parse_fields(rest)
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect();
                deck.cards.push(Card::Pt(PtCard { raw_fields: fields }));
            }
            "NT" => {
                // NT cards are preserved for staged portability.
                // Runtime semantics are currently deferred and handled by CLI warnings.
                let fields = parse_fields(rest)
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect();
                deck.cards.push(Card::Nt(NtCard { raw_fields: fields }));
            }
            "EN" => {
                deck.cards.push(Card::En(EnCard));
                // Stop at EN per NEC spec.
                break;
            }
            "GM" => {
                // GM I1 I2 F1 F2 F3 F4 F5 F6 [F7]
                // I1: tag increment, I2: last tag (0=all), F1-F3: rot x/y/z deg,
                // F4-F6: translate x/y/z m, F7: first tag (0=all, optional)
                let fields = parse_fields(rest);
                require_fields(lineno, "GM", &fields, 2)?;
                let rot_x = if fields.len() > 2 {
                    parse_f64(lineno, "GM", 3, &fields[2])?
                } else {
                    0.0
                };
                let rot_y = if fields.len() > 3 {
                    parse_f64(lineno, "GM", 4, &fields[3])?
                } else {
                    0.0
                };
                let rot_z = if fields.len() > 4 {
                    parse_f64(lineno, "GM", 5, &fields[4])?
                } else {
                    0.0
                };
                let tx = if fields.len() > 5 {
                    parse_f64(lineno, "GM", 6, &fields[5])?
                } else {
                    0.0
                };
                let ty = if fields.len() > 6 {
                    parse_f64(lineno, "GM", 7, &fields[6])?
                } else {
                    0.0
                };
                let tz = if fields.len() > 7 {
                    parse_f64(lineno, "GM", 8, &fields[7])?
                } else {
                    0.0
                };
                let first_tag = if fields.len() > 8 {
                    parse_u32(lineno, "GM", 9, &fields[8])?
                } else {
                    0
                };
                deck.cards.push(Card::Gm(GmCard {
                    tag_increment: parse_u32(lineno, "GM", 1, &fields[0])?,
                    last_tag: parse_u32(lineno, "GM", 2, &fields[1])?,
                    rot_x_deg: rot_x,
                    rot_y_deg: rot_y,
                    rot_z_deg: rot_z,
                    translate_x: tx,
                    translate_y: ty,
                    translate_z: tz,
                    first_tag,
                }));
            }
            "GR" => {
                // GR I1 I2 [F1]
                // I1: tag increment, I2: repeat count, F1: angle per copy (deg about z)
                let fields = parse_fields(rest);
                require_fields(lineno, "GR", &fields, 2)?;
                let angle = if fields.len() > 2 {
                    parse_f64(lineno, "GR", 3, &fields[2])?
                } else {
                    0.0
                };
                deck.cards.push(Card::Gr(GrCard {
                    tag_increment: parse_u32(lineno, "GR", 1, &fields[0])?,
                    count: parse_u32(lineno, "GR", 2, &fields[1])?,
                    angle_deg: angle,
                }));
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

fn parse_i32(lineno: usize, card: &str, field: usize, s: &str) -> Result<i32, ParseError> {
    // Accept floats like "-1.0" that some NEC tools emit for integer fields.
    s.parse::<f64>()
        .map(|v| v as i32)
        .map_err(|_| ParseError::BadField {
            line: lineno,
            card: card.to_string(),
            field,
            raw: s.to_string(),
        })
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
    use nec_model::card::{Card, CommentCard, EnCard, ExCard, FrCard, GeCard, GwCard, RpCard};

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
    fn gn_card_parsed_and_stored() {
        let input = "GW 1 3 0 0 1 0 0 4 0.001\nGN 1\nEX 0 1 2 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        let gn_card = result.deck.cards.iter().find_map(|c| {
            if let Card::Gn(g) = c {
                Some(g.clone())
            } else {
                None
            }
        });
        assert!(gn_card.is_some(), "GN card not found in deck");
        assert_eq!(gn_card.unwrap().ground_type, 1);
    }

    #[test]
    fn ge_card_with_reflection_flag_is_preserved() {
        let input = "GW 1 3 0 0 1 0 0 4 0.001\nGE 1\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        let ge_card = result.deck.cards.iter().find_map(|c| {
            if let Card::Ge(g) = c {
                Some(g.clone())
            } else {
                None
            }
        });
        assert_eq!(
            ge_card,
            Some(GeCard {
                ground_reflection_flag: 1
            })
        );
    }

    #[test]
    fn ge_card_without_flag_defaults_to_zero() {
        let input = "GW 1 3 0 0 1 0 0 4 0.001\nGE\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        let ge_card = result.deck.cards.iter().find_map(|c| {
            if let Card::Ge(g) = c {
                Some(g.clone())
            } else {
                None
            }
        });
        assert_eq!(
            ge_card,
            Some(GeCard {
                ground_reflection_flag: 0
            })
        );
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

    #[test]
    fn gm_card_is_parsed() {
        // GM I1 I2 F1 F2 F3 F4 F5 F6 F7
        let input = "GW 1 3 0 0 0 1 0 0 0.001\nGM 2 5 30.0 45.0 90.0 0.5 1.0 2.0 1\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        let gm = result.deck.cards.iter().find_map(|c| {
            if let Card::Gm(g) = c {
                Some(g.clone())
            } else {
                None
            }
        });
        assert!(gm.is_some(), "GM card not found");
        let gm = gm.unwrap();
        assert_eq!(gm.tag_increment, 2);
        assert_eq!(gm.last_tag, 5);
        assert!((gm.rot_x_deg - 30.0).abs() < 1e-10);
        assert!((gm.rot_y_deg - 45.0).abs() < 1e-10);
        assert!((gm.rot_z_deg - 90.0).abs() < 1e-10);
        assert!((gm.translate_x - 0.5).abs() < 1e-10);
        assert!((gm.translate_y - 1.0).abs() < 1e-10);
        assert!((gm.translate_z - 2.0).abs() < 1e-10);
        assert_eq!(gm.first_tag, 1);
    }

    #[test]
    fn gm_card_minimal_fields_default_to_zero() {
        // Only I1 and I2 required; all floats default to 0
        let input = "GM 0 0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        let gm = result.deck.cards.iter().find_map(|c| {
            if let Card::Gm(g) = c {
                Some(g.clone())
            } else {
                None
            }
        });
        let gm = gm.unwrap();
        assert_eq!(gm.tag_increment, 0);
        assert!((gm.rot_z_deg).abs() < 1e-10);
        assert!((gm.translate_z).abs() < 1e-10);
        assert_eq!(gm.first_tag, 0);
    }

    #[test]
    fn gr_card_is_parsed() {
        let input = "GR 1 3 120.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        let gr = result.deck.cards.iter().find_map(|c| {
            if let Card::Gr(g) = c {
                Some(g.clone())
            } else {
                None
            }
        });
        assert!(gr.is_some(), "GR card not found");
        let gr = gr.unwrap();
        assert_eq!(gr.tag_increment, 1);
        assert_eq!(gr.count, 3);
        assert!((gr.angle_deg - 120.0).abs() < 1e-10);
    }

    #[test]
    fn pt_card_is_parsed_without_unknown_warning() {
        let input = "PT 0 1 26 0 50.0 0.1 1.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        assert!(matches!(
            result.deck.cards.first(),
            Some(Card::Pt(pt))
                if pt.raw_fields
                    == vec!["0", "1", "26", "0", "50.0", "0.1", "1.0"]
        ));
    }

    #[test]
    fn nt_card_is_parsed_without_unknown_warning() {
        let input = "NT 1 1 26 1 1 26 50.0 0.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());
        assert!(matches!(
            result.deck.cards.first(),
            Some(Card::Nt(nt))
                if nt.raw_fields
                    == vec!["1", "1", "26", "1", "1", "26", "50.0", "0.0"]
        ));
    }

    #[test]
    fn repeated_pt_and_nt_cards_preserve_order_and_raw_fields() {
        let input = "PT 0 1 26 0 50.0 0.1 1.0\nPT 0 1 26 0 75.0 0.2 1.0\nNT 1 1 26 1 1 26 50.0 0.0\nNT 1 1 26 1 1 26 75.0 0.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());

        let pt_cards: Vec<_> = result
            .deck
            .cards
            .iter()
            .filter_map(|card| {
                if let Card::Pt(pt) = card {
                    Some(pt.raw_fields.clone())
                } else {
                    None
                }
            })
            .collect();
        let nt_cards: Vec<_> = result
            .deck
            .cards
            .iter()
            .filter_map(|card| {
                if let Card::Nt(nt) = card {
                    Some(nt.raw_fields.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            pt_cards,
            vec![
                vec!["0", "1", "26", "0", "50.0", "0.1", "1.0"],
                vec!["0", "1", "26", "0", "75.0", "0.2", "1.0"],
            ]
        );
        assert_eq!(
            nt_cards,
            vec![
                vec!["1", "1", "26", "1", "1", "26", "50.0", "0.0"],
                vec!["1", "1", "26", "1", "1", "26", "75.0", "0.0"],
            ]
        );
    }

    #[test]
    fn repeated_nt_and_pt_cards_preserve_order_and_raw_fields() {
        let input = "NT 1 1 26 1 1 26 50.0 0.0\nNT 1 1 26 1 1 26 75.0 0.0\nPT 0 1 26 0 50.0 0.1 1.0\nPT 0 1 26 0 75.0 0.2 1.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());

        let nt_cards: Vec<_> = result
            .deck
            .cards
            .iter()
            .filter_map(|card| {
                if let Card::Nt(nt) = card {
                    Some(nt.raw_fields.clone())
                } else {
                    None
                }
            })
            .collect();
        let pt_cards: Vec<_> = result
            .deck
            .cards
            .iter()
            .filter_map(|card| {
                if let Card::Pt(pt) = card {
                    Some(pt.raw_fields.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            nt_cards,
            vec![
                vec!["1", "1", "26", "1", "1", "26", "50.0", "0.0"],
                vec!["1", "1", "26", "1", "1", "26", "75.0", "0.0"],
            ]
        );
        assert_eq!(
            pt_cards,
            vec![
                vec!["0", "1", "26", "0", "50.0", "0.1", "1.0"],
                vec!["0", "1", "26", "0", "75.0", "0.2", "1.0"],
            ]
        );
    }
}
