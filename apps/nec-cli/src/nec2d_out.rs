// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `--output-format nec2d` (PAR-011): fnec's solve printed in NEC-2D's layout.
//!
//! [`nec_report::nec2d`] formats; this builds what it formats from the deck, its
//! own geometry and the points the solve returned. Nothing here solves: every
//! number is a deck value, a geometric fact about the deck's segments, or a field
//! of a `PointReport`. What NEC-2 computes from the geometry for its own
//! bookkeeping — segment connection data, junction lists, the symmetry flag — is
//! reproduced from NEC-2's `CONNECT` and `DATAGN` (`nec2dx.f`), so the tables read
//! as the kernel's would.
//!
//! What NEC-2's output would print and fnec cannot (yet) produce is refused before
//! the solve, by [`check_admissible`], so a partial block never reaches a reader
//! that takes it as the kernel's.

use nec_model::card::Card;
use nec_model::deck::{NecDeck, ProgramCard};
use nec_report::nec2d::{
    CurrentRow, EchoCard, Environment, FeedRow, LoadRow, PatternRequest, Point, Run, SegmentRow,
    Structure, StructureLine, WireRow,
};
use nec_report::PointReport;
use nec_solver::geometry::Segment;
use num_complex::Complex64;

/// What NEC-2's card machine would execute that fnec's one solve per deck does
/// not, or what its output would print that fnec does not compute: refused
/// before solving.
pub(crate) fn check_admissible(deck: &NecDeck) -> Result<(), String> {
    let refuse = |what: String| {
        Err(format!(
            "--output-format nec2d: {what}, which the NEC-2D output cannot print yet (PAR-011)"
        ))
    };
    for card in &deck.cards {
        match card {
            Card::Ga(_) | Card::Gh(_) | Card::Gc(_) => {
                return refuse("a wire arc, helix or taper (GA, GH, GC)".to_string())
            }
            Card::Tl(_) | Card::Nt(_) => {
                return refuse(
                    "a transmission line or network (TL, NT), whose loss fnec does not separate"
                        .to_string(),
                )
            }
            Card::Pt(_) => return refuse("a current print control (PT)".to_string()),
            Card::Nh(_) => return refuse("a near magnetic field (NH)".to_string()),
            Card::Gn(g) if g.ground_type == 2 => {
                return refuse("a Sommerfeld ground (GN 2)".to_string())
            }
            Card::Ex(ex) if ex.excitation_type != 0 => {
                return refuse(format!(
                    "an excitation of type {} (only EX 0, a voltage source, so far)",
                    ex.excitation_type
                ))
            }
            Card::Ld(ld) if matches!(ld.load_type, 2 | 3) => {
                return refuse("a per-length RLC load (LD 2, LD 3)".to_string())
            }
            _ => {}
        }
    }
    // FND-309: NEC-2 prints its INPUT IMPEDANCE DATA table when EX I4's units
    // digit asks for it and the FR card sweeps two or more frequencies.
    let sweeps = deck
        .cards
        .iter()
        .any(|c| matches!(c, Card::Fr(fr) if fr.steps > 1));
    if sweeps && nec_solver::input_impedance_print(deck).is_some() {
        return refuse(
            "an input-impedance table over the sweep (EX I4 units digit 1, FR I2 > 1)".to_string(),
        );
    }
    // One execution, as fnec solves: one FR, then exactly one card that executes
    // (RP, NE or XQ), and nothing after it but EN.
    let cards = &deck.program_cards;
    let fr = cards.iter().filter(|c| c.mnemonic == "FR").count();
    if fr > 1 {
        return refuse("more than one FR card".to_string());
    }
    let exec: Vec<usize> = cards
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.mnemonic.as_str(), "RP" | "NE" | "XQ"))
        .map(|(k, _)| k)
        .collect();
    let [one] = exec.as_slice() else {
        return refuse(format!(
            "{} cards that execute (RP, NE, XQ), where fnec solves a deck once",
            exec.len()
        ));
    };
    if cards[one + 1..].iter().any(|c| c.mnemonic != "EN") {
        return refuse("a card after the one that executes, other than EN".to_string());
    }
    if cards.iter().any(|c| c.fields.is_none()) {
        return refuse("a card field that is not a number".to_string());
    }
    if let Some(rp) = cards.iter().find(|c| c.mnemonic == "RP") {
        let (ints, _) = rp.fields.expect("checked above");
        let xnda = ints[3];
        let (x, n, d, a) = (xnda / 1000 % 10, xnda / 100 % 10, xnda / 10 % 10, xnda % 10);
        if ints[0] != 0 {
            return refuse("a ground-wave pattern (RP I1 != 0)".to_string());
        }
        if x != 1 {
            return refuse("major/minor-axis gains (RP XNDA X = 0)".to_string());
        }
        if n != 0 {
            return refuse("a normalized gain table (RP XNDA N != 0)".to_string());
        }
        if d != 0 {
            return refuse("directive gain (RP XNDA D = 1)".to_string());
        }
        if a == 2 {
            return refuse("a pattern whose print is suppressed (RP XNDA A = 2)".to_string());
        }
    }
    Ok(())
}

