// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-309: `EX` I4's units digit asks for the input impedance over a sweep,
//! normalized to F3 or, with F3 = 0, to the sweep's largest |Z| — nec2c 1.3.1's
//! INPUT IMPEDANCE DATA table, fnec's `INPUT_IMPEDANCE` section.
//!
//! Anchored on nec2c absolutely: the decks below are the ones nec2c was run on
//! (`EX`/`FR` as written, plus `XQ`), and its printed values are quoted. Two
//! checks per table, because they catch different slips:
//!
//! - **nec2c**: each unnormalized component and the factor within 1.5 % of
//!   nec2c's |Z|, on 81 segments per wire. The design's decks (`fixed`, `max`:
//!   the 5 m dipole on 11 segments) miss it — fnec's reactance there is 3.6 Ω
//!   (4.5 % of |Z|) from nec2c's at 28 MHz, closing as the mesh refines (2.8 %
//!   at 21, 1.75 % at 41, 1.4 % at 81 segments), which is FND-315, not a reason
//!   to widen. The 11-segment `fixed` deck is kept for the header nec2c prints
//!   (SOURCE SEGMENT 6) and the identity. Worst component at 81 segments: 1.40 %
//!   of |Z| (29 MHz, X), so the margin is thin by measurement, not by choice.
//! - **identity**: each normalized Z is that point's FEEDPOINTS Z over the
//!   printed factor, and the phase is Z's own, to the precision the report
//!   prints (Z to 1e-6 Ω, the normalized columns to seven significant digits:
//!   1e-6 relative, not the 1e-9 the text cannot carry). That pins the
//!   arithmetic where 1.5 % cannot (dividing by the wrong point's |Z| is 12 %
//!   off on deck `max`, a 1 % slip is caught here alone).
//!
//! The decks keep one source card per segment, so nothing here depends on how
//! two cards on one segment are read (FND-305).

mod common;

use common::TempDeck;
use std::process::{Command, Output};

/// nec2c's `fixed`/`max` dipole: 5 m on 11 segments, fed at segment 6.
const DIPOLE: &str = "GW 1 11 0 0 -2.5 0 0 2.5 0.001\nGE 0\n";
/// The same dipole on 81 segments, fed at 41: the mesh the nec2c values hold at.
const DIPOLE_81: &str = "GW 1 81 0 0 -2.5 0 0 2.5 0.001\nGE 0\n";
/// A second wire of a DIFFERENT length (4.5 m, 3 m away), so the two
/// feedpoints' impedances differ and a test can tell which one a table follows.
/// 81 segments each: tag 2 segment 41 is absolute segment 122.
const TWO_WIRES: &str = "GW 1 81 0 0 -2.5 0 0 2.5 0.001\nGW 2 81 3 0 -2.25 3 0 2.25 0.001\nGE 0\n";

fn fnec(name: &str, body: &str, args: &[&str]) -> Output {
    let deck = TempDeck::new(
        &format!("input_impedance_{name}_{}.nec", std::process::id()),
        &format!("CM {name}\nCE\n{body}XQ\nEN\n"),
    );
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&*deck)
        .output()
        .expect("run fnec")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A run that must have solved: exit 0 and a FEEDPOINTS section — the success
/// check every absence assertion here stands beside.
fn solved(o: &Output) -> String {
    let out = stdout(o);
    assert!(
        o.status.success() && out.contains("\nFEEDPOINTS\n"),
        "the run must solve:\n{}\n{out}",
        stderr(o)
    );
    out
}

#[derive(Debug)]
struct Table {
    source: String,
    factor: f64,
    label: String,
    /// FREQ Z_RE Z_IM Z_ABS Z_PHASE_DEG ZN_RE ZN_IM ZN_ABS
    rows: Vec<[f64; 8]>,
}

