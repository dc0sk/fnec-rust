// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! NEC deck parser.
//!
//! Parses geometry, program-control, and loading cards:
//! `CM`, `CE`, `GW`, `GE`, `GM`, `GR`, `GN`, `EX`, `FR`, `RP`, `NE`, `NH`,
//! `LD`, `TL`, `NT`, `PT`, `EN`.
//!
//! Every other card is in [`OTHER_CARDS`]: one fnec ignores with a warning,
//! because it cannot change the solved problem, or one it refuses, because it
//! would — and a mnemonic in neither list is refused too (FND-236). It used to be
//! skipped with a warning, and the deck solved as a different antenna: the
//! feet-unit dipole of `gs_card_nec2c.rs` with its `GS 0 0 .3048` ignored is a
//! wire 3.28× too long, 227.14 + j451.81 Ω in nec2c, where nec2c scaling it gives
//! 79.042 + j45.909.

pub mod template;

use nec_model::card::{
    Card, CommentCard, EnCard, ExCard, FrCard, GaCard, GcCard, GeCard, GhCard, GmCard, GnCard,
    GrCard, GsCard, GwCard, GxCard, LdCard, NearFieldCard, NtCard, PtCard, RpCard, TlCard,
};
use nec_model::deck::{NecDeck, ProgramCard};

