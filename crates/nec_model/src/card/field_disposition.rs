// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! What fnec does with every field position of every card it parses, as one
//! table, [`FIELD_DISPOSITION`], held to the code by two tests.
//!
//! The class this guards is one defect that recurred four times: a field the
//! parser accepts and the solve ignores or never sees, exit 0, a different
//! antenna's answer — `LD 2`/`LD 3`'s values (FND-280), `GN`'s radial screen
//! (FND-283), `RP`'s mode (FND-285), `RP`'s XNDA digits (FND-298). Each position
//! of each card is given a [`Disposition`], with a deck and the values that
//! probe it, and
//!
//! - `crates/nec_parser/tests/card_matrix.rs` parses the deck and the same deck
//!   with one position changed: a [`Disposition::Consumed`] position must change
//!   the typed cards, a [`Disposition::Refused`] one must be refused naming the
//!   card, an ignored one must leave the cards alone (or, [`Disposition::Kept`],
//!   keep the value as written) and warn when the table says it warns — and every
//!   ignored position is listed, with the same reason, in
//!   `docs/card-support-matrix.md`'s field-disposition table;
//! - `apps/nec-cli/tests/card_field_reach.rs` runs `fnec` on both decks: a
//!   `Consumed` position must change the report's solution (the sections that
//!   are not an echo of the deck) or be refused, and a `Kept` one must leave it
//!   exactly as it was. That is what catches a field the typed card carries and
//!   the solver never reads: `RP 1` solved as `RP 0` before FND-285.
//!
//! What it does NOT catch: a field read WRONGLY. A position that changes the
//! answer passes, whatever answer it changes it to — FND-282's tag 0 read as
//! "every tag", FND-298's XNDA digits read one place off. Only the gates anchored
//! on nec2c catch those.
//!
//! Positions are the card's fields in order, 1-based, as `ParseError` numbers
//! them: a program card's I1–I4 then F1–F6 (fields 1–10), a geometry card's I1,
//! I2 then F1–F7 (fields 1–9), and TL's fnec extensions F7/F8 as fields 11 and
//! 12. Every card has all twelve, so a position cannot be left out: the array's
//! length is the type's.