fn table(out: &str) -> Option<Table> {
    let start = out.find("\nINPUT_IMPEDANCE\n")?;
    let mut lines = out[start + 1..].lines().skip(1);
    let source = lines.next()?.to_string();
    let norm: Vec<&str> = lines.next()?.split_whitespace().collect();
    let n: usize = lines.next()?.strip_prefix("N_POINTS ")?.parse().ok()?;
    assert_eq!(
        lines.next()?,
        "FREQ_MHZ Z_RE Z_IM Z_ABS Z_PHASE_DEG ZN_RE ZN_IM ZN_ABS"
    );
    let rows: Vec<[f64; 8]> = lines
        .take_while(|l| !l.is_empty())
        .map(|l| {
            let v: Vec<f64> = l.split(' ').map(|c| c.parse().unwrap()).collect();
            v.try_into().unwrap()
        })
        .collect();
    assert_eq!(rows.len(), n);
    assert_eq!(norm[0], "NORMALIZATION_OHMS");
    assert_eq!(norm[2], "NORMALIZED_TO");
    Some(Table {
        source,
        factor: norm[1].parse().unwrap(),
        label: norm[3].to_string(),
        rows,
    })
}

/// Each frequency block's FEEDPOINTS rows as `(tag, seg, z_re, z_im)`.
fn feedpoints(out: &str) -> Vec<Vec<(usize, usize, f64, f64)>> {
    out.split("FNEC FEEDPOINT REPORT\n")
        .skip(1)
        .map(|block| {
            let start = block
                .find("TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM\n")
                .unwrap();
            block[start..]
                .lines()
                .skip(1)
                .take_while(|l| !l.is_empty())
                .map(|l| {
                    let c: Vec<&str> = l.split_whitespace().collect();
                    (
                        c[0].parse().unwrap(),
                        c[1].parse().unwrap(),
                        c[6].parse().unwrap(),
                        c[7].parse().unwrap(),
                    )
                })
                .collect()
        })
        .collect()
}

/// fnec's Z against nec2c's, each component within `tol` of nec2c's |Z|.
fn near_nec2c(what: &str, (re, im): (f64, f64), (nre, nim): (f64, f64), tol: f64) {
    let mag = nre.hypot(nim);
    assert!(
        (re - nre).abs() <= tol * mag && (im - nim).abs() <= tol * mag,
        "{what}: fnec {re} + j{im}, nec2c {nre} + j{nim}: off by {:.3} % / {:.3} % of |Z|",
        100.0 * (re - nre).abs() / mag,
        100.0 * (im - nim).abs() / mag,
    );
}

/// The identity check: the table's columns are the FEEDPOINTS row `feed` of
/// each point, its magnitude and phase, and that Z over the factor.
fn identity(t: &Table, feeds: &[Vec<(usize, usize, f64, f64)>], feed: usize) {
    assert_eq!(t.rows.len(), feeds.len());
    for (r, f) in t.rows.iter().zip(feeds) {
        let (_, _, re, im) = f[feed];
        // Six decimals in both sections: equal to the digit.
        assert_eq!((r[1], r[2]), (re, im), "Z is FEEDPOINTS row {}", feed + 1);
        let mag = re.hypot(im);
        assert!((r[3] - mag).abs() < 2e-6, "|Z| {} against {mag}", r[3]);
        assert!(
            (r[4] - im.atan2(re).to_degrees()).abs() < 2e-6,
            "the phase is Z's own: {}",
            r[4]
        );
        // The printed Z's rounding over the factor, and the normalized columns'
        // own seven significant digits.
        let zn = mag / t.factor;
        let tol = 1e-6 * (zn + 1.0 / t.factor);
        assert!(
            (r[5] - re / t.factor).abs() <= tol
                && (r[6] - im / t.factor).abs() <= tol
                && (r[7] - zn).abs() <= tol,
            "normalized {r:?} against Z / {}",
            t.factor
        );
    }
}

