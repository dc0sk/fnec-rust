// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-119 — the `GM` card, pinned against `nec2c`.
//!
//! fnec used to read field I2 as a *last tag* and F7 as a *first tag*, giving a
//! tag-range filter NEC-2 does not define, and it had no `NRPT` concept at all —
//! so it generated at most one copy and standard decks silently lost wires.
//!
//! Every expectation below was captured from `/usr/bin/nec2c` on **2026-08-30**,
//! reading `TOTAL SEGMENTS USED` and the `SEGMENTATION DATA` table. Geometry is
//! asserted, not impedance: geometry is what the card decides, and it sidesteps
//! the known Hallén-vs-nec2c systematic difference entirely.
//!
//! Each case discriminates something the previous fnec got wrong. The corpus
//! could not: both its `GM` fixtures use `NRPT = 1`, where the last-tag reading
//! and the `NRPT` reading happen to agree.

use nec_parser::parse;
use nec_solver::{build_geometry, GeometryError, Segment};

fn geometry(src: &str) -> Vec<Segment> {
    let deck = parse(src).expect("deck parses").deck;
    build_geometry(&deck).expect("geometry builds")
}

fn tags(segs: &[Segment]) -> Vec<u32> {
    segs.iter().map(|s| s.tag).collect()
}

fn z_centres(segs: &[Segment]) -> Vec<f64> {
    segs.iter().map(|s| s.midpoint[2]).collect()
}

fn close(got: &[f64], want: &[f64], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: length {:?}", got);
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g - w).abs() < 1e-3,
            "{what}: index {i} is {g}, nec2c says {w}"
        );
    }
}

/// `NRPT = 2`: two NEW structures, cumulative, tagged `tag + k*ITGI`.
///
/// nec2c: 9 segments, tags 1/11/21. fnec before the fix: 6, tags 1/11.
/// The cumulative part is what the z-centres pin — a non-cumulative reading
/// would put the third block at 0.67, not 1.67.
#[test]
fn nrpt_generates_that_many_cumulative_copies() {
    let segs = geometry("GW 1 3 0 0 -.5 0 0 .5 .001\nGM 10 2 0 0 0 0 0 1. 0\nGE\nEN\n");
    assert_eq!(segs.len(), 9, "nec2c: TOTAL SEGMENTS USED 9");
    assert_eq!(tags(&segs), vec![1, 1, 1, 11, 11, 11, 21, 21, 21]);
    close(
        &z_centres(&segs),
        &[
            -0.3333, 0.0, 0.3333, 0.6667, 1.0, 1.3333, 1.6667, 2.0, 2.3333,
        ],
        "cumulative translation",
    );
}

/// `NRPT = 0` is NEC's in-place move — and it applies `ITGI` to the tags too.
///
/// nec2c: 3 segments, ALL tag 11, shifted. fnec keyed in-place off `ITGI == 0`,
/// a different condition, and never retagged.
#[test]
fn an_in_place_move_also_applies_the_tag_increment() {
    let segs = geometry("GW 1 3 0 0 -.5 0 0 .5 .001\nGM 10 0 0 0 0 0 0 1. 0\nGE\nEN\n");
    assert_eq!(segs.len(), 3, "an in-place move copies nothing");
    assert_eq!(tags(&segs), vec![11, 11, 11], "nec2c retags in place");
    close(&z_centres(&segs), &[0.6667, 1.0, 1.3333], "in-place shift");
}

