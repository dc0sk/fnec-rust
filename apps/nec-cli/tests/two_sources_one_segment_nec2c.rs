// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-305: two `EX` cards on one segment drive it as NEC-2 drives it — the last
//! type-0 card, plus every type-5 card — not with their sum.
//!
//! nec2c 1.3.1's `etmns` (`matrix.c:743-759`) **assigns** each type-0 card's field
//! to its segment, so the last card naming the segment is the only one that drives
//! it, and then **adds** every type-5 card's (`qdsrc`). fnec summed every card: on
//! `TWO` (1 V, then 0.5 V) it solved a 1.5 V drive, three times nec2c's current and
//! power, at the same impedance — Z is drive-independent, so it never showed.
//!
//! Anchored on nec2c absolutely (decks and `.out` files of the FND-305 design
//! review, nec2c 1.3.1, the 21-segment 5.282 m dipole at 14.2 MHz):
//!
//! - `t_half` (0.5 V alone): I = 4.7331e-3 − j2.7462e-3 A (|I| 5.4722e-3),
//!   Z = 79.034 + j45.856 Ω, INPUT POWER 1.1833e-3 W, gain 2.16 dBi at θ = 90°.
//! - `t_two` (1 V, then 0.5 V): CURRENTS identical to `t_half`'s to every digit,
//!   rows 158.07 + j91.712 (1 V over the 0.5 V drive's current) and 79.034 +
//!   j45.856. Its INPUT POWER (3.5498e-3 W) sums one row per card, and its gain
//!   (−2.61 dBi with `t_half`'s E-field exactly) inherits that: **not a
//!   reference**. fnec is gated on the one-card twin instead.
//! - `t_sum` (one EX 0 at 1.5 V): |I| = 1.6416e-2 A.
//! - `t_ex0_ex5` (EX 0 1 V + EX 5 0.5 V): |I| = 1.6133e-2 A — superposed.
//!
//! The bands are inherited, not tuned: Z within the corpus's external gate
//! (2 / 8 Ω; fnec's one-card row is 78.955 + j43.484), |I| within 3 % (|Z| is
//! 1.4 % from nec2c's), the EX 5 cases within 10 % (fnec solves EX 5 as EX 0, a
//! documented ~6 % non-goal, PH8-CHK-003). The discriminator between the
//! semantics is a factor 3 (summing) or 2 (first-wins).
//!
//! The 10 % is not fnec's accuracy on EX 5 decks. fnec's `EX0_EX5` current is
//! its `SUM` current to the digit (1.6641e-2 A), +1.37 % from nec2c `t_sum` —
//! the one-card offset (`t_half` 5.4722e-3 against fnec's 5.547e-3, +1.37 %),
//! gated at 3 % in T1 and again in T5. nec2c's own `t_ex0_ex5` is 1.7 % below its
//! own `t_sum` at the same total drive, because its EX 5 is a current-slope source
//! numerically different from its EX 0: 3.15 % = 1.37 % + 1.7 %, nothing residual,
//! no offset aligned out. The alternatives the band rejects are 5.5e-3 (EX 5
//! overrides, −66 %) and 1.09e-2 (EX 5 ignored, −32 %).
//!
//! Every absence (no second row, no warning) is paired with a success check.

mod common;

use std::process::{Command, Output};

const GEOM: &str = "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\n";
const RP: &str = "RP 0 1 1 1000 90 0 0 0\n";

const TWO: &str = "EX 0 1 11 0 1 0\nEX 0 1 11 0 0.5 0\n";
const TWO_REV: &str = "EX 0 1 11 0 0.5 0\nEX 0 1 11 0 1 0\n";
const SAME: &str = "EX 0 1 11 0 1 0\nEX 0 1 11 0 1 0\n";
const TAG0: &str = "EX 0 1 11 0 1 0\nEX 0 0 11 0 0.5 0\n";
const HALF: &str = "EX 0 1 11 0 0.5 0\n";
const ONE: &str = "EX 0 1 11 0 1 0\n";
const SUM: &str = "EX 0 1 11 0 1.5 0\n";
const EX0_EX5: &str = "EX 0 1 11 0 1 0\nEX 5 1 11 0 0.5 0\n";
const EX5_EX5: &str = "EX 5 1 11 0 1 0\nEX 5 1 11 0 0.5 0\n";