/// A parse error.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// A card mnemonic in neither the parsed cards nor [`OTHER_CARDS`]: fatal,
    /// because fnec cannot tell whether skipping it changes the antenna.
    UnknownCard { line: usize, mnemonic: String },
    /// A known NEC card fnec does not model and that would change the solved
    /// problem — `what` says what it would change. Fatal.
    RefusedCard {
        line: usize,
        mnemonic: String,
        what: &'static str,
    },
    /// A known NEC card that cannot change the solved problem, skipped. A warning.
    IgnoredCard {
        line: usize,
        mnemonic: String,
        why: &'static str,
    },
    /// A field of a parsed card that fnec reads differently from NEC-2 in a way
    /// that cannot change the solved problem (a print format). A warning.
    IgnoredField {
        line: usize,
        card: String,
        field: usize,
        why: String,
    },
    /// A field could not be converted to the expected numeric type.
    BadField {
        line: usize,
        card: String,
        field: usize,
        raw: String,
    },
    /// A field parsed cleanly but names something fnec will not represent.
    ///
    /// Distinct from [`ParseError::BadField`] because the remedy differs: the
    /// deck is not malformed, it asks for something out of scope, and saying
    /// "cannot parse '-10'" about a perfectly well-formed -10 sends the reader
    /// hunting for a typo that is not there.
    UnsupportedValue {
        line: usize,
        card: String,
        field: usize,
        raw: String,
        reason: String,
    },
    /// A geometry card after `GE`, which ends the geometry (FND-245).
    GeometryAfterGe {
        line: usize,
        mnemonic: String,
        ge_line: usize,
    },
    /// A `GW` of radius 0 without its `GC`, or a `GC` without its `GW`: nec2c's
    /// "GEOMETRY DATA CARD ERROR". The rule is `nec_model`'s, one for every
    /// builder of a deck.
    TaperPairing {
        line: usize,
        error: nec_model::card::TaperPairingError,
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
            ParseError::UnknownCard { line, mnemonic } => write!(
                f,
                "line {line}: unknown card '{mnemonic}'; fnec cannot tell whether skipping it \
                 changes the antenna, so the deck is refused"
            ),
            ParseError::RefusedCard {
                line,
                mnemonic,
                what,
            } => write!(
                f,
                "line {line}: fnec does not model {mnemonic} ({what}); skipping it would solve \
                 a different antenna, so the deck is refused"
            ),
            ParseError::IgnoredCard {
                line,
                mnemonic,
                why,
            } => write!(f, "line {line}: {mnemonic} ignored: {why}"),
            ParseError::IgnoredField {
                line,
                card,
                field,
                why,
            } => write!(f, "line {line}: {card} field {field} ignored: {why}"),
            ParseError::BadField {
                line,
                card,
                field,
                raw,
            } => {
                write!(f, "line {line}: {card} field {field}: cannot parse '{raw}'")
            }
            ParseError::UnsupportedValue {
                line,
                card,
                field,
                raw,
                reason,
            } => {
                write!(f, "line {line}: {card} field {field} = '{raw}': {reason}")
            }
            ParseError::GeometryAfterGe {
                line,
                mnemonic,
                ge_line,
            } => write!(
                f,
                "line {line}: {mnemonic} after GE (line {ge_line}): GE ends the geometry, \
                 and nec2c refuses a geometry card after it; move it above the GE card"
            ),
            ParseError::TaperPairing { line, error } => write!(f, "line {line}: {error}"),
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

/// How fnec treats a NEC card it does not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherCard {
    /// Cannot change the solved problem: skipped with a warning saying why.
    Ignored(&'static str),
    /// Would change the solved problem: the deck is refused, saying what.
    Refused(&'static str),
}

/// Every NEC card fnec knows of but does not parse, and what it does with it
/// (FND-236). `docs/card-support-matrix.md` lists the same cards, checked by a
/// test; a mnemonic in neither this list nor the parser's is refused as unknown.
pub const OTHER_CARDS: &[(&str, OtherCard)] = &[
    (
        "XQ",
        OtherCard::Ignored(
            "fnec always executes the deck, once, at its last FR card (an earlier FR \
             is reported as replaced, FND-057); a pattern XQ asks for (I1 != 0) is not \
             produced -- use an RP card",
        ),
    ),
    (
        "EK",
        OtherCard::Ignored(
            "NEC's extended thin-wire kernel; fnec's kernel does not change, and NEC's \
             thin-wire limit with EK (segment/radius > 2) is not fnec's",
        ),
    ),
    (
        "KH",
        OtherCard::Ignored(
            "NEC's interaction-approximation range; fnec computes every interaction exactly",
        ),
    ),
    (
        "CP",
        OtherCard::Ignored("NEC's maximum-coupling calculation; its output is not produced"),
    ),
    (
        "PQ",
        OtherCard::Ignored("charge-density print control; fnec prints no charge densities"),
    ),
    (
        "PL",
        OtherCard::Ignored("xnec2c's plot-file output; no plot file is written"),
    ),
    (
        "WG",
        OtherCard::Ignored("writes a Numerical Green's Function file; none is written"),
    ),
    (
        "GF",
        OtherCard::Refused("a structure read from a Numerical Green's Function file"),
    ),
    (
        "GD",
        OtherCard::Ignored(
            "additional ground parameters (a second medium or a cliff), which only enter \
             nec2c's RP modes 2, 3, 5 and 6, which fnec refuses; the pattern is computed over \
             the first medium, as nec2c's RP 0 computes it",
        ),
    ),
    ("SP", OtherCard::Refused("a surface patch")),
    ("SM", OtherCard::Refused("a set of surface patches")),
    ("SC", OtherCard::Refused("a surface-patch continuation")),
    (
        "NX",
        OtherCard::Refused("a further structure in the same file"),
    ),
    (
        "SY",
        OtherCard::Refused(
            "symbol definitions; write the deck with $NAME templates and pass the values \
             with --vars",
        ),
    ),
];

/// Result of parsing a NEC deck.
///
/// Non-fatal warnings (cards that cannot change the solved problem) are
/// collected in `warnings` while parsing continues. Fatal errors — a bad field,
/// or a card fnec does not model that would change the antenna — are returned
/// early via `Err`.
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
    // The GE card's line and flag, and the last GN's type: nec2c takes the ground
    // from the last GN alone, and GE 1 with none is refused below (FND-241).
    let mut ge_one_line: Option<usize> = None;
    // FND-245: the GE card's line. nec2c refuses a geometry card after GE
    // ("FAULTY DATA CARD LABEL AFTER GEOMETRY SECTION"); fnec solved it, and a GS
    // appended after GE would have scaled the whole structure without a word.
    let mut ge_line: Option<usize> = None;
    let mut last_gn_type: Option<i32> = None;
    // A GW of radius 0 just parsed: (its line, tag, segment count). nec2c reads
    // the very next card as its GC, raw, so nothing may come between — not a CM,
    // not a `'` comment; only a blank line, which is no card.
    let mut taper_pending: Option<(usize, u32, u32)> = None;

    for (idx, raw_line) in input.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw_line.trim();

        // Skip blank lines and lines that are entirely whitespace.
        if line.is_empty() {
            continue;
        }

        if let Some((gw_line, tag, _)) = taper_pending {
            if !split_mnemonic(line).0.eq_ignore_ascii_case("GC") {
                return Err(ParseError::TaperPairing {
                    line: gw_line,
                    error: nec_model::card::TaperPairingError::WireWithoutTaper {
                        index: deck.cards.len() - 1,
                        tag,
                    },
                });
            }
        }

        // A line starting with `'` is a comment (4nec2's editor convention); it
        // cannot change the problem.
        if line.starts_with('\'') {
            continue;
        }

        // The mnemonic is the first whitespace-delimited token.
        let (mnemonic, rest) = split_mnemonic(line);

        let upper = mnemonic.to_ascii_uppercase();
        if let Some(ge_line) = ge_line {
            if matches!(
                upper.as_str(),
                "GW" | "GC" | "GM" | "GR" | "GS" | "GX" | "GA" | "GH" | "GE"
            ) {
                return Err(ParseError::GeometryAfterGe {
                    line: lineno,
                    mnemonic: upper,
                    ge_line,
                });
            }
        }

        if is_program_card(&upper) {
            deck.program_cards.push(program_card(lineno, &upper, rest));
        }

        match upper.as_str() {
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
                let tag = parse_u32(lineno, "GW", 1, fields[0])?;
                let segments = parse_u32(lineno, "GW", 2, fields[1])?;
                // Radius 0 is NEC's tapered wire: the next card is its GC.
                let radius = if parse_f64(lineno, "GW", 9, fields[8])? == 0.0 {
                    taper_pending = Some((lineno, tag, segments));
                    0.0
                } else {
                    parse_radius(lineno, "GW", 9, fields[8])?
                };
                deck.cards.push(Card::Gw(GwCard {
                    tag,
                    segments,
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
                    radius,
                }));
            }
            "GC" => {
                // GC I1 I2 RDEL RAD1 RAD2, the taper of the radius-0 GW before it.
                // nec2c reads I1/I2 and ignores them, as NEC-2 defines them.
                let Some((_, _, segments)) = taper_pending.take() else {
                    return Err(ParseError::TaperPairing {
                        line: lineno,
                        error: nec_model::card::TaperPairingError::TaperWithoutWire {
                            index: deck.cards.len(),
                        },
                    });
                };
                let fields = parse_fields(rest);
                require_fields(lineno, "GC", &fields, 5)?;
                let length_ratio = parse_f64(lineno, "GC", 3, fields[2])?;
                if length_ratio <= 0.0 {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GC".to_string(),
                        field: 3,
                        raw: fields[2].to_string(),
                        reason: "a segment-length ratio must be positive: at 0 nec2c makes \
                                 every segment after the first zero-length, and below it \
                                 reverses every other one; 1 is a uniform wire"
                            .to_string(),
                    });
                }
                let radius_start = parse_radius(lineno, "GC", 4, fields[3])?;
                let radius_end = parse_radius(lineno, "GC", 5, fields[4])?;
                if segments == 1 && radius_end != radius_start {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GC".to_string(),
                        field: 5,
                        raw: fields[4].to_string(),
                        reason: format!(
                            "one segment cannot taper: nec2c gives it RAD1 and ignores RAD2; \
                             write the wire as a GW of radius {radius_start}, or give it more \
                             segments"
                        ),
                    });
                }
                deck.cards.push(Card::Gc(GcCard {
                    length_ratio,
                    radius_start,
                    radius_end,
                }));
            }
            "GE" => {
                ge_line = Some(lineno);
                let fields = parse_fields(rest);
                let ground_reflection_flag = if fields.is_empty() {
                    0
                } else {
                    parse_i32(lineno, "GE", 1, fields[0])?
                };
                // FND-241: NEC-2 defines -1, 0 and 1. nec2c reads any other value
                // by its sign, which is its code, not the format, so it is refused.
                if !(-1..=1).contains(&ground_reflection_flag) {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GE".to_string(),
                        field: 1,
                        raw: fields[0].to_string(),
                        reason: "not a NEC-2 ground flag (1 = wire ends at z = 0 joined to \
                                 the ground image, 0 or -1 = left unjoined); nec2c reads other \
                                 values by their sign, which NEC-2 does not define, so the deck \
                                 is refused"
                            .to_string(),
                    });
                }
                if ground_reflection_flag == 1 {
                    ge_one_line = Some(lineno);
                }
                deck.cards.push(Card::Ge(GeCard {
                    ground_reflection_flag,
                }));
            }
            "GX" => {
                // GX I1 I2: I2's digits XYZ select the planes, read as nec2c reads
                // them (each nonzero digit selects; a missing I2 is 0, a no-op).
                let fields = parse_fields(rest);
                let f = |i: usize| -> &str { fields.get(i).copied().unwrap_or("0") };
                // Integer fields, whole values only: nec2c stops on `GX 0 1.5`, and
                // `parse_i32` would truncate it to a plane selection. `100.` is fine.
                let itg = parse_whole_i32(lineno, "GX", 1, f(0))?;
                if itg < 0 {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GX".to_string(),
                        field: 1,
                        raw: f(0).to_string(),
                        reason: "a negative tag increment produces tags below the \
                                 original's (nec2c gives a tag-1 wire's copy tag 0 for \
                                 -1), which fnec's unsigned tags cannot represent"
                            .to_string(),
                    });
                }
                let ns = parse_whole_i32(lineno, "GX", 2, f(1))?;
                if !(0..=999).contains(&ns) {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GX".to_string(),
                        field: 2,
                        raw: f(1).to_string(),
                        reason: "the planes are three digits XYZ, 0 to 999; nec2c reads \
                                 a sign or a thousands digit by C integer arithmetic \
                                 (1001 as X and Z, -11 as Y and Z), which is a typo's \
                                 reading, not the format"
                            .to_string(),
                    });
                }
                deck.cards.push(Card::Gx(GxCard {
                    tag_increment: itg as u32,
                    x: ns / 100 != 0,
                    y: (ns / 10) % 10 != 0,
                    z: ns % 10 != 0,
                }));
            }
            "GA" => {
                // GA ITG NS RADA ANG1 ANG2 RAD, nec2c's `arc`. What nec2c builds wrong
                // or hangs on is refused here (Fable's review of the design probed
                // each): a zero radius hangs it, a negative one is the arc turned
                // 180 degrees, NS <= 0 adds nothing or miscounts the structure, and
                // a segment spanning 180 degrees or more is a chord through the
                // centre (GA 1 1 0.5 0 360 solved a 6e-17 m wire, silently).
                let fields = parse_fields(rest);
                require_fields(lineno, "GA", &fields, 6)?;
                let refuse = |field: usize, reason: String| ParseError::UnsupportedValue {
                    line: lineno,
                    card: "GA".to_string(),
                    field,
                    raw: fields[field - 1].to_string(),
                    reason,
                };
                let tag = parse_whole_i32(lineno, "GA", 1, fields[0])?;
                if tag < 0 {
                    return Err(refuse(1, "a tag must not be negative".to_string()));
                }
                let ns = parse_whole_i32(lineno, "GA", 2, fields[1])?;
                if ns < 1 {
                    return Err(refuse(
                        2,
                        "the number of segments must be at least 1; nec2c adds nothing for \
                         0 and miscounts the structure for a negative value"
                            .to_string(),
                    ));
                }
                let rada = parse_f64(lineno, "GA", 3, fields[2])?;
                if rada <= 0.0 {
                    return Err(refuse(
                        3,
                        "an arc radius must be positive; nec2c reads a negative radius as the \
                         arc turned 180 degrees about y (spell that with ANG1 + 180), and a \
                         zero radius hangs it"
                            .to_string(),
                    ));
                }
                let ang1 = parse_f64(lineno, "GA", 4, fields[3])?;
                let ang2 = parse_f64(lineno, "GA", 5, fields[4])?;
                let span = (ang2 - ang1).abs();
                if span == 0.0 {
                    return Err(refuse(
                        5,
                        "the arc spans no angle (ANG2 = ANG1), so every segment is zero \
                         length; nec2c stops on it with a segment data error"
                            .to_string(),
                    ));
                }
                if span >= 360.00001 {
                    return Err(refuse(
                        5,
                        "the arc spans more than 360 degrees; nec2c stops with \"ARC ANGLE \
                         EXCEEDS 360 DEGREES\""
                            .to_string(),
                    ));
                }
                if span / f64::from(ns) >= 180.0 {
                    let need = (span / 180.0).floor() as i32 + 1;
                    return Err(refuse(
                        2,
                        format!(
                            "each segment would span {} degrees of the arc: a segment of 180 \
                             degrees or more is a chord through the centre, not an arc (nec2c \
                             solves a zero-length or doubled wire there); use at least {need} \
                             segments",
                            span / f64::from(ns)
                        ),
                    ));
                }
                deck.cards.push(Card::Ga(GaCard {
                    tag: tag as u32,
                    segments: ns as u32,
                    arc_radius: rada,
                    ang1_deg: ang1,
                    ang2_deg: ang2,
                    radius: parse_radius(lineno, "GA", 6, fields[5])?,
                }));
            }
            "GH" => {
                // GH ITG NS S HL A1 B1 A2 B2 RAD, nec2c's `helix`, with what it hangs
                // on or builds as something other than the deck says refused (Fable's
                // review probed each). NEC-2's manual gives B1/B2 no default; nec2c's
                // code reads B1 = 0 as A1 in a uniform helix and B2 = 0 as A2 in a
                // taper, which decks lean on and which fnec copies — but the two
                // spellings whose readings diverge are refused.
                let fields = parse_fields(rest);
                require_fields(lineno, "GH", &fields, 9)?;
                let refuse = |field: usize, reason: String| ParseError::UnsupportedValue {
                    line: lineno,
                    card: "GH".to_string(),
                    field,
                    raw: fields[field - 1].to_string(),
                    reason,
                };
                let tag = parse_whole_i32(lineno, "GH", 1, fields[0])?;
                if tag < 0 {
                    return Err(refuse(1, "a tag must not be negative".to_string()));
                }
                let ns = parse_whole_i32(lineno, "GH", 2, fields[1])?;
                if ns < 1 {
                    return Err(refuse(
                        2,
                        "the number of segments must be at least 1".to_string(),
                    ));
                }
                let s = parse_f64(lineno, "GH", 3, fields[2])?;
                if s <= 0.0 {
                    return Err(refuse(
                        3,
                        "the spacing between turns must be positive; nec2c hangs on 0 and \
                         reads a negative spacing as a mirrored helix, which NEC-2 does not \
                         define — a left-handed helix is HL < 0"
                            .to_string(),
                    ));
                }
                let hl = parse_f64(lineno, "GH", 4, fields[3])?;
                if hl == 0.0 {
                    return Err(refuse(
                        4,
                        "a zero axial length is NEC-2's flat spiral, which nec2c cannot build \
                         (every segment is zero length and the solve is NaN); fnec does not \
                         model it"
                            .to_string(),
                    ));
                }
                let r: Vec<f64> = (5..=8)
                    .map(|i| parse_f64(lineno, "GH", i, fields[i - 1]))
                    .collect::<Result<_, _>>()?;
                let (a1, b1, a2, b2) = (r[0], r[1], r[2], r[3]);
                if let Some(i) = r.iter().position(|v| *v < 0.0) {
                    return Err(refuse(
                        i + 5,
                        "a helix radius must not be negative; nec2c reads one as the helix \
                         turned 180 degrees, which NEC-2 does not define"
                            .to_string(),
                    ));
                }
                if a2 == a1 {
                    let b1e = if b1 == 0.0 { a1 } else { b1 };
                    if b2 != 0.0 && b2 != b1e {
                        return Err(refuse(
                            8,
                            format!(
                                "with A2 = A1 nec2c ignores B2 and builds the whole helix with \
                                 B1 = {b1e}, while NEC-2 defines B2 as the y radius at z = HL; \
                                 spell the helix you mean — B2 = B1 for a uniform helix, or \
                                 A2 != A1 for a taper"
                            ),
                        ));
                    }
                    if a1 == 0.0 {
                        return Err(refuse(
                            5,
                            "every radius is zero, so the helix is a straight wire on the z \
                             axis; write it as a GW"
                                .to_string(),
                        ));
                    }
                } else if b1 == 0.0 && a1 != 0.0 {
                    return Err(refuse(
                        6,
                        format!(
                            "in a tapered helix nec2c reads B1 = 0 as a y radius of zero at z = 0 \
                             (the helix starts flattened), while it reads B1 = 0 as B1 = A1 in a \
                             uniform helix and B2 = 0 as B2 = A2 here; NEC-2 defines no default \
                             — write B1 = A1 (= {a1}) for the round helix, or a small B1 for the \
                             flattened start"
                        ),
                    ));
                }
                deck.cards.push(Card::Gh(GhCard {
                    tag: tag as u32,
                    segments: ns as u32,
                    turn_spacing: s,
                    length: hl,
                    a1,
                    b1,
                    a2,
                    b2,
                    radius: parse_radius(lineno, "GH", 9, fields[8])?,
                }));
            }
            "GS" => {
                // GS I1 I2 F1: F1 scales every segment defined so far. nec2c reads
                // I1/I2 and ignores them; a missing F1 reads as 0 there, and a
                // factor <= 0 ends in its "SEGMENT DATA ERROR" at GE, so refusing
                // those is nec2c's own outcome, said earlier.
                let fields = parse_fields(rest);
                require_fields(lineno, "GS", &fields, 3)?;
                let scale = parse_finite_f64_field(lineno, "GS", 3, fields[2])?;
                if scale <= 0.0 {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GS".to_string(),
                        field: 3,
                        raw: fields[2].to_string(),
                        reason: "a scale factor must be positive: zero collapses the \
                                 structure and a negative one mirrors it, and nec2c stops \
                                 on both with a segment data error"
                            .to_string(),
                    });
                }
                deck.cards.push(Card::Gs(GsCard { scale }));
            }
            "GM" => {
                // GM ITGI NRPT ROX ROY ROZ XS YS ZS ITS
                //
                // Trailing fields may be omitted and default to 0, as nec2c does
                // — verified: a bare `GM`, `GM 0`, `GM 0 1` and the 6-field form
                // are all accepted there, where fnec used to exit 1 demanding
                // nine (FND-119). Defaults are what make that safe: an absent
                // rotation or translation is no movement, and an absent NRPT is
                // an in-place move.
                let fields = parse_fields(rest);
                let f = |i: usize| -> &str { fields.get(i).copied().unwrap_or("0") };

                // ITGI is signed in NEC and nec2c will happily produce a NEGATIVE
                // tag from it (measured: `GM -10 1 …` yields tag -9). fnec's tags
                // are u32, so that structure is unrepresentable — and `as u32`
                // saturates a negative to 0, which would silently make a same-tag
                // copy instead. Refuse it by name rather than answer a different
                // question.
                let itgi = parse_finite_f64_field(lineno, "GM", 1, f(0))?;
                if itgi < 0.0 {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GM".to_string(),
                        field: 1,
                        raw: f(0).to_string(),
                        reason: "a negative tag increment produces negative tag \
                                 numbers (nec2c yields tag -9 here), which fnec's \
                                 unsigned tags cannot represent"
                            .to_string(),
                    });
                }

                deck.cards.push(Card::Gm(GmCard {
                    tag_increment: itgi as u32,
                    repeat_count: parse_u32(lineno, "GM", 2, f(1))?,
                    rot_x_deg: parse_f64(lineno, "GM", 3, f(2))?,
                    rot_y_deg: parse_f64(lineno, "GM", 4, f(3))?,
                    rot_z_deg: parse_f64(lineno, "GM", 5, f(4))?,
                    translate_x: parse_f64(lineno, "GM", 6, f(5))?,
                    translate_y: parse_f64(lineno, "GM", 7, f(6))?,
                    translate_z: parse_f64(lineno, "GM", 8, f(7))?,
                    start_tag: parse_u32(lineno, "GM", 9, f(8))?,
                }));
            }
            "GR" => {
                // GR I1 I2, as NEC-2 reads it (FND-247): I2 occurrences in all,
                // 360/I2 apart. Whole integers only (parse_u32 would truncate 2.5
                // and saturate -2 to 0); `4.0` is fine.
                let fields = parse_fields(rest);
                require_fields(lineno, "GR", &fields, 2)?;
                let f = |i: usize| -> &str { fields.get(i).copied().unwrap_or("0") };
                let itg = parse_whole_i32(lineno, "GR", 1, f(0))?;
                if itg < 0 {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GR".to_string(),
                        field: 1,
                        raw: f(0).to_string(),
                        reason: "a negative tag increment produces tags below the \
                                 original's (nec2c gives tags 1, 0, -1 for GR -1 3), which \
                                 fnec's unsigned tags cannot represent"
                            .to_string(),
                    });
                }
                let n = parse_whole_i32(lineno, "GR", 2, f(1))?;
                if n < 1 {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GR".to_string(),
                        field: 2,
                        raw: f(1).to_string(),
                        reason: "the number of occurrences, the original included, must be \
                                 at least 1; nec2c stops on 0 or a negative count"
                            .to_string(),
                    });
                }
                // NEC-2's GR has no angle: nec2c ignores a third field. fnec read
                // one, as the step between I2 *additional* copies, until FND-247,
                // so a deck in that reading would now silently change. A third
                // field that is 0 (padding) or 360/I2 means the same either way;
                // any other value is that old reading, refused with its spelling.
                let f1 = parse_finite_f64_field(lineno, "GR", 3, f(2))?;
                let step = 360.0 / f64::from(n);
                if f1 != 0.0 && (f1 - step).abs() > 1e-6 * step {
                    let k = n as f64;
                    let closes = ((k + 1.0) * f1 - 360.0).abs() <= 1e-6 * 360.0;
                    let spelling = if closes {
                        format!("GR {itg} {}", n + 1)
                    } else {
                        format!("GM {itg} {n} 0 0 {} 0 0 0", f(2))
                    };
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GR".to_string(),
                        field: 3,
                        raw: f(2).to_string(),
                        reason: format!(
                            "NEC-2's GR has no angle: it makes {n} occurrence(s) in all, the \
                             original included, {step} degrees apart, and nec2c ignores this \
                             field. fnec before 0.23.0 read GR {itg} {n} {} as {n} additional \
                             copies {} degrees apart; that structure is `{spelling}`",
                            f(2),
                            f(2)
                        ),
                    });
                }
                deck.cards.push(Card::Gr(GrCard {
                    tag_increment: itg as u32,
                    occurrences: n as u32,
                }));
            }
            "GN" => {
                // GN I1 NRADL I3 I4 EPSE SIG F3 F4 F5 F6, read by position (FND-283).
                let p = read_positional(lineno, "GN", rest)?;
                let ground_type = p.int(1);
                // FND-241: NEC-2 defines -1..=2. nec2c solves another type
                // inconsistently: the currents over a perfect ground, the pattern
                // over a finite one. fnec solved free space with a warning.
                if !(-1..=2).contains(&ground_type) {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GN".to_string(),
                        field: 1,
                        raw: p.raw(1).to_string(),
                        reason: "not a NEC-2 ground type (-1 none, 0 finite by reflection \
                                 coefficient, 1 perfect, 2 finite by Sommerfeld); nec2c solves \
                                 it inconsistently (perfect-ground currents under a finite-ground \
                                 pattern), so the deck is refused"
                            .to_string(),
                    });
                }
                let gn = gn_card(&p, &mut warnings)?;
                last_gn_type = Some(ground_type);
                deck.cards.push(Card::Gn(gn));
            }
            "EX" => {
                // EX I1 I2 I3 I4 F1..F6 by position, missing fields zero (nec2c's
                // readmn). F3 is the plane-wave polarization angle η, F4/F5 the
                // incidence-angle-sweep increments Δθ/Δφ, F6 the axial ratio; all
                // unused by the source types.
                let p = read_positional(lineno, "EX", rest)?;
                deck.cards.push(Card::Ex(ExCard {
                    excitation_type: p.count(1)?,
                    tag: p.count(2)?,
                    segment: p.count(3)?,
                    i4: p.count(4)?,
                    voltage_real: p.float(1),
                    voltage_imag: p.float(2),
                    polarization_deg: p.float(3),
                    polarization_ratio: p.float(6),
                    theta_inc: p.float(4),
                    phi_inc: p.float(5),
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
                let card = NearFieldCard {
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
                // FR I1 I2 I3 I4 F1 F2 by position (FND-287): the frequency is F1,
                // the fifth field, whatever the card's length. fnec used to read a
                // card of fewer than six fields as `FR I1 I2 F1 F2`, so the canonical
                // `FR 0 1 0 0 29.9` read its I3 as the frequency.
                if let Some(e) = fr_shorthand(lineno, rest) {
                    return Err(e);
                }
                let p = read_positional(lineno, "FR", rest)?;
                deck.cards.push(Card::Fr(FrCard {
                    step_type: p.count(1)?,
                    steps: p.count(2)?,
                    frequency_mhz: p.float(1),
                    step_mhz: p.float(2),
                }));
            }
            "RP" => {
                // RP I1 NTH NPH XNDA θ0 φ0 Δθ Δφ RFLD GNOR by position (FND-284).
                if let Some(e) = rp_float_xnda(lineno, rest) {
                    return Err(e);
                }
                let p = read_positional(lineno, "RP", rest)?;
                deck.cards.push(Card::Rp(rp_card(&p, &mut warnings)?));
            }
            "LD" => {
                // LD I1 I2 I3 I4 F1 F2 F3 by position: I1 = load type, I2 = tag,
                // I3..I4 = the segment range, F1–F3 the values (F4–F6 unread, as
                // in nec2c).
                let p = read_positional(lineno, "LD", rest)?;
                deck.cards.push(Card::Ld(LdCard {
                    load_type: p.int(1),
                    tag: p.count(2)?,
                    seg_first: p.count(3)?,
                    seg_last: p.count(4)?,
                    f1: p.float(1),
                    f2: p.float(2),
                    f3: p.float(3),
                }));
            }
            "TL" => {
                // NEC-2: TL I1 I2 I3 I4 F1 F2 F3 F4 F5 F6, plus fnec's optional F7
                // (velocity factor) and F8 (matched-line loss, dB) in fields 11
                // and 12, past the ten NEC reads. See TlCard.
                let fields = parse_fields(rest);
                if let Some(nec2) = legacy_tl_layout(&fields) {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "TL".to_string(),
                        field: 5,
                        raw: fields[4..].join(" "),
                        reason: format!(
                            "this is fnec's old TL layout (NSEG TYPE Z0 LENGTH VF); TL now \
                             uses the NEC-2 layout (Z0 LENGTH Y1r Y1i Y2r Y2i [VF] [LOSS_dB]). \
                             Rewrite it as: {nec2}. If it really is a NEC-2 card, write Z0 \
                             with a decimal point"
                        ),
                    });
                }
                let p = read_positional(lineno, "TL", rest)?;
                deck.cards.push(Card::Tl(TlCard {
                    tag1: p.count(1)?,
                    segment1: p.count(2)?,
                    tag2: p.count(3)?,
                    segment2: p.count(4)?,
                    z0: p.float(1),
                    length: p.float(2),
                    shunt1: (p.float(3), p.float(4)),
                    shunt2: (p.float(5), p.float(6)),
                    velocity_factor: p.extension(11)?.unwrap_or(1.0),
                    loss_db: p.extension(12)?.unwrap_or(0.0),
                }));
            }
            "NT" => {
                // NT I1 I2 I3 I4 F1..F6 by position: the two ports and the
                // short-circuit admittances Y11, Y12, Y22. The solver stamps them
                // into the MoM matrix (build_nt_stamps).
                let p = read_positional(lineno, "NT", rest)?;
                deck.cards.push(Card::Nt(NtCard {
                    tag1: p.count(1)?,
                    segment1: p.count(2)?,
                    tag2: p.count(3)?,
                    segment2: p.count(4)?,
                    y11: (p.float(1), p.float(2)),
                    y12: (p.float(3), p.float(4)),
                    y22: (p.float(5), p.float(6)),
                }));
            }
            "PT" => {
                // PT I1 I2 I3 I4 by position. The solver applies the current-print
                // control to the CURRENTS table (apply_pt_current_filter,
                // PH9-CHK-004, FND-299). I1 = 1..3 are nec2c's receive-pattern
                // print formats: the same segments, another layout, which fnec
                // does not have.
                let p = read_positional(lineno, "PT", rest)?;
                let mode = p.int(1);
                if (1..=3).contains(&mode) {
                    warnings.push(ParseError::IgnoredField {
                        line: lineno,
                        card: "PT".to_string(),
                        field: 1,
                        why: format!(
                            "PT {mode} selects one of NEC-2's receive-pattern print formats for \
                             the currents; fnec prints the selected segments in its CURRENTS \
                             table as for PT 0"
                        ),
                    });
                }
                deck.cards.push(Card::Pt(PtCard {
                    mode,
                    tag: p.count(2)?,
                    seg_first: p.count(3)?,
                    seg_last: p.count(4)?,
                }));
            }
            "EN" => {
                deck.cards.push(Card::En(EnCard));
                // Stop at EN per NEC spec.
                break;
            }
            other => match OTHER_CARDS.iter().find(|(m, _)| *m == other) {
                Some((_, OtherCard::Ignored(why))) => warnings.push(ParseError::IgnoredCard {
                    line: lineno,
                    mnemonic: other.to_string(),
                    why,
                }),
                Some((_, OtherCard::Refused(what))) => {
                    return Err(ParseError::RefusedCard {
                        line: lineno,
                        mnemonic: other.to_string(),
                        what,
                    });
                }
                None => {
                    return Err(ParseError::UnknownCard {
                        line: lineno,
                        mnemonic: other.to_string(),
                    });
                }
            },
        }
    }

    if let Some((line, tag, _)) = taper_pending {
        return Err(ParseError::TaperPairing {
            line,
            error: nec_model::card::TaperPairingError::WireWithoutTaper {
                index: deck.cards.len() - 1,
                tag,
            },
        });
    }
    debug_assert_eq!(nec_model::card::taper_pairing(&deck.cards), Ok(()));

    // FND-241: GE 1 declares a ground plane, but only GN defines one. nec2c then
    // solves free space while joining any wire end at z = 0 to an image that is
    // not there (a quarter-wave monopole reads 16.28 - j1905 ohm); fnec inferred a
    // perfect ground. Neither is the deck's antenna, so it is refused.
    if let Some(line) = ge_one_line {
        if matches!(last_gn_type, None | Some(-1)) {
            return Err(ParseError::UnsupportedValue {
                line,
                card: "GE".to_string(),
                field: 1,
                raw: "1".to_string(),
                reason: "GE 1 declares a ground plane but no GN card defines one; NEC-2 then \
                         solves in free space and joins any wire end at z = 0 to an image that \
                         is not there. Add `GN 1` for the perfect ground fnec used to infer \
                         here, or set `GE 0` for free space"
                    .to_string(),
            });
        }
    }

    Ok(ParseResult { deck, warnings })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// A wire radius, positive and finite: `GW`'s, `GA`'s, `GH`'s and `GC`'s, one