/// `ITS` selects a SUFFIX in definition order — not a tag range, and not
/// "every tag >= ITS".
///
/// Wires defined 5, 2, 3; moving from tag 2 moves wires 2 **and** 3 and leaves 5
/// alone. A range reading would move only wire 2; a "tags >= 2" reading would
/// also move wire 5. This is the only case that separates the three.
#[test]
fn its_selects_a_suffix_in_definition_order() {
    let segs = geometry(
        "GW 5 3 0 0 -.5 0 0 .5 .001\nGW 2 3 1 0 -.5 1 0 .5 .001\n\
         GW 3 3 2 0 -.5 2 0 .5 .001\nGM 0 0 0 0 0 0 0 10. 2.\nGE\nEN\n",
    );
    assert_eq!(segs.len(), 9);
    assert_eq!(tags(&segs), vec![5, 5, 5, 2, 2, 2, 3, 3, 3]);
    close(
        &z_centres(&segs),
        &[
            -0.3333, 0.0, 0.3333, 9.6667, 10.0, 10.3333, 9.6667, 10.0, 10.3333,
        ],
        "wire 5 unmoved; wires 2 AND 3 moved",
    );
}

/// `ITGI = 0` with `NRPT = 1` is a same-tag copy — the case the corpus fixture
/// `dipole-freesp-gm-inplace-shifted.nec` used to be, while claiming to test
/// in-place invariance.
#[test]
fn a_zero_increment_copy_keeps_the_tag() {
    let segs = geometry("GW 1 3 0 0 -.5 0 0 .5 .001\nGM 0 1 0 0 0 1. 0 0 0\nGE\nEN\n");
    assert_eq!(segs.len(), 6, "nec2c: 6 segments");
    assert_eq!(tags(&segs), vec![1; 6], "ITGI 0 leaves every tag alone");
    let xs: Vec<f64> = segs.iter().map(|s| s.midpoint[0]).collect();
    close(&xs, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], "copy translated in x");
}

/// An `ITS` naming a tag no wire carries is refused.
///
/// nec2c exits 255 with "NO SEGMENT HAS AN ITAG OF 7". fnec used to match
/// nothing, move nothing, and solve the unmoved structure at exit 0 — so a deck
/// that mistyped a tag got a confident answer about a different antenna.
#[test]
fn an_its_naming_no_wire_is_refused() {
    let deck = parse("GW 1 3 0 0 -.5 0 0 .5 .001\nGM 0 0 0 0 0 0 0 1. 7.\nGE\nEN\n")
        .expect("deck parses")
        .deck;
    let err = build_geometry(&deck).expect_err("tag 7 does not exist");
    assert!(
        matches!(err, GeometryError::StartTagNotFound { tag: 7 }),
        "{err:?}"
    );
}

/// A tag-0 segment is never retagged, in either branch.
///
/// nec2c guards its retag with `if (itag[i] != 0)`: a tag-0 wire copied with
/// `ITGI = 10` gives 6 segments **all still tag 0**, translated but not
/// renumbered. Tag 0 means "untagged" in NEC, and incrementing it would invent
/// a tag the deck never named.
///
/// This test exists because sabotaging the exemption changed nothing: I had
/// implemented the rule with no test behind it, which the six other cases could
/// not see.
#[test]
fn a_tag_zero_segment_is_never_retagged() {
    let segs = geometry("GW 0 3 0 0 -.5 0 0 .5 .001\nGM 10 1 0 0 0 0 0 1. 0\nGE\nEN\n");
    assert_eq!(segs.len(), 6, "nec2c: 6 segments");
    assert_eq!(
        tags(&segs),
        vec![0; 6],
        "tag 0 means untagged; ITGI must not invent a tag"
    );
    close(
        &z_centres(&segs),
        &[-0.3333, 0.0, 0.3333, 0.6667, 1.0, 1.3333],
        "the copy is still made and still moved",
    );
}

/// The negative control the other cases need: an ordinary `GM` copy still works.
///
/// Without it, a change that refused every `GM` card would satisfy the refusal
/// test above and leave the rest looking fine.
#[test]
fn an_ordinary_gm_copy_is_unaffected() {
    let segs = geometry("GW 1 3 0 0 -.5 0 0 .5 .001\nGM 1 1 0 0 0 1. 0 0 0\nGE\nEN\n");
    assert_eq!(segs.len(), 6);
    assert_eq!(tags(&segs), vec![1, 1, 1, 2, 2, 2]);
}
