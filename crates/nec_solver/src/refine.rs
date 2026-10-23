// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! A deck meshed `S` times finer, every card that names a segment naming the
//! sub-segment that models the same thing (`fnec converge`, GAP-012).
//!
//! Every deck segment becomes `S` equal sub-segments (`S` odd, so a segment
//! centre stays a segment centre), and a reference `k` on a wire becomes
//! `S·k − (S−1)/2`, the sub-segment whose midpoint is the original's — the rule
//! `scripts/nec2c-fine-mesh.py` applies to every reference when it captures a
//! nec2c reference at a converged mesh (FND-232). The two are a twin on purpose:
//! the script writes text for nec2c, this works on the parsed deck for fnec, and
//! `converge_contract.rs` holds them to the same answer.
//!
//! One rule differs, by choice ([`EndRule`]). A reference on a wire's first or
//! last segment models a gap *at* the wire's end — a feed beside a junction, a
//! load beside a bend, a base feed at the ground. Centred refinement moves such a
//! gap off the node, four sub-segments away at 9×, and what the deck models is no
//! longer solved (FND-278: "no gap touches the node, the fix is inert"). So the
//! default keeps an end reference on the sub-segment still touching the end, and
//! says so: the gap then moves toward the node as the mesh refines, and the series
//! may have no limit (FND-277, FND-278). `EndRule::Centre` is the script's rule.

use nec_model::card::{Card, FeedpointRole, LdCard};
use nec_model::deck::NecDeck;

use crate::geometry::{build_deck_geometry, find_deck_segment, wire_endpoints_from_segs, Segment};

/// What happens to a reference on a wire's first or last segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndRule {
    /// It stays on the sub-segment touching the wire end: the deck's gap at the
    /// node stays at the node (the default).
    Keep,
    /// It takes the centre sub-segment like any other: the gap keeps its position
    /// and leaves the node (`scripts/nec2c-fine-mesh.py`'s rule).
    Centre,
}

impl EndRule {
    /// The `--ends` word.
    pub fn label(self) -> &'static str {
        match self {
            EndRule::Keep => "keep",
            EndRule::Centre => "centre",
        }
    }
}

/// One segment reference and where refinement sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedRef {
    /// The card's mnemonic: `EX`, `LD`, `NT` or `TL`.
    pub card: &'static str,
    pub tag: u32,
    /// The deck's segment number (a port's `0` already resolved to its centre).
    pub segment: u32,
    /// The refined deck's segment number.
    pub mapped: u32,
    /// Whether the deck's segment is the first or last of its wire.
    pub touches_end: bool,
}

/// A refined deck, the references it moved and what a user should know.
#[derive(Debug, Clone)]
pub struct Refined {
    pub deck: NecDeck,
    pub refs: Vec<MappedRef>,
    /// Notes in deck order, each once: a dropped card class, a re-interpolated
    /// taper, a reference that touches a wire end.
    pub notes: Vec<String>,
}

/// The sub-segment of refined segment `k` whose midpoint is the original's.
fn centre(factor: u32, k: u32) -> u32 {
    factor * k - (factor - 1) / 2
}

/// Refuse a reference with tag 0 and a segment number — NEC-2's absolute segment
/// number, which fnec does not read so yet (FND-282). The one place every card's
/// tag-0 reference comes through (`EX`, `LD`, `NT`, `TL`), so that when
/// `find_deck_segment` resolves the absolute number this becomes a mapping here
/// and nowhere else. `shown` is the card as the user wrote its reference.
fn refuse_absolute(tag: u32, seg: u32, shown: impl FnOnce() -> String) -> Result<(), String> {
    if tag == 0 && seg != 0 {
        return Err(format!(
            "refine: `{}` names an absolute segment number (tag 0), which refinement does \
             not map yet — give the wire a tag",
            shown()
        ));
    }
    Ok(())
}