/// What fnec does with one value at one field position of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The value reaches the solve: it changes the typed card and the report.
    Consumed,
    /// The value is refused at parse time, by an error naming the card.
    Refused,
    /// Read and dropped with a parse warning carrying this text; the typed card
    /// is unchanged.
    Ignored(&'static str),
    /// Read and dropped without a word, for the reason given; the typed card is
    /// unchanged. Only for what nec2c also drops, or a print layout.
    Silent(&'static str),
    /// Kept on the typed card as written — another case of the card reads it, or
    /// it is written back out — and not read by any solve in this case: the report
    /// is unchanged. `warned` says whether the parse warns, with `why`.
    Kept { why: &'static str, warned: bool },
    /// No field of NEC-2's card format: nothing is read here.
    Unused,
    /// TEMPORARY: should be [`Disposition::Consumed`] and is not — a field fnec
    /// parses and never reads, found by this guard and not yet decided. The tests
    /// list it as a failure and tolerate it; one that passes as `Consumed` fails
    /// until it is promoted.
    KnownGap(&'static str),
}

/// One value written at one position, and what fnec does with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    pub value: &'static str,
    pub disposition: Disposition,
}

/// One card in one case: the deck that probes it and every position's probes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldCase {
    /// The card's mnemonic.
    pub card: &'static str,
    /// The other fields' values this row holds for (`"any"` when none matter).
    pub case: &'static str,
    /// A deck that solves, with `{card}` where the probed card goes.
    pub deck: &'static str,
    /// The probed card as written in the baseline deck.
    pub line: &'static str,
    /// Extra `fnec` arguments the deck needs to solve (GA and GH solve on MPIE).
    pub fnec_args: &'static [&'static str],
    /// Each position's probes; none is empty.
    pub fields: [&'static [Probe]; 12],
}

/// The geometry cards, whose positions are I1, I2, F1–F7.
pub const GEOMETRY_CARDS: [&str; 9] = ["GW", "GC", "GE", "GM", "GR", "GS", "GX", "GA", "GH"];

/// A position's NEC-2 name: `I1`, `F3`, …; `field N` past the card's format.
pub fn field_name(card: &str, position: usize) -> String {
    let (ints, floats) = if GEOMETRY_CARDS.contains(&card) {
        (2, 7)
    } else if card == "TL" {
        (4, 8)
    } else {
        (4, 6)
    };
    if position <= ints {
        format!("I{position}")
    } else if position <= ints + floats {
        format!("F{}", position - ints)
    } else {
        format!("field {position}")
    }
}

/// The deck with `line` written in place of `{card}`.
pub fn deck_with(case: &FieldCase, line: &str) -> String {
    case.deck.replace("{card}", line)
}

/// The probed card with `value` at `position` (1-based), missing fields
/// before it written as `0`.
pub fn perturbed_line(line: &str, position: usize, value: &str) -> String {
    let mut tokens: Vec<String> = line.split_whitespace().map(str::to_string).collect();
    while tokens.len() <= position {
        tokens.push("0".to_string());
    }
    tokens[position] = value.to_string();
    tokens.join(" ")
}

const fn p(value: &'static str, disposition: Disposition) -> Probe {
    Probe { value, disposition }
}

const fn kept(why: &'static str) -> Disposition {
    Disposition::Kept { why, warned: false }
}

const fn kept_warned(why: &'static str) -> Disposition {
    Disposition::Kept { why, warned: true }
}

use Disposition::{Consumed as C, Ignored, KnownGap, Refused, Silent, Unused};

const UNUSED: &[Probe] = &[p("1", Unused)];

// The decks. Each solves on main; the probed card goes at `{card}`.
const DIPOLE_WITH: &str = "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\nGE 0\n{card}\n\
                           EX 0 1 6 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
const WIRE: &str = "CM probe\nCE\n{card}\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
const WIRE_THEN: &str = "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\n{card}\nGE 0\n\
                         EX 0 1 6 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
const SOURCE: &str = "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\nGE 0\n{card}\n\
                      FR 0 1 0 0 29.9 0\nEN\n";
const TILTED_SOURCE: &str = "CM probe\nCE\nGW 1 11 0 0 -2.5 1 0 2.4 0.001\nGE 0\n{card}\n\
                             FR 0 1 0 0 29.9 0\nEN\n";
const LOADED_PATTERN: &str = "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\nGE 0\n\
                              EX 0 1 6 0 1 0\nLD 4 1 3 3 50 0\nFR 0 1 0 0 29.9 0\n{card}\nEN\n";
const OVER_GROUND: &str = "CM probe\nCE\nGW 1 11 0 0 5 0 0 10 0.001\nGE 0\n{card}\n\
                           EX 0 1 6 0 1 0\nFR 0 1 0 0 29.9 0\nRP 0 2 1 1000 45 0 30 0\nEN\n";
const TWO_WIRES: &str = "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\n\
                         GW 2 11 1 0 -2.5 1 0 2.5 0.001\nGE 0\nEX 0 1 6 0 1 0\n{card}\n\
                         FR 0 1 0 0 29.9 0\nEN\n";
const NEAR: &str = "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\nGE 0\nEX 0 1 6 0 1 0\n\
                    FR 0 1 0 0 29.9 0\n{card}\nEN\n";

const GN_FINITE_I34: &str = "NEC-2 never reads GN I3/I4";
const GN_FINITE_F36: &str = "GN F3–F6 (a second medium or a cliff, NEC-2's GD parameters) \
                             only enter RP modes 2, 3, 5 and 6, which fnec refuses";
const GN_IDEAL: &str = "I2–F6 are ignored, as nec2c ignores them";
const EX_PRINT: &str = "FND-pending: a voltage source's I4 units digit asks nec2c for the input \
                        impedance normalized to F3 (its INPUT IMPEDANCE DATA table); fnec keeps \
                        the value and drops the request without the warning PT I1 = 1..3 and \
                        RP's N digit give";
const EX_I4_TENS: &str = "I4's tens digit, NEC-2's admittance-matrix asymmetry print, which nec2c \
                          1.3.1 ignores too";
const EX_SOURCE_F3: &str = "a voltage source's F3 is nec2c's impedance-normalization constant, \
                            read only for I4's print request";
const EX_SOURCE_F46: &str = "NEC-2 leaves F4–F6 blank for a voltage source";
const EX_LINEAR_F6: &str = "the axial ratio; NEC-2 reads it for the elliptic types 2 and 3 only";
const LD_IMPEDANCE_F3: &str = "NEC-2 reads no F3 for a series impedance";
const LD_CONDUCTIVITY_F23: &str = "NEC-2 reads only F1, the conductivity, for a wire conductivity";
const LD_WHOLE_TAG_I4: &str = "with I3 = 0 the whole tag is loaded and I4 is not read, as nec2c \
                               reads it";
const EX_WAVE_I4_UNITS: &str = "I4's units digit is read for a voltage source only";

/// Every card fnec parses, in every case where a position's disposition
/// depends on another field's value.
pub const FIELD_DISPOSITION: &[FieldCase] = &[
    // ---- geometry ----
    FieldCase {
        card: "GW",
        case: "any",
        deck: WIRE,
        line: "GW 1 11 0 0 -2.5 0 0 2.5 0.001",
        fnec_args: &[],
        fields: [
            &[p("2", C)],
            &[p("13", C)],
            &[p("0.1", C)],
            &[p("0.1", C)],
            &[p("-2.4", C)],
            &[p("0.1", C)],
            &[p("0.1", C)],
            &[p("2.4", C)],
            &[p("0.002", C)],
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GC",
        case: "after a GW of radius 0",
        deck: "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0\n{card}\nGE 0\nEX 0 1 6 0 1 0\n\
               FR 0 1 0 0 29.9 0\nEN\n",
        line: "GC 0 0 1 0.001 0.002",
        fnec_args: &[],
        fields: [
            UNUSED,
            UNUSED,
            &[p("1.05", C)],
            &[p("0.0015", C)],
            &[p("0.003", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GE",
        case: "any",
        deck: "CM probe\nCE\nGW 1 10 0 0 0 0 0 2.5 0.001\n{card}\nGN 1\nEX 0 1 1 0 1 0\n\
               FR 0 1 0 0 29.9 0\nEN\n",
        line: "GE 1",
        fnec_args: &[],
        fields: [
            &[p("0", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GM",
        case: "any",
        deck: "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\nGW 2 11 1 0 -2.5 1 0 2.5 0.001\n\
               {card}\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n",
        line: "GM 10 1 0 0 0 0 3 0 2",
        fnec_args: &[],
        fields: [
            &[p("20", C)],
            &[p("2", C)],
            &[p("10", C)],
            &[p("10", C)],
            &[p("10", C)],
            &[p("0.5", C)],
            &[p("4", C)],
            &[p("0.5", C)],
            &[p("1", C)],
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GR",
        case: "any",
        deck: "CM probe\nCE\nGW 1 11 2 0 -2.5 2 0 2.5 0.001\n{card}\nGE 0\nEX 0 1 6 0 1 0\n\
               FR 0 1 0 0 29.9 0\nEN\n",
        line: "GR 10 2",
        fnec_args: &[],
        fields: [
            &[p("20", C)],
            &[p("3", C)],
            // NEC-2's GR has no angle: 360/I2 means the same either way, any other
            // value is fnec's pre-0.23 reading, refused (FND-247).
            &[p("17", Refused), p("180", Unused)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GS",
        case: "any",
        deck: WIRE_THEN,
        line: "GS 0 0 1.1",
        fnec_args: &[],
        fields: [
            UNUSED,
            UNUSED,
            &[p("1.2", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GX",
        case: "any",
        deck: "CM probe\nCE\nGW 1 11 1 0.5 -2.5 1 0.5 2.5 0.001\n{card}\nGE 0\n\
               EX 0 1 6 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n",
        line: "GX 10 100",
        fnec_args: &[],
        fields: [
            &[p("20", C)],
            &[p("10", C), p("110", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GA",
        case: "any",
        deck: "CM probe\nCE\n{card}\nGE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n",
        line: "GA 1 9 1.6 0 180 0.001",
        fnec_args: &["--solver", "mpie"],
        fields: [
            &[p("2", C)],
            &[p("11", C)],
            &[p("1.5", C)],
            &[p("10", C)],
            &[p("170", C)],
            &[p("0.002", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GH",
        case: "uniform (A2 = A1)",
        deck: "CM probe\nCE\n{card}\nGE 0\nEX 0 1 10 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n",
        line: "GH 1 20 0.5 2 0.1 0.1 0.1 0 0.001",
        fnec_args: &["--solver", "mpie"],
        fields: [
            &[p("2", C)],
            &[p("22", C)],
            &[p("0.4", C)],
            &[p("2.2", C)],
            &[p("0.12", C)],
            &[p("0.12", C)],
            &[p("0.12", C)],
            // nec2c ignores B2 when A2 = A1; NEC-2 defines it: refused.
            &[p("0.12", Refused)],
            &[p("0.002", C)],
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GH",
        case: "tapered (A2 ≠ A1)",
        deck: "CM probe\nCE\n{card}\nGE 0\nEX 0 1 10 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n",
        line: "GH 1 20 0.5 2 0.1 0.1 0.15 0.15 0.001",
        fnec_args: &["--solver", "mpie"],
        fields: [
            &[p("2", C)],
            &[p("22", C)],
            &[p("0.4", C)],
            &[p("2.2", C)],
            &[p("0.12", C)],
            &[p("0.12", C)],
            &[p("0.12", C)],
            &[p("0.12", C)],
            &[p("0.002", C)],
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    // ---- ground ----
    FieldCase {
        card: "GN",
        case: "I1 = 0, I2 = 0",
        deck: OVER_GROUND,
        line: "GN 0 0 0 0 13 0.005 0 0 0 0",
        fnec_args: &[],
        fields: [
            &[p("1", C)],
            &[p("16", Refused)],
            &[p("1", Ignored(GN_FINITE_I34))],
            &[p("1", Ignored(GN_FINITE_I34))],
            &[p("20", C)],
            &[p("0.05", C)],
            &[p("5", kept_warned(GN_FINITE_F36))],
            &[p("0.01", kept_warned(GN_FINITE_F36))],
            &[p("10", kept_warned(GN_FINITE_F36))],
            &[p("1", kept_warned(GN_FINITE_F36))],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GN",
        case: "I1 = 2, I2 = 0",
        deck: OVER_GROUND,
        line: "GN 2 0 0 0 13 0.005 0 0 0 0",
        fnec_args: &[],
        fields: [
            &[p("1", C)],
            &[p("16", Refused)],
            &[p("1", Ignored(GN_FINITE_I34))],
            &[p("1", Ignored(GN_FINITE_I34))],
            &[p("20", C)],
            &[p("0.05", C)],
            &[p("5", kept_warned(GN_FINITE_F36))],
            &[p("0.01", kept_warned(GN_FINITE_F36))],
            &[p("10", kept_warned(GN_FINITE_F36))],
            &[p("1", kept_warned(GN_FINITE_F36))],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GN",
        case: "I1 = 1",
        deck: OVER_GROUND,
        line: "GN 1 0 0 0 0 0 0 0 0 0",
        fnec_args: &[],
        fields: [
            &[p("0", C)],
            &[p("16", kept_warned(GN_IDEAL))],
            &[p("1", Ignored(GN_IDEAL))],
            &[p("1", Ignored(GN_IDEAL))],
            &[p("13", kept_warned(GN_IDEAL))],
            &[p("0.005", kept_warned(GN_IDEAL))],
            &[p("5", kept_warned(GN_IDEAL))],
            &[p("0.01", kept_warned(GN_IDEAL))],
            &[p("10", kept_warned(GN_IDEAL))],
            &[p("1", kept_warned(GN_IDEAL))],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GN",
        case: "I1 = 1, I2 ≠ 0",
        deck: OVER_GROUND,
        line: "GN 1 4 0 0 0 0 10 0.001 0 0",
        fnec_args: &[],
        fields: [
            &[p("0", Refused)],
            &[p("16", kept_warned(GN_IDEAL))],
            &[p("1", Ignored(GN_IDEAL))],
            &[p("1", Ignored(GN_IDEAL))],
            &[p("13", kept_warned(GN_IDEAL))],
            &[p("0.005", kept_warned(GN_IDEAL))],
            &[p("12", kept_warned(GN_IDEAL))],
            &[p("0.002", kept_warned(GN_IDEAL))],
            &[p("10", Ignored(GN_IDEAL))],
            &[p("1", Ignored(GN_IDEAL))],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "GN",
        case: "I1 = -1",
        deck: OVER_GROUND,
        line: "GN -1 0 0 0 0 0 0 0 0 0",
        fnec_args: &[],
        fields: [
            &[p("1", C)],
            &[p("16", kept_warned(GN_IDEAL))],
            &[p("1", Ignored(GN_IDEAL))],
            &[p("1", Ignored(GN_IDEAL))],
            &[p("13", kept_warned(GN_IDEAL))],
            &[p("0.005", kept_warned(GN_IDEAL))],
            &[p("5", kept_warned(GN_IDEAL))],
            &[p("0.01", kept_warned(GN_IDEAL))],
            &[p("10", kept_warned(GN_IDEAL))],
            &[p("1", kept_warned(GN_IDEAL))],
            UNUSED,
            UNUSED,
        ],
    },
    // ---- excitation ----
    FieldCase {
        card: "EX",
        case: "I1 = 0 (voltage source)",
        deck: SOURCE,
        line: "EX 0 1 6 0 1 0 0 0 0 0",
        fnec_args: &[],
        fields: [
            &[p("4", C)],
            &[p("2", C)],
            &[p("4", C)],
            &[p("1", KnownGap(EX_PRINT)), p("10", kept(EX_I4_TENS))],
            &[p("2", C)],
            &[p("1", C)],
            &[p("50", kept(EX_SOURCE_F3))],
            &[p("5", kept(EX_SOURCE_F46))],
            &[p("5", kept(EX_SOURCE_F46))],
            &[p("0.5", kept(EX_SOURCE_F46))],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "EX",
        case: "I1 = 1 (linear plane wave)",
        deck: TILTED_SOURCE,
        line: "EX 1 2 2 0 90 0 0 10 10 0",
        fnec_args: &[],
        fields: [
            &[p("0", C)],
            &[p("3", C)],
            &[p("3", C)],
            &[p("10", kept(EX_I4_TENS)), p("1", kept(EX_WAVE_I4_UNITS))],
            &[p("80", C)],
            &[p("10", C)],
            &[p("30", C)],
            &[p("5", C)],
            &[p("5", C)],
            &[p("0.5", kept(EX_LINEAR_F6))],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "EX",
        case: "I1 = 2 (elliptic plane wave)",
        deck: TILTED_SOURCE,
        // phi = 45: a wave from phi = 0 has its phi component across this wire in
        // the xz plane, so the handedness (I1 = 2 or 3) cannot show, in nec2c too.
        line: "EX 2 2 2 0 90 45 0 10 10 0.5",
        fnec_args: &[],
        fields: [
            &[p("3", C)],
            &[p("3", C)],
            &[p("3", C)],
            &[p("10", kept(EX_I4_TENS)), p("1", kept(EX_WAVE_I4_UNITS))],
            &[p("80", C)],
            &[p("10", C)],
            &[p("30", C)],
            &[p("5", C)],
            &[p("5", C)],
            &[p("0.3", C)],
            UNUSED,
            UNUSED,
        ],
    },
    // ---- program control ----
    FieldCase {
        card: "FR",
        case: "any",
        deck: "CM probe\nCE\nGW 1 11 0 0 -2.5 0 0 2.5 0.001\nGE 0\nEX 0 1 6 0 1 0\n{card}\nEN\n",
        line: "FR 0 2 0 0 29.9 1.1",
        fnec_args: &[],
        fields: [
            &[p("1", C)],
            &[p("3", C)],
            UNUSED,
            UNUSED,
            &[p("28", C)],
            &[p("1.2", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "RP",
        case: "I1 = 0, XNDA's N = 0",
        deck: LOADED_PATTERN,
        line: "RP 0 3 2 1000 0 0 30 90 0 0",
        fnec_args: &[],
        fields: [
            &[p("1", Refused), p("4", Refused)],
            &[p("4", C)],
            &[p("3", C)],
            &[
                p("1100", C),
                p("1010", C),
                p("1001", C),
                p(
                    "0",
                    Silent(
                        "XNDA's X digit picks nec2c's column labels; fnec's table has one \
                         layout, and the NEC-2D output honours it",
                    ),
                ),
            ],
            &[p("10", C)],
            &[p("10", C)],
            &[p("20", C)],
            &[p("45", C)],
            &[p(
                "1000",
                Ignored("asks nec2c to print the field at that range"),
            )],
            &[p(
                "3",
                Silent("GNOR is read only under XNDA's N digit, as nec2c reads it"),
            )],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "RP",
        case: "I1 = 0, XNDA's N ≠ 0",
        deck: LOADED_PATTERN,
        line: "RP 0 3 2 1100 0 0 30 90 0 0",
        fnec_args: &[],
        fields: [
            &[p("1", Refused)],
            &[p("4", C)],
            &[p("3", C)],
            &[
                p("1000", C),
                p(
                    "1200",
                    Ignored("asks nec2c to normalize one gain component"),
                ),
            ],
            &[p("10", C)],
            &[p("10", C)],
            &[p("20", C)],
            &[p("45", C)],
            &[p(
                "1000",
                Ignored("asks nec2c to print the field at that range"),
            )],
            &[p("3", Ignored("asks nec2c to normalize to that gain"))],
            UNUSED,
            UNUSED,
        ],
    },
    // ---- loads ----
    FieldCase {
        card: "LD",
        case: "I1 = 0",
        deck: DIPOLE_WITH,
        line: "LD 0 1 3 4 50 1e-7 1e-10",
        fnec_args: &[],
        fields: [
            &[p("1", C)],
            &[p("2", C)],
            &[p("2", C)],
            &[p("5", C)],
            &[p("60", C)],
            &[p("2e-7", C)],
            &[p("2e-10", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "LD",
        case: "I1 = 1",
        deck: DIPOLE_WITH,
        line: "LD 1 1 3 4 50 1e-7 1e-10",
        fnec_args: &[],
        fields: [
            &[p("0", C)],
            &[p("2", C)],
            &[p("2", C)],
            &[p("5", C)],
            &[p("60", C)],
            &[p("2e-7", C)],
            &[p("2e-10", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "LD",
        case: "I1 = 2",
        deck: DIPOLE_WITH,
        line: "LD 2 1 3 4 50 1e-7 1e-10",
        fnec_args: &[],
        fields: [
            &[p("3", C)],
            &[p("2", C)],
            &[p("2", C)],
            &[p("5", C)],
            &[p("60", C)],
            &[p("2e-7", C)],
            &[p("2e-10", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "LD",
        case: "I1 = 3",
        deck: DIPOLE_WITH,
        line: "LD 3 1 3 4 50 1e-7 1e-10",
        fnec_args: &[],
        fields: [
            &[p("2", C)],
            &[p("2", C)],
            &[p("2", C)],
            &[p("5", C)],
            &[p("60", C)],
            &[p("2e-7", C)],
            &[p("2e-10", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "LD",
        case: "I1 = 4",
        deck: DIPOLE_WITH,
        line: "LD 4 1 3 4 50 20 0",
        fnec_args: &[],
        fields: [
            &[p("0", C)],
            &[p("2", C)],
            &[p("2", C)],
            &[p("5", C)],
            &[p("60", C)],
            &[p("30", C)],
            &[p("1e-10", kept(LD_IMPEDANCE_F3))],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "LD",
        case: "I1 = 5",
        deck: DIPOLE_WITH,
        line: "LD 5 1 3 4 1e6 0 0",
        fnec_args: &[],
        fields: [
            &[p("4", C)],
            &[p("2", C)],
            &[p("2", C)],
            &[p("5", C)],
            &[p("1e5", C)],
            &[p("1e-7", kept(LD_CONDUCTIVITY_F23))],
            &[p("1e-10", kept(LD_CONDUCTIVITY_F23))],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "LD",
        case: "I1 = 4, I3 = 0 (the whole tag)",
        deck: DIPOLE_WITH,
        line: "LD 4 1 0 0 50 20 0",
        fnec_args: &[],
        fields: [
            &[p("0", C)],
            &[p("2", C)],
            &[p("2", C)],
            &[p("5", kept(LD_WHOLE_TAG_I4))],
            &[p("60", C)],
            &[p("30", C)],
            &[p("1e-10", kept(LD_IMPEDANCE_F3))],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    // ---- networks ----
    FieldCase {
        card: "TL",
        case: "any",
        deck: TWO_WIRES,
        line: "TL 1 6 2 6 50. 0.5 0 0 0 0 1 0",
        fnec_args: &[],
        fields: [
            &[p("2", C)],
            &[p("4", C)],
            &[p("1", C)],
            &[p("4", C)],
            &[p("75", C)],
            &[p("0.7", C)],
            &[p("0.01", C)],
            &[p("0.01", C)],
            &[p("0.01", C)],
            &[p("0.01", C)],
            &[p("0.8", C)],
            &[p("1", C)],
        ],
    },
    FieldCase {
        card: "NT",
        case: "any",
        deck: TWO_WIRES,
        line: "NT 1 6 2 6 0 -0.02 0 0.02 0 -0.02",
        fnec_args: &[],
        fields: [
            &[p("2", C)],
            &[p("4", C)],
            &[p("1", C)],
            &[p("4", C)],
            &[p("0.001", C)],
            &[p("-0.03", C)],
            &[p("0.001", C)],
            &[p("0.03", C)],
            &[p("0.001", C)],
            &[p("-0.03", C)],
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "PT",
        case: "I3 ≠ 0",
        deck: TWO_WIRES,
        line: "PT 0 1 2 4",
        fnec_args: &[],
        fields: [
            &[
                p("-1", C),
                p(
                    "1",
                    kept_warned("selects one of NEC-2's receive-pattern print formats"),
                ),
            ],
            &[p("2", C)],
            &[p("3", C)],
            &[p("5", C)],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    FieldCase {
        card: "PT",
        case: "I3 = 0",
        deck: TWO_WIRES,
        line: "PT 0 1 0 0",
        fnec_args: &[],
        fields: [
            &[p("-1", C)],
            &[p(
                "2",
                kept("with I3 = 0 every segment is printed, whatever I2 and I4 say"),
            )],
            &[p("2", C)],
            &[p(
                "5",
                kept("with I3 = 0 every segment is printed, whatever I2 and I4 say"),
            )],
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ],
    },
    // ---- near fields ----
    FieldCase {
        card: "NE",
        case: "any",
        deck: NEAR,
        line: "NE 0 2 2 2 1 0 0 0.5 0.5 0.5",
        fnec_args: &[],
        fields: NEAR_FIELDS,
    },
    FieldCase {
        card: "NH",
        case: "any",
        deck: NEAR,
        line: "NH 0 2 2 2 1 0 0 0.5 0.5 0.5",
        fnec_args: &[],
        fields: NEAR_FIELDS,
    },
];

const NEAR_FIELDS: [&[Probe]; 12] = [
    &[p("1", C)],
    &[p("3", C)],
    &[p("3", C)],
    &[p("3", C)],
    &[p("2", C)],
    &[p("0.5", C)],
    &[p("0.5", C)],
    &[p("0.7", C)],
    &[p("0.7", C)],
    &[p("0.7", C)],
    UNUSED,
    UNUSED,
];