/// nec2c `t_half`: the surviving 0.5 V drive.
const NEC2C_HALF_I_MAG: f64 = 5.4722e-3;
const NEC2C_HALF_Z: (f64, f64) = (79.034, 45.856);
const NEC2C_HALF_P_IN: f64 = 1.1833e-3;
const NEC2C_HALF_GAIN_DBI: f64 = 2.16;
/// nec2c `t_ex0_ex5`: |1.3944e-2 − j8.1147e-3|.
const NEC2C_EX0_EX5_I_MAG: f64 = 1.6133e-2;
/// nec2c `t_sum`: |1.4199e-2 − j8.2385e-3|, one EX 0 at 1.5 V.
const NEC2C_SUM_I_MAG: f64 = 1.6416e-2;

/// The override warning's stable words (`validate::overridden_source_warnings`).
const APPLIES_ONLY_THE_LAST: &str = "NEC-2 applies only the last";

fn run_file(name: &str, text: &str, args: &[&str]) -> Output {
    let deck = common::TempDeck::new(&format!("fnd305_{name}_{}.nec", std::process::id()), text);
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--exec", "cpu"])
        .args(args)
        .arg(&*deck)
        .output()
        .expect("run fnec")
}

/// The dipole driven by `cards`, with `extra` cards before `EN`.
fn run(name: &str, cards: &str, extra: &str, args: &[&str]) -> Output {
    run_file(
        name,
        &format!("CM {name}\nCE\n{GEOM}{cards}FR 0 1 0 0 14.2 0\n{extra}EN\n"),
        args,
    )
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn ok(name: &str, o: &Output) -> String {
    assert!(o.status.success(), "{name}: fnec failed:\n{}", stderr(o));
    stdout(o)
}

/// A text section's lines after its header line(s), up to the blank line.
fn section(report: &str, name: &str, header_lines: usize) -> String {
    let start = report
        .find(&format!("\n{name}\n"))
        .unwrap_or_else(|| panic!("no {name} section in:\n{report}"));
    report[start + name.len() + 2..]
        .lines()
        .skip(header_lines)
        .take_while(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// FEEDPOINTS rows, each `[tag, seg, V_RE, V_IM, I_RE, I_IM, Z_RE, Z_IM]`.
fn feed_rows(report: &str) -> Vec<Vec<f64>> {
    section(report, "FEEDPOINTS", 1)
        .lines()
        .map(|l| l.split_whitespace().map(|c| c.parse().unwrap()).collect())
        .collect()
}

fn currents(report: &str) -> String {
    section(report, "CURRENTS", 1)
}

fn i_mag(row: &[f64]) -> f64 {
    row[4].hypot(row[5])
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

/// NEC-2D output's `INPUT POWER   = x WATTS` value, as printed.
fn input_power(nec2d: &str) -> String {
    let line = nec2d
        .lines()
        .find(|l| l.contains("INPUT POWER   ="))
        .unwrap_or_else(|| panic!("no INPUT POWER line in:\n{nec2d}"));
    line.split('=')
        .nth(1)
        .unwrap()
        .trim()
        .trim_end_matches("WATTS")
        .trim()
        .to_string()
}

/// The θ = 90° gain of the text report's `RADIATION_PATTERN`.
fn gain_dbi(report: &str) -> f64 {
    let rows = section(report, "RADIATION_PATTERN", 2);
    let row = rows.lines().next().expect("one pattern row");
    row.split_whitespace().nth(2).unwrap().parse().unwrap()
}

/// T1: the last type-0 card drives the segment, as nec2c does — text and JSON.
#[test]
fn the_last_type0_card_drives_the_segment_as_nec2c() {
    let out = run("two", TWO, "", &[]);
    let report = ok("two", &out);
    let rows = feed_rows(&report);
    assert_eq!(rows.len(), 1, "one row per driven segment:\n{report}");
    let r = &rows[0];
    assert_eq!((r[0], r[1]), (1.0, 11.0));
    // The applied drive, not the sum (1.5) nor the first card (1.0).
    assert_eq!((r[2], r[3]), (0.5, 0.0), "V column:\n{report}");
    assert!(
        rel(i_mag(r), NEC2C_HALF_I_MAG) < 0.03,
        "|I| {} against nec2c {NEC2C_HALF_I_MAG}",
        i_mag(r)
    );
    assert!(
        (r[6] - NEC2C_HALF_Z.0).abs() < 2.0 && (r[7] - NEC2C_HALF_Z.1).abs() < 8.0,
        "Z {} {:+} against nec2c {:?}",
        r[6],
        r[7],
        NEC2C_HALF_Z
    );
    assert!(
        report.contains("N_SOURCES 2\n"),
        "SOURCES echoes the cards:\n{report}"
    );
    let err = stderr(&out);
    assert!(
        err.contains(APPLIES_ONLY_THE_LAST)
            && err.contains("`EX 0 1 11` 1 V")
            && err.contains("tag 1 segment 11")
            && err.contains("solved at 0.5 V"),
        "the override warning:\n{err}"
    );

    // JSON: one record, one feedpoint; the warning on stderr as every caveat is.
    let out = run("two_json", TWO, "", &["--output-format", "json"]);
    let records: Vec<serde_json::Value> =
        serde_json::from_str(&ok("two_json", &out)).expect("JSON");
    assert_eq!(records.len(), 1);
    let feeds = records[0]["feedpoints"].as_array().expect("feedpoints");
    assert_eq!(feeds.len(), 1, "{records:?}");
    assert_eq!(
        (feeds[0]["tag"].as_i64(), feeds[0]["seg"].as_i64()),
        (Some(1), Some(11))
    );
    assert!(
        stderr(&out).contains(APPLIES_ONLY_THE_LAST),
        "{}",
        stderr(&out)
    );
}

/// T2: an overridden card contributes nothing — the CURRENTS block equals the
/// block of the deck with that card deleted, string for string.
#[test]
fn an_overridden_card_leaves_the_currents_byte_identical() {
    let c = |name: &str, cards: &str| currents(&ok(name, &run(name, cards, "", &[])));
    let (one, half) = (c("one", ONE), c("half", HALF));
    assert_ne!(one, half, "the control: the two drives differ");
    assert_eq!(c("two_rev", TWO_REV), one, "0.5 V then 1 V is the 1 V deck");
    assert_eq!(c("same", SAME), one, "1 V twice is the 1 V deck");
    assert_eq!(c("two_c", TWO), half, "1 V then 0.5 V is the 0.5 V deck");
}

/// T3: the segment is resolved before the rule — `EX 0 0 11` names segment 11
/// by its absolute number, so it overrides `EX 0 1 11`.
#[test]
fn a_tag_zero_spelling_names_the_same_segment() {
    let two = ok("two_t3", &run("two_t3", TWO, "", &[]));
    let out = run("tag0", TAG0, "", &[]);
    let tag0 = ok("tag0", &out);
    assert_eq!(
        section(&tag0, "FEEDPOINTS", 1),
        section(&two, "FEEDPOINTS", 1)
    );
    assert_eq!(currents(&tag0), currents(&two));
    assert!(
        stderr(&out).contains("then `EX 0 0 11` 0.5 V"),
        "the card quoted as written:\n{}",
        stderr(&out)
    );
}

/// T4: power and gain follow the applied drive, not the cards. nec2c's own
/// `t_two` prints 3.5498e-3 W and −2.61 dBi, summing a row per card over the
/// E-field of `t_half`; fnec matches `t_half`.
#[test]
fn power_and_gain_follow_the_applied_drive_not_the_cards() {
    let nec2d =
        |name: &str, cards: &str| ok(name, &run(name, cards, RP, &["--output-format", "nec2d"]));
    let two_p = input_power(&nec2d("two_p", TWO));
    let half_p = input_power(&nec2d("half_p", HALF));
    assert_eq!(two_p, half_p, "INPUT POWER: the 0.5 V deck's");
    let p: f64 = two_p.parse().unwrap();
    assert!(
        rel(p, NEC2C_HALF_P_IN) < 0.03,
        "INPUT POWER {p} against nec2c's t_half {NEC2C_HALF_P_IN}"
    );
    let text = |name: &str, cards: &str| ok(name, &run(name, cards, RP, &[]));
    let (two_g, half_g) = (
        gain_dbi(&text("two_g", TWO)),
        gain_dbi(&text("half_g", HALF)),
    );
    assert_eq!(two_g, half_g, "gain: the 0.5 V deck's");
    assert!(
        (two_g - NEC2C_HALF_GAIN_DBI).abs() < 0.05,
        "gain {two_g} against nec2c's t_half {NEC2C_HALF_GAIN_DBI} dBi"
    );
}

/// T5: a type-5 card adds to the type-0 drive (nec2c's `qdsrc`). The CURRENTS
/// equality with `SUM` is fnec-consistency only — fnec solves EX 5 as EX 0.
///
/// Two absolute anchors, split as the header explains: within 3 % of nec2c
/// `t_sum` (1.6416e-2; fnec's EX 5 is EX 0, so the same band as T1), and within
/// 10 % of nec2c `t_ex0_ex5` (1.6133e-2), whose extra 1.7 % below `t_sum` is
/// nec2c's own EX 5 numerics (the documented non-goal), not fnec's error.
#[test]
fn a_type5_card_adds_to_the_type0_drive() {
    let out = run("ex0_ex5", EX0_EX5, "", &[]);
    let report = ok("ex0_ex5", &out);
    let rows = feed_rows(&report);
    assert_eq!(rows.len(), 1, "{report}");
    assert_eq!((rows[0][2], rows[0][3]), (1.5, 0.0));
    assert!(
        rel(i_mag(&rows[0]), NEC2C_EX0_EX5_I_MAG) < 0.10,
        "|I| {} against nec2c {NEC2C_EX0_EX5_I_MAG}",
        i_mag(&rows[0])
    );
    assert!(
        rel(i_mag(&rows[0]), NEC2C_SUM_I_MAG) < 0.03,
        "fnec's EX 5 is EX 0 — the same 3 % band as T1: |I| {} against nec2c t_sum \
         {NEC2C_SUM_I_MAG}",
        i_mag(&rows[0])
    );
    assert_eq!(
        currents(&report),
        currents(&ok("sum5", &run("sum5", SUM, "", &[])))
    );
    assert!(
        !stderr(&out).contains(APPLIES_ONLY_THE_LAST),
        "{}",
        stderr(&out)
    );
}

/// T6: two type-5 cards sum (nec2c `ex5_ex5` = 1.5 × `ex5` exactly).
#[test]
fn two_type5_cards_sum() {
    let out = run("ex5_ex5", EX5_EX5, "", &[]);
    let report = ok("ex5_ex5", &out);
    let rows = feed_rows(&report);
    assert_eq!(rows.len(), 1, "{report}");
    assert_eq!((rows[0][2], rows[0][3]), (1.5, 0.0));
    assert_eq!(
        currents(&report),
        currents(&ok("sum6", &run("sum6", SUM, "", &[])))
    );
    assert!(
        !stderr(&out).contains(APPLIES_ONLY_THE_LAST),
        "{}",
        stderr(&out)
    );
}

/// Two parallel 5 m dipoles 1 m apart at 29.9 MHz (nec2c `*_tl2`, `*_nt2`).
fn two_wires(cards: &str, network: &str) -> String {
    format!(
        "CM network\nCE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 1 0 -2.5 1 0 2.5 0.001\nGE 0\n\
         FR 0 1 0 0 29.9 0\n{cards}{network}EN\n"
    )
}

const TL_ON_FEED: &str = "TL 1 11 2 11 50 0 0 0 0 0\n";
const NT_ON_FEED: &str = "NT 1 11 2 11 0.01 0 0.002 0 0.01 0\n";
const NT_ELSEWHERE: &str = "NT 2 5 2 15 0.01 0 0.002 0 0.01 0\n";

/// T8: a TL/NT port on a segment more than one card drives is refused — nec2c
/// prices its network from the first card and drives the structure with the
/// last, an answer matching neither (`two_tl2`). Negative controls: one card
/// with the same line solves, against nec2c `half_tl2`; two cards on a segment
/// that is no port solve, equal to the one-card deck.
#[test]
fn a_network_port_on_a_doubly_fed_segment_is_refused() {
    for (name, network) in [("tl", TL_ON_FEED), ("nt", NT_ON_FEED)] {
        let out = run_file(&format!("two_{name}"), &two_wires(TWO, network), &[]);
        assert!(
            !out.status.success(),
            "{name}: must be refused:\n{}",
            stdout(&out)
        );
        let err = stderr(&out);
        assert!(
            err.contains("TL/NT: a port on tag 1 segment 11")
                && err.contains("first card")
                && err.contains("with the last"),
            "{name}: {err}"
        );
    }
    // Control: one card, the same line — solves, and matches nec2c half_tl2.
    let half = ok(
        "half_tl",
        &run_file("half_tl", &two_wires(HALF, TL_ON_FEED), &[]),
    );
    let r = &feed_rows(&half)[0];
    assert!(
        (r[6] - 50.991).abs() < 2.0 && (r[7] + 34.802).abs() < 8.0,
        "half + TL: {} {:+} against nec2c 50.991 - j34.802",
        r[6],
        r[7]
    );
    // Control: two cards on a segment that is no port.
    let two = ok(
        "two_nt5",
        &run_file("two_nt5", &two_wires(TWO, NT_ELSEWHERE), &[]),
    );
    let half = ok(
        "half_nt5",
        &run_file("half_nt5", &two_wires(HALF, NT_ELSEWHERE), &[]),
    );
    assert_eq!(
        section(&two, "FEEDPOINTS", 1),
        section(&half, "FEEDPOINTS", 1)
    );
    assert_eq!(currents(&two), currents(&half));
}

/// T9: every route applies the rule. Each deck with two cards on one segment
/// reports what its one-card twin reports, string for string, on its own route.
#[test]
fn every_route_applies_the_rule() {
    struct Case {
        name: &'static str,
        body: String,
        args: &'static [&'static str],
        route: &'static [&'static str],
    }
    let dipole = |cards: &str| format!("CE\n{GEOM}{cards}FR 0 1 0 0 14.2 0\nEN\n");
    let cases = |two: &str, one: &str| {
        let pick = |cards: &str| cards.to_string();
        let (two, one) = (pick(two), pick(one));
        vec![
            Case {
                name: "image",
                // A monopole on PEC ground, fed at its base: the image problem.
                body: "CE\nGW 1 10 0 0 0 0 0 2.64 0.001\nGE 1\nGN 1\nCARDS\
                       FR 0 1 0 0 28 0\nEN\n"
                    .to_string(),
                args: &[],
                route: &["contact=yes"],
            },
            Case {
                name: "paths",
                // An inverted-V, two cards on an arm segment.
                body: "CE\nGW 1 11 0 0 10 -3.5 0 6.5 .001\nGW 2 11 0 0 10 3.5 0 6.5 .001\n\
                       GE 0\nCARDSFR 0 1 0 0 14.2 0\nEN\n"
                    .to_string(),
                args: &[],
                route: &["route=paths"],
            },
            Case {
                name: "graph",
                // A T: a stem and two arms joined at one node, two cards on the stem.
                body: "CE\nGW 1 10 0 0 0 0 0 2.5 .001\nGW 2 10 0 0 2.5 -2.5 0 2.5 .001\n\
                       GW 3 10 0 0 2.5 2.5 0 2.5 .001\nGE 0\nCARDSFR 0 1 0 0 28 0\nEN\n"
                    .to_string(),
                args: &[],
                // The section graph: the paths label, and no residual (FND-306).
                route: &["route=paths", "rel_res=n/a"],
            },
            Case {
                name: "mpie",
                body: dipole("CARDS"),
                args: &["--solver", "mpie"],
                route: &["route=mpie"],
            },
            Case {
                name: "sinusoidal",
                body: dipole("CARDS"),
                args: &["--solver", "sinusoidal"],
                route: &["mode=sinusoidal", "route=plain"],
            },
        ]
        .into_iter()
        .map(|c| {
            let two_deck = c.body.replace("CARDS", &two);
            let one_deck = c.body.replace("CARDS", &one);
            (c, two_deck, one_deck)
        })
        .collect::<Vec<_>>()
    };
    let two_cards = |seg: u32| format!("EX 0 1 {seg} 0 1 0\nEX 0 1 {seg} 0 0.5 0\n");
    let one_card = |seg: u32| format!("EX 0 1 {seg} 0 0.5 0\n");
    let mut seen = 0;
    for (c, two, one) in cases(&two_cards(3), &one_card(3)) {
        // The image deck feeds its base segment; the others segment 3.
        let (two, one) = if c.name == "image" {
            (
                two.replace("EX 0 1 3 ", "EX 0 1 1 "),
                one.replace("EX 0 1 3 ", "EX 0 1 1 "),
            )
        } else {
            (two, one)
        };
        let t = run_file(&format!("{}_two", c.name), &two, c.args);
        let o = run_file(&format!("{}_one", c.name), &one, c.args);
        let (tr, or) = (ok(c.name, &t), ok(c.name, &o));
        assert!(
            c.route.iter().all(|r| stderr(&t).contains(r)),
            "{}: not on the route under test:\n{}",
            c.name,
            stderr(&t)
        );
        assert_eq!(
            section(&tr, "FEEDPOINTS", 1),
            section(&or, "FEEDPOINTS", 1),
            "{}",
            c.name
        );
        assert_eq!(currents(&tr), currents(&or), "{}", c.name);
        assert!(stderr(&t).contains(APPLIES_ONLY_THE_LAST), "{}", c.name);
        seen += 1;
    }
    assert_eq!(seen, 5);
}

/// T10: a single feed spelt with two cards is one feed, so the Sommerfeld
/// correction still applies (it declined on a card count of two).
#[test]
fn a_doubly_fed_single_feed_still_takes_the_sommerfeld_correction() {
    let deck = |cards: &str| {
        format!(
            "CE\nGW 1 21 0 -5.282 10 0 5.282 10 0.001\nGE 0\nGN 2 0 0 0 13 0.005\n{cards}\
             FR 0 1 0 0 14.2 0\nEN\n"
        )
    };
    let args = ["--ground-solver", "sommerfeld"];
    let two = run_file("somm_two", &deck(TWO), &args);
    let one = run_file("somm_one", &deck(HALF), &args);
    let (tr, or) = (ok("somm_two", &two), ok("somm_one", &one));
    common::assert_diag_field(&stderr(&one), "gsolver", "sommerfeld-applied");
    common::assert_diag_field(&stderr(&two), "gsolver", "sommerfeld-applied");
    assert_eq!(section(&tr, "FEEDPOINTS", 1), section(&or, "FEEDPOINTS", 1));
}

/// T11: negative controls — the owner does not over-apply to current sources or
/// across segments. The two refusals keep their text; a two-segment deck keeps
/// one row per segment.
#[test]
fn current_sources_and_separate_segments_are_unchanged() {
    let corpus = |name: &str| {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + name;
        Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--exec", "cpu"])
            .arg(path)
            .output()
            .expect("run fnec")
    };
    let mixed = corpus("dipole-mixed-sources-51seg.nec");
    assert!(!mixed.status.success());
    assert!(
        stderr(&mixed).contains("both a voltage source"),
        "{}",
        stderr(&mixed)
    );
    let two_i = corpus("dipole-two-current-sources-freesp.nec");
    assert!(!two_i.status.success());
    assert!(
        stderr(&two_i).contains("2 current sources"),
        "{}",
        stderr(&two_i)
    );
    let multi = corpus("multi-source.nec");
    let report = ok("multi-source", &multi);
    let rows = feed_rows(&report);
    assert!(rows.len() >= 2, "one row per segment:\n{report}");
    assert!(!stderr(&multi).contains(APPLIES_ONLY_THE_LAST));
}

/// T12: FND-309's input-impedance print follows its source by segment. The
/// second card carries the request and is the source; counted among the cards it
/// is row 2, but FEEDPOINTS has one row — the segment's.
#[test]
fn the_input_impedance_print_finds_the_source_by_its_segment() {
    let deck = format!(
        "CM t12\nCE\n{GEOM}EX 0 1 11 0 1 0\nEX 0 1 11 1 0.5 0 50\nFR 0 2 0 0 14.2 0.1\nEN\n"
    );
    let out = run_file("t12", &deck, &[]);
    let report = ok("t12", &out);
    assert!(
        report.contains("\nINPUT_IMPEDANCE\nSOURCE_TAG 1 SOURCE_SEG 11 SOURCE_SEGMENT 11\n"),
        "{report}\n{}",
        stderr(&out)
    );
    let table = section(&report, "INPUT_IMPEDANCE", 4);
    let first: Vec<f64> = table
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .map(|c| c.parse().unwrap())
        .collect();
    let row = &feed_rows(&report)[0];
    assert_eq!((first[1], first[2]), (row[6], row[7]), "the single row's Z");
}