/// The mapper: resolves a reference on the unrefined geometry, and knows the
/// wire ends.
struct Mapper<'a> {
    segs: &'a [Segment],
    wires: Vec<(usize, usize)>,
    factor: u32,
    ends: EndRule,
}

impl Mapper<'_> {
    fn map(&self, card: &'static str, tag: u32, k: u32) -> Result<MappedRef, String> {
        refuse_absolute(tag, k, || format!("{card} 0 {k}"))?;
        let idx = find_deck_segment(self.segs, tag, k)
            .ok_or_else(|| format!("refine: `{card}` names no segment: tag {tag} segment {k}"))?;
        let (first, last) = self
            .wires
            .iter()
            .copied()
            .find(|&(f, l)| (f..=l).contains(&idx))
            .ok_or_else(|| format!("refine: tag {tag} segment {k} belongs to no wire"))?;
        let (at_start, at_end) = (idx == first, idx == last);
        let s = self.factor;
        let mapped = match self.ends {
            EndRule::Keep if at_start && at_end => {
                return Err(format!(
                    "refine: tag {tag} has one segment; its `{card}` reference touches both \
                     ends — give the wire two segments or use `--ends centre`"
                ));
            }
            EndRule::Keep if at_start => s * (k - 1) + 1,
            EndRule::Keep if at_end => s * k,
            _ => centre(s, k),
        };
        Ok(MappedRef {
            card,
            tag,
            segment: k,
            mapped,
            touches_end: at_start || at_end,
        })
    }
}

/// The per-metre load types, whose range covers wire length rather than points:
/// `LD 2`/`3` (per metre since FND-280) and `LD 5` (conductivity).
fn per_metre(load_type: i32) -> bool {
    matches!(load_type, 2 | 3 | 5)
}