/// The comment cards' text as NEC-2 reads it: columns 3 to 78 of each `CM`/`CE`
/// card before the geometry, tabs and all.
fn comments(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('\'') {
            continue;
        }
        let head: String = line
            .chars()
            .take(2)
            .collect::<String>()
            .to_ascii_uppercase();
        if head != "CM" && head != "CE" {
            break;
        }
        out.push(line.chars().skip(2).collect());
        if head == "CE" {
            break;
        }
    }
    out
}

/// NEC-2's connection data for the deck's segments (`CONNECT`): for each end, the
/// first other segment — searched from the next one up, wrapping round — with an
/// end within 1e-3 of the segment's length (by the sum of coordinate differences);
/// signed `-j` for a join to `j`'s first end at this first end, `+j` for its
/// second, and the other way round at the second end; the segment's own number
/// where an end touches a ground plane; 0 when free.
fn connections(segs: &[Segment], ground: bool) -> Vec<(i64, i64)> {
    let n = segs.len();
    let sep =
        |a: [f64; 3], b: [f64; 3]| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let s = &segs[i];
        let slen = s.length * nec_solver::geometry::NEC_SMIN;
        let search = |p: [f64; 3], first_end_sign: i64| -> i64 {
            for k in 1..n {
                let j = (i + k) % n;
                if sep(p, segs[j].start) <= slen {
                    return first_end_sign * (j as i64 + 1);
                }
                if sep(p, segs[j].end) <= slen {
                    return -first_end_sign * (j as i64 + 1);
                }
            }
            0
        };
        let touches = |p: [f64; 3]| ground && p[2].abs() <= slen;
        let icon1 = if touches(s.start) {
            i as i64 + 1
        } else {
            search(s.start, -1)
        };
        let icon2 = if touches(s.end) {
            i as i64 + 1
        } else {
            search(s.end, 1)
        };
        out.push((icon1, icon2));
    }
    out
}

/// NEC-2's junctions of three or more segment ends, as `CONNECT` walks them:
/// from each end of segment `J`, follow the connection data from segment to
/// segment — a join to the next one's first end keeps the end sign, a join to its
/// second end flips it — until the walk returns to `J`. A walk that meets a
/// lower-numbered segment was printed from there; one that ends at a free or
/// grounded end is no ring. `-j` names a first end, `+j` a second.
fn junctions(icon: &[(i64, i64)]) -> Vec<Vec<i64>> {
    let mut out = Vec::new();
    for j in 1..=icon.len() as i64 {
        for start_end in [-1_i64, 1] {
            let mut jco = vec![start_end * j];
            let mut jend = start_end;
            let own = icon[j as usize - 1];
            let mut ix = if start_end < 0 { own.0 } else { own.1 };
            let ring = loop {
                if ix == 0 || ix == j || ix > 10_000 {
                    break false;
                }
                if ix < 0 {
                    ix = -ix;
                } else {
                    jend = -jend;
                }
                if ix == j {
                    break true;
                }
                if ix < j || jco.len() >= 30 {
                    break false;
                }
                jco.push(ix * jend);
                let c = icon[ix as usize - 1];
                ix = if jend == 1 { c.1 } else { c.0 };
            };
            if ring && jco.len() >= 3 {
                out.push(jco);
            }
        }
    }
    out
}

