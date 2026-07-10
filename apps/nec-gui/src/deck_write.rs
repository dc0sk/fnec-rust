// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! NEC deck **writer** (GUI-CHK-007).
//!
//! The workspace has a parser (`nec_parser`) but no serializer — the visual
//! editors need to turn an edited [`NecDeck`] back into deck text to preview and
//! save it. This module is that missing half: [`write_deck`] renders a deck to a
//! canonical NEC card stream.
//!
//! **Correctness oracle:** the writer is exact iff `parse(write_deck(&d)).deck`
//! equals `d` for every deck the parser accepts. That round-trip is unit-tested
//! here against the whole `corpus/` set, so any card whose written form the
//! parser would read back differently is a test failure, not a silent drift.
//!
//! Float fields are emitted with Rust's default `f64` `Display`, which is the
//! shortest decimal string that round-trips to the same bits — so coordinates
//! and radii survive parse→write→parse unchanged.

use nec_model::card::{Card, NeCard};
use nec_model::deck::NecDeck;

/// Serialize a whole deck to NEC card text (one card per line, `\n`-terminated).
///
/// The output re-parses (via `nec_parser::parse`) to a deck equal to the input
/// for any parser-accepted deck; see the module-level round-trip tests.
pub fn write_deck(deck: &NecDeck) -> String {
    let mut out = String::new();
    for card in &deck.cards {
        out.push_str(&write_card(card));
        out.push('\n');
    }
    out
}

/// Serialize a single card to its NEC line (no trailing newline).
pub fn write_card(card: &Card) -> String {
    match card {
        // Comments: we cannot tell an original `CE` from a `CM` after parsing
        // (both become `Card::Comment`), so every comment is written as `CM`.
        // The parser reads it straight back, so the round-trip is exact.
        Card::Comment(c) => join(&["CM".into(), c.text.clone()]),
        Card::Gw(c) => join(&[
            "GW".into(),
            u(c.tag),
            u(c.segments),
            f(c.start[0]),
            f(c.start[1]),
            f(c.start[2]),
            f(c.end[0]),
            f(c.end[1]),
            f(c.end[2]),
            f(c.radius),
        ]),
        Card::Gm(c) => join(&[
            "GM".into(),
            u(c.tag_increment),
            u(c.last_tag),
            f(c.rot_x_deg),
            f(c.rot_y_deg),
            f(c.rot_z_deg),
            f(c.translate_x),
            f(c.translate_y),
            f(c.translate_z),
            u(c.first_tag),
        ]),
        Card::Gr(c) => join(&["GR".into(), u(c.tag_increment), u(c.count), f(c.angle_deg)]),
        Card::Ge(c) => join(&["GE".into(), i(c.ground_reflection_flag)]),
        Card::Gn(c) => {
            // Bare form `GN <type>` when no medium is given; otherwise the full
            // `GN <type> 0 0 0 <eps> <sig>` (the parser reads EPSE at field 5,
            // SIG at field 6, so the three placeholder integers are required).
            match (c.eps_r, c.sigma) {
                (None, None) => join(&["GN".into(), i(c.ground_type)]),
                (eps, sig) => join(&[
                    "GN".into(),
                    i(c.ground_type),
                    "0".into(),
                    "0".into(),
                    "0".into(),
                    f(eps.unwrap_or(0.0)),
                    f(sig.unwrap_or(0.0)),
                ]),
            }
        }
        Card::Ld(c) => trim_floats(
            vec![
                "LD".into(),
                i(c.load_type),
                u(c.tag),
                u(c.seg_first),
                u(c.seg_last),
                f(c.f1),
                f(c.f2),
                f(c.f3),
            ],
            5, // keep mnemonic + 4 required integer fields
        ),
        Card::Tl(c) => join(&[
            "TL".into(),
            u(c.tag1),
            u(c.segment1),
            u(c.tag2),
            u(c.segment2),
            u(c.num_segments),
            u(c.tl_type),
            f(c.z0),
            f(c.length),
            f(c.f3),
        ]),
        Card::Pt(c) => join_raw("PT", &c.raw_fields),
        Card::Nt(c) => join_raw("NT", &c.raw_fields),
        Card::Ex(c) => trim_floats(
            vec![
                "EX".into(),
                u(c.excitation_type),
                u(c.tag),
                u(c.segment),
                u(c.i4),
                f(c.voltage_real),
                f(c.voltage_imag),
                f(c.polarization_deg),
                f(c.theta_inc),
                f(c.phi_inc),
                f(c.polarization_ratio),
            ],
            5, // keep mnemonic + 4 required integer fields
        ),
        // Shorthand 4-field form `FR I1 I2 F1 F2`; the parser reads the frequency
        // from field 3 when there are fewer than 6 fields, so this round-trips.
        Card::Fr(c) => join(&[
            "FR".into(),
            u(c.step_type),
            u(c.steps),
            f(c.frequency_mhz),
            f(c.step_mhz),
        ]),
        Card::Rp(c) => {
            // Encode the normalize/avg-power flags back into the XNDA field the
            // parser decodes: X (thousands) = normalize, A (units) = avg power.
            let xnda = u32::from(c.normalize) * 1000 + u32::from(c.avg_power_gain);
            join(&[
                "RP".into(),
                u(c.mode),
                u(c.n_theta),
                u(c.n_phi),
                u(xnda),
                f(c.theta0),
                f(c.phi0),
                f(c.d_theta),
                f(c.d_phi),
            ])
        }
        Card::Ne(c) => write_near("NE", c),
        Card::Nh(c) => write_near("NH", c),
        Card::En(_) => "EN".to_string(),
    }
}