/// rule. (A GW's zero, NEC's tapered wire, is read before this is asked; it used
/// to reach the solve and end as "a current that is not a finite number",
/// FND-236.)
fn parse_radius(line: usize, card: &str, field: usize, raw: &str) -> Result<f64, ParseError> {
    let r = parse_f64(line, card, field, raw)?;
    if r > 0.0 && r.is_finite() {
        return Ok(r);
    }
    Err(ParseError::UnsupportedValue {
        line,
        card: card.to_string(),
        field,
        raw: raw.to_string(),
        reason: "a wire radius must be positive".to_string(),
    })
}

/// An integer field written as a whole number (`4`, or `4.` and `1.0` as 4nec2
/// writes them), as nec2c needs: it stops on a fractional value where fnec used
/// to truncate it, so `FR 0 1 29.9 0` read I3 as 29 without a word (FND-300).
/// Shared by every integer field of every card, so they cannot word or test it
/// differently.
fn parse_whole(line: usize, card: &str, field: usize, raw: &str) -> Result<f64, ParseError> {
    let v = parse_finite_f64_field(line, card, field, raw)?;
    if v.fract() != 0.0 {
        return Err(ParseError::UnsupportedValue {
            line,
            card: card.to_string(),
            field,
            raw: raw.to_string(),
            reason: "an integer field; nec2c stops on a fractional value here".to_string(),
        });
    }
    Ok(v)
}