/// `deck` meshed `factor` times finer (odd, at least 1); `factor` 1 is the deck
/// itself, less the cards that do not change its feedpoint.
///
/// Refused: an even factor; a receiving antenna (`EX 1`/`2`/`3` names no
/// segment); a reference with tag 0 and a segment number (an absolute segment
/// number, which fnec reads otherwise — FND-282); a reference naming no segment;
/// under [`EndRule::Keep`], a reference into a one-segment wire; a geometry the
/// deck cannot build.
pub fn refine_deck(deck: &NecDeck, factor: u32, ends: EndRule) -> Result<Refined, String> {
    if factor == 0 || factor % 2 == 0 {
        return Err(format!(
            "refine: the factor must be an odd integer, got {factor}: an odd factor keeps a \
             segment centre at a segment centre"
        ));
    }
    let segs = build_deck_geometry(deck).map_err(|e| format!("refine: {e}"))?;
    let mapper = Mapper {
        segs: &segs,
        wires: wire_endpoints_from_segs(&segs),
        factor,
        ends,
    };
    let s = factor;
    let scaled = |n: u32, what: &str| -> Result<u32, String> {
        n.checked_mul(s)
            .ok_or_else(|| format!("refine: {what} with {n} segments overflows at {s}×"))
    };

    let mut cards = Vec::with_capacity(deck.cards.len());
    let mut refs = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let note = |text: String, notes: &mut Vec<String>| {
        if !notes.contains(&text) {
            notes.push(text);
        }
    };
    let mut dropped: Vec<&str> = Vec::new();

    for card in &deck.cards {
        match card {
            Card::Gw(gw) => {
                let mut gw = gw.clone();
                gw.segments = scaled(gw.segments, "GW")?;
                cards.push(Card::Gw(gw));
            }
            Card::Ga(ga) => {
                let mut ga = ga.clone();
                ga.segments = scaled(ga.segments, "GA")?;
                cards.push(Card::Ga(ga));
            }
            Card::Gh(gh) => {
                let mut gh = gh.clone();
                gh.segments = scaled(gh.segments, "GH")?;
                cards.push(Card::Gh(gh));
            }
            Card::Gc(gc) => {
                let mut gc = gc.clone();
                if s > 1 {
                    // Each sub-segment is the S-th root of the step longer than the
                    // one before, so the sub-segments tile the deck's segments
                    // exactly (the two geometric series sum to the same length).
                    // The radii run geometrically over S·n instead of n, so a
                    // sub-segment's radius is the deck's interpolated, not its
                    // parent's: at most a quarter of a taper step apart, at the
                    // wire ends.
                    gc.length_ratio = gc.length_ratio.powf(1.0 / f64::from(s));
                    note(
                        "GC: the taper's length step is taken to the 1/S power and its radii \
                         re-interpolated over the finer mesh, so a sub-segment's radius differs \
                         from its deck segment's by up to a quarter of a taper step at the wire ends"
                            .to_string(),
                        &mut notes,
                    );
                }
                cards.push(Card::Gc(gc));
            }
            Card::Ex(ex) => match ex.kind().feedpoint_role() {
                FeedpointRole::DeltaGap | FeedpointRole::CurrentSource => {
                    refuse_absolute(ex.tag, ex.segment, || {
                        format!("EX {} 0 {}", ex.excitation_type, ex.segment)
                    })?;
                    let m = mapper.map("EX", ex.tag, ex.segment)?;
                    let mut ex = ex.clone();
                    ex.segment = m.mapped;
                    refs.push(m);
                    cards.push(Card::Ex(ex));
                }
                FeedpointRole::PlaneWave => {
                    return Err(format!(
                        "refine: this deck is a receiving antenna (EX type {}); there is no \
                         feedpoint impedance to converge",
                        ex.excitation_type
                    ));
                }
                // Refused by `pre_solve_error` as every frontend refuses it.
                FeedpointRole::Unknown => cards.push(card.clone()),
            },
            Card::Ld(ld) => {
                refuse_absolute(ld.tag, ld.seg_first, || {
                    format!("LD {} 0 {} {}", ld.load_type, ld.seg_first, ld.seg_last)
                })?;
                if per_metre(ld.load_type) {
                    let mut ld = ld.clone();
                    if ld.seg_first != 0 {
                        // The same length of wire: the whole of each deck segment.
                        let last = if ld.seg_last == 0 {
                            ld.seg_first
                        } else {
                            ld.seg_last
                        };
                        for k in [ld.seg_first, last] {
                            if find_deck_segment(&segs, ld.tag, k).is_none() {
                                return Err(format!(
                                    "refine: `LD {}` names no segment: tag {} segment {k}",
                                    ld.load_type, ld.tag
                                ));
                            }
                        }
                        ld.seg_first = s * (ld.seg_first - 1) + 1;
                        ld.seg_last = s * last;
                    }
                    cards.push(Card::Ld(ld));
                } else if matches!(ld.load_type, 0 | 1 | 4) {
                    // A lumped load sits on each segment of its range, once. The
                    // range expands to one card per deck segment, each on the
                    // sub-segment that models it — never S loads per segment.
                    let targets: Vec<&Segment> = segs
                        .iter()
                        .filter(|seg| crate::loads::segment_matches(ld, seg))
                        .collect();
                    if targets.is_empty() {
                        return Err(format!(
                            "refine: `LD {}` names no segment: tag {} segments {}..{}",
                            ld.load_type, ld.tag, ld.seg_first, ld.seg_last
                        ));
                    }
                    for seg in targets {
                        let m = mapper.map("LD", seg.tag, seg.tag_index)?;
                        cards.push(Card::Ld(LdCard {
                            load_type: ld.load_type,
                            tag: seg.tag,
                            seg_first: m.mapped,
                            seg_last: m.mapped,
                            f1: ld.f1,
                            f2: ld.f2,
                            f3: ld.f3,
                        }));
                        refs.push(m);
                    }
                } else {
                    // An unsupported type: `pre_solve_error` refuses it by name.
                    cards.push(card.clone());
                }
            }
            Card::Tl(tl) => {
                let mut tl = tl.clone();
                let shown =
                    || format!("TL {} {} {} {}", tl.tag1, tl.segment1, tl.tag2, tl.segment2);
                refuse_absolute(tl.tag1, tl.segment1, shown)?;
                refuse_absolute(tl.tag2, tl.segment2, shown)?;
                let a = map_port(&mapper, "TL", tl.tag1, tl.segment1)?;
                let b = map_port(&mapper, "TL", tl.tag2, tl.segment2)?;
                tl.segment1 = a.mapped;
                tl.segment2 = b.mapped;
                refs.push(a);
                refs.push(b);
                cards.push(Card::Tl(tl));
            }
            Card::Nt(nt) => {
                let mut nt = nt.clone();
                let ports: Option<Vec<u32>> = nt
                    .raw_fields
                    .iter()
                    .take(4)
                    .map(|f| f.parse::<u32>().ok())
                    .collect();
                // Too few or non-numeric fields are refused at the solve by name.
                if let Some([t1, s1, t2, s2]) = ports.as_deref() {
                    let shown = || format!("NT {t1} {s1} {t2} {s2}");
                    refuse_absolute(*t1, *s1, shown)?;
                    refuse_absolute(*t2, *s2, shown)?;
                    let a = map_port(&mapper, "NT", *t1, *s1)?;
                    let b = map_port(&mapper, "NT", *t2, *s2)?;
                    nt.raw_fields[1] = a.mapped.to_string();
                    nt.raw_fields[3] = b.mapped.to_string();
                    refs.push(a);
                    refs.push(b);
                }
                cards.push(Card::Nt(nt));
            }
            Card::Pt(_) => dropped.push("PT"),
            Card::Rp(_) => dropped.push("RP"),
            Card::Ne(_) => dropped.push("NE"),
            Card::Nh(_) => dropped.push("NH"),
            // Names no segment, or scales / copies / reflects the structure so
            // that NEC's per-tag numbering carries the map into every copy.
            Card::Comment(_)
            | Card::Gm(_)
            | Card::Gr(_)
            | Card::Gs(_)
            | Card::Gx(_)
            | Card::Ge(_)
            | Card::Gn(_)
            | Card::Fr(_)
            | Card::En(_) => cards.push(card.clone()),
        }
    }
    if !dropped.is_empty() {
        dropped.sort_unstable();
        dropped.dedup();
        note(
            format!(
                "{} dropped: a pattern, near-field or print request does not change the \
                 feedpoint impedance",
                dropped.join(", ")
            ),
            &mut notes,
        );
    }
    for r in &refs {
        if r.touches_end {
            let text = match ends {
                EndRule::Keep => format!(
                    "`{}` on tag {} segment {} touches a wire end (a junction, bend, ground \
                     contact or free end); `--ends keep` keeps it on the sub-segment touching \
                     the end, so the gap moves toward the node as the mesh refines and this \
                     series may have no limit (FND-277, FND-278) — `--ends centre` studies \
                     the gap at a fixed position instead",
                    r.card, r.tag, r.segment
                ),
                EndRule::Centre => format!(
                    "`{}` on tag {} segment {} touches a wire end; `--ends centre` moves it to \
                     the centre sub-segment, off the node, so the refined decks model a gap \
                     the deck does not (FND-278)",
                    r.card, r.tag, r.segment
                ),
            };
            note(text, &mut notes);
        }
    }
    Ok(Refined {
        deck: NecDeck {
            cards,
            program_cards: deck.program_cards.clone(),
        },
        refs,
        notes,
    })
}