/// NE/NH share an identical 10-field layout.
fn write_near(mnemonic: &str, c: &NeCard) -> String {
    join(&[
        mnemonic.to_string(),
        u(c.coord_type),
        u(c.nx),
        u(c.ny),
        u(c.nz),
        f(c.x0),
        f(c.y0),
        f(c.z0),
        f(c.dx),
        f(c.dy),
        f(c.dz),
    ])
}

// ── field formatting ─────────────────────────────────────────────────────────

fn u(v: u32) -> String {
    v.to_string()
}

fn i(v: i32) -> String {
    v.to_string()
}

/// Format a float with the shortest round-tripping decimal (Rust default), but
/// render integral values without a trailing `.0` so `14` stays `14`, not `14`
/// vs `14.0` churn (both parse identically, this is purely cosmetic).
fn f(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn join(fields: &[String]) -> String {
    fields
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

fn join_raw(mnemonic: &str, raw_fields: &[String]) -> String {
    let mut v = vec![mnemonic.to_string()];
    v.extend(raw_fields.iter().cloned());
    v.join(" ")
}

/// Join, then drop trailing fields that are the float literal `0` down to a
/// minimum length. Used for EX/LD, whose trailing floats default to 0.0 in the
/// parser, so omitting them round-trips while keeping the line uncluttered.
fn trim_floats(mut fields: Vec<String>, min_len: usize) -> String {
    while fields.len() > min_len && fields.last().map(String::as_str) == Some("0") {
        fields.pop();
    }
    join(&fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_parser::parse;
    use std::fs;
    use std::path::PathBuf;

    fn corpus_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
    }

    /// The core oracle: for every corpus deck the parser accepts, parsing the
    /// writer's output reproduces the original card list exactly.
    #[test]
    fn corpus_round_trips_parse_write_parse() {
        let dir = corpus_dir();
        let mut checked = 0usize;
        for entry in fs::read_dir(&dir).expect("read corpus dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("nec") {
                continue;
            }
            let text = fs::read_to_string(&path).expect("read deck");
            // Only decks the parser fully accepts (no unknown cards) are a fair
            // oracle — an unknown card is dropped on the way in and can't be
            // reproduced. All current corpus decks parse cleanly.
            let Ok(first) = parse(&text) else { continue };
            if !first.warnings.is_empty() {
                continue;
            }
            let written = write_deck(&first.deck);
            let second = parse(&written)
                .unwrap_or_else(|e| panic!("re-parse of written {} failed: {e}", path.display()));
            assert_eq!(
                first.deck.cards,
                second.deck.cards,
                "round-trip mismatch for {}\n--- written ---\n{written}",
                path.display()
            );
            checked += 1;
        }
        assert!(
            checked > 20,
            "expected many corpus decks, checked {checked}"
        );
    }

    #[test]
    fn gw_line_is_canonical() {
        let deck = parse("GW 1 51 0 0 -5.232 0 0 5.232 0.001\nGE 0\nEN\n")
            .unwrap()
            .deck;
        let s = write_card(&deck.cards[0]);
        assert_eq!(s, "GW 1 51 0 0 -5.232 0 0 5.232 0.001");
    }

    #[test]
    fn gn_bare_vs_medium() {
        let bare = parse("GN 1\nEN\n").unwrap().deck;
        assert_eq!(write_card(&bare.cards[0]), "GN 1");
        let med = parse("GN 2 0 0 0 13 0.005\nEN\n").unwrap().deck;
        assert_eq!(write_card(&med.cards[0]), "GN 2 0 0 0 13 0.005");
    }

    #[test]
    fn ex_trims_trailing_zero_floats() {
        let deck = parse("EX 0 1 26 0 1.0 0.0\nEN\n").unwrap().deck;
        // vr=1 kept, everything after is zero and dropped; the 4 integer fields stay.
        assert_eq!(write_card(&deck.cards[0]), "EX 0 1 26 0 1");
    }

    #[test]
    fn fr_shorthand_round_trips_frequency() {
        let deck = parse("FR 0 1 0 0 14.2 0\nEN\n").unwrap().deck;
        let s = write_card(&deck.cards[0]);
        assert_eq!(s, "FR 0 1 14.2 0");
        // and it reads back to the same frequency
        let re = parse(&format!("{s}\nEN\n")).unwrap().deck;
        assert_eq!(re.cards[0], deck.cards[0]);
    }

    #[test]
    fn rp_encodes_normalize_and_avg_power() {
        let deck = parse("RP 0 37 1 1001 0 0 5 0\nEN\n").unwrap().deck;
        let s = write_card(&deck.cards[0]);
        let re = parse(&format!("{s}\nEN\n")).unwrap().deck;
        assert_eq!(re.cards[0], deck.cards[0]);
    }
}