/// [`parse_whole`] as an `i32`, refusing a value outside its range rather than
/// saturating to a plausible one.
fn parse_whole_i32(line: usize, card: &str, field: usize, raw: &str) -> Result<i32, ParseError> {
    let v = parse_whole(line, card, field, raw)?;
    if v < f64::from(i32::MIN) || v > f64::from(i32::MAX) {
        return Err(ParseError::UnsupportedValue {
            line,
            card: card.to_string(),
            field,
            raw: raw.to_string(),
            reason: "an integer field, and this is outside its range".to_string(),
        });
    }
    Ok(v as i32)
}

/// Whether a card is a program card, echoed by NEC-2D's output: any card that is
/// neither a comment nor part of the geometry section.
fn is_program_card(mnemonic: &str) -> bool {
    !matches!(
        mnemonic,
        "CM" | "CE"
            | "GW"
            | "GC"
            | "GM"
            | "GR"
            | "GS"
            | "GX"
            | "GA"
            | "GH"
            | "GE"
            | "SP"
            | "SM"
            | "SC"
    )
}

/// A program card as NEC reads it, for the record of every program card: its
/// fields through the one positional reader, [`read_positional`]. A field NEC
/// could not read — not a number, or a fraction in an integer field — leaves the
/// fields unreadable (`None`), which only a card fnec ignores can carry: every
/// card fnec parses refuses such a field.
fn program_card(line: usize, mnemonic: &str, rest: &str) -> ProgramCard {
    ProgramCard {
        line,
        mnemonic: mnemonic.to_string(),
        fields: read_positional(line, mnemonic, rest)
            .ok()
            .map(|p| (p.ints, p.floats)),
    }
}

/// A card's fields read by position, as nec2c's `readmn` reads them: I1–I4
/// integers, then F1–F6 reals, a missing field zero, anything after the tenth
/// unread. The ONE field reader: every typed program-card arm (FR, RP, GN, LD,
/// EX, TL, NT, PT) takes its fields from it, and so does the record of every
/// program card ([`program_card`]). A card is never read by counting its fields:
/// that is how `RP 0 4 1 1000 0 0 30` read XNDA as θ₀ (FND-284) and
/// `FR 0 1 0 0 29.9` read I3 as the frequency (FND-287).
///
/// An integer field holds a whole number (`4`, or `4.` and `1.0` as 4nec2 writes
/// them); a fraction is refused, as nec2c refuses it, where it used to be
/// truncated (FND-300).
struct Positional<'a> {
    line: usize,
    card: &'a str,
    raw: Vec<&'a str>,
    ints: [i32; 4],
    floats: [f64; 6],
}

fn read_positional<'a>(
    line: usize,
    card: &'a str,
    rest: &'a str,
) -> Result<Positional<'a>, ParseError> {
    let raw = parse_fields(rest);
    let mut ints = [0_i32; 4];
    let mut floats = [0.0_f64; 6];
    for (i, field) in raw.iter().take(10).enumerate() {
        if i < 4 {
            ints[i] = parse_whole_i32(line, card, i + 1, field)?;
        } else {
            floats[i - 4] = parse_f64(line, card, i + 1, field)?;
        }
    }
    Ok(Positional {
        line,
        card,
        raw,
        ints,
        floats,
    })
}

impl Positional<'_> {
    /// Integer field `In`, `n` in 1..=4.
    fn int(&self, n: usize) -> i32 {
        self.ints[n - 1]
    }

    /// Integer field `In` as a count or a segment reference, which may not be
    /// negative. (It used to saturate to 0, so `EX 0 -1 3` fed tag 0.)
    fn count(&self, n: usize) -> Result<u32, ParseError> {
        u32::try_from(self.ints[n - 1]).map_err(|_| ParseError::UnsupportedValue {
            line: self.line,
            card: self.card.to_string(),
            field: n,
            raw: self.raw(n).to_string(),
            reason: "a count or a segment reference, which cannot be negative".to_string(),
        })
    }

    /// Real field `Fn`, `n` in 1..=6 (card field `n + 4`).
    fn float(&self, n: usize) -> f64 {
        self.floats[n - 1]
    }

    /// The text of card field `field` (1-based), `0` when the card stops short.
    fn raw(&self, field: usize) -> &str {
        self.raw.get(field - 1).copied().unwrap_or("0")
    }

    /// A field past the ten NEC reads (fnec's TL extensions), if written.
    fn extension(&self, field: usize) -> Result<Option<f64>, ParseError> {
        self.raw
            .get(field - 1)
            .map(|s| parse_f64(self.line, self.card, field, s))
            .transpose()
    }
}