/// The structure table, the segmentation and NEC-2's symmetry bookkeeping.
pub(crate) fn structure(text: &str, deck: &NecDeck) -> Result<(Structure, Vec<Segment>), String> {
    let segs = nec_solver::build_deck_geometry(deck).map_err(|e| e.to_string())?;
    let ge_flag = deck
        .cards
        .iter()
        .find_map(|c| match c {
            Card::Ge(g) => Some(g.ground_reflection_flag),
            _ => None,
        })
        .unwrap_or(0);
    let mut lines = Vec::new();
    // NP and IPSYM as DATAGN keeps them: a wire resets the cell to the whole
    // structure; a reflection makes the structure so far the cell.
    let (mut np, mut ipsym) = (0_u32, 0_i32);
    let count = |upto: usize| -> Result<u32, String> {
        // Before the first wire there are no segments (and no geometry to build).
        if !deck.cards[..upto].iter().any(|c| matches!(c, Card::Gw(_))) {
            return Ok(0);
        }
        let mut prefix = NecDeck::new();
        prefix.cards = deck.cards[..upto].to_vec();
        Ok(nec_solver::build_deck_geometry(&prefix)
            .map_err(|e| e.to_string())?
            .len() as u32)
    };
    for (k, card) in deck.cards.iter().enumerate() {
        match card {
            Card::Gw(w) => {
                let before = count(k)?;
                lines.push(StructureLine::Wire(WireRow {
                    tag: w.tag,
                    start: w.start,
                    end: w.end,
                    radius: w.radius,
                    segments: w.segments,
                    first: before + 1,
                    last: before + w.segments,
                }));
                np = before + w.segments;
                ipsym = 0;
            }
            Card::Gm(m) => lines.push(StructureLine::Moved {
                i1: i64::from(m.tag_increment),
                i2: i64::from(m.repeat_count),
                reals: [
                    m.rot_x_deg,
                    m.rot_y_deg,
                    m.rot_z_deg,
                    m.translate_x,
                    m.translate_y,
                    m.translate_z,
                    f64::from(m.start_tag),
                ],
            }),
            Card::Gx(x) => {
                np = count(k)?;
                ipsym = if x.z { 2 } else { 1 };
                lines.push(StructureLine::Reflected {
                    axes: [x.x, x.y, x.z],
                    tag_increment: i64::from(x.tag_increment),
                });
            }
            Card::Gr(r) => {
                np = count(k)?;
                ipsym = -1;
                lines.push(StructureLine::Rotated {
                    copies: i64::from(r.occurrences),
                    tag_increment: i64::from(r.tag_increment),
                });
            }
            Card::Gs(s) => lines.push(StructureLine::Scaled(s.scale)),
            _ => {}
        }
    }
    let n = segs.len() as u32;
    if np == n {
        ipsym = 0;
    }
    // A wire end touches the ground only where GE asks for it (I1 >= 1).
    let icon = connections(&segs, ge_flag >= 1);
    let segments = segs
        .iter()
        .zip(&icon)
        .map(|(s, &(icon1, icon2))| {
            let d = [
                s.end[0] - s.start[0],
                s.end[1] - s.start[1],
                s.end[2] - s.start[2],
            ];
            SegmentRow {
                centre: s.midpoint,
                length: s.length,
                alpha_deg: (d[2] / s.length).clamp(-1.0, 1.0).asin().to_degrees(),
                beta_deg: if d[0] == 0.0 && d[1] == 0.0 {
                    0.0
                } else {
                    d[1].atan2(d[0]).to_degrees()
                },
                radius: s.radius,
                icon1,
                icon2,
                tag: s.tag,
            }
        })
        .collect();
    Ok((
        Structure {
            comments: comments(text),
            lines,
            ge_flag,
            symmetric_cell: np,
            symmetry_flag: ipsym,
            junctions: junctions(&icon),
            segments,
        },
        segs,
    ))
}