/// A network port: `0` is the tag's centre segment by the `TL`/`NT` rule, resolved
/// on the deck's own mesh before it is mapped (the refined mesh's centre is not the
/// deck's centre's sub-segment when the count is even).
fn map_port(
    mapper: &Mapper<'_>,
    card: &'static str,
    tag: u32,
    seg: u32,
) -> Result<MappedRef, String> {
    let seg = if seg == 0 {
        crate::tl::find_segment_index(mapper.segs, tag, 0)
            .map(|(_, resolved, _)| resolved)
            .ok_or_else(|| format!("refine: `{card}` port names no segment: tag {tag}"))?
    } else {
        seg
    };
    mapper.map(card, tag, seg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(text: &str) -> NecDeck {
        nec_parser::parse(text).expect("parses").deck
    }

    fn gw_segments(d: &NecDeck) -> Vec<u32> {
        d.cards
            .iter()
            .filter_map(|c| match c {
                Card::Gw(gw) => Some(gw.segments),
                _ => None,
            })
            .collect()
    }

    fn ex_segments(d: &NecDeck) -> Vec<u32> {
        d.cards
            .iter()
            .filter_map(|c| match c {
                Card::Ex(ex) => Some(ex.segment),
                _ => None,
            })
            .collect()
    }

    fn ld_ranges(d: &NecDeck) -> Vec<(u32, u32, u32)> {
        d.cards
            .iter()
            .filter_map(|c| match c {
                Card::Ld(ld) => Some((ld.tag, ld.seg_first, ld.seg_last)),
                _ => None,
            })
            .collect()
    }

    const DIPOLE: &str = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\n\
                          FR 0 1 0 0 14.2 0.0\nEN\n";

    /// Segment 26 of 51 is the middle third, 77 of 153 (the script's own check),
    /// 230 of 459 at 9×; factor 1 is the deck; an even factor is refused.
    #[test]
    fn an_interior_reference_takes_the_centre_sub_segment() {
        let d = deck(DIPOLE);
        for (s, n, k) in [(1, 51, 26), (3, 153, 77), (9, 459, 230)] {
            let r = refine_deck(&d, s, EndRule::Keep).unwrap();
            assert_eq!(gw_segments(&r.deck), [n], "{s}x");
            assert_eq!(ex_segments(&r.deck), [k], "{s}x");
            assert_eq!(r.refs.len(), 1);
            assert!(!r.refs[0].touches_end);
            assert!(r.notes.is_empty(), "{:?}", r.notes);
        }
        assert_eq!(refine_deck(&d, 1, EndRule::Keep).unwrap().deck, d);
        let err = refine_deck(&d, 2, EndRule::Keep).unwrap_err();
        assert!(err.contains("odd"), "{err}");
    }

    /// A base-fed monopole keeps its feed on the base sub-segment under `keep`
    /// (1 → 1 at 3×) and takes the centre under `centre` (1 → 2); the far end of a
    /// wire keeps to S·n; a one-segment wire is refused under `keep` only.
    #[test]
    fn an_end_reference_keeps_its_end_or_takes_the_centre() {
        let mono = deck(
            "CE\nGW 1 26 0 0 0 0 0 5.282 0.001\nGE 1\nGN 1\nEX 0 1 1 0 1.0 0.0\n\
             FR 0 1 0 0 14.2 0\nEN\n",
        );
        let keep = refine_deck(&mono, 3, EndRule::Keep).unwrap();
        assert_eq!(ex_segments(&keep.deck), [1]);
        assert!(keep.refs[0].touches_end);
        assert!(
            keep.notes
                .iter()
                .any(|n| n.contains("touches a wire end") && n.contains("keep")),
            "{:?}",
            keep.notes
        );
        let centre = refine_deck(&mono, 3, EndRule::Centre).unwrap();
        assert_eq!(ex_segments(&centre.deck), [2]);
        assert!(centre.notes.iter().any(|n| n.contains("centre")));

        let top = deck(
            "CE\nGW 1 10 0 0 -2.64 0 0 0 .001\nGW 2 10 0 0 0 0 0 2.64 .001\nGE 0\n\
             EX 0 1 10 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n",
        );
        assert_eq!(
            ex_segments(&refine_deck(&top, 3, EndRule::Keep).unwrap().deck),
            [30]
        );
        assert_eq!(
            ex_segments(&refine_deck(&top, 9, EndRule::Keep).unwrap().deck),
            [90]
        );
        assert_eq!(
            ex_segments(&refine_deck(&top, 3, EndRule::Centre).unwrap().deck),
            [29]
        );

        let one = deck("CE\nGW 1 1 0 0 0 0 0 1 .001\nGE\nEX 0 1 1 0 1 0\nFR 0 1 0 0 100 0\nEN\n");
        let err = refine_deck(&one, 3, EndRule::Keep).unwrap_err();
        assert!(
            err.contains("one segment") && err.contains("--ends centre"),
            "{err}"
        );
        assert_eq!(
            ex_segments(&refine_deck(&one, 3, EndRule::Centre).unwrap().deck),
            [2]
        );
    }

    /// A lumped load over a range becomes one load per deck segment, each on its
    /// centre sub-segment; a tag-wide one likewise; a per-metre range covers the
    /// same wire; tag 0 with a segment is the absolute number fnec cannot map.
    #[test]
    fn loads_expand_to_points_or_stretch_as_lengths() {
        let base =
            "CE\nGW 1 21 0 0 -5 0 0 5 .001\nGE\nEX 0 1 11 0 1 0\n{LD}FR 0 1 0 0 14.2 0\nEN\n";
        let with = |ld: &str| deck(&base.replace("{LD}", ld));
        let r = refine_deck(&with("LD 0 1 3 7 10 1e-6 0\n"), 3, EndRule::Keep).unwrap();
        assert_eq!(
            ld_ranges(&r.deck),
            [
                (1, 8, 8),
                (1, 11, 11),
                (1, 14, 14),
                (1, 17, 17),
                (1, 20, 20)
            ]
        );
        assert_eq!(r.refs.iter().filter(|m| m.card == "LD").count(), 5);
        let r = refine_deck(&with("LD 4 1 0 0 1 0 0\n"), 3, EndRule::Keep).unwrap();
        let ranges = ld_ranges(&r.deck);
        assert_eq!(ranges.len(), 21);
        // The end segments keep their ends, the rest their centres.
        assert_eq!(ranges[0], (1, 1, 1));
        assert_eq!(ranges[1], (1, 5, 5));
        assert_eq!(ranges[20], (1, 63, 63));
        let r = refine_deck(&with("LD 5 1 3 7 5.8e7 0 0\n"), 3, EndRule::Keep).unwrap();
        assert_eq!(ld_ranges(&r.deck), [(1, 7, 21)]);
        assert!(r.refs.iter().all(|m| m.card != "LD"));
        let r = refine_deck(&with("LD 5 1 0 0 5.8e7 0 0\n"), 3, EndRule::Keep).unwrap();
        assert_eq!(ld_ranges(&r.deck), [(1, 0, 0)]);
        // Per metre since FND-280: the range stretches like LD 5.
        let r = refine_deck(&with("LD 2 1 4 4 0 2.1e-5 0\n"), 3, EndRule::Keep).unwrap();
        assert_eq!(ld_ranges(&r.deck), [(1, 10, 12)]);
        // Tag 0 with a segment number, on every card that names one: one refusal.
        for (card, shown) in [
            ("LD 4 0 5 5 50 0 0\n", "`LD 4 0 5 5`"),
            ("EX 0 0 5 0 1 0\n", "`EX 0 0 5`"),
            ("TL 1 3 0 5 50 0.1\n", "`TL 1 3 0 5`"),
            ("NT 0 5 1 3 0 -0.002 0 0.003 0 -0.002\n", "`NT 0 5 1 3`"),
        ] {
            let err = refine_deck(&with(card), 3, EndRule::Keep).unwrap_err();
            assert_eq!(
                err,
                format!(
                    "refine: {shown} names an absolute segment number (tag 0), which \
                     refinement does not map yet — give the wire a tag"
                ),
                "{card}"
            );
        }
        let err = refine_deck(&with("LD 4 1 30 30 50 0 0\n"), 3, EndRule::Keep).unwrap_err();
        assert!(err.contains("names no segment"), "{err}");
    }

    /// A port's `0` is the tag's centre on the deck's mesh, then mapped — on an
    /// even wire the lower centre (10 of 20 → 29 of 60, not 30); NT's raw fields
    /// are rewritten; a receiving antenna is refused.
    #[test]
    fn ports_resolve_zero_before_mapping_and_a_receive_deck_is_refused() {
        let two = "CE\nGW 1 20 0 0 -5 0 0 5 .001\nGW 2 21 1 0 -5 1 0 5 .001\nGE\n\
                   TL 1 0 2 0 50 0.1\nNT 1 20 2 11 0 -0.002 0 0.003 0 -0.002\n\
                   EX 0 1 10 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
        let r = refine_deck(&deck(two), 3, EndRule::Keep).unwrap();
        let tl = r
            .deck
            .cards
            .iter()
            .find_map(|c| match c {
                Card::Tl(tl) => Some((tl.segment1, tl.segment2)),
                _ => None,
            })
            .unwrap();
        assert_eq!(tl, (29, 32));
        let nt = r
            .deck
            .cards
            .iter()
            .find_map(|c| match c {
                Card::Nt(nt) => Some(nt.raw_fields.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(&nt[..4], ["1", "60", "2", "32"]);
        assert_eq!(nt.len(), 10);
        let ports: Vec<(u32, u32, u32)> = r
            .refs
            .iter()
            .filter(|m| m.card != "EX")
            .map(|m| (m.tag, m.segment, m.mapped))
            .collect();
        assert_eq!(ports, [(1, 10, 29), (2, 11, 32), (1, 20, 60), (2, 11, 32)]);

        let rx =
            deck("CE\nGW 1 21 0 0 -5 0 0 5 .001\nGE\nEX 1 1 1 0 90 0 0\nFR 0 1 0 0 14.2 0\nEN\n");
        let err = refine_deck(&rx, 3, EndRule::Keep).unwrap_err();
        assert!(err.contains("receiving antenna"), "{err}");
    }

    /// A `GM` copy fed in the copy maps inside the copy (NEC numbers a tag's
    /// segments across copies, FND-135); a `GC` taper takes the S-th root of its
    /// step; `RP` and `PT` are dropped and said so, once.
    #[test]
    fn copies_tapers_and_output_cards() {
        let gm = deck(
            "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGM 10 1 0 0 0 1 0 0 0\nGE\n\
             EX 0 11 26 0 1.0 0.0\nRP 0 7 13 1000 0 0 30 30\nPT -1\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let r = refine_deck(&gm, 3, EndRule::Keep).unwrap();
        assert_eq!(ex_segments(&r.deck), [77]);
        assert!(r
            .deck
            .cards
            .iter()
            .all(|c| !matches!(c, Card::Rp(_) | Card::Pt(_))));
        assert_eq!(r.notes.len(), 1, "{:?}", r.notes);
        assert!(r.notes[0].starts_with("PT, RP dropped"), "{:?}", r.notes);

        let gc = deck(
            "CE\nGW 1 10 0 0 0 0 0 5 0\nGC 0 0 0.8 0.01 0.002\nGE\nEX 0 1 5 0 1 0\n\
             FR 0 1 0 0 14.2 0\nEN\n",
        );
        let r = refine_deck(&gc, 3, EndRule::Keep).unwrap();
        let ratio = r
            .deck
            .cards
            .iter()
            .find_map(|c| match c {
                Card::Gc(gc) => Some(gc.length_ratio),
                _ => None,
            })
            .unwrap();
        assert!((ratio - 0.8f64.powf(1.0 / 3.0)).abs() < 1e-15, "{ratio}");
        assert!(r.notes.iter().any(|n| n.starts_with("GC:")));
        assert_eq!(
            gw_segments(&refine_deck(&gc, 1, EndRule::Keep).unwrap().deck),
            [10]
        );
    }
}