/// Whether a field is written as a plain integer (digits, an optional sign).
fn plain_integer(s: &str) -> bool {
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// fnec's retired `FR I1 I2 F1 F2` shorthand, refused with its rewrite (FND-287,
/// D4): a card of at most four fields whose third or fourth is written as a real.
/// Read by position it is `FR I1 I2 I3 I4` — nec2c stops on the `.` — and fnec
/// read it as the frequency for releases.
fn fr_shorthand(line: usize, rest: &str) -> Option<ParseError> {
    let f = parse_fields(rest);
    if f.len() > 4 {
        return None;
    }
    let field = (2..f.len()).find(|&i| !plain_integer(f[i]))?;
    let get = |i: usize| f.get(i).copied().unwrap_or("0");
    let card = f.join(" ");
    Some(ParseError::UnsupportedValue {
        line,
        card: "FR".to_string(),
        field: field + 1,
        raw: f[field].to_string(),
        reason: format!(
            "`FR {card}` is fnec's old 4-field shorthand `FR I1 I2 F1 F2`; NEC-2's FR is \
             `FR I1 I2 I3 I4 F1 F2`, and nec2c stops on '{}' in integer field {}. Write \
             FR {} {} 0 0 {} {}",
            f[field],
            field + 1,
            get(0),
            get(1),
            get(2),
            get(3)
        ),
    })
}

/// RP's XNDA written as a real (D4, FND-284): fnec's retired 7-field form
/// `RP I1 NTH NPH θ0 φ0 Δθ Δφ`, which nec2c refuses (a `.` in integer field 4) and
/// fnec read without XNDA, refused with its rewrite.
fn rp_float_xnda(line: usize, rest: &str) -> Option<ParseError> {
    let f = parse_fields(rest);
    let xnda = *f.get(3)?;
    if plain_integer(xnda) {
        return None;
    }
    let card = f.join(" ");
    let rewrite = if f.len() == 7 {
        format!(
            "fnec before 0.26 read this 7-field form as RP without XNDA. Write RP {} 0 {}",
            f[..3].join(" "),
            f[3..].join(" ")
        )
    } else {
        "write XNDA as an integer (e.g. 0, or 1000 as 4nec2 writes it)".to_string()
    };
    Some(ParseError::UnsupportedValue {
        line,
        card: "RP".to_string(),
        field: 4,
        raw: xnda.to_string(),
        reason: format!(
            "`RP {card}`: field 4 is XNDA, an integer; nec2c stops on '{xnda}' here, and \
             {rewrite}"
        ),
    })
}

/// The `GN` card from its positional fields (FND-283), with the parts fnec does
/// not model refused or warned as nec2c reads them.
fn gn_card(p: &Positional, warnings: &mut Vec<ParseError>) -> Result<GnCard, ParseError> {
    let (ground_type, nradl) = (p.int(1), p.int(2));
    let finite = matches!(ground_type, 0 | 2);
    let line = p.line;
    let warn = |field: usize, why: String| ParseError::IgnoredField {
        line,
        card: "GN".to_string(),
        field,
        why,
    };
    if finite && nradl != 0 {
        let reason = if ground_type == 2 {
            format!(
                "a radial-wire ground screen of {nradl} wires, which nec2c stops on over a \
                 Sommerfeld ground (RADIAL WIRE G.S. APPROXIMATION MAY NOT BE USED WITH \
                 SOMMERFELD GROUND OPTION) and fnec does not model; remove I2–F4, or model \
                 the radials as wires above the ground with --solver mpie"
            )
        } else {
            format!(
                "a radial-wire ground screen of {nradl} wires (F3 = radius {} m, F4 = wire \
                 radius {} m), which fnec does not model — nec2c's currents change with it \
                 (a dipole 5 m up at 29.9 MHz: 75.13 + j30.98 → 71.56 + j24.85 Ω); model the \
                 radials as wires above the ground with --solver mpie, or remove I2–F4",
                p.raw(7),
                p.raw(8)
            )
        };
        return Err(ParseError::UnsupportedValue {
            line,
            card: "GN".to_string(),
            field: 2,
            raw: p.raw(2).to_string(),
            reason,
        });
    }
    let nonzero = |field: usize| {
        if field <= 4 {
            p.int(field) != 0
        } else {
            p.float(field - 4) != 0.0
        }
    };
    if finite {
        if let Some(field) = (3..=4).find(|&f| nonzero(f)) {
            warnings.push(warn(field, "NEC-2 never reads GN I3/I4".to_string()));
        }
        if let Some(field) = (7..=10).find(|&f| nonzero(f)) {
            warnings.push(warn(
                field,
                "GN F3–F6 (a second medium or a cliff, NEC-2's GD parameters) only enter RP \
                 modes 2, 3, 5 and 6, which fnec refuses; the pattern is computed over the \
                 first medium, as nec2c's RP 0 computes it"
                    .to_string(),
            ));
        }
    } else if let Some(field) = (2..=10).find(|&f| nonzero(f)) {
        let over = if ground_type == 1 {
            "over a perfect ground"
        } else {
            "in free space"
        };
        warnings.push(warn(
            field,
            format!("GN {ground_type}: I2–F6 are ignored, as nec2c ignores them {over}"),
        ));
    }
    // F1/F2 both zero is a card that gives no medium: fnec's finite ground then
    // takes its average-ground default (13, 0.005), as it always has.
    let medium_given = p.float(1) != 0.0 || p.float(2) != 0.0;
    let f36 = [p.float(3), p.float(4), p.float(5), p.float(6)];
    Ok(GnCard {
        ground_type,
        radials: nradl,
        eps_r: medium_given.then(|| p.float(1)),
        sigma: medium_given.then(|| p.float(2)),
        screen: (nradl != 0).then(|| (p.float(3), p.float(4))),
        medium2: (nradl == 0 && f36.iter().any(|v| *v != 0.0)).then_some(f36),
    })
}

/// The `RP` card from its positional fields: the mode, the grid, and XNDA's
/// digits as nec2c splits them (FND-284, FND-285, FND-298).
fn rp_card(p: &Positional, warnings: &mut Vec<ParseError>) -> Result<RpCard, ParseError> {
    let line = p.line;
    let mode = p.int(1);
    if mode != 0 {
        let reason = match mode {
            1 => format!(
                "RP 1: radiated fields near the ground at RHO = {} m (nec2c's gfld), a \
                 quantity fnec does not compute; RP 0 gives the far-field pattern",
                p.raw(9)
            ),
            2..=6 => format!(
                "RP {mode}: a pattern over {}, which fnec does not model; RP 0 gives the \
                 far-field pattern over the first medium",
                match mode {
                    2 => "a linear cliff (GN or GD's second-medium fields)",
                    3 => "a circular cliff (GN or GD's second-medium fields)",
                    4 => "a radial-wire ground screen",
                    5 => "a radial-wire ground screen and a linear cliff",
                    _ => "a radial-wire ground screen and a circular cliff",
                }
            ),
            _ => "not a NEC-2 RP mode (0–6)".to_string(),
        };
        return Err(ParseError::UnsupportedValue {
            line,
            card: "RP".to_string(),
            field: 1,
            raw: p.raw(1).to_string(),
            reason,
        });
    }
    // XNDA = X·1000 + N·100 + D·10 + A, as nec2c splits it.
    let xnda = p.count(4)?;
    let (n, d, a) = ((xnda / 100) % 10, (xnda / 10) % 10, xnda % 10);
    let mut warn = |field: usize, why: String| {
        warnings.push(ParseError::IgnoredField {
            line,
            card: "RP".to_string(),
            field,
            why,
        });
    };
    // X (thousands) picks nec2c's column labels, vertical/horizontal for 1, and
    // nothing else: not warned. The NEC-2D output honours it (`nec2d_out`), fnec's own
    // table has one layout, and every 4nec2 deck writes 1000, so a warning would
    // ride on every such report and say nothing.
    if !matches!(n, 0 | 5) {
        warn(
            4,
            format!(
                "XNDA's N digit ({n}) asks nec2c to normalize one gain component (1 major, \
                 2 minor, 3 vertical, 4 horizontal, 5 total); fnec's NORMALIZED_PATTERN \
                 normalizes the total gain"
            ),
        );
    }
    if p.float(5) != 0.0 {
        warn(
            9,
            format!(
                "RFLD ({} m) asks nec2c to print the field at that range; fnec prints the \
                 field without the e^-jkR/R range factor, as nec2c does for RFLD = 0, and \
                 the gains do not depend on it",
                p.raw(9)
            ),
        );
    }
    if n != 0 && p.float(6) != 0.0 {
        warn(
            10,
            format!(
                "GNOR ({} dB) asks nec2c to normalize to that gain; fnec normalizes to the \
                 pattern's peak",
                p.raw(10)
            ),
        );
    }
    Ok(RpCard {
        mode: 0,
        // nec2c reads a zero count as one.
        n_theta: p.count(2)?.max(1),
        n_phi: p.count(3)?.max(1),
        theta0: p.float(1),
        phi0: p.float(2),
        d_theta: p.float(3),
        d_phi: p.float(4),
        normalize: n != 0,
        directive_gain: d != 0,
        avg_power_gain: a != 0,
    })
}

/// A field separator: whitespace or a comma, as NEC reads them — nec2c solves
/// `GW,1,21,0,0,-2.5,0,0,2.5,.001` (FND-237).
fn is_separator(c: char) -> bool {
    c.is_ascii_whitespace() || c == ','
}

fn split_mnemonic(line: &str) -> (&str, &str) {
    match line.find(is_separator) {
        Some(pos) => (&line[..pos], &line[pos..]),
        None => (line, ""),
    }
}

fn parse_fields(s: &str) -> Vec<&str> {
    s.split(is_separator).filter(|f| !f.is_empty()).collect()
}

/// Recognise fnec's retired TL layout, `TL t1 s1 t2 s2 NSEG TYPE Z0 LEN [VF]`, and
/// return its NEC-2 rewrite.
///
/// Read as NEC-2 such a card is silently wrong: `TL 1 26 2 26 1 0 50.0 0.1` would
/// become a 1 Ω line of length zero with 50 S across one end (FND-111). The tell is
/// the two bare integers where NEC-2 has Z0 and the length — NSEG, a small count,
/// and TYPE, 0 or 1 — followed by at least Z0 and the length. A genuine NEC-2 card
/// matches only if it writes a Z0 of at most 16 Ω and a length of exactly 0 or 1
/// as integers; the refusal says to add a decimal point.
fn legacy_tl_layout(fields: &[&str]) -> Option<String> {
    let bare_int = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if fields.len() < 8 || !bare_int(fields[4]) || !matches!(fields[5], "0" | "1") {
        return None;
    }
    if fields[4].parse::<u32>().ok()? > 16 {
        return None;
    }
    let z0 = fields[6];
    let len = fields[7];
    let head = fields[..4].join(" ");
    Some(match (fields[5], fields.get(8)) {
        // Lossy: F3 was the matched-line loss in dB, at velocity factor 1.
        ("1", Some(loss)) => format!("TL {head} {z0} {len} 0 0 0 0 1 {loss}"),
        (_, Some(vf)) if *vf != "1" && *vf != "1.0" => {
            format!("TL {head} {z0} {len} 0 0 0 0 {vf}")
        }
        _ => format!("TL {head} {z0} {len}"),
    })
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

/// Parse one integer field.
///
/// These go through `f64` because 4nec2 sometimes emits "1.0" for an integer
/// field — which means they inherit `from_str`'s acceptance of "NaN" and "inf",
/// and Rust's saturating `as` cast then turns those into *plausible* integers
/// rather than errors. `NaN as u32` is **0** and `inf as u32` is **u32::MAX**, so
/// `FR 0 NaN 0 0 14.2 0` parsed and solved as a one-step sweep, and
/// `FR 0 inf ...` asked for a four-billion-step one. Rejecting non-finite here
/// keeps the numeric contract the same across integer and float fields; without
/// it, "non-finite fields are rejected" would be true only of the float ones.
fn parse_finite_f64_field(
    lineno: usize,
    card: &str,
    field: usize,
    s: &str,
) -> Result<f64, ParseError> {
    let bad = || ParseError::BadField {
        line: lineno,
        card: card.to_string(),
        field,
        raw: s.to_string(),
    };
    let v = s.parse::<f64>().map_err(|_| bad())?;
    if !v.is_finite() {
        return Err(bad());
    }
    Ok(v)
}

fn parse_u32(lineno: usize, card: &str, field: usize, s: &str) -> Result<u32, ParseError> {
    // Accept a whole number written as a real ("1.0"), as 4nec2 emits for integer
    // fields; refuse a fraction (FND-300).
    parse_whole(lineno, card, field, s).map(|v| v as u32)
}

fn parse_i32(lineno: usize, card: &str, field: usize, s: &str) -> Result<i32, ParseError> {
    parse_whole(lineno, card, field, s).map(|v| v as i32)
}

/// Parse one numeric field, rejecting `NaN` and the infinities.
///
/// Rust's `f64::from_str` accepts "NaN", "inf" and "-inf", and a NEC deck is
/// plain text, so those reach the solver from any card. They do not merely give a
/// wrong answer — they give an *unrecognisable* one, because `NaN` fails every
/// comparison: `current.norm() > 1e-60` is false for `NaN`, so the feedpoint
/// falls into its zero-current branch and reports the source voltage, and
/// `is_negative_resistance` (`z_re < 0.0`) is false too, so no caveat fires.
/// Measured before this check: `GW 1 21 0 0 NaN 0 0 5.0 0.001` printed a feedpoint
/// row of `NaN NaN NaN NaN NaN NaN` at exit 0 with no warning (FND-030).
///
/// Rejecting at the parser closes this input route in one place, for every card
/// and every frontend, rather than teaching each downstream check to re-ask "is
/// this actually a number". No corpus deck or test fixture uses a non-finite
/// field. It does **not** close every route to a non-finite frequency: a finite
/// field near the top of the range still overflows once multiplied into hertz,
/// which `validate::frequency_error` catches instead.
fn parse_f64(lineno: usize, card: &str, field: usize, s: &str) -> Result<f64, ParseError> {
    parse_finite_f64_field(lineno, card, field, s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(line: &str) -> Result<TlCard, ParseError> {
        let deck = parse(&format!("GW 1 5 0 0 0 0 0 1 .001\n{line}\nEN\n"))?.deck;
        Ok(deck
            .cards
            .into_iter()
            .find_map(|c| match c {
                Card::Tl(t) => Some(t),
                _ => None,
            })
            .expect("a TL card"))
    }

    /// NEC-2 layout (FND-111), with NEC's blank-means-zero defaults and fnec's
    /// optional F7 velocity factor (default 1) and F8 loss in dB (default 0).
    #[test]
    fn a_tl_card_reads_the_nec2_layout() {
        let t = tl("TL 1 2 1 4 50.0 0.1").unwrap();
        assert_eq!((t.tag1, t.segment1, t.tag2, t.segment2), (1, 2, 1, 4));
        assert_eq!((t.z0, t.length), (50.0, 0.1));
        assert_eq!((t.shunt1, t.shunt2), ((0.0, 0.0), (0.0, 0.0)));
        assert_eq!((t.velocity_factor, t.loss_db), (1.0, 0.0));

        let t = tl("TL 1 2 1 4 -75.0 0 0.01 0.02 0.03 0.04 0.66 3.0").unwrap();
        assert_eq!((t.z0, t.length), (-75.0, 0.0));
        assert_eq!((t.shunt1, t.shunt2), ((0.01, 0.02), (0.03, 0.04)));
        assert_eq!((t.velocity_factor, t.loss_db), (0.66, 3.0));
    }

    #[test]
    fn the_retired_tl_layout_is_refused_with_its_rewrite() {
        let err = tl("TL 1 2 1 4 1 0 50.0 0.1 1.0").unwrap_err().to_string();
        assert!(
            err.contains("TL 1 2 1 4 50.0 0.1") && err.contains("NEC-2"),
            "{err}"
        );
    }

    /// The legacy test must not catch genuine NEC-2 cards that happen to use
    /// integers: a realistic Z0, or any card with fewer than eight fields.
    #[test]
    fn integer_valued_nec2_cards_are_not_mistaken_for_the_retired_layout() {
        for line in [
            "TL 1 2 1 4 50 1 0 0 0 0",
            "TL 1 2 1 4 300 0",
            "TL 1 2 1 4 10 1",
        ] {
            assert!(tl(line).is_ok(), "{line} is a NEC-2 card");
        }
    }

    /// `f64::from_str` accepts "NaN", "inf" and "-inf", and a NEC deck is plain
    /// text — so before this check those reached the solver from any card.
    /// Measured on the CLI beforehand: `GW 1 21 0 0 NaN 0 0 5.0 0.001` printed a
    /// feedpoint row of `NaN NaN NaN NaN NaN NaN` at exit 0 with **no** warning,
    /// because `NaN` fails every comparison the downstream caveats test (FND-030).
    #[test]
    fn a_non_finite_field_is_rejected_on_any_card() {
        for (deck, what) in [
            (
                "GW 1 21 0 0 NaN 0 0 5.0 0.001\nGE 0\nEN\n",
                "NaN in a GW coordinate",
            ),
            (
                "GW 1 21 0 0 inf 0 0 5.0 0.001\nGE 0\nEN\n",
                "inf in a GW coordinate",
            ),
            (
                "GW 1 21 0 0 -1 0 0 1 -inf\nGE 0\nEN\n",
                "-inf in a GW radius",
            ),
            (
                "GW 1 21 0 0 -1 0 0 1 0.001\nGE 0\nFR 0 1 0 0 NaN 0\nEN\n",
                "NaN in an FR frequency",
            ),
            (
                "GW 1 21 0 0 -1 0 0 1 0.001\nGE 0\nEX 0 1 11 0 NaN 0.0\nEN\n",
                "NaN in an EX voltage",
            ),
        ] {
            assert!(parse(deck).is_err(), "{what} must be rejected");
        }
    }

    /// Integer fields go through `f64` (4nec2 emits "1.0"), so they inherited
    /// `from_str`'s acceptance of "NaN"/"inf" — and the saturating `as` cast then
    /// produced a *plausible* integer instead of an error. `NaN as u32` is 0, so
    /// `FR 0 NaN 0 0 14.2 0` parsed and solved as a one-step sweep; `inf as u32`
    /// is `u32::MAX`, asking for a four-billion-step one.
    #[test]
    fn a_non_finite_integer_field_is_rejected_too() {
        for (deck, what) in [
            (
                "GW 1 21 0 0 -1 0 0 1 0.001\nGE 0\nFR 0 NaN 0 0 14.2 0\nEN\n",
                "NaN as an FR step count",
            ),
            (
                "GW 1 21 0 0 -1 0 0 1 0.001\nGE 0\nFR 0 inf 0 0 14.2 0\nEN\n",
                "inf as an FR step count",
            ),
            (
                "GW 1 NaN 0 0 -1 0 0 1 0.001\nGE 0\nEN\n",
                "NaN as a GW segment count",
            ),
        ] {
            assert!(parse(deck).is_err(), "{what} must be rejected");
        }
    }

    #[test]
    fn ordinary_numeric_fields_still_parse() {
        // Including the forms that look unusual but are finite.
        assert!(parse("GW 1 21 0 0 -5.2782e0 0 0 5.2782 1e-3\nGE 0\nEN\n").is_ok());
        // 4nec2 writes integer fields as floats; that must keep working.
        assert!(parse("GW 1 21.0 0 0 -1 0 0 1 0.001\nGE 0\nFR 0 1.0 0 0 14.2 0\nEN\n").is_ok());
    }
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
FR 0 1 0 0 28.0 0.0
RP 0 37 1 0 0.0 0.0 5.0 0.0
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
                directive_gain: false,
                avg_power_gain: false,
            })
        );
        // EN
        assert_eq!(cards[6], Card::En(EnCard));
    }

    /// FND-236: a mnemonic fnec does not know is refused — it used to be skipped
    /// with a warning, and the deck solved as a different antenna.
    #[test]
    fn an_unknown_card_is_refused() {
        let err = parse("CM test\nXX some unknown card\nEN\n")
            .err()
            .expect("an unknown card is fatal");
        assert!(
            matches!(&err, ParseError::UnknownCard { line: 2, mnemonic } if mnemonic == "XX"),
            "{err:?}"
        );
    }

    #[test]
    fn a_card_that_would_change_the_antenna_is_refused_by_name() {
        let deck = "GW 1 21 0 0 -8.2 0 0 8.2 .04\nGF 0\nGE 0\nEN\n";
        let err = parse(deck).err().expect("GF is refused");
        assert!(
            matches!(&err, ParseError::RefusedCard { line: 2, mnemonic, .. } if mnemonic == "GF"),
            "{err:?}"
        );
        assert!(
            err.to_string().contains("Numerical Green's Function"),
            "{err}"
        );
    }

    /// nec2c reads the card after a radius-0 GW as its GC, raw: anything else
    /// between them — a CM, a `'` comment, the end of the deck — is its "GEOMETRY
    /// DATA CARD ERROR", and so is a GC after anything but a radius-0 GW. A blank
    /// line is no card, and the pair beside them parses.
    #[test]
    fn a_gc_is_the_card_directly_after_its_radius_zero_gw() {
        let gw = "GW 1 9 0 0 -1 0 0 1 0";
        let gc = "GC 0 0 1.1 .001 .002";
        for (deck, line, wire_first) in [
            (format!("{gw}\nCM between\n{gc}\nEN\n"), 1, true),
            (format!("{gw}\n' between\n{gc}\nEN\n"), 1, true),
            (format!("{gw}\nGE 0\nEN\n"), 1, true),
            (format!("CE\n{gw}"), 2, true),
            (format!("{gc}\nEN\n"), 1, false),
            (format!("GW 1 9 0 0 -1 0 0 1 .001\n{gc}\nEN\n"), 2, false),
        ] {
            let err = parse(&deck)
                .err()
                .unwrap_or_else(|| panic!("{deck} parsed"));
            let ParseError::TaperPairing { line: at, error } = &err else {
                panic!("{deck}: {err:?}");
            };
            assert_eq!(
                (
                    *at,
                    matches!(
                        error,
                        nec_model::card::TaperPairingError::WireWithoutTaper { .. }
                    )
                ),
                (line, wire_first),
                "{deck}"
            );
        }
        for deck in [
            format!("{gw}\n{gc}\nEN\n"),
            format!("{gw}\n\n{gc}\nEN\n"),
            format!("{gw}\nGC 7 -3 1.1 .001 .002\nEN\n"),
        ] {
            let cards = parse(&deck)
                .unwrap_or_else(|e| panic!("{deck}: {e}"))
                .deck
                .cards;
            assert_eq!(nec_model::card::taper_pairing(&cards), Ok(()), "{deck}");
        }
    }

    #[test]
    fn a_card_that_cannot_change_the_antenna_is_a_warning() {
        let r = parse("GW 1 11 0 0 -1 0 0 1 .001\nGE 0\nEK\nXQ\nEN\n").expect("parses");
        let names: Vec<&str> = r
            .warnings
            .iter()
            .map(|w| match w {
                ParseError::IgnoredCard { mnemonic, .. } => mnemonic.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, ["EK", "XQ"]);
    }

    /// FND-237: commas separate fields, as in NEC — the whole line used to be one
    /// unknown mnemonic.
    #[test]
    fn comma_separated_fields_parse_as_spaced_ones() {
        let spaced =
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
        let commas =
            "GW,1,21,0,0,-2.5,0,0,2.5,.001\nGE,0\nEX, 0,1,11,0,1,0\nFR,0,1,0,0,29.9,0\nEN\n";
        assert_eq!(
            parse(commas).expect("commas").deck,
            parse(spaced).expect("spaces").deck
        );
    }

    #[test]
    fn an_apostrophe_line_is_a_comment() {
        let r = parse("' a 4nec2 comment\nGW 1 11 0 0 -1 0 0 1 .001\nEN\n").expect("parses");
        assert!(r.warnings.is_empty());
        assert_eq!(r.deck.cards.len(), 2);
    }

    #[test]
    fn a_zero_or_negative_gw_radius_is_refused() {
        for (r, says) in [("0", "tapered wire"), ("-0.001", "must be positive")] {
            let err = parse(&format!("GW 1 11 0 0 -1 0 0 1 {r}\nEN\n"))
                .err()
                .expect("refused");
            assert!(err.to_string().contains(says), "{r}: {err}");
        }
    }

    /// Every NEC-2 card is parsed, ignored or refused by name — none falls to the
    /// unknown-card refusal, whose message cannot say what the card would do. The
    /// list is NEC-2's card set (NEC-2 User's Guide, Part III) plus xnec2c's PL
    /// and 4nec2's SY.
    #[test]
    fn every_nec2_card_has_a_named_treatment() {
        const NEC2: [&str; 36] = [
            "CM", "CE", "GA", "GC", "GD", "GE", "GF", "GH", "GM", "GR", "GS", "GW", "GX", "SC",
            "SM", "SP", "CP", "EK", "EN", "EX", "FR", "GN", "KH", "LD", "NE", "NH", "NT", "NX",
            "PQ", "PT", "RP", "TL", "WG", "XQ", "PL", "SY",
        ];
        for m in NEC2 {
            if let Err(ParseError::UnknownCard { .. }) = parse(&format!("{m}\n")) {
                panic!("{m} has no named treatment");
            }
        }
        // And the list holds no card the parser also parses.
        for (m, _) in OTHER_CARDS {
            assert!(
                !matches!(parse(&format!("{m}\n")), Ok(ref r) if r.warnings.is_empty()),
                "{m} is in OTHER_CARDS but parses silently"
            );
        }
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

    fn rp(line: &str) -> Result<(RpCard, Vec<ParseError>), ParseError> {
        let r = parse(&format!("{line}\nEN\n"))?;
        let Some(Card::Rp(rp)) = r.deck.cards.first().cloned() else {
            panic!("expected an RP card");
        };
        Ok((rp, r.warnings))
    }

    /// FND-284: RP is read by position, whatever its length. Seven integer
    /// fields are `RP I1 NTH NPH XNDA θ0 φ0 Δθ` with Δφ zero, as nec2c reads them
    /// (θ 0/30/60/90); fnec read them as the old no-XNDA layout, θ0 = 1000.
    #[test]
    fn rp_reads_its_fields_by_position() {
        let (seven, _) = rp("RP 0 4 1 1000 0 0 30").unwrap();
        let (eight, _) = rp("RP 0 4 1 1000 0 0 30 0").unwrap();
        assert_eq!(seven, eight);
        assert_eq!(
            (seven.theta0, seven.phi0, seven.d_theta, seven.d_phi),
            (0.0, 0.0, 30.0, 0.0)
        );
        // nec2c reads a zero count as one.
        let (one, _) = rp("RP 0 0 0 0 10 20 5 5").unwrap();
        assert_eq!((one.n_theta, one.n_phi), (1, 1));
    }

    /// D4: the retired 7-field form with a real XNDA is refused with its
    /// rewrite, which reads back as the grid fnec used to solve.
    #[test]
    fn rp_with_a_real_xnda_is_refused_with_its_rewrite() {
        let err = rp("RP 0 37 73 0.0 0.0 5.0 5.0").unwrap_err();
        assert!(
            matches!(&err, ParseError::UnsupportedValue { field: 4, .. }),
            "{err:?}"
        );
        let msg = err.to_string();
        let rewrite = "RP 0 37 73 0 0.0 0.0 5.0 5.0";
        assert!(msg.contains(rewrite) && msg.contains("XNDA"), "{msg}");
        let (r, _) = rp(rewrite).unwrap();
        assert_eq!(
            (r.n_theta, r.n_phi, r.theta0, r.phi0, r.d_theta, r.d_phi),
            (37, 73, 0.0, 0.0, 5.0, 5.0)
        );
        // Control: an integer XNDA on an 8-field card is accepted.
        assert!(rp("RP 0 37 73 1000 0.0 0.0 5.0 5.0").is_ok());
    }

    /// FND-298: XNDA's digits as nec2c splits them. X (thousands) only picks
    /// column labels — every 4nec2 deck writes 1000 — and changes nothing else;
    /// N (hundreds) normalizes, D (tens) asks for directive gain, A
    /// (units) for the average gain.
    #[test]
    fn rp_xnda_digits_are_read_as_nec2c_splits_them() {
        let flags = |r: &RpCard| (r.normalize, r.directive_gain, r.avg_power_gain);
        let (x, w) = rp("RP 0 19 1 1000 0 0 10 0").unwrap();
        assert_eq!(flags(&x), (false, false, false));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            flags(&rp("RP 0 19 1 0500 0 0 10 0").unwrap().0),
            (true, false, false)
        );
        assert_eq!(
            flags(&rp("RP 0 19 1 0010 0 0 10 0").unwrap().0),
            (false, true, false)
        );
        assert_eq!(
            flags(&rp("RP 0 19 1 0001 0 0 10 0").unwrap().0),
            (false, false, true)
        );
        assert_eq!(
            flags(&rp("RP 0 19 1 1511 0 0 10 0").unwrap().0),
            (true, true, true)
        );
        // N = 5 (the total gain) is what fnec normalizes; another component warns.
        let (_, w) = rp("RP 0 19 1 0500 0 0 10 0").unwrap();
        assert!(w.is_empty(), "{w:?}");
        let (_, w) = rp("RP 0 19 1 0300 0 0 10 0").unwrap();
        assert!(w.iter().any(|w| w.to_string().contains("N digit")), "{w:?}");
        // RFLD, and GNOR with a normalization, are ignored with a reason.
        let (_, w) = rp("RP 0 19 1 0500 0 0 10 0 1000 3").unwrap();
        let fields: Vec<usize> = w
            .iter()
            .filter_map(|w| match w {
                ParseError::IgnoredField { field, .. } => Some(*field),
                _ => None,
            })
            .collect();
        assert_eq!(fields, vec![9, 10], "{w:?}");
    }

    /// FND-285: only RP 0 is a far-field pattern. Mode 1 (fields near the
    /// ground) and the cliff/screen modes 2–6 are refused by name; another value
    /// is no NEC-2 mode.
    #[test]
    fn rp_modes_other_than_zero_are_refused() {
        for (line, needle) in [
            (
                "RP 1 3 1 1000 0 0 5 0 1000",
                "near the ground at RHO = 1000 m",
            ),
            ("RP 2 19 1 0 0 0 10 0", "linear cliff"),
            ("RP 4 19 1 0 0 0 10 0", "radial-wire ground screen"),
            ("RP 6 19 1 0 0 0 10 0", "circular cliff"),
            ("RP 7 19 1 0 0 0 10 0", "not a NEC-2 RP mode"),
            ("RP -1 19 1 0 0 0 10 0", "not a NEC-2 RP mode"),
        ] {
            let err = rp(line).unwrap_err();
            assert!(
                matches!(&err, ParseError::UnsupportedValue { field: 1, .. })
                    && err.to_string().contains(needle),
                "{line}: {err}"
            );
        }
        assert!(rp("RP 0 19 1 0 0 0 10 0").is_ok());
    }

    fn fr(line: &str) -> Result<FrCard, ParseError> {
        let r = parse(&format!("{line}\nEN\n"))?;
        let Some(Card::Fr(fr)) = r.deck.cards.first().cloned() else {
            panic!("expected an FR card");
        };
        Ok(fr)
    }

    /// FND-287: FR is read by position. The five-field canonical card reads
    /// F1 = 29.9 (nec2c solves it, 79.004 + j41.448); fnec read field 3 as the
    /// frequency and refused it as 0 MHz.
    #[test]
    fn fr_reads_its_fields_by_position() {
        let five = fr("FR 0 1 0 0 29.9").unwrap();
        assert_eq!(
            (five.steps, five.frequency_mhz, five.step_mhz),
            (1, 29.9, 0.0)
        );
        assert_eq!(fr("FR 0 1 0 0 29.9 0").unwrap(), five);
        let sweep = fr("FR 0 5 0 0 10 2.5").unwrap();
        assert_eq!(
            (sweep.steps, sweep.frequency_mhz, sweep.step_mhz),
            (5, 10.0, 2.5)
        );
        // All integers: I3 = 30, F1 = 0, as nec2c reads it.
        assert_eq!(fr("FR 0 1 30 0").unwrap().frequency_mhz, 0.0);
    }

    /// D4: the retired `FR I1 I2 F1 F2` shorthand is refused with its rewrite.
    #[test]
    fn the_fr_shorthand_is_refused_with_its_rewrite() {
        for (line, rewrite) in [
            ("FR 0 1 29.9 0", "FR 0 1 0 0 29.9 0"),
            ("FR 0 3 14.0 0.5", "FR 0 3 0 0 14.0 0.5"),
            ("FR 0 1 14.2", "FR 0 1 0 0 14.2 0"),
        ] {
            let err = fr(line).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("shorthand") && msg.contains(rewrite),
                "{line}: {msg}"
            );
            assert_eq!(
                fr(rewrite).unwrap().frequency_mhz,
                line.split(' ').nth(3).unwrap().parse::<f64>().unwrap()
            );
        }
    }

    /// FND-300: an integer field refuses a fraction, as nec2c does, where it
    /// used to be truncated; a whole number written as a real is accepted.
    #[test]
    fn an_integer_field_refuses_a_fraction() {
        for deck in [
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nEX 0 1 11.5 0 1 0\nFR 0 1 0 0 30 0\nEN\n",
            "GW 1 21.5 0 0 -2.5 0 0 2.5 .001\nGE 0\nEN\n",
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nLD 4 1 0.5 0 50 0\nEN\n",
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nFR 0 1 0 0.5 30 0\nEN\n",
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nRP 0 1.5 1 0 0 0 0 0\nEN\n",
        ] {
            let err = parse(deck).err().expect("a fraction in an integer field");
            assert!(
                matches!(&err, ParseError::UnsupportedValue { .. })
                    && err
                        .to_string()
                        .contains("nec2c stops on a fractional value"),
                "{deck}: {err}"
            );
        }
        let ok = parse(
            "GW 1 21. 0 0 -2.5 0 0 2.5 .001\nGE 0\nEX 0 1. 11.0 0 1 0\nFR 0 1 0 0 30 0\nEN\n",
        )
        .expect("whole numbers written as reals");
        let Some(Card::Ex(ex)) = ok.deck.cards.get(2) else {
            panic!("EX");
        };
        assert_eq!((ex.tag, ex.segment), (1, 11));
    }

    /// A negative count or segment reference is refused, where it saturated to 0
    /// (and tag 0 is an absolute segment number now, FND-282).
    #[test]
    fn a_negative_segment_reference_is_refused() {
        let err = parse("GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nEX 0 -1 11 0 1 0\nEN\n")
            .err()
            .expect("refused");
        assert!(
            matches!(&err, ParseError::UnsupportedValue { field: 2, .. }),
            "{err:?}"
        );
    }

    fn gn(line: &str) -> Result<(GnCard, Vec<ParseError>), ParseError> {
        let r = parse(&format!(
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\n{line}\nEN\n"
        ))?;
        let gn = r
            .deck
            .cards
            .iter()
            .find_map(|c| match c {
                Card::Gn(g) => Some(g.clone()),
                _ => None,
            })
            .expect("a GN card");
        Ok((gn, r.warnings))
    }

    /// FND-283: a radial-wire screen over a finite ground changes nec2c's
    /// currents, and fnec does not model one: refused, over GN 0 by what it is,
    /// over GN 2 as nec2c refuses it.
    #[test]
    fn a_radial_screen_over_a_finite_ground_is_refused() {
        let err = gn("GN 0 16 0 0 13 .005 20 .001").unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(&err, ParseError::UnsupportedValue { field: 2, .. })
                && msg.contains("16 wires")
                && msg.contains("radius 20 m")
                && msg.contains("71.56 + j24.85"),
            "{msg}"
        );
        let err = gn("GN 2 16 0 0 13 .005 20 .001").unwrap_err();
        assert!(err.to_string().contains("RADIAL WIRE G.S."), "{err}");
    }

    /// D3: what nec2c ignores, fnec ignores with a warning; the 4nec2/xnec2c
    /// form `GN I1 0 0 0 F1 F2 [0 0 0 0]` is accepted silently.
    #[test]
    fn gn_fields_nec2c_ignores_are_warned_and_ignored() {
        for (line, field, needle) in [
            ("GN 1 16 0 0 0 0 20 .001", 2, "over a perfect ground"),
            ("GN -1 0 0 0 13 .005", 5, "in free space"),
            ("GN 2 0 1 0 13 .005", 3, "never reads GN I3/I4"),
            ("GN 0 0 0 0 13 .005 20 .001 0 0", 7, "second medium"),
        ] {
            let (_, w) = gn(line).unwrap_or_else(|e| panic!("{line}: {e}"));
            assert!(
                matches!(w.as_slice(), [ParseError::IgnoredField { field: f, why, .. }]
                    if *f == field && why.contains(needle)),
                "{line}: {w:?}"
            );
        }
        let (g, w) = gn("GN 1 16 0 0 0 0 20 .001").unwrap();
        assert_eq!(
            (g.ground_type, g.radials, g.screen),
            (1, 16, Some((20.0, 0.001)))
        );
        let (g, w2) = gn("GN 0 0 0 0 13 .005 20 .001 0 0").unwrap();
        assert_eq!(g.medium2, Some([20.0, 0.001, 0.0, 0.0]));
        assert!(!w.is_empty() && !w2.is_empty());
        for line in [
            "GN 2 0 0 0 13 .005",
            "GN 2 0 0 0 13 .005 0 0 0 0",
            "GN 1",
            "GN -1",
        ] {
            let (_, w) = gn(line).unwrap();
            assert!(w.is_empty(), "{line}: {w:?}");
        }
    }

    /// D3: GD (the second medium on its own card) is ignored with the reason,
    /// as GN's F3–F6 are, where it was refused.
    #[test]
    fn a_gd_card_is_ignored_with_a_warning() {
        let r = parse(
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nGN 2 0 0 0 13 .005\nGD 0 0 0 0 5 .001 20 0\nEN\n",
        )
        .expect("GD is ignored");
        assert!(
            matches!(r.warnings.as_slice(), [ParseError::IgnoredCard { mnemonic, .. }] if mnemonic == "GD"),
            "{:?}",
            r.warnings
        );
    }

    /// TL, NT and PT are read by position too: a short card's missing fields are
    /// zero, as nec2c reads them, and fnec's TL extensions sit past the tenth.
    #[test]
    fn tl_nt_and_pt_read_their_fields_by_position() {
        let deck = parse(
            "GW 1 21 0 0 -2.5 0 0 2.5 .001\nGW 2 21 1 0 -2.5 1 0 2.5 .001\nGE 0\n\
             NT 1 11 2 11 0 -0.002\nPT 0 2 1\nTL 1 5 2 5 50.0 1 0 0 0 0 0.66 3\nEN\n",
        )
        .unwrap()
        .deck;
        let nt = deck.cards.iter().find_map(|c| match c {
            Card::Nt(n) => Some(n.clone()),
            _ => None,
        });
        assert_eq!(
            nt,
            Some(NtCard {
                tag1: 1,
                segment1: 11,
                tag2: 2,
                segment2: 11,
                y11: (0.0, -0.002),
                y12: (0.0, 0.0),
                y22: (0.0, 0.0),
            })
        );
        let pt = deck.cards.iter().find_map(|c| match c {
            Card::Pt(p) => Some(p.clone()),
            _ => None,
        });
        assert_eq!(
            pt,
            Some(PtCard {
                mode: 0,
                tag: 2,
                seg_first: 1,
                seg_last: 0
            })
        );
        let tl = deck.cards.iter().find_map(|c| match c {
            Card::Tl(t) => Some(t.clone()),
            _ => None,
        });
        let tl = tl.unwrap();
        assert_eq!(
            (tl.z0, tl.length, tl.velocity_factor, tl.loss_db),
            (50.0, 1.0, 0.66, 3.0)
        );
        assert!(parse("GW 1 5 0 0 0 0 0 1 .001\nGE 0\nNT 1 1 1 5 0 q\nEN\n").is_err());
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
                radials: 0,
                eps_r: Some(13.0),
                sigma: Some(0.005),
                screen: None,
                medium2: None,
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
                repeat_count: 1,
                rot_x_deg: 0.0,
                rot_y_deg: 0.0,
                rot_z_deg: 0.0,
                translate_x: 1.0,
                translate_y: 0.0,
                translate_z: 0.0,
                start_tag: 1,
            })
        );
    }

    #[test]
    fn parser_recognises_gr_card() {
        let input = "GW 1 3 0.5 0 1 0.5 0 4 0.001\nGR 1 2\nEN\n";
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
                occurrences: 2,
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
        let pt = pt.unwrap();
        assert_eq!((pt.mode, pt.tag, pt.seg_first, pt.seg_last), (-1, 0, 0, 0));
    }

    #[test]
    fn a_short_gm_card_is_accepted_with_zero_defaults() {
        // This test used to assert the opposite. nec2c accepts a GM card with
        // any number of trailing fields omitted -- measured: a bare `GM`, `GM 0`,
        // `GM 0 1` and the 6-field form all parse there, where fnec exited 1
        // demanding nine (FND-119). Absent fields default to 0, which is what
        // makes that safe: no rotation, no translation, and NRPT = 0 is an
        // in-place move.
        let result = parse("GM 0 1 0 0\nEN\n").expect("a short GM card must parse");
        let gm = result
            .deck
            .cards
            .iter()
            .find_map(|c| if let Card::Gm(gm) = c { Some(gm) } else { None })
            .expect("the GM card is present");
        assert_eq!(gm.tag_increment, 0);
        assert_eq!(gm.repeat_count, 1);
        assert_eq!(gm.translate_z, 0.0, "an absent translation is no movement");
        assert_eq!(gm.start_tag, 0, "an absent ITS is the whole structure");
    }

    #[test]
    fn a_negative_tag_increment_is_refused_by_name() {
        // nec2c accepts it and produces a NEGATIVE tag (measured: tag -9 for
        // `GM -10 1 ...`). fnec's tags are unsigned, so that structure cannot be
        // represented -- and `as u32` would saturate it to 0, silently making a
        // same-tag copy instead. Refuse, and say why.
        let Err(err) = parse("GW 1 3 0 0 -.5 0 0 .5 .001\nGM -10 1 0 0 0 1. 0 0 0\nEN\n") else {
            panic!("a negative tag increment must be refused");
        };
        let msg = err.to_string();
        assert!(msg.contains("negative tag"), "{msg}");
        assert!(
            !msg.contains("cannot parse"),
            "the deck is well-formed; the value is out of scope, not malformed: {msg}"
        );
    }

    #[test]
    fn gr_too_few_fields_is_error() {
        // GR I1 I2 (FND-247): two fields are the card; one is too few.
        assert!(matches!(
            parse("GR 1\nEN\n"),
            Err(ParseError::TooFewFields { .. })
        ));
        assert!(parse("GW 1 3 0.5 0 1 0.5 0 4 0.001\nGR 1 1\nEN\n").is_ok());
    }

    /// PAR-011: every program card is recorded with its fields as NEC reads them,
    /// in source order, including the cards fnec ignores and the fields its typed
    /// cards drop. None of the 4nec2 fixtures carries GN I2–I4 or an EK, so they
    /// are pinned here.
    #[test]
    fn program_cards_keep_every_field_nec_reads() {
        let deck = parse(
            "CM a comment\nCE\nGW 1 5 0 0 -1 0 0 1 .001\nGE 0\nEK\n\
             GN 2 0 1 0 13 .005 1.5 .002\nEX 0 1 3 0 1 0\nFR 0 1 0 0 14.2\n\
             RP 0 7 13 1003 0 0 30 30\nXQ 1\nEN\n",
        )
        .unwrap()
        .deck;
        let got: Vec<(&str, usize, [i32; 4], [f64; 6])> = deck
            .program_cards
            .iter()
            .map(|c| {
                let (i, f) = c.fields.expect("readable");
                (c.mnemonic.as_str(), c.line, i, f)
            })
            .collect();
        let zero = [0.0; 6];
        assert_eq!(
            got,
            vec![
                ("EK", 5, [0, 0, 0, 0], zero),
                ("GN", 6, [2, 0, 1, 0], [13.0, 0.005, 1.5, 0.002, 0.0, 0.0]),
                ("EX", 7, [0, 1, 3, 0], [1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
                ("FR", 8, [0, 1, 0, 0], [14.2, 0.0, 0.0, 0.0, 0.0, 0.0]),
                ("RP", 9, [0, 7, 13, 1003], [0.0, 0.0, 30.0, 30.0, 0.0, 0.0]),
                ("XQ", 10, [1, 0, 0, 0], zero),
                ("EN", 11, [0, 0, 0, 0], zero),
            ]
        );
    }

    /// An integer written as a real is read as its integer part, a separator may be
    /// a comma, and an eleventh field is ignored, as NEC reads them.
    #[test]
    fn program_card_fields_are_read_as_nec_reads_them() {
        let deck = parse(
            "GW 1 5 0 0 -1 0 0 1 .001\nGE\nEX,0.,1,3.,0,1,0,0,0,0,0,7\nFR 0 1 0 0 14.2\nEN\n",
        )
        .unwrap()
        .deck;
        assert_eq!(
            deck.program_cards[0].fields,
            Some(([0, 1, 3, 0], [1.0, 0.0, 0.0, 0.0, 0.0, 0.0]))
        );
    }

    /// A card fnec ignores is not checked, so its fields may not be numbers; the
    /// record says so instead of inventing zeros, and the deck still parses.
    #[test]
    fn an_unreadable_field_on_an_ignored_card_is_recorded_as_unreadable() {
        let deck = parse("GW 1 5 0 0 -1 0 0 1 .001\nGE\nXQ x\nEX 0 1 3 0 1\nFR 0 1 0 0 14.2\nEN\n")
            .unwrap()
            .deck;
        let xq = &deck.program_cards[0];
        assert_eq!((xq.mnemonic.as_str(), xq.fields), ("XQ", None));
        assert!(
            deck.program_cards[1].fields.is_some(),
            "EX: {:?}",
            deck.program_cards[1]
        );
    }
}