fn echo_card(c: &ProgramCard) -> EchoCard {
    let (ints, floats) = c.fields.expect("check_admissible refuses unreadable cards");
    EchoCard {
        mnemonic: c.mnemonic.clone(),
        ints,
        floats,
    }
}

/// The echo before and after execution. The kernel reads the end of the file as
/// `EN`, and echoes an `EN` with the previous card's fields (pinned against it in
/// `tests/par011_card_echo.rs`).
fn echoes(deck: &NecDeck) -> (Vec<EchoCard>, Vec<EchoCard>, bool) {
    let cards = &deck.program_cards;
    let exec = cards
        .iter()
        .position(|c| matches!(c.mnemonic.as_str(), "RP" | "NE" | "XQ"))
        .expect("check_admissible requires one");
    let before: Vec<EchoCard> = cards[..=exec].iter().map(echo_card).collect();
    let last = before.last().expect("one card").clone();
    let en = EchoCard {
        mnemonic: "EN".to_string(),
        ..last
    };
    let from_end_of_file = cards.last().map_or(true, |c| c.mnemonic != "EN");
    (before, vec![en], from_end_of_file)
}

/// NEC-2's loading table rows from the deck's `LD` cards.
fn load_rows(deck: &NecDeck, segs: &[Segment]) -> Vec<LoadRow> {
    deck.cards
        .iter()
        .filter_map(|c| match c {
            Card::Ld(ld) => Some(ld),
            _ => None,
        })
        .map(|ld| {
            let (values, kind) = match ld.load_type {
                0 => ([ld.f1, ld.f2, ld.f3, 0.0, 0.0, 0.0], " SERIES "),
                1 => ([ld.f1, ld.f2, ld.f3, 0.0, 0.0, 0.0], "PARALLEL"),
                4 => ([0.0, 0.0, 0.0, ld.f1, ld.f2, 0.0], "FIXED IMPEDANCE "),
                _ => ([0.0, 0.0, 0.0, 0.0, 0.0, ld.f1], "  WIRE  "),
            };
            LoadRow {
                tag: i64::from(ld.tag),
                from: i64::from(ld.seg_first),
                thru: i64::from(ld.seg_last),
                segments: segs
                    .iter()
                    .filter(|s| {
                        nec_solver::deck_segment_in_range(s, ld.tag, ld.seg_first, ld.seg_last)
                    })
                    .map(|s| s.abs_index)
                    .collect(),
                values,
                kind,
            }
        })
        .collect()
}

fn environment(deck: &NecDeck, freq_hz: f64) -> Environment {
    let gn = deck.cards.iter().rev().find_map(|c| match c {
        Card::Gn(g) => Some(g),
        _ => None,
    });
    match gn {
        None => Environment::FreeSpace,
        Some(g) if g.ground_type == -1 => Environment::FreeSpace,
        Some(g) if g.ground_type == 1 => Environment::PerfectGround,
        Some(g) => {
            let eps_r = g.eps_r.unwrap_or(0.0);
            let sigma = g.sigma.unwrap_or(0.0);
            let omega_eps0 = 2.0 * std::f64::consts::PI * freq_hz * 8.854_187_812_8e-12;
            Environment::FiniteGround {
                eps_r,
                sigma,
                complex: Complex64::new(eps_r, -sigma / omega_eps0),
                sommerfeld: g.ground_type == 2,
            }
        }
    }
}

/// The index of the deck segment `(tag, segment)` names, NEC-2's number less one.
fn global(segs: &[Segment], tag: usize, seg: usize) -> Option<usize> {
    segs.iter()
        .position(|s| s.tag as usize == tag && s.tag_index as usize == seg)
}