/// nec2c deck `fixed` (`EX 0 1 6 11 1 0 50`, `FR 0 2 0 0 29.9 1`): SOURCE
/// SEGMENT 6, factor 50. On 81 segments (`EX 0 1 41 11 1 0 50`) nec2c prints
/// SOURCE SEGMENT 41 and 79.491 + j42.045 → 1.5898 + j0.84091, 88.646 +
/// j88.649 → 1.7729 + j1.7730.
#[test]
fn a_fixed_factor_table_matches_nec2c() {
    let o = fnec(
        "fixed",
        &format!("{DIPOLE}EX 0 1 6 11 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &[],
    );
    let out = solved(&o);
    let t = table(&out).expect("an INPUT_IMPEDANCE section");
    assert_eq!(t.source, "SOURCE_TAG 1 SOURCE_SEG 6 SOURCE_SEGMENT 6");
    assert_eq!((t.factor, t.label.as_str()), (50.0, "F3"));
    identity(&t, &feedpoints(&out), 0);

    let o = fnec(
        "fixed81",
        &format!("{DIPOLE_81}EX 0 1 41 11 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &[],
    );
    let out = solved(&o);
    let t = table(&out).expect("an INPUT_IMPEDANCE section");
    assert_eq!(t.source, "SOURCE_TAG 1 SOURCE_SEG 41 SOURCE_SEGMENT 41");
    assert_eq!((t.factor, t.label.as_str()), (50.0, "F3"));
    let nec2c = [(79.491, 42.045), (88.646, 88.649)];
    for (r, n) in t.rows.iter().zip(nec2c) {
        near_nec2c(&format!("{} MHz", r[0]), (r[1], r[2]), n, 0.015);
        near_nec2c(
            &format!("{} MHz normalized", r[0]),
            (r[5], r[6]),
            (n.0 / 50.0, n.1 / 50.0),
            0.015,
        );
    }
    identity(&t, &feedpoints(&out), 0);
}

/// nec2c deck `max` (`EX 0 1 6 1 1 0 0`, `FR 0 3 0 0 28 1`) on 81 segments:
/// factor 92.9392 (the 30 MHz |Z|); 28 MHz 64.642 − j45.948 → 0.69553 −
/// j0.49439, 29 MHz 72.076 + j0.34869, 30 MHz 80.361 + j46.689.
#[test]
fn a_largest_magnitude_table_matches_nec2c() {
    let o = fnec(
        "max",
        &format!("{DIPOLE_81}EX 0 1 41 1 1 0 0\nFR 0 3 0 0 28 1\n"),
        &[],
    );
    let out = solved(&o);
    let t = table(&out).expect("an INPUT_IMPEDANCE section");
    assert_eq!(t.source, "SOURCE_TAG 1 SOURCE_SEG 41 SOURCE_SEGMENT 41");
    assert_eq!(t.label, "MAX");
    assert!(
        (t.factor - 92.9392).abs() <= 0.015 * 92.9392,
        "factor {} against nec2c's 92.9392",
        t.factor
    );
    let nec2c = [(64.642, -45.948), (72.076, 0.34869), (80.361, 46.689)];
    for (r, n) in t.rows.iter().zip(nec2c) {
        near_nec2c(&format!("{} MHz", r[0]), (r[1], r[2]), n, 0.015);
    }
    near_nec2c(
        "28 MHz normalized",
        (t.rows[0][5], t.rows[0][6]),
        (0.69553, -0.49439),
        0.015,
    );
    // The factor is this sweep's own largest |Z|: the 30 MHz point's.
    let largest = t.rows.iter().map(|r| r[3]).fold(0.0, f64::max);
    assert!((t.factor - largest).abs() < 1e-6 && t.rows[2][3] == largest);
    identity(&t, &feedpoints(&out), 0);
}

/// nec2c prints no table for one frequency (`nfrq == 1`); the same card over
/// a two-point sweep does.
#[test]
fn one_frequency_prints_no_table() {
    let one = fnec(
        "one",
        &format!("{DIPOLE}EX 0 1 6 1 1 0 50\nFR 0 1 0 0 29.9 0\n"),
        &[],
    );
    let out = solved(&one);
    assert!(!out.contains("INPUT_IMPEDANCE"), "{out}");
    let two = fnec(
        "one_ctl",
        &format!("{DIPOLE}EX 0 1 6 1 1 0 50\nFR 0 2 0 0 29.9 0.5\n"),
        &[],
    );
    assert!(table(&solved(&two)).is_some());
}

/// nec2c deck `two`: the last voltage-source card's units 0 cancels the
/// earlier card's 1, and nec2c prints no table. In the other order it does.
#[test]
fn the_last_voltage_source_card_decides() {
    let o = fnec(
        "two",
        &format!("{TWO_WIRES}EX 0 2 41 1 1 0 50\nEX 0 1 41 0 1 0 0\nFR 0 2 0 0 29.9 1\n"),
        &[],
    );
    let out = solved(&o);
    assert!(out.contains("\nSWEEP_POINTS\n"), "{out}");
    assert!(!out.contains("INPUT_IMPEDANCE"), "{out}");
    let swapped = fnec(
        "two_ctl",
        &format!("{TWO_WIRES}EX 0 1 41 0 1 0 0\nEX 0 2 41 1 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &[],
    );
    assert!(table(&solved(&swapped)).is_some());
}

/// The two candidate sources' impedances must differ at every point, or a test
/// of which was chosen cannot fail.
fn candidates_differ(feeds: &[Vec<(usize, usize, f64, f64)>]) {
    for f in feeds {
        assert_eq!(f.len(), 2, "{f:?}");
        let d = (f[0].2 - f[1].2).hypot(f[0].3 - f[1].3);
        assert!(
            d > 0.25 * f[0].2.hypot(f[0].3),
            "the two feedpoints are too alike to tell apart: {f:?}"
        );
    }
}

/// The last EX 0 is the source when there is no EX 5 (nec2c's `main.c:1806`).
/// nec2c on this deck: SOURCE SEGMENT 122; 26.171 − j99.204 and 52.161 −
/// j60.804 (the first wire's feed: 104.24 + j93.596 and 229.43 + j163.61).
#[test]
fn the_table_follows_the_last_ex_0() {
    let o = fnec(
        "last_ex0",
        &format!("{TWO_WIRES}EX 0 1 41 0 1 0 0\nEX 0 2 41 1 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &[],
    );
    let out = solved(&o);
    let feeds = feedpoints(&out);
    candidates_differ(&feeds);
    let t = table(&out).expect("an INPUT_IMPEDANCE section");
    assert_eq!(t.source, "SOURCE_TAG 2 SOURCE_SEG 41 SOURCE_SEGMENT 122");
    for (r, n) in t.rows.iter().zip([(26.171, -99.204), (52.161, -60.804)]) {
        near_nec2c(&format!("{} MHz", r[0]), (r[1], r[2]), n, 0.015);
    }
    identity(&t, &feeds, 1);
}

/// A tag-0 card names an absolute segment, and its row carries its wire's tag
/// (FND-282): the table is chosen by the card's place among the feedpoints, so
/// it still finds the row. nec2c: SOURCE SEGMENT 122, the same table as above.
#[test]
fn a_tag_0_source_is_found_by_its_card() {
    let o = fnec(
        "tag0",
        &format!("{TWO_WIRES}EX 0 1 41 0 1 0 0\nEX 0 0 122 1 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &[],
    );
    let out = solved(&o);
    let feeds = feedpoints(&out);
    candidates_differ(&feeds);
    let t = table(&out).expect("an INPUT_IMPEDANCE section");
    assert_eq!(t.source, "SOURCE_TAG 2 SOURCE_SEG 41 SOURCE_SEGMENT 122");
    for (r, n) in t.rows.iter().zip([(26.171, -99.204), (52.161, -60.804)]) {
        near_nec2c(&format!("{} MHz", r[0]), (r[1], r[2]), n, 0.015);
    }
    identity(&t, &feeds, 1);
}

/// An EX 5 is the source even when an EX 0 comes later and carries the digit.
/// nec2c: SOURCE SEGMENT 41. Compared with fnec's own FEEDPOINTS row, not with
/// nec2c: EX 5 is a documented ~6 % non-goal (card-support-matrix.md).
#[test]
fn an_ex_5_is_preferred_to_a_later_ex_0() {
    let o = fnec(
        "ex5",
        &format!("{TWO_WIRES}EX 5 1 41 0 1 0 0\nEX 0 2 41 1 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &[],
    );
    let out = solved(&o);
    let feeds = feedpoints(&out);
    candidates_differ(&feeds);
    let t = table(&out).expect("an INPUT_IMPEDANCE section");
    assert_eq!(t.source, "SOURCE_TAG 1 SOURCE_SEG 41 SOURCE_SEGMENT 41");
    assert_eq!((t.factor, t.label.as_str()), (50.0, "F3"));
    identity(&t, &feeds, 0);
}

/// What NEC-2 does not define is refused, naming the field and the remedy; a
/// plane wave's I4 (not a print request) still parses.
#[test]
fn undefined_print_requests_are_refused() {
    for (name, card, field) in [
        ("u2", "EX 0 1 6 2 1 0 0", "EX field 4 = '2'"),
        ("u5", "EX 5 1 6 15 1 0 50", "EX field 4 = '15'"),
        ("neg", "EX 0 1 6 1 1 0 -50", "EX field 7 = '-50'"),
    ] {
        let o = fnec(name, &format!("{DIPOLE}{card}\nFR 0 2 0 0 29.9 1\n"), &[]);
        let err = stderr(&o);
        assert!(
            !o.status.success() && err.contains(field) && err.contains("write "),
            "{card}: {err}"
        );
        assert!(stdout(&o).is_empty(), "{card} printed a report");
    }
    let wave = fnec(
        "wave",
        &format!("{DIPOLE}EX 1 1 1 2 90 0 0 0 0 0\nFR 0 1 0 0 29.9 0\n"),
        &[],
    );
    assert!(
        wave.status.success(),
        "a plane wave's I4 is not refused: {}",
        stderr(&wave)
    );
}

/// `--output-format json` records, parsed.
fn json(o: &Output) -> Vec<serde_json::Value> {
    assert!(o.status.success(), "{}", stderr(o));
    serde_json::from_str(&stdout(o)).unwrap_or_else(|e| panic!("{e}: {}", stdout(o)))
}

/// A record's `input_impedance` as `(feedpoint_index, (tag, seg, abs_seg),
/// normalization_ohms, normalized_to, zn_re, zn_im, zn_abs)`.
type JsonTable = (u64, (u64, u64, u64), f64, String, f64, f64, f64);

fn json_table(rec: &serde_json::Value) -> Option<JsonTable> {
    let t = rec.get("input_impedance")?;
    let u = |k: &str| t[k].as_u64().unwrap_or_else(|| panic!("{k}: {t}"));
    let f = |k: &str| t[k].as_f64().unwrap_or_else(|| panic!("{k}: {t}"));
    Some((
        u("feedpoint_index"),
        (u("source_tag"), u("source_seg"), u("source_abs_seg")),
        f("normalization_ohms"),
        t["normalized_to"]
            .as_str()
            .expect("normalized_to")
            .to_string(),
        f("zn_re"),
        f("zn_im"),
        f("zn_abs"),
    ))
}

/// Every value of `"key":` in the JSON text, in order, parsed by `str::parse`.
/// serde_json's default float parser is not correctly rounded (it is one ulp
/// off on these records), Rust's is, so an exact assertion reads the token.
fn exact_numbers(out: &str, key: &str) -> Vec<f64> {
    let pat = format!("\"{key}\":");
    out.match_indices(&pat)
        .map(|(i, _)| {
            let tail = &out[i + pat.len()..];
            let end = tail.find([',', '}', ']']).expect("a number ends");
            tail[..end].parse().expect("a number")
        })
        .collect()
}

/// The chosen feedpoint's exact Z per record, and each record's exact `zn_*`.
/// A record writes `z_re` at its top level and then once per feedpoint, so with
/// `n_feeds` feedpoints the chosen one's is occurrence `1 + feed` of each
/// `1 + n_feeds`.
fn exact_zn(out: &str, n_feeds: usize, feed: usize) -> Vec<((f64, f64), (f64, f64))> {
    let (re, im) = (exact_numbers(out, "z_re"), exact_numbers(out, "z_im"));
    let (zn_re, zn_im) = (exact_numbers(out, "zn_re"), exact_numbers(out, "zn_im"));
    assert_eq!(re.len(), zn_re.len() * (1 + n_feeds), "{out}");
    (0..zn_re.len())
        .map(|r| {
            let k = r * (1 + n_feeds) + 1 + feed;
            ((re[k], im[k]), (zn_re[r], zn_im[r]))
        })
        .collect()
}

/// A record's `feedpoints[i]` Z.
fn json_z(rec: &serde_json::Value, i: usize) -> (f64, f64) {
    let f = &rec["feedpoints"][i];
    (f["z_re"].as_f64().unwrap(), f["z_im"].as_f64().unwrap())
}

/// FND-314: JSON carries the table as each record's `input_impedance`, from the
/// decision the text section takes. Its normalized Z is the record's chosen
/// feedpoint over the factor exactly: the division is componentwise and `{}`
/// round-trips an f64, read back by `str::parse` ([`exact_zn`]). The comparison with the text table from the same
/// binary is the wiring check — one function renders both, so it is not a
/// numeric gate; the nec2c anchor is the next test. The same deck with units
/// digit 0 carries no field (the control), and a single point carries none and
/// warns of nothing, as the text report prints no table there.
#[test]
fn json_records_carry_the_table() {
    let body = format!("{DIPOLE}EX 0 1 6 11 1 0 50\nFR 0 2 0 0 29.9 1\n");
    let o = fnec("json", &body, &["--output-format", "json"]);
    assert!(!stderr(&o).contains("input-impedance"), "{}", stderr(&o));
    for ((re, im), zn) in exact_zn(&stdout(&o), 1, 0) {
        assert_eq!(zn, (re / 50.0, im / 50.0));
    }
    let recs = json(&o);
    assert_eq!(recs.len(), 2);
    let text = table(&solved(&fnec("json_text", &body, &[]))).expect("the text table");
    for (rec, row) in recs.iter().zip(&text.rows) {
        let (idx, src, factor, to, zn_re, zn_im, zn_abs) = json_table(rec).expect("the field");
        assert_eq!((idx, src, factor, to.as_str()), (0, (1, 6, 6), 50.0, "F3"));
        let (re, im) = json_z(rec, 0);
        assert!((zn_abs - re.hypot(im) / 50.0).abs() <= 4.0 * f64::EPSILON * zn_abs);
        // The text table's columns, to the seven digits it prints.
        for (j, v) in [(5, zn_re), (6, zn_im), (7, zn_abs)] {
            assert!(
                (row[j] - v).abs() <= 1e-6 * v.abs().max(1e-3),
                "{row:?} vs {rec}"
            );
        }
    }
    let off = fnec(
        "json_off",
        &format!("{DIPOLE}EX 0 1 6 10 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &["--output-format", "json"],
    );
    let off = json(&off);
    assert!(
        off.len() == 2 && off.iter().all(|r| json_table(r).is_none()),
        "{off:?}"
    );
    let one = fnec(
        "json_one",
        &format!("{DIPOLE}EX 0 1 6 11 1 0 50\nFR 0 1 0 0 29.9 0\n"),
        &["--output-format", "json"],
    );
    assert!(
        !stderr(&one).contains("input-impedance"),
        "{}",
        stderr(&one)
    );
    let one = json(&one);
    assert!(one.len() == 1 && json_table(&one[0]).is_none(), "{one:?}");
}

/// The JSON table against nec2c, on the largest-|Z| deck — where the factor is
/// computed, not echoed from F3: 92.9392 (the 30 MHz |Z|), and 28 MHz
/// normalized to 0.69553 − j0.49439 (deck `max`, 81 segments).
#[test]
fn a_json_largest_magnitude_table_matches_nec2c() {
    let o = fnec(
        "json_max",
        &format!("{DIPOLE_81}EX 0 1 41 1 1 0 0\nFR 0 3 0 0 28 1\n"),
        &["--output-format", "json"],
    );
    let recs = json(&o);
    assert_eq!(recs.len(), 3);
    let (idx, src, factor, to, zn_re, zn_im, _) = json_table(&recs[0]).expect("the field");
    assert_eq!((idx, src, to.as_str()), (0, (1, 41, 41), "MAX"));
    assert!(
        (factor - 92.9392).abs() <= 0.015 * 92.9392,
        "factor {factor} against nec2c's 92.9392"
    );
    near_nec2c(
        "28 MHz normalized",
        (zn_re, zn_im),
        (0.69553, -0.49439),
        0.015,
    );
    // The factor is the sweep's largest |Z|, the 30 MHz record's.
    let z_abs: Vec<f64> = recs.iter().map(|r| r["z_abs"].as_f64().unwrap()).collect();
    assert_eq!(factor, z_abs.iter().copied().fold(0.0, f64::max));
    assert_eq!(factor, z_abs[2]);
    for r in &recs {
        assert_eq!(json_table(r).expect("every record").2, factor);
    }
}

/// The table follows the last `EX 0`, FEEDPOINTS row 2 — tag 2 segment 41,
/// absolute segment 122, on two wires whose impedances differ — and the JSON
/// names that row by index and both segment numbers. On one wire `seg` and
/// `abs_seg` coincide, so only this deck tells them apart; normalizing row 1's
/// Z instead fails `candidates_differ`'s margin.
#[test]
fn the_json_table_names_the_row_it_follows() {
    let body = format!("{TWO_WIRES}EX 0 1 41 0 1 0 0\nEX 0 2 41 1 1 0 50\nFR 0 2 0 0 29.9 1\n");
    candidates_differ(&feedpoints(&solved(&fnec("json_row_text", &body, &[]))));
    let o = fnec("json_row", &body, &["--output-format", "json"]);
    let recs = json(&o);
    assert_eq!(recs.len(), 2);
    for rec in &recs {
        let (idx, src, factor, to, ..) = json_table(rec).expect("the field");
        assert_eq!(
            (idx, src, factor, to.as_str()),
            (1, (2, 41, 122), 50.0, "F3")
        );
    }
    let rows = exact_zn(&stdout(&o), 2, 1);
    assert_eq!(rows.len(), 2);
    for ((re, im), zn) in rows {
        assert_eq!(zn, (re / 50.0, im / 50.0));
    }
}

/// A partial sweep carries no table in JSON either — a largest-|Z| factor over
/// the points that solved is not NEC-2's — and says so on stderr, as the text
/// report does. 1e-200 MHz fails while its neighbours solve
/// (`sweep_partial_output.rs`); without it the same deck and sweep carry the
/// field (the control).
#[test]
fn a_partial_json_sweep_carries_no_table_and_says_so() {
    let run = |name: &str, points: &str| {
        let cfg = TempDeck::new(
            &format!("input_impedance_{name}_{}.toml", std::process::id()),
            &format!("[frequency]\npoints_mhz = [{points}]\n"),
        );
        let deck = TempDeck::new(
            &format!("input_impedance_{name}_{}.nec", std::process::id()),
            &format!("CE\n{DIPOLE}EX 0 1 6 1 1 0 0\nFR 0 1 0 0 29.9 0\nXQ\nEN\n"),
        );
        Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--output-format", "json", "--sweep-config"])
            .arg(&*cfg)
            .arg(&*deck)
            .output()
            .expect("run fnec")
    };
    let partial = run("partial", "29.9, 1e-200, 30.4");
    assert!(!partial.status.success(), "a failed point exits non-zero");
    let recs: Vec<serde_json::Value> = serde_json::from_str(&stdout(&partial)).expect("JSON");
    assert!(
        recs.len() == 2 && recs.iter().all(|r| json_table(r).is_none()),
        "{recs:?}"
    );
    assert!(
        stderr(&partial).contains("input-impedance print (INPUT_IMPEDANCE) omitted")
            && stderr(&partial).contains("0.000000"),
        "{}",
        stderr(&partial)
    );
    let whole = json(&run("whole", "29.9, 30.4"));
    assert!(
        whole.len() == 2 && whole.iter().all(|r| json_table(r).is_some()),
        "{whole:?}"
    );
}

/// The NEC-2D layout would carry the table, and has no format for it yet:
/// refused before solving. The same deck without the digit, or at one
/// frequency, is printed.
#[test]
fn nec2d_refuses_a_deck_that_would_print_the_table() {
    let args = ["--exec", "cpu", "--output-format", "nec2d"];
    let o = fnec(
        "nec2d",
        &format!("{DIPOLE}EX 0 1 6 1 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        &args,
    );
    assert!(
        !o.status.success() && stderr(&o).contains("an input-impedance table"),
        "{}",
        stderr(&o)
    );
    assert!(stdout(&o).is_empty(), "no partial NEC-2D report");
    for (name, body) in [
        (
            "nec2d_i4_0",
            format!("{DIPOLE}EX 0 1 6 0 1 0 50\nFR 0 2 0 0 29.9 1\n"),
        ),
        (
            "nec2d_one",
            format!("{DIPOLE}EX 0 1 6 1 1 0 50\nFR 0 1 0 0 29.9 0\n"),
        ),
    ] {
        let ok = fnec(name, &body, &args);
        assert!(
            ok.status.success() && !stdout(&ok).is_empty(),
            "{name}: {}",
            stderr(&ok)
        );
    }
}
