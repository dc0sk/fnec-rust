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
//! skipped with a warning, and the deck solved as a different antenna: a `GS`
//! scale card ignored gave 290.5 + j218.5 Ω where nec2c gives 84.8 + j45.8.

pub mod template;

use nec_model::card::{
    Card, CommentCard, EnCard, ExCard, FrCard, GeCard, GmCard, GnCard, GrCard, GsCard, GwCard,
    LdCard, NearFieldCard, NtCard, PtCard, RpCard, TlCard,
};
use nec_model::deck::NecDeck;

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
    ("GA", OtherCard::Refused("a wire arc")),
    ("GH", OtherCard::Refused("a helix or spiral")),
    (
        "GX",
        OtherCard::Refused("a reflected copy of the structure"),
    ),
    ("GC", OtherCard::Refused("a tapered wire")),
    (
        "GF",
        OtherCard::Refused("a structure read from a Numerical Green's Function file"),
    ),
    (
        "GD",
        OtherCard::Refused("additional ground parameters: a cliff or a second medium"),
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

    for (idx, raw_line) in input.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw_line.trim();

        // Skip blank lines and lines that are entirely whitespace.
        if line.is_empty() {
            continue;
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
            if matches!(upper.as_str(), "GW" | "GM" | "GR" | "GS" | "GE") {
                return Err(ParseError::GeometryAfterGe {
                    line: lineno,
                    mnemonic: upper,
                    ge_line,
                });
            }
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
                    radius: parse_radius(lineno, fields[8])?,
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

                let ground_type = parse_i32(lineno, "GN", 1, fields[0])?;
                // FND-241: NEC-2 defines -1..=2. nec2c solves another type
                // inconsistently: the currents over a perfect ground, the pattern
                // over a finite one. fnec solved free space with a warning.
                if !(-1..=2).contains(&ground_type) {
                    return Err(ParseError::UnsupportedValue {
                        line: lineno,
                        card: "GN".to_string(),
                        field: 1,
                        raw: fields[0].to_string(),
                        reason: "not a NEC-2 ground type (-1 none, 0 finite by reflection \
                                 coefficient, 1 perfect, 2 finite by Sommerfeld); nec2c solves \
                                 it inconsistently (perfect-ground currents under a finite-ground \
                                 pattern), so the deck is refused"
                            .to_string(),
                    });
                }
                last_gn_type = Some(ground_type);
                deck.cards.push(Card::Gn(GnCard {
                    ground_type,
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
                // With >= 8 fields the four angle floats start at index 4 (after
                // XNDA); with 7 they start at index 3. The XNDA `X` (thousands)
                // digit requests normalized-gain output; the `A` (units) digit
                // requests the average power gain.
                let a = if fields.len() >= 8 { 4 } else { 3 };
                let (normalize, avg_power_gain) = if fields.len() >= 8 {
                    let xnda = parse_u32(lineno, "RP", 4, fields[3])?;
                    ((xnda / 1000) % 10 != 0, xnda % 10 != 0)
                } else {
                    (false, false)
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
                    avg_power_gain,
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
                // NEC-2: TL I1 I2 I3 I4 F1 F2 F3 F4 F5 F6, plus fnec's optional F7
                // (velocity factor) and F8 (matched-line loss, dB). See TlCard.
                let fields = parse_fields(rest);
                require_fields(lineno, "TL", &fields, 5)?;
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
                let float_at = |i: usize, default: f64| -> Result<f64, ParseError> {
                    match fields.get(i) {
                        Some(raw) => parse_f64(lineno, "TL", i + 1, raw),
                        None => Ok(default),
                    }
                };
                deck.cards.push(Card::Tl(TlCard {
                    tag1: parse_u32(lineno, "TL", 1, fields[0])?,
                    segment1: parse_u32(lineno, "TL", 2, fields[1])?,
                    tag2: parse_u32(lineno, "TL", 3, fields[2])?,
                    segment2: parse_u32(lineno, "TL", 4, fields[3])?,
                    z0: float_at(4, 0.0)?,
                    length: float_at(5, 0.0)?,
                    shunt1: (float_at(6, 0.0)?, float_at(7, 0.0)?),
                    shunt2: (float_at(8, 0.0)?, float_at(9, 0.0)?),
                    velocity_factor: float_at(10, 1.0)?,
                    loss_db: float_at(11, 0.0)?,
                }));
            }
            "NT" => {
                // NT — network definition card. Raw fields are captured here;
                // the solver stamps the supported two-port forms into the MoM
                // matrix (build_nt_stamps) and warns on the rest.
                let fields = parse_fields(rest);
                deck.cards.push(Card::Nt(NtCard {
                    raw_fields: fields
                        .into_iter()
                        .map(std::string::ToString::to_string)
                        .collect(),
                }));
            }
            "PT" => {
                // PT — print-control card. Raw fields are captured here; the
                // solver applies the supported current-print-control subset to the
                // CURRENTS table (apply_pt_current_filter, PH9-CHK-004).
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

/// A GW radius: positive and finite. Zero is NEC's tapered wire, whose radii a
/// GC card gives and fnec does not model; it used to reach the solve and end as
/// "a current that is not a finite number" (FND-236).
fn parse_radius(line: usize, raw: &str) -> Result<f64, ParseError> {
    let r = parse_f64(line, "GW", 9, raw)?;
    if r > 0.0 && r.is_finite() {
        return Ok(r);
    }
    Err(ParseError::UnsupportedValue {
        line,
        card: "GW".to_string(),
        field: 9,
        raw: raw.to_string(),
        reason: if r == 0.0 {
            "a zero radius is NEC's tapered wire (its radii on a GC card), which fnec does not \
             model; give the wire its radius"
                .to_string()
        } else {
            "a wire radius must be positive".to_string()
        },
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
    // Accept floats like "1.0" that 4nec2 sometimes emits for integer fields.
    parse_finite_f64_field(lineno, card, field, s).map(|v| v as u32)
}

fn parse_i32(lineno: usize, card: &str, field: usize, s: &str) -> Result<i32, ParseError> {
    parse_finite_f64_field(lineno, card, field, s).map(|v| v as i32)
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
        let deck = "GW 1 21 0 0 -8.2 0 0 8.2 .04\nGX 1 100\nGE 0\nEN\n";
        let err = parse(deck).err().expect("GX is refused");
        assert!(
            matches!(&err, ParseError::RefusedCard { line: 2, mnemonic, .. } if mnemonic == "GX"),
            "{err:?}"
        );
        assert!(
            err.to_string().contains("reflected copy of the structure"),
            "{err}"
        );
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
        let input = "GR 1 1\nEN\n";
        assert!(parse(input).is_err());
    }
}