/// The pattern request of the deck's `RP`, and its average gain when asked.
fn pattern_request(deck: &NecDeck, report: &PointReport) -> Option<PatternRequest> {
    let rp = deck.program_cards.iter().find(|c| c.mnemonic == "RP")?;
    let (ints, floats) = rp.fields?;
    let a = ints[3] % 10;
    let average = (a != 0).then(|| {
        // The solid angle averaged over, in units of π, as 4nec2's kernel prints
        // it: the directions the request covers, each counted once — a θ range
        // through negative angles covers the same polar band twice.
        let (nth, nph) = (f64::from(ints[1]), f64::from(ints[2]));
        let (th0, dth, dph) = (floats[0], floats[2], floats[3]);
        let th1 = th0 + (nth - 1.0) * dth;
        let (lo, hi) = if th0 < 0.0 {
            (0.0, (-th0).max(th1))
        } else {
            (th0, th1)
        };
        let span_phi = ((nph - 1.0) * dph).min(360.0).to_radians();
        let band = lo.to_radians().cos() - hi.min(180.0).to_radians().cos();
        // fnec's average power gain is already linear.
        (
            report.avg_power_gain.unwrap_or(0.0),
            span_phi * band / std::f64::consts::PI,
        )
    });
    Some(PatternRequest {
        axes: ints[3] / 1000 % 10,
        directive: ints[3] / 10 % 10 != 0,
        average,
    })
}

/// The whole report: one block per solved point (`elapsed` in seconds each).
pub(crate) fn render(
    text: &str,
    deck: &NecDeck,
    points: &[(PointReport, f64)],
    run_time_s: f64,
) -> Result<String, String> {
    let (structure, segs) = structure(text, deck)?;
    let (before, after, en_from_end_of_file) = echoes(deck);
    let loads = load_rows(deck, &segs);
    let mut blocks = Vec::new();
    for (report, elapsed) in points {
        let lambda = 299_792_458.0 / report.frequency_hz;
        let feeds = report
            .rows
            .iter()
            .map(|r| {
                let idx = global(&segs, r.tag, r.seg).ok_or("a feed on no deck segment")?;
                Ok(FeedRow {
                    tag: r.tag as u32,
                    segment: idx as u32 + 1,
                    voltage: r.v_source,
                    current: r.current,
                    power_w: 0.5 * (r.v_source * r.current.conj()).re,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let currents = report
            .current_table
            .iter()
            .map(|r| {
                let idx = global(&segs, r.tag, r.seg).ok_or("a current on no deck segment")?;
                let s = &segs[idx];
                Ok(CurrentRow {
                    segment: idx as u32 + 1,
                    tag: s.tag,
                    centre_wl: s.midpoint.map(|v| v / lambda),
                    length_wl: s.length / lambda,
                    current: r.current,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        if report.pattern_table.iter().any(|r| r.e_field.is_none()) {
            return Err(
                "--output-format nec2d: the pattern was computed on the GPU, which keeps \
                        gains only; run with --exec cpu"
                    .to_string(),
            );
        }
        blocks.push(Point {
            report,
            loads: &loads,
            environment: environment(deck, report.frequency_hz),
            // fnec does not time the fill and the factorization apart: the point's
            // whole solve is printed as FILL.
            fill_s: *elapsed,
            factor_s: 0.0,
            feeds,
            currents,
            pattern: pattern_request(deck, report),
            near_field: deck.cards.iter().any(|c| matches!(c, Card::Ne(_))),
        });
    }
    let notes = vec![format!(
        "SOLVED BY FNEC {} (NOT NEC-2): HALLEN/MPIE FORMULATION, NEC-2D OUTPUT LAYOUT",
        crate::VERSION
    )];
    Ok(nec_report::nec2d::render(&Run {
        structure: &structure,
        before: &before,
        points: blocks,
        after: &after,
        en_from_end_of_file,
        notes: &notes,
        run_time_s,
    }))
}
