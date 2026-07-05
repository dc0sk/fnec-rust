// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! NEC deck parser.
//!
//! Parses geometry, program-control, and loading cards:
//! `CM`, `CE`, `GW`, `GE`, `GM`, `GR`, `GN`, `EX`, `FR`, `RP`, `NE`, `NH`,
//! `LD`, `TL`, `NT`, `PT`, `EN`.
//!
//! Unknown cards produce a [`ParseError::UnknownCard`] but do not stop
//! parsing — callers decide whether to treat them as fatal.

pub mod template;

use nec_model::card::{
    Card, CommentCard, EnCard, ExCard, FrCard, GeCard, GmCard, GnCard, GrCard, GwCard, LdCard,
    NeCard, NtCard, PtCard, RpCard, TlCard,
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
                    tag: parse_u32(lineno, "GW", 1, fields[0])?,
                    segments: parse_u32(lineno, "GW", 2, fields[1])?,
                    start: [
                        parse_f64(lineno, "GW", 3, fields[2])?,
                        parse_f64(lineno, "GW", 4, fields[3])?,
                        parse_f64(lineno, "GW", 5, fields[4])?,
                    ],
                    end: [
                        parse_f64(lineno, "GW", 6, fields[5])?,
                        parse_f64(lineno, "GW", 7, fields[6])?,
                        parse_f64(lineno, "GW", 8, fields[7])?,
                    ],
                    radius: parse_f64(lineno, "GW", 9, fields[8])?,
                }));
            }
            "GE" => {
                let fields = parse_fields(rest);
                let ground_reflection_flag = if fields.is_empty() {
                    0
                } else {
                    parse_i32(lineno, "GE", 1, fields[0])?
                };
                deck.cards.push(Card::Ge(GeCard {
                    ground_reflection_flag,
                }));
            }
            "GM" => {
                // GM I1 I2 F1 F2 F3 F4 F5 F6 F7
                let fields = parse_fields(rest);
                require_fields(lineno, "GM", &fields, 9)?;
                deck.cards.push(Card::Gm(GmCard {
                    tag_increment: parse_u32(lineno, "GM", 1, fields[0])?,
                    last_tag: parse_u32(lineno, "GM", 2, fields[1])?,
                    rot_x_deg: parse_f64(lineno, "GM", 3, fields[2])?,
                    rot_y_deg: parse_f64(lineno, "GM", 4, fields[3])?,
                    rot_z_deg: parse_f64(lineno, "GM", 5, fields[4])?,
                    translate_x: parse_f64(lineno, "GM", 6, fields[5])?,
                    translate_y: parse_f64(lineno, "GM", 7, fields[6])?,
                    translate_z: parse_f64(lineno, "GM", 8, fields[7])?,
                    first_tag: parse_u32(lineno, "GM", 9, fields[8])?,
                }));
            }
            "GR" => {
                // GR I1 I2 F1
                let fields = parse_fields(rest);
                require_fields(lineno, "GR", &fields, 3)?;
                deck.cards.push(Card::Gr(GrCard {
                    tag_increment: parse_u32(lineno, "GR", 1, fields[0])?,
                    count: parse_u32(lineno, "GR", 2, fields[1])?,
                    angle_deg: parse_f64(lineno, "GR", 3, fields[2])?,
                }));
            }
            "GN" => {
                let fields = parse_fields(rest);
                require_fields(lineno, "GN", &fields, 1)?;
                // GN I1 NRADL I3 I4 EPSE SIG
                // We currently only consume I1 and optional EPSE/SIG.
                let eps_r = if fields.len() > 4 {
                    Some(parse_f64(lineno, "GN", 5, fields[4])?)
                } else {
                    None
                };
                let sigma = if fields.len() > 5 {
                    Some(parse_f64(lineno, "GN", 6, fields[5])?)
                } else {
                    None
                };

                deck.cards.push(Card::Gn(GnCard {
                    ground_type: parse_i32(lineno, "GN", 1, fields[0])?,
                    eps_r,
                    sigma,
                }));
            }
            "EX" => {
                let fields = parse_fields(rest);
                // Standard NEC EX card: I1 I2 I3 I4 F1 F2 F3
                // I4 is required by the spec; F1/F2 default to 0.0 if absent.
                // F3 carries the plane-wave polarization angle η (types 1/2/3);
                // it is unused for source types and defaults to 0.0.
                require_fields(lineno, "EX", &fields, 4)?;
                let vr = if fields.len() > 4 {
                    parse_f64(lineno, "EX", 5, fields[4])?
                } else {
                    0.0
                };
                let vi = if fields.len() > 5 {
                    parse_f64(lineno, "EX", 6, fields[5])?
                } else {
                    0.0
                };
                let f3 = if fields.len() > 6 {
                    parse_f64(lineno, "EX", 7, fields[6])?
                } else {
                    0.0
                };
                // F4/F5 (fields 7/8) are the plane-wave incidence-angle-sweep
                // increments Δθ/Δφ; F6 (field 9) is the plane-wave axial ratio.
                let f4 = if fields.len() > 7 {
                    parse_f64(lineno, "EX", 8, fields[7])?
                } else {
                    0.0
                };
                let f5 = if fields.len() > 8 {
                    parse_f64(lineno, "EX", 9, fields[8])?
                } else {
                    0.0
                };
                let f6 = if fields.len() > 9 {
                    parse_f64(lineno, "EX", 10, fields[9])?
                } else {
                    0.0
                };
                deck.cards.push(Card::Ex(ExCard {
                    excitation_type: parse_u32(lineno, "EX", 1, fields[0])?,
                    tag: parse_u32(lineno, "EX", 2, fields[1])?,
                    segment: parse_u32(lineno, "EX", 3, fields[2])?,
                    i4: parse_u32(lineno, "EX", 4, fields[3])?,
                    voltage_real: vr,
                    voltage_imag: vi,
                    polarization_deg: f3,
                    polarization_ratio: f6,
                    theta_inc: f4,
                    phi_inc: f5,
                }));
            }
            m @ ("NE" | "NH") => {
                // N* I1 NX NY NZ X0 Y0 Z0 DX DY DZ  (rectangular near-field grid);
                // NE = electric field, NH = magnetic field (identical field layout).
                let fields = parse_fields(rest);
                require_fields(lineno, m, &fields, 4)?;
                let f = |i: usize| -> Result<f64, ParseError> {
                    if fields.len() > i {
                        parse_f64(lineno, m, i + 1, fields[i])
                    } else {
                        Ok(0.0)
                    }
                };
                let card = NeCard {
                    coord_type: parse_u32(lineno, m, 1, fields[0])?,
                    nx: parse_u32(lineno, m, 2, fields[1])?,
                    ny: parse_u32(lineno, m, 3, fields[2])?,
                    nz: parse_u32(lineno, m, 4, fields[3])?,
                    x0: f(4)?,
                    y0: f(5)?,
                    z0: f(6)?,
                    dx: f(7)?,
                    dy: f(8)?,
                    dz: f(9)?,
                };
                deck.cards.push(if m == "NE" {
                    Card::Ne(card)
                } else {
                    Card::Nh(card)
                });
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
                    step_type: parse_u32(lineno, "FR", 1, fields[0])?,
                    steps: parse_u32(lineno, "FR", 2, fields[1])?,
                    frequency_mhz: parse_f64(lineno, "FR", freq_idx + 1, fields[freq_idx])?,
                    step_mhz: parse_f64(lineno, "FR", step_idx + 1, fields[step_idx])?,
                }));
            }
            "RP" => {
                let fields = parse_fields(rest);
                require_fields(lineno, "RP", &fields, 7)?;
                // Canonical NEC RP card: RP mode N1 N2 XNDA θ0 φ0 Δθ Δφ (8 fields,
                // XNDA = output/normalization options at I4). fnec historically
                // also accepts a 7-field form without XNDA. Distinguish by count:
                // with >= 8 fields the four angle floats start at index 4 (after
                // XNDA); with 7 they start at index 3. The XNDA `X` digit (its
                // thousands digit) requests normalized-gain output.
                let a = if fields.len() >= 8 { 4 } else { 3 };
                let normalize = if fields.len() >= 8 {
                    let xnda = parse_u32(lineno, "RP", 4, fields[3])?;
                    (xnda / 1000) % 10 != 0
                } else {
                    false
                };
                deck.cards.push(Card::Rp(RpCard {
                    mode: parse_u32(lineno, "RP", 1, fields[0])?,
                    n_theta: parse_u32(lineno, "RP", 2, fields[1])?,
                    n_phi: parse_u32(lineno, "RP", 3, fields[2])?,
                    theta0: parse_f64(lineno, "RP", a + 1, fields[a])?,
                    phi0: parse_f64(lineno, "RP", a + 2, fields[a + 1])?,
                    d_theta: parse_f64(lineno, "RP", a + 3, fields[a + 2])?,
                    d_phi: parse_f64(lineno, "RP", a + 4, fields[a + 3])?,
                    normalize,
                }));
            }
            "LD" => {
                // LD I1 I2 I3 I4 F1 F2 F3
                // I1=load_type, I2=tag, I3=seg_first, I4=seg_last
                // F1–F3 default to 0.0 when absent.
                let fields = parse_fields(rest);
                require_fields(lineno, "LD", &fields, 4)?;
                let f1 = if fields.len() > 4 {
                    parse_f64(lineno, "LD", 5, fields[4])?
                } else {
                    0.0
                };
                let f2 = if fields.len() > 5 {
                    parse_f64(lineno, "LD", 6, fields[5])?
                } else {
                    0.0
                };
                let f3 = if fields.len() > 6 {
                    parse_f64(lineno, "LD", 7, fields[6])?
                } else {
                    0.0
                };
                deck.cards.push(Card::Ld(LdCard {
                    load_type: parse_i32(lineno, "LD", 1, fields[0])?,
                    tag: parse_u32(lineno, "LD", 2, fields[1])?,
                    seg_first: parse_u32(lineno, "LD", 3, fields[2])?,
                    seg_last: parse_u32(lineno, "LD", 4, fields[3])?,
                    f1,
                    f2,
                    f3,
                }));
            }
            "TL" => {
                // TL I1 I2 I3 I4 I5 I6 F1 F2 [F3]
                // I1=tag1, I2=seg1, I3=tag2, I4=seg2, I5=num_segs, I6=tl_type
                // F1=z0, F2=length, F3=velocity_factor (default 1.0)
                let fields = parse_fields(rest);
                require_fields(lineno, "TL", &fields, 8)?;
                let f3 = if fields.len() > 8 {
                    parse_f64(lineno, "TL", 9, fields[8])?
                } else {
                    1.0
                };
                deck.cards.push(Card::Tl(TlCard {
                    tag1: parse_u32(lineno, "TL", 1, fields[0])?,
                    segment1: parse_u32(lineno, "TL", 2, fields[1])?,
                    tag2: parse_u32(lineno, "TL", 3, fields[2])?,
                    segment2: parse_u32(lineno, "TL", 4, fields[3])?,
                    num_segments: parse_u32(lineno, "TL", 5, fields[4])?,
                    tl_type: parse_u32(lineno, "TL", 6, fields[5])?,
                    z0: parse_f64(lineno, "TL", 7, fields[6])?,
                    length: parse_f64(lineno, "TL", 8, fields[7])?,
                    f3,
                }));
            }
            "NT" => {
                // NT — network definition card.
                // Semantics are not yet implemented in the solver; the card is
                // parsed and stored for explicit deferred-support warnings at
                // solve time (replaces the previous "unknown card 'NT'" path).
                let fields = parse_fields(rest);
                deck.cards.push(Card::Nt(NtCard {
                    raw_fields: fields
                        .into_iter()
                        .map(std::string::ToString::to_string)
                        .collect(),
                }));
            }
            "PT" => {
                // PT — transmission-line source card.
                // Semantics are not yet implemented in the solver; the card is
                // parsed and stored for explicit deferred-support warnings at
                // solve time (replaces the previous "unknown card 'PT'" path).
                let fields = parse_fields(rest);
                deck.cards.push(Card::Pt(PtCard {
                    raw_fields: fields
                        .into_iter()
                        .map(std::string::ToString::to_string)
                        .collect(),
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

fn parse_i32(lineno: usize, card: &str, field: usize, s: &str) -> Result<i32, ParseError> {
    s.parse::<f64>()
        .map(|v| v as i32)
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
    use nec_model::card::{
        Card, CommentCard, EnCard, ExCard, FrCard, GeCard, GmCard, GnCard, GrCard, GwCard, RpCard,
    };

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
        assert_eq!(
            cards[1],
            Card::Comment(CommentCard {
                text: String::new()
            })
        );
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
                polarization_deg: 0.0,
                polarization_ratio: 0.0,
                theta_inc: 0.0,
                phi_inc: 0.0,
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
                normalize: false,
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
                polarization_deg: 0.0,
                polarization_ratio: 0.0,
                theta_inc: 0.0,
                phi_inc: 0.0,
            })
        );
    }

    #[test]
    fn rp_card_accepts_both_7_field_and_8_field_xnda_forms() {
        // Canonical NEC RP has 8 fields (with XNDA at I4); fnec also accepts a
        // 7-field form. Both must parse the SAME angle grid (XNDA only affects
        // output options, here the normalize flag).
        let seven = parse("RP 0 19 1 30.0 0.0 5.0 0.0\nEN\n").expect("parse 7-field");
        let eight = parse("RP 0 19 1 1000 30.0 0.0 5.0 0.0\nEN\n").expect("parse 8-field");
        let (Card::Rp(r7), Card::Rp(r8)) = (&seven.deck.cards[0], &eight.deck.cards[0]) else {
            panic!("expected RP cards");
        };
        // Angles identical; the XNDA=1000 value must NOT leak into θ0.
        assert_eq!(r8.theta0, 30.0, "8-field θ0 must be 30, not XNDA=1000");
        assert_eq!(
            (r7.mode, r7.n_theta, r7.n_phi, r7.theta0, r7.phi0, r7.d_theta, r7.d_phi),
            (r8.mode, r8.n_theta, r8.n_phi, r8.theta0, r8.phi0, r8.d_theta, r8.d_phi),
            "7-field and 8-field RP must parse identical angles"
        );
        // XNDA=1000 → X digit 1 → normalized output requested.
        assert!(r8.normalize, "XNDA X-digit 1 must request normalization");
        assert!(!r7.normalize, "7-field RP has no XNDA, so no normalization");
    }

    #[test]
    fn ex_plane_wave_polarization_f3_is_captured() {
        // NEC2 plane wave: EX 1 NTHETA NPHI 0 THETA PHI ETA.
        // Here NTHETA=1, NPHI=1, THETA=30, PHI=0, ETA(polarization)=45.
        let input = "EX 1 1 1 0 30.0 0.0 45.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert_eq!(
            result.deck.cards[0],
            Card::Ex(ExCard {
                excitation_type: 1,
                tag: 1,
                segment: 1,
                i4: 0,
                voltage_real: 30.0,
                voltage_imag: 0.0,
                polarization_deg: 45.0,
                polarization_ratio: 0.0,
                theta_inc: 0.0,
                phi_inc: 0.0,
            })
        );
        // The F3 field only populates polarization; no F3 → 0.0.
        let no_f3 = parse("EX 0 1 26 0 1.0 0.0\nEN\n").expect("parse");
        let Card::Ex(ex) = &no_f3.deck.cards[0] else {
            panic!("expected EX card");
        };
        assert_eq!(ex.polarization_deg, 0.0);
    }

    #[test]
    fn parse_gn_type_and_optional_medium_params() {
        let input = "GW 1 3 0 0 1 0 0 4 0.001\nGN 2 0 0 0 13.0 0.005\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());

        let gn =
            result.deck.cards.iter().find_map(
                |c| {
                    if let Card::Gn(gn) = c {
                        Some(gn)
                    } else {
                        None
                    }
                },
            );
        assert_eq!(
            gn,
            Some(&GnCard {
                ground_type: 2,
                eps_r: Some(13.0),
                sigma: Some(0.005),
            })
        );
    }

    #[test]
    fn parse_ge_defaults_flag_to_zero_when_omitted() {
        let input = "GW 1 3 0 0 1 0 0 4 0.001\nGE\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());

        let ge =
            result.deck.cards.iter().find_map(
                |c| {
                    if let Card::Ge(ge) = c {
                        Some(ge)
                    } else {
                        None
                    }
                },
            );
        assert_eq!(
            ge,
            Some(&GeCard {
                ground_reflection_flag: 0
            })
        );
    }

    #[test]
    fn parser_recognises_gm_card() {
        let input = "GW 1 3 0 0 1 0 0 4 0.001\nGM 0 1 0 0 0 1.0 0 0 1\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());

        let gm =
            result.deck.cards.iter().find_map(
                |c| {
                    if let Card::Gm(gm) = c {
                        Some(gm)
                    } else {
                        None
                    }
                },
            );
        assert_eq!(
            gm,
            Some(&GmCard {
                tag_increment: 0,
                last_tag: 1,
                rot_x_deg: 0.0,
                rot_y_deg: 0.0,
                rot_z_deg: 0.0,
                translate_x: 1.0,
                translate_y: 0.0,
                translate_z: 0.0,
                first_tag: 1,
            })
        );
    }

    #[test]
    fn parser_recognises_gr_card() {
        let input = "GW 1 3 0.5 0 1 0.5 0 4 0.001\nGR 1 1 180.0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());

        let gr =
            result.deck.cards.iter().find_map(
                |c| {
                    if let Card::Gr(gr) = c {
                        Some(gr)
                    } else {
                        None
                    }
                },
            );
        assert_eq!(
            gr,
            Some(&GrCard {
                tag_increment: 1,
                count: 1,
                angle_deg: 180.0,
            })
        );
    }

    #[test]
    fn parser_recognises_pt_card() {
        let input = "GW 1 3 0 0 1 0 0 4 0.001\nPT -1 0 0 0 0 0 0 0 0 0 0 0 0\nEN\n";
        let result = parse(input).expect("parse must succeed");
        assert!(result.warnings.is_empty());

        let pt =
            result.deck.cards.iter().find_map(
                |c| {
                    if let Card::Pt(pt) = c {
                        Some(pt)
                    } else {
                        None
                    }
                },
            );
        assert!(pt.is_some());
        assert_eq!(pt.unwrap().raw_fields.len(), 13);
    }

    #[test]
    fn gm_too_few_fields_is_error() {
        let input = "GM 0 1 0 0\nEN\n";
        assert!(parse(input).is_err());
    }

    #[test]
    fn gr_too_few_fields_is_error() {
        let input = "GR 1 1\nEN\n";
        assert!(parse(input).is_err());
    }
}
