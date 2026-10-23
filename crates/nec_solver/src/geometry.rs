// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Geometry builder: converts parsed `GwCard` entries into a flat list of
//! wire [`Segment`]s with precomputed spatial properties.
//!
//! Each NEC `GW` card defines a straight wire subdivided into N equal segments.
//! The builder expands every card into N segments and computes, for each:
//!
//! - `start` / `end` — endpoints in metres
//! - `midpoint`      — centre point (used as the match point in MoM)
//! - `direction`     — unit vector from start to end
//! - `length`        — segment length in metres
//! - `radius`        — wire radius in metres
//! - `tag`           — wire tag number from the GW card
//! - `tag_index`     — 1-based occurrence of this segment's tag, in definition
//!   order: NEC's segment number within a tag, as nec2c's `isegno` counts it
//! - `global_index`  — 0-based index in the flat segment list

use nec_model::card::{Card, GaCard, GhCard, GmCard, GrCard, GwCard, GxCard};
use nec_model::deck::NecDeck;

/// Ground model extracted from a GN card (or absence thereof).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GroundModel {
    /// No ground — free-space simulation (default when no GN card is present).
    #[default]
    FreeSpace,
    /// Perfect electric conductor (PEC) ground at z = 0.
    ///
    /// Implemented via the image method: for each real segment a mirror image
    /// at z → −z is added to the Green's function kernel.
    PerfectConductor,
    /// Simple finite-ground model (GN type 0) using medium parameters.
    ///
    /// This Phase-1 model is a pragmatic Fresnel-coefficient approximation
    /// used to enable baseline non-PEC ground behavior in impedance solves.
    SimpleFiniteGround {
        /// Relative dielectric constant (EPSE).
        eps_r: f64,
        /// Conductivity in S/m (SIG).
        sigma: f64,
    },
}

/// Extract the ground model from a parsed deck.
///
/// The ground is the **last** `GN` card's, as in nec2c, where each `GN`
/// overwrites the one before and `GN -1` resets to free space. `GE I1` does not
/// select a ground: nec2c reads it only to decide whether wire ends at z = 0 are
/// joined to their images ([`ge_flag`], [`snap_ground_contacts`]). fnec used to
/// infer a perfect ground from `GE 1` with no `GN`, where nec2c solves free space
/// (FND-241); that deck is now refused at the parse.
///
/// - `GN -1` or no `GN` → [`GroundModel::FreeSpace`].
/// - `GN 1` → [`GroundModel::PerfectConductor`].
/// - `GN 0` / `GN 2` → [`GroundModel::SimpleFiniteGround`] using parsed EPSE/SIG
///   (or average-ground defaults when omitted).
///
/// Other `GN` types are refused at the parse (FND-241), so they do not reach here.
pub fn ground_model_from_deck(deck: &NecDeck) -> GroundModel {
    let Some(gn) = deck.cards.iter().rev().find_map(|card| match card {
        Card::Gn(gn) => Some(gn),
        _ => None,
    }) else {
        return GroundModel::FreeSpace;
    };
    match gn.ground_type {
        1 => GroundModel::PerfectConductor,
        0 | 2 => GroundModel::SimpleFiniteGround {
            // Keep GN0 usable even when medium params are omitted.
            // Values are the same "average ground" defaults commonly
            // used in NEC workflows. GN2 shares this scoped scalar-Γ
            // approximation by default; the exact Sommerfeld treatment is
            // opt-in via `--ground-solver sommerfeld` / `--solver mpie`.
            eps_r: gn.eps_r.unwrap_or(13.0),
            sigma: gn.sigma.unwrap_or(0.005),
        },
        // -1 is the null ground. Any other type is refused at the parse.
        _ => GroundModel::FreeSpace,
    }
}

/// The deck's `GE I1`: `1` joins wire ends at the ground to their images, `0`
/// and `-1` leave them open (nec2c's `conect`). A deck with no `GE` card reads as
/// `0`; the parser refuses values outside `-1..=1` (FND-241).
pub fn ge_flag(deck: &NecDeck) -> i32 {
    deck.cards
        .iter()
        .find_map(|c| match c {
            Card::Ge(ge) => Some(ge.ground_reflection_flag),
            _ => None,
        })
        .unwrap_or(0)
}

/// nec2c's connection tolerance (`SMIN`, nec2c.h), as a fraction of a segment's
/// length. One number decides both of nec2c's connections in `conect`: a segment
/// end within it of z = 0 is on the ground (FND-243), and two segment ends within
/// it of each other are joined (FND-246, [`snap_junctions`]). fnec used absolute
/// tolerances for both — 1 nm for the ground, so a base 1 µm up was a free end
/// hanging over the plane (a λ/4 monopole on `GN 1` read 29.26 − j5497 Ω where
/// nec2c reads 39.88 + j23.11), and 1 µm for junctions, so two ends 50 µm apart
/// stayed apart (a split dipole read 18.05 − j3487 Ω where nec2c joins it and
/// reads 80.17 + j45.80).
pub const NEC_SMIN: f64 = 1.0e-3;

/// Whether a point at height `z` on a segment of length `len` is on the ground
/// plane, by nec2c's rule: `|z| <= 1e-3 * len`. Below that band it is buried.
pub fn on_ground_plane(z: f64, len: f64) -> bool {
    z.abs() <= NEC_SMIN * len
}

/// Snap every segment end within [`NEC_SMIN`] of its length of z = 0 onto it,
/// as nec2c does under `GE 1` over a ground: there such an end is joined to its
/// image, and the solvers' contact handling keys on z = 0 exactly. Not applied
/// under `GE 0` / `-1`, where a deck reaching the ground is refused instead
/// ([`crate::validate::ground_contact_topology_error`]).
fn snap_ground_contacts(segments: &mut [Segment]) {
    for s in segments {
        let len = s.length;
        let mut moved = false;
        for p in [&mut s.start, &mut s.end] {
            if p[2] != 0.0 && on_ground_plane(p[2], len) {
                p[2] = 0.0;
                moved = true;
            }
        }
        if moved {
            s.rederive_from_ends();
        }
    }
}

/// One segment from its ends, numbered as the `i`-th (0-based) of its card.
fn segment_between(
    tag: u32,
    i: u32,
    start: [f64; 3],
    end: [f64; 3],
    radius: f64,
    at: usize,
) -> Segment {
    let mut s = Segment {
        tag,
        tag_index: i + 1,
        abs_index: 0,
        global_index: at,
        start,
        end,
        midpoint: [0.0; 3],
        direction: [0.0; 3],
        length: 0.0,
        radius,
        part: SegmentPart::Whole,
    };
    s.rederive_from_ends();
    s
}

/// `GA`: nec2c's `arc`. `NS` segments on a circle of radius `RADA` in the plane
/// y = 0, from `ANG1` toward `ANG2` (degrees, from +x toward +z), each starting at
/// the previous one's end (carried, as nec2c carries it, so the ends are shared
/// floats). The parser refused what nec2c builds wrong.
fn expand_arc(ga: &GaCard, out: &mut Vec<Segment>) -> Result<(), GeometryError> {
    check_segment_budget(out.len(), u64::from(ga.segments), "GA")?;
    let to_rad = std::f64::consts::PI / 180.0;
    let mut ang = ga.ang1_deg * to_rad;
    let dang = (ga.ang2_deg - ga.ang1_deg) * to_rad / f64::from(ga.segments);
    let mut p = [ga.arc_radius * ang.cos(), 0.0, ga.arc_radius * ang.sin()];
    for i in 0..ga.segments {
        ang += dang;
        let q = [ga.arc_radius * ang.cos(), 0.0, ga.arc_radius * ang.sin()];
        let at = out.len();
        out.push(segment_between(ga.tag, i, p, q, ga.radius, at));
        p = q;
    }
    Ok(())
}

/// `GH`: nec2c's `helix`. `NS` segments from z = 0 rising by |`HL`|/`NS` (the
/// heights accumulated as nec2c accumulates them, so the ends are shared floats),
/// `x = a(z)·cos(2πz/S)`, `y = b(z)·sin(2πz/S)`: constant (`A1`, `B1`, `B1 = 0`
/// reading as `A1`) when `A2 = A1`, else linear from (`A1`, `B1`) to (`A2`, `B2`,
/// `B2 = 0` reading as `A2`); `HL < 0` swaps x and y. The parser refused the
/// spellings nec2c reads otherwise than NEC-2 defines.
fn expand_helix(gh: &GhCard, out: &mut Vec<Segment>) -> Result<(), GeometryError> {
    check_segment_budget(out.len(), u64::from(gh.segments), "GH")?;
    let hl = gh.length.abs();
    let zinc = hl / f64::from(gh.segments);
    let uniform = gh.a2 == gh.a1;
    let b1 = if uniform && gh.b1 == 0.0 {
        gh.a1
    } else {
        gh.b1
    };
    let b2 = if !uniform && gh.b2 == 0.0 {
        gh.a2
    } else {
        gh.b2
    };
    let two_pi = 2.0 * std::f64::consts::PI;
    let point = |z: f64| -> [f64; 3] {
        let (a, b) = if uniform {
            (gh.a1, b1)
        } else {
            (gh.a1 + (gh.a2 - gh.a1) * z / hl, b1 + (b2 - b1) * z / hl)
        };
        let t = two_pi * z / gh.turn_spacing;
        let (x, y) = (a * t.cos(), b * t.sin());
        if gh.length > 0.0 {
            [x, y, z]
        } else {
            [y, x, z]
        }
    };
    let mut z1 = 0.0;
    for i in 0..gh.segments {
        let z2 = z1 + zinc;
        let at = out.len();
        out.push(segment_between(
            gh.tag,
            i,
            point(z1),
            point(z2),
            gh.radius,
            at,
        ));
        z1 = z2;
    }
    Ok(())
}

/// `GS`: multiply every segment defined so far — ends and radius — by `scale`,
/// as nec2c's `datagn` does. The midpoint, length and direction are re-derived
/// from the scaled ends rather than scaled themselves, so they agree with the
/// ends exactly, as nec2c's (recomputed at `GE`) do.
fn apply_gs(scale: f64, segments: &mut [Segment]) {
    for s in segments {
        s.start = s.start.map(|c| c * scale);
        s.end = s.end.map(|c| c * scale);
        s.radius *= scale;
        s.rederive_from_ends();
    }
}

/// A single NEC wire segment.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Wire tag number (from GW card).
    pub tag: u32,
    /// NEC's segment number within the tag: the 1-based occurrence of `tag` in
    /// definition order, as nec2c's `isegno` counts it.
    ///
    /// Not the index within one `GW` card. A same-tag `GM` copy (`ITGI = 0`)
    /// numbered its segments from 1 again, so "tag 1 segment 77" on a 51-segment
    /// wire copied once named nothing, and every EX/LD/NT/PT that addressed the
    /// copy reached the original instead (FND-135). Numbered here, once, so the
    /// twenty sites that resolve `(tag, segment)` agree with nec2c unchanged.
    pub tag_index: u32,
    /// NEC's absolute segment number: the 1-based position of this segment among
    /// the deck's segments in build order, as nec2c's SEGMENTATION DATA numbers
    /// them and as a reference with tag 0 names them (`isegno(0, m)` returns `m`;
    /// FND-282). fnec builds GW, GM, GR, GX, GA and GH in nec2c's order, so the
    /// count is the same (pinned in `gm_nec2c.rs`, `tag_zero_nec2c.rs` and the
    /// GA/GH segmentation test). A refined free end's flank (FND-227) is not a
    /// deck segment and is numbered 0; its centre carries the deck segment's
    /// number.
    pub abs_index: u32,
    /// 0-based index in the global segment list.
    pub global_index: usize,
    /// Segment start point in metres.
    pub start: [f64; 3],
    /// Segment end point in metres.
    pub end: [f64; 3],
    /// Centre (match) point in metres.
    pub midpoint: [f64; 3],
    /// Unit direction vector (start → end).
    pub direction: [f64; 3],
    /// Segment length in metres.
    pub length: f64,
    /// Wire radius in metres.
    pub radius: f64,
    /// What this segment is to the deck: a deck segment, or a third of one at a
    /// free wire end (FND-227).
    pub part: SegmentPart,
}

/// What a [`Segment`] is to the deck it was built from.
///
/// [`build_geometry`] splits every segment at a free wire end into three equal
/// thirds (FND-227): Hallén cannot see a source, load or port on a segment with no
/// test point between it and the tip, and the coarse end segment costs every
/// answer accuracy. The deck's numbering is untouched — the middle third carries
/// the deck segment's tag and number, and its midpoint is the deck segment's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SegmentPart {
    /// A deck segment, as its `GW` card made it.
    #[default]
    Whole,
    /// The middle third of a deck segment at a free wire end. A deck reference to
    /// that segment resolves here.
    Centre,
    /// An outer third of that segment, numbered 0: no deck card can name it
    /// ([`find_deck_segment`]). `parent` is the deck segment number it belongs to.
    Flank {
        /// The deck segment number (`tag_index`) of the segment this third is of.
        parent: u32,
    },
}

impl Segment {
    /// Recompute `midpoint`, `length` and `direction` from `start` and `end`,
    /// after an operation that moved the ends other than rigidly (the `GS` scale,
    /// the ground-contact snap).
    fn rederive_from_ends(&mut self) {
        let d = [
            self.end[0] - self.start[0],
            self.end[1] - self.start[1],
            self.end[2] - self.start[2],
        ];
        self.length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        self.direction = [d[0] / self.length, d[1] / self.length, d[2] / self.length];
        self.midpoint = [
            0.5 * (self.start[0] + self.end[0]),
            0.5 * (self.start[1] + self.end[1]),
            0.5 * (self.start[2] + self.end[2]),
        ];
    }

    /// Whether this is a segment the deck names — every one but a flank. Outputs
    /// listing segments list these.
    pub fn is_deck_segment(&self) -> bool {
        !matches!(self.part, SegmentPart::Flank { .. })
    }

    /// The length of the deck segment this is (part of): three times a centre's.
    pub fn deck_length(&self) -> f64 {
        match self.part {
            SegmentPart::Centre => 3.0 * self.length,
            _ => self.length,
        }
    }
}

/// The index of the segment a deck card names as `(tag, segment)`, in NEC's
/// numbering; `None` when it names none.
///
/// NEC-2 reads the pair as nec2c's `isegno` does: a tag names the `segment`-th
/// segment carrying it ([`Segment::tag_index`]); **tag 0 names absolute segment
/// number `segment`** ([`Segment::abs_index`]), whatever tag that segment carries
/// (FND-282 — fnec read tag 0 as "the wire tagged 0", so `EX 0 0 32` on two
/// 21-segment dipoles was refused where nec2c feeds segment 32, and with a wire
/// tagged 0 present it fed that wire instead).
///
/// The one place a deck reference becomes a segment. Nineteen sites — feeds,
/// current sources, ports, the gain correction, validation and every front end —
/// each spelled `position(|s| s.tag == t && s.tag_index == n)` inline, so a change
/// to what a reference may name had nineteen places to be made, and the next
/// copy would miss it (FND-135 was a numbering change of that kind). A gate
/// (`apps/nec-cli/tests/deck_reference_resolver.rs`) refuses a new inline copy.
/// Segment 0 names nothing: NEC numbers segments from 1, and the sites that read
/// 0 as "all" or "the middle" (`LD`, `TL`/`NT` ports) do so before calling this.
/// A range of segments (`LD`, `PT`) is resolved by [`deck_segment_in_range`].
pub fn find_deck_segment(segs: &[Segment], tag: u32, segment: u32) -> Option<usize> {
    if segment == 0 {
        return None;
    }
    if tag == 0 {
        return segs.iter().position(|s| s.abs_index == segment);
    }
    segs.iter()
        .position(|s| s.tag == tag && s.tag_index == segment)
}

/// Whether `seg` is one of the segments a deck card names as `(tag, first, last)`
/// — NEC-2's `LD` and `PT` selection, as nec2c's `load()` and its current print
/// read it (FND-282, FND-299):
///
/// - `tag ≠ 0`: the segments carrying `tag`; `first == 0` selects every one of
///   them (`last` is then not read, as in nec2c), otherwise those numbered
///   `first..=last` within the tag, `last == 0` meaning `first` alone;
/// - `tag == 0`, `first == last == 0`: every segment;
/// - `tag == 0`, `first ≠ 0`: the segments whose **absolute** number
///   ([`Segment::abs_index`]) is in `first..=last` (`last == 0` → `first`), of
///   any tag.
///
/// `tag == 0`, `first == 0`, `last ≠ 0` reads as `0 0 0` here (`first == 0` is
/// "all"); NEC-2 has no segment 0 (nec2c reads past its arrays there), and
/// [`crate::loads::ld_card_problem`] refuses that card by name before any solve. A flank of a refined free end
/// (FND-227) is never selected — it is not a deck segment; the one load that
/// belongs on it (a type-5 conductivity, per metre) is given to it by its parent
/// in `loads.rs`.
///
/// The one range resolver: `loads::segment_matches`, the Laplace loads, the `PT`
/// current filter and the NEC-2D "loaded twice" note each had their own copy of
/// the tag-0 rule, and all four read it as "every tag".
pub fn deck_segment_in_range(seg: &Segment, tag: u32, first: u32, last: u32) -> bool {
    if !seg.is_deck_segment() {
        return false;
    }
    let last = if last == 0 { first } else { last };
    if tag == 0 {
        return first == 0 || (first..=last).contains(&seg.abs_index);
    }
    seg.tag == tag && (first == 0 || (first..=last).contains(&seg.tag_index))
}

/// The message for a reference `(tag, segment)` that names no segment, for every
/// card that resolves one (`EX`, `TL`, `NT`): with tag 0 it says what tag 0 means,
/// because the deck's author most likely expected the wire tagged 0 (FND-282).
pub fn missing_reference(card: &str, tag: u32, segment: u32, segs: &[Segment]) -> String {
    missing_reference_text(
        card,
        tag,
        segment,
        Some(segs.iter().filter(|s| s.is_deck_segment()).count()),
    )
}

/// [`missing_reference`] without the geometry at hand (an error type that carries
/// only the reference).
pub fn missing_reference_text(
    card: &str,
    tag: u32,
    segment: u32,
    deck_segments: Option<usize>,
) -> String {
    let count = deck_segments
        .map(|n| format!("; the deck has {n}"))
        .unwrap_or_default();
    if tag == 0 && segment == 0 {
        // Only a network port has fnec's segment-0 shorthand to be confused with.
        let shorthand = if card.starts_with("TL") || card.starts_with("NT") {
            "; only a tagged port may use fnec's segment-0 shorthand for the tag's centre"
        } else {
            ""
        };
        format!(
            "{card}: tag 0 with segment 0 names no segment — tag 0 is an absolute segment \
             number in NEC-2, and segment 0 names none{shorthand}"
        )
    } else if tag == 0 {
        format!(
            "{card}: no segment numbered {segment} (tag 0 names an absolute segment number, \
             as NEC-2 reads it{count})"
        )
    } else {
        format!("{card}: no segment with tag {tag}, index {segment}")
    }
}

/// The largest segment count [`build_geometry`] will produce.
///
/// The binding constraint is not the segment list — a `Segment` is about 128
/// bytes, so even 100 000 of them is ~13 MB — but the dense impedance matrix
/// every solver allocates from it: `ZMatrix::new(n)` is `n * n` `Complex64`,
/// i.e. `16 * N^2` bytes, and its factorisation is `O(N^3)`. At this cap that
/// is 1.6 GB and minutes of arithmetic, which is the last point at which
/// refusing is more use to the caller than trying and dying on allocation.
///
/// The largest deck in `corpus/` is 255 segments (`yagi-5elm-51seg.nec`), so
/// this clears every real fixture by a factor of about 39.
pub const MAX_SEGMENTS: usize = 10_000;

/// Error returned by [`build_geometry`].
#[derive(Debug, Clone, PartialEq)]
pub enum GeometryError {
    /// A GW card specified zero segments.
    ZeroSegments { tag: u32 },
    /// A GW card has zero-length wire (start == end).
    ZeroLengthWire { tag: u32 },
    /// No GW cards were found in the deck.
    NoWires,
    /// A `GM` card's `ITS` names a tag no wire carries.
    ///
    /// nec2c refuses this outright ("NO SEGMENT HAS AN ITAG OF n", exit 255).
    /// fnec used to match nothing, move nothing, and solve the unmoved structure
    /// at exit 0 — a deck asking to move a wire it had mistyped got a confident
    /// answer about a different antenna (FND-119).
    StartTagNotFound { tag: u32 },
    /// The deck asks for more segments than [`MAX_SEGMENTS`].
    ///
    /// Refused rather than truncated: a silently shortened antenna is a wrong
    /// answer, and the caller asked for something this solver will not build
    /// (FND-125).
    SegmentBudgetExceeded { card: &'static str, projected: u64 },
    /// A `GM` or `GR` card's tag arithmetic left the `u32` tag-number space.
    ///
    /// Unchecked, this was an `attempt to multiply with overflow` panic in debug
    /// and a silently wrapped tag in release — and in `fnec worker --stdio` it
    /// killed the process without emitting a result line at all, which the
    /// controller cannot tell from a dead worker (FND-113).
    TagOverflow { card: &'static str, detail: String },
    /// A `GA` arc or `GH` helix asked of the Hallén geometry. Each is a bend at
    /// every node, so every straight run is one segment long; Hallén's section
    /// graph refuses that, and the per-wire fallback read a half-circle arc 40 %
    /// off nec2c (39.1 + j115.9 against 63.9 + j90.8, with only a warning) and a
    /// full circle as a false zero-current error. The MPIE models the bends;
    /// `mpie` says whether it takes the rest of the deck.
    CurvedForHallen { card: &'static str, mpie: bool },
    /// A `GX` reflection of a segment that lies in, or crosses, its plane of
    /// symmetry: the copy would coincide with or overlap the original. nec2c stops
    /// on it ("LIES IN PLANE OF SYMMETRY").
    InSymmetryPlane {
        /// The tag of the segment.
        tag: u32,
        /// Its absolute position in the structure so far (1-based), as nec2c
        /// names it.
        segment: usize,
        /// The coordinate the plane negates: 'x', 'y' or 'z'.
        axis: char,
    },
    /// A radius-0 `GW` without its `GC`, or a `GC` without its `GW` — in a deck
    /// built other than by the parser, which refuses both with a line number.
    TaperPairing(nec_model::card::TaperPairingError),
    /// A `GC` taper whose segment `segment` (1-based within its tag) is shorter
    /// than [`crate::validate::MIN_LENGTH_TO_RADIUS_RATIO`] radii, or than three
    /// junction tolerances — the Hallén refinement thirds an end segment, and a
    /// third under [`crate::hallen_session::JUNCTION_TOL_M`] would join wires
    /// that do not touch.
    TaperTooShort {
        tag: u32,
        segment: u32,
        length: f64,
        radius: f64,
    },
    /// Two segment ends `gap` apart (L1) that NEC joins from the longer segment's
    /// side only: within [`NEC_SMIN`] of its length, not of the shorter one's
    /// (FND-246). nec2c aborts or hangs on it. `a` and `b` are `(tag, segment)`.
    OneSidedJunction {
        a: (u32, u32),
        b: (u32, u32),
        gap: f64,
        band_short: f64,
        band_long: f64,
    },
    /// A chain of near-miss ends, each joined to the next by NEC's rule, whose
    /// ends `a` and `b` are `gap` apart, past the join distance `band` (FND-246).
    /// nec2c hangs on it.
    NonTransitiveJunction {
        a: (u32, u32),
        b: (u32, u32),
        gap: f64,
        band: f64,
    },
    /// Segments `a` and `b` (`(tag, segment)`) lie along each other for `length`
    /// metres, closer than their radii: one conductor drawn on top of another
    /// (FND-250). `duplicate` when the overlap is the whole of the shorter one.
    OverlappingSegments {
        a: (u32, u32),
        b: (u32, u32),
        length: f64,
        duplicate: bool,
    },
    /// A closed loop, or a junction where three or more wires meet, with a straight
    /// run one segment long at tag `tag` segment `seg`, asked of the Hallén geometry
    /// (FND-251). Hallén solves loops and junctions on the section graph, which
    /// cannot take a one-segment run; the per-wire fallback it used to take gave no
    /// current at all on a feed on that run, and answers 2 to 18 times off nec2c
    /// elsewhere. `junction` names a junction (a loop otherwise); `mpie` whether the
    /// MPIE takes the deck, so the remedy is named only where it applies.
    OneSegmentRunTopology {
        junction: bool,
        tag: u32,
        seg: u32,
        mpie: bool,
    },
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeometryError::ZeroSegments { tag } => {
                write!(f, "GW tag {tag}: segment count must be ≥ 1")
            }
            GeometryError::ZeroLengthWire { tag } => {
                write!(f, "GW tag {tag}: wire has zero length (start == end)")
            }
            GeometryError::NoWires => write!(f, "deck contains no GW (wire) cards"),
            GeometryError::SegmentBudgetExceeded { card, projected } => write!(
                f,
                "{card} card: the deck reaches {projected} segments, past the \
                 {MAX_SEGMENTS} this solver will build — the impedance matrix is \
                 16*N^2 bytes and its factorisation O(N^3), so the cap is already \
                 about 1.6 GB of matrix"
            ),
            GeometryError::StartTagNotFound { tag } => write!(
                f,
                "GM card: no segment has tag {tag}, so there is nothing to move \
                 from there"
            ),
            GeometryError::CurvedForHallen { card, .. } => write!(
                f,
                "geometry contains {card}, which is a bend at every segment; the Hallén solve \
                 cannot model a bend on a straight run one segment long (a half-circle arc \
                 reads 40 % off nec2c), so the deck is refused"
            ),
            GeometryError::InSymmetryPlane { tag, segment, axis } => write!(
                f,
                "GX card: segment {segment} (tag {tag}) lies in or crosses the plane {axis} = 0 \
                 it reflects in, so its copy would coincide with or overlap it; nec2c stops \
                 on it too. Model the half that lies on one side of the plane"
            ),
            GeometryError::TaperPairing(e) => write!(f, "{e}"),
            GeometryError::TaperTooShort {
                tag,
                segment,
                length,
                radius,
            } => {
                // Each bound says itself, so a test can tell which one refused.
                let ratio = crate::validate::MIN_LENGTH_TO_RADIUS_RATIO;
                let why = if *length < ratio * radius {
                    format!(
                        "shorter than {ratio} radii, where the thin-wire kernel is outside its \
                         validity and nec2c and fnec disagree"
                    )
                } else {
                    format!(
                        "shorter than {MIN_TAPER_SEGMENT_M:.0e} m, three junction tolerances, \
                         under which the Hallén refinement would join it to wires it does not \
                         touch"
                    )
                };
                write!(
                    f,
                    "GC card: segment {segment} of tag {tag} tapers to {length:.3e} m against a \
                     radius of {radius:.3e} m, {why}. Bring the length ratio nearer 1 or use \
                     fewer segments"
                )
            }
            GeometryError::OneSidedJunction {
                a,
                b,
                gap,
                band_short,
                band_long,
            } => write!(
                f,
                "segment {} of tag {} and segment {} of tag {} end {gap:.3e} m apart (by NEC's \
                 measure, the sum of the coordinate differences): within 1e-3 of the longer \
                 segment's length ({band_long:.3e} m) but not of the shorter's \
                 ({band_short:.3e} m), so NEC joins them from one side only, and nec2c aborts \
                 or hangs on it. Move the ends onto one point, or more than {band_long:.3e} m \
                 apart",
                a.1, a.0, b.1, b.0
            ),
            GeometryError::OverlappingSegments {
                a,
                b,
                length,
                duplicate,
            } => {
                let remedy = if *duplicate {
                    "remove the duplicate"
                } else {
                    "end the second wire where the first ends, or split the first at that point"
                };
                write!(
                    f,
                    "segments overlap: segment {} of tag {} and segment {} of tag {} lie along \
                     each other for {length:.3e} m, closer than their radii — one conductor drawn \
                     on top of another, which no solver separates (nec2c answered such a deck \
                     650 + j1453 Ω where the antenna meant reads 203.8 + j383.2); {remedy}. A GR, \
                     GX or GM that copies a wire onto itself or onto the axis is the usual cause",
                    a.1, a.0, b.1, b.0
                )
            }
            GeometryError::NonTransitiveJunction { a, b, gap, band } => write!(
                f,
                "wire ends between segment {} of tag {} and segment {} of tag {} form a chain \
                 of near-misses: each end is within NEC's join distance (1e-3 of a segment's \
                 length) of the next, but these two are {gap:.3e} m apart, past {band:.3e} m, \
                 so the ends do not meet at one point, and nec2c hangs on it. Bring the ends \
                 to one point",
                a.1, a.0, b.1, b.0
            ),
            GeometryError::OneSegmentRunTopology {
                junction, tag, seg, ..
            } => {
                let what = if *junction {
                    "a junction where three or more wires meet"
                } else {
                    "a closed loop"
                };
                write!(
                    f,
                    "geometry contains {what} with a straight run one segment long (tag {tag} \
                     segment {seg}); the Hallén solve models loops and junctions on its section \
                     graph, which cannot take a one-segment run (its single row cannot fix the \
                     run's two constants), and without it the answer is unreliable — measured 2 \
                     to 18 times off nec2c, or no current at all on a feed on that run — so the \
                     deck is refused. Give every straight run at least two segments"
                )
            }
            GeometryError::TagOverflow { card, detail } => write!(
                f,
                "{card} card: {detail} leaves the u32 tag-number space (largest \
                 representable tag is {})",
                u32::MAX
            ),
        }
    }
}

impl std::error::Error for GeometryError {}

impl GeometryError {
    /// Whether only the Hallén geometry refuses this, the deck's own segments being
    /// well formed: a frontend drawing the deck, or solving it with the MPIE, can
    /// take [`build_deck_geometry`] instead. The one list, so a new Hallén-only
    /// refusal cannot be missed by one caller (the GUI viewport matched on
    /// `CurvedForHallen` by hand).
    pub fn is_hallen_only(&self) -> bool {
        matches!(
            self,
            GeometryError::CurvedForHallen { .. } | GeometryError::OneSegmentRunTopology { .. }
        )
    }

    /// Whether the MPIE solves a deck this refuses: a Hallén-only refusal of a
    /// deck the MPIE takes ([`crate::validate::mpie_compatible_deck`]).
    pub fn mpie_solves_it(&self) -> bool {
        matches!(
            self,
            GeometryError::CurvedForHallen { mpie: true, .. }
                | GeometryError::OneSegmentRunTopology { mpie: true, .. }
        )
    }

    /// The refusal as a frontend shows it, with `remedy` — that frontend's own
    /// way of reaching the MPIE — where the MPIE solves the deck. `Display` names
    /// no remedy: it had the CLI's flag baked in, which the GUI, `sweep
    /// --resonance` and the bindings cannot take (FND-271). The one place the
    /// remedy is joined on, so no frontend states it twice.
    pub fn with_mpie_remedy(&self, remedy: &str) -> String {
        if self.mpie_solves_it() {
            format!("{self}. The MPIE solves this deck: {remedy} (PH9-CHK-007)")
        } else {
            self.to_string()
        }
    }
}

/// Compute per-wire endpoint indices from a flat segment list.
///
/// A wire is a run of segments sharing one tag, each starting where the one
/// before it ended. Returns a `Vec` of `(first, last)` inclusive global-index
/// pairs, one per wire, in deck order. An empty segment list returns an empty
/// `Vec`.
///
/// The continuity condition is new (FND-135). A run of one tag used to be one
/// wire however far apart its segments were, so a same-tag `GM` copy translated
/// away from its original was solved as ONE conductor jumping the gap.
pub fn wire_endpoints_from_segs(segs: &[Segment]) -> Vec<(usize, usize)> {
    let gap = |a: &[f64; 3], b: &[f64; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut current_tag = u32::MAX;
    let mut first = 0usize;
    for (i, seg) in segs.iter().enumerate() {
        let breaks = seg.tag != current_tag
            || (i > 0 && gap(&segs[i - 1].end, &seg.start) > crate::hallen_session::JUNCTION_TOL_M);
        if breaks {
            if current_tag != u32::MAX {
                out.push((first, i - 1));
            }
            current_tag = seg.tag;
            first = i;
        }
    }
    if current_tag != u32::MAX {
        out.push((first, segs.len() - 1));
    }
    split_at_touching_ends(segs, out)
}

/// Break a wire at an interior joint where a segment of ANOTHER wire starts or
/// ends (FND-192).
///
/// NEC connects coincident segment ends, wherever they are on their wires. A stem
/// whose end meets a bar at one of the bar's joints is joined to it — the usual
/// way to draw a T — and two wires crossing at a joint each has are joined into
/// four arms. fnec saw unconnected wires and solved them so, silently: a T drawn
/// that way gave 12.3 − j1123 Ω where nec2c gives 23.0 − j66.9. Breaking the wire
/// at the joint makes it a wire-end meeting like any other, for every consumer of
/// the wire list. A touch in the middle of a segment stays unconnected, as in NEC.
/// One pass: segment end points do not move when a wire is split.
fn split_at_touching_ends(segs: &[Segment], wires: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let near = |a: &[f64; 3], b: &[f64; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
            <= crate::hallen_session::JUNCTION_TOL_M
    };
    let mut out = Vec::with_capacity(wires.len());
    for &(f, l) in &wires {
        let mut start = f;
        for m in f..l {
            let joint = &segs[m].end;
            let touched = segs
                .iter()
                .enumerate()
                .any(|(i, o)| (i < f || i > l) && (near(&o.start, joint) || near(&o.end, joint)));
            if touched {
                out.push((start, m));
                start = m + 1;
            }
        }
        out.push((start, l));
    }
    out
}

/// How close two wire ends must be for [`merge_collinear_wire_endpoints`] to
/// treat them as one conductor, in metres.
///
/// Named, and not a local, because a safety property depends on its *relation*
/// to [`crate::hallen_session::JUNCTION_TOL_M`] rather than on its value: the
/// merged and raw endpoint lists may differ only on geometry the plane-wave
/// builder refuses, and that holds precisely because every merge event is also
/// a detected junction. Widen this past the junction tolerance and the two
/// endpoint lists diverge on geometry that still solves.
/// The `const _: () = assert!(..)` below is the check, and it is a compile error
/// rather than a test so that it cannot be filtered out of a run.
pub const MERGE_POS_TOL_M: f64 = 1e-6;

// Checked where it cannot be skipped: a test can be filtered out, a const
// assertion cannot — widen the merge tolerance past the junction tolerance and
// the crate stops compiling. `merged_and_raw_endpoints_differ_only_on_junctioned_geometry`
// gates the same invariant over real geometry, but it can only see geometry that
// exists, and no corpus deck has an endpoint gap between the two tolerances.
const _: () = assert!(
    MERGE_POS_TOL_M <= crate::hallen_session::JUNCTION_TOL_M,
    "merge tolerance exceeds the junction tolerance: two wire ends could then be merged \
     into one conductor without being detected as a junction, so the merged and raw \
     endpoint lists would differ on geometry the plane-wave builder still accepts"
);

/// Merge tag-based wire chains that are **collinear continuations** of one another
/// into single logical wires, for the Hallén homogeneous solution (PH9-CHK-002).
///
/// Two consecutive wire blocks are merged when the first block's last segment
/// connects **end-to-start** to the next block's first segment, the two segments
/// share direction (collinear) and radius, and they are contiguous in the segment
/// array. The result is the natural single-wire grouping for a straight conductor
/// that was split across several `GW` cards — so `cos(k·s)` and the homogeneous
/// constant are shared across the whole conductor rather than reset at each split.
///
/// For any geometry without such collinear splits (single wires, parallel arrays,
/// bends, T/Y junctions) this returns exactly [`wire_endpoints_from_segs`] — the
/// merge is a strict no-op there.
pub fn merge_collinear_wire_endpoints(segs: &[Segment]) -> Vec<(usize, usize)> {
    let base = wire_endpoints_from_segs(segs);
    if base.len() < 2 {
        return base;
    }
    // How many wire ends meet at a point. A joint where a third wire ends is a
    // junction, not a continuation, however straight the two halves are: after
    // `wire_endpoints_from_segs` splits a bar where a stem meets it (FND-192),
    // the merge must not weld the halves back over the stem.
    let ends_at = |p: [f64; 3]| {
        base.iter()
            .flat_map(|&(f, l)| [segs[f].start, segs[l].end])
            .filter(|&e| dist2(e, p) < MERGE_POS_TOL_M * MERGE_POS_TOL_M)
            .count()
    };
    let mut merged: Vec<(usize, usize)> = Vec::new();
    let mut cur = base[0];
    for &next in &base[1..] {
        // last segment of the current (possibly merged) block, first of the candidate
        if cur.1 + 1 == next.0
            && continues_straight(&segs[cur.1], &segs[next.0])
            && ends_at(segs[cur.1].end) == 2
        {
            cur = (cur.0, next.1); // extend the merged chain
        } else {
            merged.push(cur);
            cur = next;
        }
    }
    merged.push(cur);
    merged
}

/// Whether segment `b` continues segment `a` as the same straight conductor:
/// `b` starts where `a` ends, points the same way, and has the same radius.
///
/// The ONE rule for "straight" (FND-172, FND-175). The collinear merge above and
/// [`ConductorPath::is_trivial`] each had their own: the merge required the
/// direction within 1e-6 rad and equal radii, `is_trivial` the direction within
/// 4.5e-5 rad and no radius at all. A path between the two — a kink of 1e-6 to
/// 4.5e-5 rad, which 4-decimal coordinates produce, or a stepped radius — was
/// called trivial, then not merged, and took the plain basis's pairwise junction
/// row: 0.32 − j930 Ω on a kinked 10 m wire where nec2c gives 119.8 − j71.1.
pub(crate) fn continues_straight(a: &Segment, b: &Segment) -> bool {
    const DIR_TOL: f64 = 1e-6;
    dist2(a.end, b.start) < MERGE_POS_TOL_M * MERGE_POS_TOL_M
        && dir_aligned(a.direction, b.direction, DIR_TOL)
        && (a.radius - b.radius).abs() < MERGE_POS_TOL_M
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

fn dir_aligned(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    // Same direction (not merely parallel): the cross product ~0 and dot > 0.
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    dist2(cross, [0.0, 0.0, 0.0]) < tol * tol && dot > 0.0
}

/// A detected physical junction between two wire endpoints.
///
/// Encodes the current-continuity constraint that must be applied in
/// the Hallén augmented system instead of an I = 0 endpoint constraint.
///
/// The constraint is: `I[seg_a] + sign * I[seg_b] = 0`
///
/// - `sign = -1.0` for an end-to-start connection (current flows continuously
///   from wire A into wire B): `I[A_last] - I[B_first] = 0`.
/// - `sign = +1.0` for an end-to-end or start-to-start connection (current
///   reverses at the junction): `I[A_last] + I[B_last] = 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct WireJunction {
    /// Segment index of the endpoint on wire A (first or last segment of A).
    pub seg_a: usize,
    /// Segment index of the endpoint on wire B (first or last segment of B).
    pub seg_b: usize,
    /// Sign for the continuity constraint. See struct-level docs.
    pub sign: f64,
}

/// Detect physical wire junctions from segment geometry and wire endpoints.
///
/// Two wire endpoints are considered a junction when the physical point
/// represented by each endpoint is within
/// [`crate::hallen_session::JUNCTION_TOL_M`] of the other. The tolerance is not a
/// parameter: it decides which solver runs, and as a parameter it was a bare
/// `1e-6` repeated at every call site (FND-079) — one change away from two
/// callers disagreeing about whether two wires touch.
/// For segment index `first`, the physical point is `segs[first].start`.
/// For segment index `last`, the physical point is `segs[last].end`.
///
/// Returns one `WireJunction` per detected junction pair. Each (seg_a, seg_b)
/// pair appears at most once. Self-junctions (wire A connecting to itself
/// for a degenerate single-segment wire) are not emitted.
pub fn detect_wire_junctions(
    segs: &[Segment],
    wire_endpoints: &[(usize, usize)],
) -> Vec<WireJunction> {
    let tol = crate::hallen_session::JUNCTION_TOL_M;
    // Build the list of (segment_index, point, is_start) for every wire endpoint.
    // is_start = true  → this is the first segment of a wire; its physical node
    //                     is segs[seg].start.
    // is_start = false → this is the last segment of a wire; its physical node
    //                     is segs[seg].end.
    struct EndpointNode {
        seg: usize,
        point: [f64; 3],
        is_start: bool,
    }

    let mut nodes: Vec<EndpointNode> = Vec::with_capacity(wire_endpoints.len() * 2);
    for &(first, last) in wire_endpoints {
        nodes.push(EndpointNode {
            seg: first,
            point: segs[first].start,
            is_start: true,
        });
        nodes.push(EndpointNode {
            seg: last,
            point: segs[last].end,
            is_start: false,
        });
    }

    let pt_dist = |a: &[f64; 3], b: &[f64; 3]| {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    };

    let mut junctions: Vec<WireJunction> = Vec::new();

    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            // Skip two endpoints that belong to the same segment (degenerate wire).
            if nodes[i].seg == nodes[j].seg {
                continue;
            }
            if pt_dist(&nodes[i].point, &nodes[j].point) > tol {
                continue;
            }
            // Determine sign from the endpoint types.
            //   end-to-start or start-to-end → sign = -1.0 (continuous flow)
            //   end-to-end or start-to-start → sign = +1.0 (anti-parallel)
            let sign = if nodes[i].is_start == nodes[j].is_start {
                1.0_f64
            } else {
                -1.0_f64
            };
            junctions.push(WireJunction {
                seg_a: nodes[i].seg,
                seg_b: nodes[j].seg,
                sign,
            });
        }
    }

    junctions
}

/// A logical conductor: a maximal chain of physical wires connected end-to-end
/// through **degree-2** junctions (bends, start-to-start / end-to-end splits, or
/// collinear continuations), forming a single continuous current path.
///
/// This is the general-junction generalization of [`merge_collinear_wire_endpoints`]
/// (PH9-CHK-002). Where the collinear merge only joins straight end-to-start `GW`
/// chains, a `ConductorPath` walks *any* degree-2 chain — including reversed
/// (start-to-start) and bent connections — and carries the per-segment traversal
/// **sign** and signed **arc-length** needed to make the Hallén homogeneous basis
/// (`cos(k·s)`) continuous across the junction.
///
/// The current on segment `m` in its own (NEC) direction is `sign[m] · I_path(s)`,
/// where `I_path` is the continuous path current and `s = s_mid[m]` is the signed
/// arc-length of the segment midpoint measured from the path's arc-length centre.
#[derive(Debug, Clone, PartialEq)]
pub struct ConductorPath {
    /// Global segment indices along the path, in traversal order.
    pub segs: Vec<usize>,
    /// Per-listed-segment sign: `+1.0` if the segment's own direction aligns with
    /// path traversal, `-1.0` if it is traversed in reverse. Parallel to `segs`.
    pub signs: Vec<f64>,
    /// Signed arc-length at each segment midpoint, measured from the path's
    /// arc-length centre (`s = 0` at the middle of the total path length).
    /// Parallel to `segs`.
    pub s_mid: Vec<f64>,
    /// The two terminal (free-end) segment indices where the `I = 0` boundary
    /// condition applies (the outermost segments of the open chain).
    pub free_ends: (usize, usize),
}

impl ConductorPath {
    /// Whether this path is a plain single wire or a collinear end-to-start chain
    /// — i.e. all segments traversed forward (`sign = +1`) in contiguous ascending
    /// index order. Such a path is handled identically by the pre-existing
    /// collinear-merge Hallén code, so the general path solver is only needed when
    /// at least one path is **not** trivial (a reversed or bent junction).
    pub fn is_trivial(&self, segs: &[Segment]) -> bool {
        if self.signs.iter().any(|&s| s != 1.0) {
            return false;
        }
        // Straight as well as forward and contiguous. Without this, a BENT
        // end-to-start chain — the usual way to write an inverted-V — counted as
        // trivial, took the plain per-wire basis with a pairwise junction row, and
        // came out as garbage (-20.9 - j1274.6 where nec2c and the same antenna
        // written start-to-start give 86.8 + j197.2).
        //
        // And straight by the same rule the collinear merge uses, so a path is
        // trivial exactly when the plain route would merge it into one conductor
        // (FND-172, FND-175): see [`continues_straight`].
        self.segs
            .windows(2)
            .all(|w| w[1] == w[0] + 1 && continues_straight(&segs[w[0]], &segs[w[1]]))
    }
}

/// A reordering of the segments that puts every chain of wires in walk order,
/// for a basis that holds a conductor as one index range (FND-233).
///
/// The plain merged basis (the sinusoidal solve) can only weld wires that are
/// adjacent in the segment array, so three collinear wires written out of order
/// were one conductor when listed in chain order and a "split" refused when the
/// first was listed last. Solving the reordered segments and restoring the
/// currents is the same system re-indexed: every matrix entry, load column and
/// source is a function of the segments, never of their positions — provided the
/// WHOLE solve, from assembly on, runs on [`SegmentOrder::segments`].
///
/// The order is read from [`build_conductor_paths`], so the rule for what joins
/// is that walk's, not a third copy. Each chain is put in walk order, walked so
/// that its lowest deck segment runs forward, and every segment the walk crosses
/// against its own direction is FLIPPED (FND-249): a dipole written as two halves
/// meeting start-to-start — the natural `GX` spelling — becomes one forward chain
/// the plain basis welds. A flip turns the segment end for end, which negates the
/// sign of everything measured along it: a source voltage entering the solve goes
/// in through [`SegmentOrder::flips_source`], and the currents come back through
/// [`SegmentOrder::restore_currents`], so the report keeps each segment's own
/// start→end sense, as nec2c prints it. Loads and radii carry no sign. It is the
/// identity whenever every chain is already contiguous and forward, so a deck that
/// solved before is solved on the same indices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegmentOrder {
    /// `order[i]` is the deck index of solve index `i`; `None` is the identity.
    order: Option<Vec<usize>>,
    /// `flip[i]`: solve segment `i` runs against its deck direction. Empty when
    /// nothing is flipped.
    flip: Vec<bool>,
}

impl SegmentOrder {
    /// The chain order of `segs`.
    pub fn chains(segs: &[Segment]) -> Self {
        let Some(paths) = build_conductor_paths(segs) else {
            // A junction or loop: no plain-basis chain to restore.
            return Self::default();
        };
        // Each chain as (deck index, flipped) in walk order, walked so that its
        // lowest deck segment is crossed forward: deterministic, and on `GW` + `GX`
        // it keeps the first-written (usually fed) wire in its own frame.
        let mut chains: Vec<Vec<(usize, bool)>> = paths
            .into_iter()
            .map(|p| {
                let mut c: Vec<(usize, bool)> = p
                    .segs
                    .iter()
                    .zip(&p.signs)
                    .map(|(&i, &s)| (i, s < 0.0))
                    .collect();
                let lowest = c.iter().min_by_key(|e| e.0).map(|e| e.1);
                if lowest == Some(true) {
                    c.reverse();
                    for e in &mut c {
                        e.1 = !e.1;
                    }
                }
                c
            })
            .collect();
        chains.sort_by_key(|c| c.iter().map(|e| e.0).min());
        let (order, flip): (Vec<usize>, Vec<bool>) = chains.into_iter().flatten().unzip();
        let mut seen = vec![false; segs.len()];
        let is_permutation = order.len() == segs.len()
            && order
                .iter()
                .all(|&i| i < seen.len() && !std::mem::replace(&mut seen[i], true));
        if !is_permutation {
            return Self::default();
        }
        let identity = order.iter().enumerate().all(|(i, &o)| i == o);
        let flip = if flip.iter().any(|&f| f) {
            flip
        } else {
            Vec::new()
        };
        Self {
            order: (!identity || !flip.is_empty()).then_some(order),
            flip,
        }
    }

    /// Whether solve segment `i` is flipped against its deck direction.
    pub fn is_flipped(&self, i: usize) -> bool {
        self.flip.get(i).copied().unwrap_or(false)
    }

    /// A source on solve segment `i`, as the flipped frame sees it: a gap voltage
    /// drives current along the segment's own direction, so a flipped segment
    /// takes it negated.
    pub fn flips_source(&self, i: usize, v: num_complex::Complex64) -> num_complex::Complex64 {
        if self.is_flipped(i) {
            -v
        } else {
            v
        }
    }

    /// A deck-ordered per-segment SIGNED quantity (a gap-voltage vector) in solve
    /// order and frame.
    pub fn apply_signed(
        &self,
        deck_ordered: &[num_complex::Complex64],
    ) -> Vec<num_complex::Complex64> {
        let mut out = self.apply(deck_ordered);
        for (i, v) in out.iter_mut().enumerate() {
            *v = self.flips_source(i, *v);
        }
        out
    }

    /// Solve-ordered currents back in deck order and each segment's own sense.
    pub fn restore_currents(
        &self,
        solve_ordered: &[num_complex::Complex64],
    ) -> Vec<num_complex::Complex64> {
        let unflipped: Vec<num_complex::Complex64> = solve_ordered
            .iter()
            .enumerate()
            .map(|(i, &c)| self.flips_source(i, c))
            .collect();
        self.restore(&unflipped)
    }

    /// Whether solve order is deck order.
    pub fn is_identity(&self) -> bool {
        self.order.is_none()
    }

    /// A deck-ordered per-segment vector in solve order.
    pub fn apply<T: Clone>(&self, deck_ordered: &[T]) -> Vec<T> {
        match &self.order {
            None => deck_ordered.to_vec(),
            Some(o) => o.iter().map(|&i| deck_ordered[i].clone()).collect(),
        }
    }

    /// A solve-ordered per-segment vector back in deck order.
    pub fn restore<T: Clone>(&self, solve_ordered: &[T]) -> Vec<T> {
        match &self.order {
            None => solve_ordered.to_vec(),
            Some(o) => {
                let mut out = solve_ordered.to_vec();
                for (i, &d) in o.iter().enumerate() {
                    out[d] = solve_ordered[i].clone();
                }
                out
            }
        }
    }

    /// The segments in solve order, each `global_index` its new position and each
    /// flipped one turned end for end (midpoint and length unchanged).
    pub fn segments(&self, segs: &[Segment]) -> Vec<Segment> {
        let mut out = self.apply(segs);
        for (i, s) in out.iter_mut().enumerate() {
            s.global_index = i;
            if self.is_flipped(i) {
                std::mem::swap(&mut s.start, &mut s.end);
                s.direction = [-s.direction[0], -s.direction[1], -s.direction[2]];
            }
        }
        out
    }
}

/// Decompose the geometry into [`ConductorPath`]s for the general-junction Hallén
/// solve (PH9-CHK-002).
///
/// Returns `Some(paths)` **only** when every junction node in the deck is a simple
/// **degree-2** connection and every conductor is an **open chain** (exactly two
/// free ends). This is the supported class for the continuous-basis fix: single
/// wires, collinear splits, bent dipoles / inverted-V, and end-to-end or
/// start-to-start splits.
///
/// Returns `None` when the topology is out of scope for this slice — any node
/// where **three or more** wire ends meet (T/Y junctions), or a **closed loop**
/// (a conductor with no free end). Those cases fall back to the guarded per-wire
/// path and still require the general junction-basis work.
///
/// A single straight wire maps to one path with all `signs = +1` and `s_mid`
/// identical to the straight-axis coordinate used by [`build_hallen_rhs`], so the
/// decomposition is a numeric no-op for the non-junction case.
///
/// [`build_hallen_rhs`]: crate::build_hallen_rhs
pub fn build_conductor_paths(segs: &[Segment]) -> Option<Vec<ConductorPath>> {
    let wires = wire_endpoints_from_segs(segs);
    let w = wires.len();
    if w == 0 {
        return Some(Vec::new());
    }

    const TOL: f64 = crate::hallen_session::JUNCTION_TOL_M;

    // Endpoint identity: 2 per wire. `end = 0` is the wire start (segs[first].start),
    // `end = 1` is the wire end (segs[last].end).
    let ep_point = |wi: usize, end: usize| -> [f64; 3] {
        let (first, last) = wires[wi];
        if end == 0 {
            segs[first].start
        } else {
            segs[last].end
        }
    };

    // Cluster the 2·W endpoints into coincident nodes. `node_of[wi*2 + end]` is the
    // node id; `node_members[node]` lists the (wi, end) endpoints at that node.
    let mut node_of = vec![usize::MAX; 2 * w];
    let mut node_members: Vec<Vec<(usize, usize)>> = Vec::new();
    for wi in 0..w {
        for end in 0..2 {
            let id = wi * 2 + end;
            if node_of[id] != usize::MAX {
                continue;
            }
            let p = ep_point(wi, end);
            let node = node_members.len();
            let mut members = vec![(wi, end)];
            node_of[id] = node;
            // Find all other unassigned endpoints coincident with p.
            for wj in 0..w {
                for endj in 0..2 {
                    let idj = wj * 2 + endj;
                    if node_of[idj] != usize::MAX {
                        continue;
                    }
                    if dist2(p, ep_point(wj, endj)) < TOL * TOL {
                        node_of[idj] = node;
                        members.push((wj, endj));
                    }
                }
            }
            node_members.push(members);
        }
    }

    // Any node where 3+ ends meet is out of scope for this slice (T/Y junction).
    if node_members.iter().any(|m| m.len() >= 3) {
        return None;
    }

    // Group wires into connected chains via degree-2 nodes (union-find).
    let mut parent: Vec<usize> = (0..w).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        let mut c = x;
        while parent[c] != c {
            let next = parent[c];
            parent[c] = r;
            c = next;
        }
        r
    }
    for members in &node_members {
        if members.len() == 2 {
            let (a, _) = members[0];
            let (b, _) = members[1];
            // A degree-2 node joining a wire to itself is a closed single-wire loop.
            if a == b {
                return None;
            }
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            parent[ra] = rb;
        }
    }

    // For each component, order the wires into a linear chain and emit a path.
    let mut comp_wires: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for wi in 0..w {
        let r = find(&mut parent, wi);
        comp_wires.entry(r).or_default().push(wi);
    }

    let mut paths: Vec<ConductorPath> = Vec::new();
    for (_, group) in comp_wires {
        // Count free ends (degree-1 nodes) in this component. A valid open chain
        // has exactly two; anything else (cycle, isolated closed loop) is skipped.
        let mut free_ends: Vec<(usize, usize)> = Vec::new(); // (wi, end)
        for &wi in &group {
            for end in 0..2 {
                let node = node_of[wi * 2 + end];
                if node_members[node].len() == 1 {
                    free_ends.push((wi, end));
                }
            }
        }
        if free_ends.len() != 2 {
            return None;
        }

        // Walk from one free end through the chain, appending segments with signs
        // and cumulative arc-length. Track visited wires to avoid revisiting.
        let (start_wi, start_end) = free_ends[0];
        let mut ordered_segs: Vec<usize> = Vec::new();
        let mut signs: Vec<f64> = Vec::new();
        let mut u_mid: Vec<f64> = Vec::new(); // arc-length at midpoint (from path start)
        let mut arc = 0.0f64;

        let mut cur_wi = start_wi;
        // Enter from `start_end`; we traverse *away* from the entry node.
        // entry end 0 (wire start) → forward (start→end), sign +1.
        // entry end 1 (wire end)   → reverse (end→start), sign -1.
        let mut enter_end = start_end;
        let mut visited = vec![false; w];
        loop {
            visited[cur_wi] = true;
            let (first, last) = wires[cur_wi];
            let forward = enter_end == 0;
            let sign = if forward { 1.0 } else { -1.0 };
            // Segment order along traversal.
            let seg_iter: Vec<usize> = if forward {
                (first..=last).collect()
            } else {
                (first..=last).rev().collect()
            };
            for m in seg_iter {
                let half = segs[m].length * 0.5;
                u_mid.push(arc + half);
                arc += segs[m].length;
                ordered_segs.push(m);
                signs.push(sign);
            }
            // Exit node is the opposite end of this wire.
            let exit_end = 1 - enter_end;
            let exit_node = node_of[cur_wi * 2 + exit_end];
            // Find the next wire at the exit node (degree-2) that we have not visited.
            let members = &node_members[exit_node];
            if members.len() == 1 {
                break; // reached the other free end
            }
            // degree-2: pick the partner wire.
            let mut next: Option<(usize, usize)> = None;
            for &(mwi, mend) in members {
                if mwi != cur_wi && !visited[mwi] {
                    next = Some((mwi, mend));
                }
            }
            match next {
                Some((nwi, nend)) => {
                    cur_wi = nwi;
                    // We arrive at the partner's `nend`; traverse away from it.
                    enter_end = nend;
                }
                None => break, // no unvisited partner (defensive; should be a free end)
            }
        }

        // Re-centre arc-length so s = 0 is the path midpoint.
        let s_mid: Vec<f64> = u_mid.iter().map(|u| u - arc * 0.5).collect();
        let free_a = *ordered_segs.first().unwrap();
        let free_b = *ordered_segs.last().unwrap();
        paths.push(ConductorPath {
            segs: ordered_segs,
            signs,
            s_mid,
            free_ends: (free_a, free_b),
        });
    }

    Some(paths)
}

/// Why a geometry is out of scope for the [`ConductorPath`] (degree-2) Hallén
/// solve — used for accurate user-facing diagnostics (PH9-CHK-002 guardrail).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedTopology {
    /// A node where **three or more** wire ends meet (a T/Y junction). The current
    /// splits among the arms with a Kirchhoff constraint that the continuous
    /// single-path basis cannot represent.
    HighDegreeJunction,
    /// A **closed loop** — a conductor with no free end. The Hallén homogeneous
    /// solution needs a periodic (rather than `I = 0`) closure that is not yet
    /// modelled.
    ClosedLoop,
}

/// Classify why [`build_conductor_paths`] rejects a geometry, so callers can warn
/// with a class-specific message instead of silently returning an unreliable solve.
///
/// Returns `None` exactly when the geometry **is** supported (single wires,
/// collinear chains, or any degree-2 junctioned conductor path). When
/// [`build_conductor_paths`] returns `None`, the only causes are a node where 3+
/// wire ends meet ([`UnsupportedTopology::HighDegreeJunction`]) or a conductor with
/// no free end ([`UnsupportedTopology::ClosedLoop`]); the high-degree case is
/// reported in preference when both are present.
pub fn classify_unsupported_topology(segs: &[Segment]) -> Option<UnsupportedTopology> {
    if build_conductor_paths(segs).is_some() {
        return None;
    }
    if has_high_degree_node(segs) {
        Some(UnsupportedTopology::HighDegreeJunction)
    } else {
        Some(UnsupportedTopology::ClosedLoop)
    }
}

/// Whether any coincident-endpoint node joins **three or more** wire ends (a T/Y
/// junction), using the same endpoint clustering as [`build_conductor_paths`].
fn has_high_degree_node(segs: &[Segment]) -> bool {
    let wires = wire_endpoints_from_segs(segs);
    let w = wires.len();
    if w == 0 {
        return false;
    }
    const TOL: f64 = crate::hallen_session::JUNCTION_TOL_M;
    let ep_point = |wi: usize, end: usize| -> [f64; 3] {
        let (first, last) = wires[wi];
        if end == 0 {
            segs[first].start
        } else {
            segs[last].end
        }
    };
    let mut assigned = vec![false; 2 * w];
    for wi in 0..w {
        for end in 0..2 {
            let id = wi * 2 + end;
            if assigned[id] {
                continue;
            }
            let p = ep_point(wi, end);
            let mut count = 1;
            assigned[id] = true;
            for wj in 0..w {
                for endj in 0..2 {
                    let idj = wj * 2 + endj;
                    if assigned[idj] {
                        continue;
                    }
                    if dist2(p, ep_point(wj, endj)) < TOL * TOL {
                        assigned[idj] = true;
                        count += 1;
                    }
                }
            }
            if count >= 3 {
                return true;
            }
        }
    }
    false
}

///
/// Cards are processed in deck order:
/// - `GW` appends new segments.
/// - `GM` transforms (rotate + translate) a range of existing wires, optionally
///   creating numbered copies when `tag_increment > 0`.
/// - `GR I1 I2` makes the structure so far occur `I2` times in all, 360°/`I2`
///   apart about z (NEC-2's reading, FND-247).
/// - `GS` scales and `GX` reflects everything defined so far.
///
/// Segments are assigned consecutive `global_index` values in output order.
pub fn build_geometry(deck: &NecDeck) -> Result<Vec<Segment>, GeometryError> {
    // The Hallén geometry: every Hallén route builds it here, and none can take a
    // GA or GH (see GeometryError::CurvedForHallen). The MPIE's build_deck_geometry
    // and the GUI's viewport, which draws the deck's own segments, do not refuse.
    if let Some(card) = deck.cards.iter().find_map(|c| match c {
        Card::Ga(_) => Some("a wire arc (GA)"),
        Card::Gh(_) => Some("a helix (GH)"),
        _ => None,
    }) {
        return Err(GeometryError::CurvedForHallen {
            card,
            mpie: crate::validate::mpie_compatible_deck(deck),
        });
    }
    let segments = build_unnumbered(deck)?;
    let grounded = matches!(
        ground_model_from_deck(deck),
        GroundModel::PerfectConductor | GroundModel::SimpleFiniteGround { .. }
    );
    let free = free_end_segments(&segments, grounded);
    let segs = number(refine(segments, &free));
    match one_segment_run_topology(deck, &segs) {
        Some(e) => Err(e),
        None => Ok(segs),
    }
}

/// A loop or junction the Hallén solve has no reliable answer for (FND-251): one
/// the conductor paths cannot carry, so it needs the section graph, which refuses
/// it on a straight run one segment long — its only refusal. Judged on the
/// segments the solve runs on: a deck touching PEC ground is solved as its
/// doubled image problem (FND-082), where a one-segment run standing on the
/// ground becomes two and the graph takes it (FND-179). Every drive's graph is
/// built by the same builder, so no drive takes a deck this refuses.
fn one_segment_run_topology(deck: &NecDeck, segs: &[Segment]) -> Option<GeometryError> {
    let ground = ground_model_from_deck(deck);
    let image = crate::ground_contact::pec_ground_contact(deck, segs, &ground);
    let solved: &[Segment] = match &image {
        Ok(Some(img)) => &img.segs,
        Ok(None) => segs,
        // A contact the image route refuses is refused there, with its own reason.
        Err(_) => return None,
    };
    let kind = classify_unsupported_topology(solved)?;
    let Err(crate::section_graph::GraphRefusal::OneSegmentRun { tag, seg }) =
        crate::section_graph::section_graph(solved)
    else {
        return None;
    };
    Some(GeometryError::OneSegmentRunTopology {
        junction: matches!(kind, UnsupportedTopology::HighDegreeJunction),
        tag,
        seg,
        mpie: crate::validate::mpie_compatible_deck(deck),
    })
}

/// The deck's own segments, without the free-end refinement [`build_geometry`]
/// applies — for the solvers that do not take it (the MPIE, the experimental
/// pulse and continuity bases), so their answers do not change with it.
pub fn build_deck_geometry(deck: &NecDeck) -> Result<Vec<Segment>, GeometryError> {
    Ok(number(build_unnumbered(deck)?))
}

/// The geometry `solver` solves: refined at free ends ([`build_geometry`]) for
/// Hallén, the deck's own ([`build_deck_geometry`]) for the MPIE.
///
/// Chosen before the geometry is built, not coarsened after: the far and near
/// field weight each segment as a point source, so three thirds carrying one
/// current radiate differently from the whole — the MPIE would change with a
/// refinement it does not take (review of FND-227's design).
pub fn geometry_for(
    deck: &NecDeck,
    solver: crate::validate::SolverKind,
) -> Result<Vec<Segment>, GeometryError> {
    match solver {
        crate::validate::SolverKind::Hallen => build_geometry(deck),
        crate::validate::SolverKind::Mpie => build_deck_geometry(deck),
    }
}

fn build_unnumbered(deck: &NecDeck) -> Result<Vec<Segment>, GeometryError> {
    nec_model::card::taper_pairing(&deck.cards).map_err(GeometryError::TaperPairing)?;
    let mut segments: Vec<Segment> = Vec::new();
    // The segments each GC made, by index: GM, GR and GX only append and GS keeps
    // the order, so the indices hold to the end, where the lengths are final.
    let mut tapered: Vec<std::ops::Range<usize>> = Vec::new();

    for (i, card) in deck.cards.iter().enumerate() {
        match card {
            Card::Gw(gw) => match nec_model::card::taper_of(&deck.cards, i) {
                Some(gc) => {
                    let first = segments.len();
                    expand_tapered_wire(gw, gc, &mut segments)?;
                    tapered.push(first..segments.len());
                }
                None => expand_wire(gw, &mut segments)?,
            },
            Card::Gm(gm) => {
                apply_gm(gm, &mut segments)?;
            }
            Card::Gr(gr) => {
                apply_gr(gr, &mut segments)?;
            }
            Card::Gs(gs) => apply_gs(gs.scale, &mut segments),
            Card::Gx(gx) => apply_gx(gx, &mut segments)?,
            Card::Ga(ga) => expand_arc(ga, &mut segments)?,
            Card::Gh(gh) => expand_helix(gh, &mut segments)?,
            _ => {}
        }
    }

    if segments.is_empty() {
        return Err(GeometryError::NoWires);
    }
    for s in tapered.into_iter().flat_map(|r| segments[r].iter()) {
        let ratio = crate::validate::MIN_LENGTH_TO_RADIUS_RATIO;
        if s.length < ratio * s.radius || s.length < MIN_TAPER_SEGMENT_M {
            return Err(GeometryError::TaperTooShort {
                tag: s.tag,
                segment: s.tag_index,
                length: s.length,
                radius: s.radius,
            });
        }
    }
    if ge_flag(deck) == 1 && !matches!(ground_model_from_deck(deck), GroundModel::FreeSpace) {
        snap_ground_contacts(&mut segments);
    }
    // In any environment, and after the ground snap: an end that joins a ground
    // contact is then already at z = 0 too, so the cluster keeps its contact.
    snap_junctions(&mut segments)?;
    // After the snap, so ends that should meet do; before every topology check,
    // which would otherwise read a duplicate as a junction (FND-250).
    if let Some(e) = overlapping_segments(&segments) {
        return Err(e);
    }
    Ok(segments)
}

/// Two segments that lie along each other, closer than their radii, for part of
/// their length: one conductor drawn on top of another (FND-250).
///
/// No solver separates them. A dipole with a second wire laid along its upper half
/// and beyond reads, against the antenna it was meant to be (the dipole and a
/// 0.86 m extension: nec2c 203.8 + j383.2 Ω), Hallén −3.35 − j234.0, the MPIE
/// 176.8 + j334.8 with no caveat, and nec2c 650 + j1453 — and the same with the
/// second wire's axis 0.6 mm off the first's inside a 1 mm radius. Exact duplicates
/// were refused as a junction with a one-segment run, or as "no current flows at
/// the feedpoint", naming the wrong cause.
///
/// Judged in the wires' own scale, not in metres (FND-243, FND-246): `b`'s ends are
/// projected onto `a`'s axis, and where they overlap `a` by more than 1e-6 of the
/// shorter segment, `b` is within `a.radius + b.radius` of that axis at both ends
/// of the overlap. A crossing (one point near the axis, the rest far) is
/// `validate::segment_intersection_error`'s; a T onto an interior and the adjacent
/// segments of a wire meet at a point, overlap 0. Wires whose surfaces touch
/// (axes two radii apart) are not refused: a thin-wire validity question.
fn overlapping_segments(segs: &[Segment]) -> Option<GeometryError> {
    for (i, a) in segs.iter().enumerate() {
        for (j, b) in segs.iter().enumerate().skip(i + 1) {
            let Some(o) = overlap_along(a, b) else {
                continue;
            };
            let reach = a.radius + b.radius;
            if o.off_lo < reach && o.off_hi < reach {
                return Some(GeometryError::OverlappingSegments {
                    a: nec_name(segs, i),
                    b: nec_name(segs, j),
                    length: o.length,
                    duplicate: o.length >= (1.0 - 1e-6) * a.length.min(b.length),
                });
            }
        }
    }
    None
}

/// Where segment `b` runs alongside segment `a`: the length of `a`'s axis that
/// `b`'s ends project onto, and `b`'s distance from that axis at each end of it.
struct Overlap {
    length: f64,
    off_lo: f64,
    off_hi: f64,
}

/// `b` projected onto `a`'s axis, or `None` where the projection overlaps `a` by
/// no more than 1e-6 of the shorter segment — `a`'s own neighbours along a wire,
/// a T onto an interior, segments far apart along the axis. The one copy of the
/// projection, for the refusal ([`overlapping_segments`]) and the caveat
/// ([`close_parallel_segments`]).
fn overlap_along(a: &Segment, b: &Segment) -> Option<Overlap> {
    let dot = |p: [f64; 3], q: [f64; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    if a.length <= 0.0 {
        return None;
    }
    let along = |p: [f64; 3]| {
        dot(
            [p[0] - a.start[0], p[1] - a.start[1], p[2] - a.start[2]],
            a.direction,
        )
    };
    let (t0, t1) = (along(b.start), along(b.end));
    let (lo, hi) = (t0.min(t1).max(0.0), t0.max(t1).min(a.length));
    if hi - lo <= 1e-6 * a.length.min(b.length) {
        return None;
    }
    // `b`'s point at axis position `t`, and its distance from `a`'s axis.
    let off_axis = |t: f64| {
        let f = (t - t0) / (t1 - t0);
        let p = [
            b.start[0] + f * (b.end[0] - b.start[0]) - a.start[0],
            b.start[1] + f * (b.end[1] - b.start[1]) - a.start[1],
            b.start[2] + f * (b.end[2] - b.start[2]) - a.start[2],
        ];
        let s = dot(p, a.direction);
        let r = [
            p[0] - s * a.direction[0],
            p[1] - s * a.direction[1],
            p[2] - s * a.direction[2],
        ];
        dot(r, r).sqrt()
    };
    Some(Overlap {
        length: hi - lo,
        off_lo: off_axis(lo),
        off_hi: off_axis(hi),
    })
}

/// Two parallel segments close enough that no code here answers them reliably,
/// though not so close that [`overlapping_segments`] refuses them (FND-274).
#[derive(Debug, Clone, PartialEq)]
pub struct CloseParallel {
    /// The two wires' tags.
    pub tags: (u32, u32),
    /// Their axes' distance, metres.
    pub spacing: f64,
    /// The sum of their radii, metres.
    pub radii: f64,
    /// The longer of the two segments, metres.
    pub segment: f64,
}

impl CloseParallel {
    /// Inside the thin-wire kernel's reach: spacing under ten radius sums.
    pub fn within_radii(&self) -> bool {
        self.spacing < CLOSE_PARALLEL_RADII * self.radii
    }
    /// Segments longer than the spacing: nec2c's own answer is not converged.
    pub fn within_segment(&self) -> bool {
        self.spacing < self.segment
    }
}

/// The spacing, in sums of the two radii, below which parallel wires get the
/// FND-274 caveat. Measured on a dipole with a parallel wire along its upper half
/// (28.4 MHz, 1 mm radii): Hallén and the MPIE agree within 2.5 % from 10 radius
/// sums apart, part by 4 % at 7.5, 21 % at 5 and 19 % (reactance) at 2.5, while
/// nec2c moves by more than either under refinement.
pub const CLOSE_PARALLEL_RADII: f64 = 10.0;

/// The first pair of segments of different wires that run parallel (within 1°)
/// alongside each other closer than [`CLOSE_PARALLEL_RADII`] radius sums or than
/// the longer segment's length (FND-274). Two clauses, both measured: under ten
/// radius sums Hallén and the MPIE disagree by 4–21 %; with segments longer
/// than the spacing nec2c disagrees with its own refinement by more than 5 %
/// (17 % in reactance at a fifth of a segment), aligned or not. Pairs on one wire
/// never qualify: a wire's segments do not run alongside each other.
pub fn close_parallel_segments(segs: &[Segment]) -> Option<CloseParallel> {
    let cos_1deg = 1.0_f64.to_radians().cos();
    for (i, a) in segs.iter().enumerate() {
        for b in segs.iter().skip(i + 1) {
            if a.tag == b.tag {
                continue;
            }
            let d = a.direction;
            let e = b.direction;
            if (d[0] * e[0] + d[1] * e[1] + d[2] * e[2]).abs() < cos_1deg {
                continue;
            }
            let Some(o) = overlap_along(a, b) else {
                continue;
            };
            let pair = CloseParallel {
                tags: (a.tag, b.tag),
                spacing: o.off_lo.min(o.off_hi),
                radii: a.radius + b.radius,
                segment: a.length.max(b.length),
            };
            if pair.within_radii() || pair.within_segment() {
                return Some(pair);
            }
        }
    }
    None
}

/// Join segment ends as nec2c's `conect` joins them, and move each joined group
/// onto one point (FND-246).
///
/// nec2c joins an end of segment `i` to the first end of another segment within
/// [`NEC_SMIN`] × `i`'s length, by the sum of the coordinate differences (the L1
/// distance), and then moves every end of a junction to the group's centroid.
/// Its band is the searching segment's own, so between a long and a short segment
/// it can reach from one side only; nec2c then aborts (`SBF - SEGMENT CONNECTION
/// ERROR`) or hangs, depending on the shape. A chain of near-misses — A within
/// the band of B, B of C, A not of C — hangs it. The outcomes for two ends `d`
/// apart are therefore:
/// - `d <= NEC_SMIN · min(len_i, len_j)`: joined;
/// - within the longer segment's band only: refused ([`GeometryError::OneSidedJunction`]);
/// - beyond both: free.
///
/// A joined group must be a clique — every pair of its ends joined by that rule —
/// or it is refused ([`GeometryError::NonTransitiveJunction`]). Each group moves
/// onto its earliest member (lowest segment, start before end), not nec2c's
/// centroid: an exactly drawn joint, the usual case, then never moves, and the two
/// differ by under the band (about 1e-5 of the impedance on a 50 µm gap).
/// Afterwards every junction is exact to float noise, which is what every
/// consumer's [`crate::hallen_session::JUNCTION_TOL_M`] comparison relies on.
fn snap_junctions(segments: &mut [Segment]) -> Result<(), GeometryError> {
    let n = segments.len();
    // Point p is end (p % 2) of segment p / 2: 0 the start, 1 the end.
    let point = |segs: &[Segment], p: usize| {
        let s = &segs[p / 2];
        if p % 2 == 0 {
            s.start
        } else {
            s.end
        }
    };
    let l1 =
        |a: [f64; 3], b: [f64; 3]| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
    let max_len = segments.iter().map(|s| s.length).fold(0.0_f64, f64::max);
    // No band exceeds the cell and the L1 distance bounds each coordinate's
    // difference, so a partner is always in the point's cell or a neighbour.
    let cell = NEC_SMIN * max_len;
    // Zero-length segments are refused upstream; a NaN length is no geometry.
    if cell.is_nan() || cell <= 0.0 {
        return Ok(());
    }
    let key = |p: [f64; 3]| {
        [
            (p[0] / cell).floor() as i64,
            (p[1] / cell).floor() as i64,
            (p[2] / cell).floor() as i64,
        ]
    };
    let mut grid: std::collections::HashMap<[i64; 3], Vec<usize>> =
        std::collections::HashMap::new();
    for p in 0..2 * n {
        grid.entry(key(point(segments, p))).or_default().push(p);
    }
    let mut parent: Vec<usize> = (0..2 * n).collect();
    fn root(parent: &mut [usize], mut p: usize) -> usize {
        while parent[p] != p {
            parent[p] = parent[parent[p]];
            p = parent[p];
        }
        p
    }
    let band = |segs: &[Segment], i: usize, j: usize| {
        let (a, b) = (segs[i].length, segs[j].length);
        (NEC_SMIN * a.min(b), NEC_SMIN * a.max(b))
    };
    for p in 0..2 * n {
        let pp = point(segments, p);
        let k = key(pp);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(cands) = grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) else {
                        continue;
                    };
                    for &q in cands {
                        let (i, j) = (p / 2, q / 2);
                        if q <= p || i == j {
                            continue;
                        }
                        let d = l1(pp, point(segments, q));
                        let (short, long) = band(segments, i, j);
                        if d <= short {
                            let (rp, rq) = (root(&mut parent, p), root(&mut parent, q));
                            parent[rp.max(rq)] = rp.min(rq);
                        } else if d <= long {
                            return Err(GeometryError::OneSidedJunction {
                                a: nec_name(segments, i),
                                b: nec_name(segments, j),
                                gap: d,
                                band_short: short,
                                band_long: long,
                            });
                        }
                    }
                }
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for p in 0..2 * n {
        let r = root(&mut parent, p);
        groups.entry(r).or_default().push(p);
    }
    let mut moved = vec![false; n];
    for members in groups.values().filter(|m| m.len() > 1) {
        // A clique, or refused: report the widest pair the rule does not join.
        let mut worst: Option<(usize, usize, f64, f64)> = None;
        for (x, &p) in members.iter().enumerate() {
            for &q in &members[x + 1..] {
                let d = l1(point(segments, p), point(segments, q));
                let short = if p / 2 == q / 2 {
                    -1.0
                } else {
                    band(segments, p / 2, q / 2).0
                };
                if d > short && worst.map_or(true, |w| d > w.2) {
                    worst = Some((p / 2, q / 2, d, short.max(0.0)));
                }
            }
        }
        if let Some((i, j, gap, short)) = worst {
            return Err(GeometryError::NonTransitiveJunction {
                a: nec_name(segments, i),
                b: nec_name(segments, j),
                gap,
                band: short,
            });
        }
        let anchor = point(segments, members[0]);
        for &p in &members[1..] {
            if point(segments, p) == anchor {
                continue;
            }
            let s = &mut segments[p / 2];
            if p % 2 == 0 {
                s.start = anchor;
            } else {
                s.end = anchor;
            }
            moved[p / 2] = true;
        }
    }
    for (s, m) in segments.iter_mut().zip(moved) {
        if m {
            s.rederive_from_ends();
        }
    }
    Ok(())
}

/// `(tag, segment number)` for the `i`-th segment as NEC numbers it: the running
/// occurrence of its tag, which is what [`number`] assigns later.
fn nec_name(segs: &[Segment], i: usize) -> (u32, u32) {
    let tag = segs[i].tag;
    (
        tag,
        segs[..=i].iter().filter(|s| s.tag == tag).count() as u32,
    )
}

/// The centre a flank at `segs[i]` is a third of (FND-227): the next segment when
/// it is a centre (the start flank), else the one before — the rule [`number`]
/// numbers a flank's parent by, so a caller that judges a flank as its deck
/// segment (a type-5 load, per metre) reads the centre's `tag_index` and
/// `abs_index`. `None` when `segs[i]` is not a flank.
pub(crate) fn flank_centre(segs: &[Segment], i: usize) -> Option<&Segment> {
    if !matches!(segs.get(i)?.part, SegmentPart::Flank { .. }) {
        return None;
    }
    let next = segs.get(i + 1).filter(|s| s.part == SegmentPart::Centre);
    next.or_else(|| {
        i.checked_sub(1)
            .and_then(|j| segs.get(j))
            .filter(|s| s.part == SegmentPart::Centre)
    })
}

/// Re-number `global_index` in final order, `tag_index` as NEC counts it — the
/// running occurrence of each tag (see `Segment::tag_index`, FND-135) — and
/// `abs_index` as NEC's absolute segment number, the running count of deck
/// segments (FND-282). Both numberings in one loop, so they cannot drift apart. A
/// flank does not count — the deck does not know it — and is numbered 0 in both,
/// its parent the centre beside it.
fn number(mut segments: Vec<Segment>) -> Vec<Segment> {
    let mut seen: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut absolute = 0u32;
    for i in 0..segments.len() {
        segments[i].global_index = i;
        let tag = segments[i].tag;
        let n = seen.entry(tag).or_insert(0);
        match segments[i].part {
            SegmentPart::Flank { .. } => {
                // Before its centre (the start flank) it belongs to the next number.
                let before_centre = segments
                    .get(i + 1)
                    .is_some_and(|s| s.part == SegmentPart::Centre);
                let parent = if before_centre { *n + 1 } else { *n };
                segments[i].tag_index = 0;
                segments[i].abs_index = 0;
                segments[i].part = SegmentPart::Flank { parent };
            }
            _ => {
                *n += 1;
                absolute += 1;
                segments[i].tag_index = *n;
                segments[i].abs_index = absolute;
            }
        }
    }
    segments
}

/// `values` (one per solved segment — currents, charges) at the deck's segments
/// only: a refined end's flanks dropped, its centre standing for the deck segment
/// (FND-227). Index `i` of the result is the deck's `i`-th segment, as an output
/// row or a reference solver (nec2c) numbers it.
pub fn deck_values<T: Copy>(segs: &[Segment], values: &[T]) -> Vec<T> {
    segs.iter()
        .zip(values)
        .filter(|(s, _)| s.is_deck_segment())
        .map(|(_, v)| *v)
        .collect()
}

/// For each segment, whether it has a free end: an end point no other segment
/// shares (within [`crate::hallen_session::JUNCTION_TOL_M`]), and — over a ground —
/// not on the ground plane, where its image joins it.
///
/// The rule the Hallén solve applies (`wire_endpoints_from_segs`,
/// `split_at_touching_ends`): wires join where end points meet; a touch in the
/// middle of a segment stays unconnected, as in NEC, so that end is free.
pub fn free_end_segments(segs: &[Segment], grounded: bool) -> Vec<bool> {
    let tol = crate::hallen_session::JUNCTION_TOL_M;
    // End points bucketed on a grid much coarser than the tolerance; a match is
    // looked for in the point's cell and its neighbours.
    let cell = 1e3 * tol;
    let key = |p: &[f64; 3]| {
        [
            (p[0] / cell).floor() as i64,
            (p[1] / cell).floor() as i64,
            (p[2] / cell).floor() as i64,
        ]
    };
    let mut grid: std::collections::HashMap<[i64; 3], Vec<(usize, [f64; 3])>> =
        std::collections::HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        for p in [s.start, s.end] {
            grid.entry(key(&p)).or_default().push((i, p));
        }
    }
    let shared = |i: usize, p: &[f64; 3]| {
        let k = key(p);
        (-1..=1).any(|dx| {
            (-1..=1).any(|dy| {
                (-1..=1).any(|dz| {
                    grid.get(&[k[0] + dx, k[1] + dy, k[2] + dz])
                        .is_some_and(|pts| {
                            pts.iter().any(|&(j, q)| {
                                j != i
                                    && ((p[0] - q[0]).powi(2)
                                        + (p[1] - q[1]).powi(2)
                                        + (p[2] - q[2]).powi(2))
                                    .sqrt()
                                        <= tol
                            })
                        })
                })
            })
        })
    };
    let on_ground = |p: &[f64; 3]| grounded && p[2] <= crate::ground_contact::GROUND_CONTACT_EPS_M;
    segs.iter()
        .enumerate()
        .map(|(i, s)| {
            [s.start, s.end]
                .iter()
                .any(|p| !shared(i, p) && !on_ground(p))
        })
        .collect()
}

/// Split each segment `free` marks into three equal thirds: flank, centre, flank.
fn refine(segs: Vec<Segment>, free: &[bool]) -> Vec<Segment> {
    let mut out = Vec::with_capacity(segs.len() + 2 * free.iter().filter(|&&f| f).count());
    for (s, &f) in segs.into_iter().zip(free) {
        if !f {
            out.push(s);
            continue;
        }
        let at = |t: f64| {
            [
                s.start[0] + (s.end[0] - s.start[0]) * t,
                s.start[1] + (s.end[1] - s.start[1]) * t,
                s.start[2] + (s.end[2] - s.start[2]) * t,
            ]
        };
        for (k, part) in [
            SegmentPart::Flank { parent: 0 },
            SegmentPart::Centre,
            SegmentPart::Flank { parent: 0 },
        ]
        .into_iter()
        .enumerate()
        {
            let (t0, t1) = (k as f64 / 3.0, (k as f64 + 1.0) / 3.0);
            out.push(Segment {
                start: at(t0),
                end: at(t1),
                // The centre's midpoint is the deck segment's, exactly.
                midpoint: if part == SegmentPart::Centre {
                    s.midpoint
                } else {
                    at(0.5 * (t0 + t1))
                },
                length: s.length / 3.0,
                part,
                ..s.clone()
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Refuse a card that would take the running segment total past [`MAX_SEGMENTS`].
///
/// A *running* total, checked before each card expands, rather than a per-card
/// limit: `GM` copy cards double the structure, so forty innocent-looking cards
/// compose into a 2^40 blow-up that no per-card check would ever see.
///
/// Done in `u64` on purpose. The deck supplies `u32` counts, and the projections
/// this guards — `have + adding`, and `originals * count` for `GR` — overflow
/// `u32` for exactly the inputs the guard exists to catch, so doing the
/// arithmetic in the input's own type would wrap into a passing number.
fn check_segment_budget(have: usize, adding: u64, card: &'static str) -> Result<(), GeometryError> {
    let projected = have as u64 + adding;
    if projected > MAX_SEGMENTS as u64 {
        return Err(GeometryError::SegmentBudgetExceeded { card, projected });
    }
    Ok(())
}

/// The shortest segment a `GC` taper may make, absolute: three junction
/// tolerances, since the Hallén refinement splits a free-end segment into thirds
/// before the tolerance is applied to them (FND-227).
const MIN_TAPER_SEGMENT_M: f64 = 3.0 * crate::hallen_session::JUNCTION_TOL_M;

/// A `GW` of radius 0 tapered by its `GC`: nec2c's `wire`, in its float order.
/// Each segment `RDEL` times the one before (the first `L(1 − RDEL)/(1 − RDEL^NS)`;
/// within 1e-6 of 1, `NS` equal steps of `(P2 − P1)/NS`), each starting at the
/// previous one's end, the last ending at `P2` exactly; the radii from `RAD1` by
/// `(RAD2/RAD1)^(1/(NS − 1))` per segment, accumulated.
fn expand_tapered_wire(
    gw: &GwCard,
    gc: &nec_model::card::GcCard,
    out: &mut Vec<Segment>,
) -> Result<(), GeometryError> {
    if gw.segments == 0 {
        return Err(GeometryError::ZeroSegments { tag: gw.tag });
    }
    if gw.start == gw.end {
        return Err(GeometryError::ZeroLengthWire { tag: gw.tag });
    }
    check_segment_budget(out.len(), u64::from(gw.segments), "GW")?;
    let ns = gw.segments;
    let mut d: [f64; 3] = std::array::from_fn(|k| gw.end[k] - gw.start[k]);
    let (mut delz, rd) = if (gc.length_ratio - 1.0).abs() >= 1.0e-6 {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        d = d.map(|c| c / len);
        let r = gc.length_ratio;
        (len * (1.0 - r) / (1.0 - r.powf(f64::from(ns))), r)
    } else {
        let fns = f64::from(ns);
        d = d.map(|c| c / fns);
        (1.0, 1.0)
    };
    // NS = 1 has no step (the parser refused one with RAD2 ≠ RAD1); nec2c's
    // 1/(NS − 1) is then infinite, but only multiplies after the one segment.
    let rrad = if ns > 1 {
        (gc.radius_end / gc.radius_start).powf(1.0 / (f64::from(ns) - 1.0))
    } else {
        1.0
    };
    let mut radz = gc.radius_start;
    let mut p = gw.start;
    for i in 0..ns {
        let mut q = [p[0] + d[0] * delz, p[1] + d[1] * delz, p[2] + d[2] * delz];
        if i + 1 == ns {
            q = gw.end;
        }
        let at = out.len();
        out.push(segment_between(gw.tag, i, p, q, radz, at));
        delz *= rd;
        radz *= rrad;
        p = q;
    }
    Ok(())
}

fn expand_wire(gw: &GwCard, out: &mut Vec<Segment>) -> Result<(), GeometryError> {
    if gw.segments == 0 {
        return Err(GeometryError::ZeroSegments { tag: gw.tag });
    }

    let [x1, y1, z1] = gw.start;
    let [x2, y2, z2] = gw.end;

    let wire_dx = x2 - x1;
    let wire_dy = y2 - y1;
    let wire_dz = z2 - z1;
    let wire_len = (wire_dx * wire_dx + wire_dy * wire_dy + wire_dz * wire_dz).sqrt();

    if wire_len == 0.0 {
        return Err(GeometryError::ZeroLengthWire { tag: gw.tag });
    }

    // Before the loop, not inside it: `gw.segments` is a u32 straight off the
    // card, so `GW 1 4000000000` is a ~512 GB allocation from a one-line deck.
    check_segment_budget(out.len(), u64::from(gw.segments), "GW")?;

    let direction = [wire_dx / wire_len, wire_dy / wire_len, wire_dz / wire_len];
    let n = gw.segments as f64;
    let seg_len = wire_len / n;

    for i in 0..gw.segments {
        let t0 = i as f64 / n;
        let t1 = (i as f64 + 1.0) / n;
        let tm = (t0 + t1) * 0.5;

        let start = [x1 + wire_dx * t0, y1 + wire_dy * t0, z1 + wire_dz * t0];
        let end = [x1 + wire_dx * t1, y1 + wire_dy * t1, z1 + wire_dz * t1];
        let midpoint = [x1 + wire_dx * tm, y1 + wire_dy * tm, z1 + wire_dz * tm];

        out.push(Segment {
            tag: gw.tag,
            tag_index: i + 1,
            abs_index: 0,
            global_index: out.len(),
            start,
            end,
            midpoint,
            direction,
            length: seg_len,
            radius: gw.radius,
            part: SegmentPart::Whole,
        });
    }

    Ok(())
}

/// Apply a GM card: rotate then translate a suffix of the structure, optionally
/// generating `repeat_count` copies of it.
///
/// The three rules here are NEC-2's, and all three differed from what fnec did
/// before (FND-119). Each is pinned by a test against nec2c.
///
/// - **`ITS` selects a SUFFIX in definition order.** It resolves to the first
///   segment carrying that tag; everything from there to the end of the
///   structure is affected. Not a tag range, and not "every tag ≥ ITS": with
///   wires defined 5, 2, 3, moving from tag 2 moves wires 2 **and** 3 and leaves
///   5 alone. `0` means the whole structure. A tag no wire carries is refused —
///   nec2c exits 255 there, and fnec used to match nothing and carry on.
/// - **`NRPT`, not `ITGI`, decides copy-versus-in-place.** `NRPT == 0` moves the
///   suffix where it stands; fnec keyed this off `ITGI == 0`, a different
///   condition producing a different structure.
/// - **An in-place move applies `ITGI` to the tags as well**, so
///   `GM 10 0 … 1. 0` leaves three segments all tagged 11. Tag-0 segments are
///   never retagged, matching nec2c's `if (itag[i] != 0)` guard.
///
/// Copies are cumulative: copy *k* is the transform applied *k* times, tagged
/// `tag + k·ITGI`. That falls out of building each copy from the PREVIOUS one
/// rather than from the original, which is also what nec2c does — so neither
/// property needs special-casing.
fn apply_gm(gm: &GmCard, segs: &mut Vec<Segment>) -> Result<(), GeometryError> {
    // Resolve ITS to where the affected suffix begins.
    let start_idx = if gm.start_tag == 0 {
        0
    } else {
        segs.iter()
            .position(|s| s.tag == gm.start_tag)
            .ok_or(GeometryError::StartTagNotFound { tag: gm.start_tag })?
    };
    let suffix_len = segs.len() - start_idx;

    let retag = |tag: u32| copy_tag(tag, gm.tag_increment, "GM");

    if gm.repeat_count == 0 {
        for seg in segs.iter_mut().skip(start_idx) {
            seg.tag = retag(seg.tag)?;
            seg.start = transform_point(seg.start, gm);
            seg.end = transform_point(seg.end, gm);
            seg.midpoint = transform_point(seg.midpoint, gm);
            seg.direction = recompute_direction(seg.start, seg.end);
        }
        return Ok(());
    }

    // Before the loop, and in u64: NRPT is a deck-supplied multiplier exactly
    // like GR's count, and fifteen composing GM cards was FND-125's lesson.
    check_segment_budget(
        segs.len(),
        suffix_len as u64 * u64::from(gm.repeat_count),
        "GM",
    )?;

    let mut src = start_idx..segs.len();
    for _ in 0..gm.repeat_count {
        let next_start = segs.len();
        for i in src.clone() {
            let mut seg = segs[i].clone();
            seg.tag = retag(seg.tag)?;
            seg.start = transform_point(seg.start, gm);
            seg.end = transform_point(seg.end, gm);
            seg.midpoint = transform_point(seg.midpoint, gm);
            seg.direction = recompute_direction(seg.start, seg.end);
            segs.push(seg);
        }
        // The next copy is built from the one just made, which is what makes the
        // transform and the tag increment both cumulative.
        src = next_start..segs.len();
    }
    Ok(())
}

/// The tag of a copy that a `GM`, `GR` or `GX` card makes of a segment tagged
/// `tag`: `tag + step`, except that tag 0 stays 0, as nec2c keeps it on all three
/// (its `move`, the rotation branch and `reflc`). One rule, so the three cannot
/// differ: `GR` did — it retagged tag-0 copies (found by Fable's review of `GX`).
fn copy_tag(tag: u32, step: u32, card: &'static str) -> Result<u32, GeometryError> {
    if tag == 0 {
        return Ok(0);
    }
    tag.checked_add(step)
        .ok_or_else(|| GeometryError::TagOverflow {
            card,
            detail: format!("tag {tag} plus tag increment {step}"),
        })
}

/// `GX`: reflect in the selected planes, z then y then x, as nec2c's `reflc`
/// does. Each pass appends a mirrored copy of every segment present, tagged
/// `tag + step` (tag 0 stays 0), and `step` doubles for the next pass. A segment
/// in or across the plane is refused by nec2c's absolute test (`|e1| + |e2| <=
/// 1e-5` m), and fnec refuses any crossing at all: nec2c lets one under
/// 1 mm × 1 mm through (`e1·e2 < -1e-6`) and then solves a structure with
/// overlapping segments (Fable's review measured 4e-11 − j2.9e-4 Ω).
fn apply_gx(gx: &GxCard, segs: &mut Vec<Segment>) -> Result<(), GeometryError> {
    let mut step = Some(gx.tag_increment);
    for (on, axis) in [(gx.z, 2usize), (gx.y, 1), (gx.x, 0)] {
        if !on {
            continue;
        }
        let step_now = step.ok_or_else(|| GeometryError::TagOverflow {
            card: "GX",
            detail: format!("tag increment {} doubled per plane", gx.tag_increment),
        })?;
        for (i, s) in segs.iter().enumerate() {
            let (e1, e2) = (s.start[axis], s.end[axis]);
            if e1.abs() + e2.abs() <= 1.0e-5 || e1 * e2 < 0.0 {
                return Err(GeometryError::InSymmetryPlane {
                    tag: s.tag,
                    segment: i + 1,
                    axis: ['x', 'y', 'z'][axis],
                });
            }
        }
        check_segment_budget(segs.len(), segs.len() as u64, "GX")?;
        let originals = segs.clone();
        for orig in &originals {
            let mut seg = orig.clone();
            seg.tag = copy_tag(seg.tag, step_now, "GX")?;
            seg.start[axis] = -seg.start[axis];
            seg.end[axis] = -seg.end[axis];
            seg.rederive_from_ends();
            segs.push(seg);
        }
        step = step_now.checked_mul(2);
    }
    Ok(())
}

/// Apply a GR card as NEC-2 reads it (FND-247): the structure so far occurs
/// `occurrences` times in all, copy `k` rotated `k·360°/occurrences` about z and
/// tagged `+ k·I1` (tag 0 kept). nec2c builds each copy from the one before by one
/// step; the direct `k·360/n` agrees with it far below printed digits (its own
/// TP carries 2e-10 rad of error) and is exact at 90°, 180°, 270°.
fn apply_gr(gr: &GrCard, segs: &mut Vec<Segment>) -> Result<(), GeometryError> {
    if gr.occurrences <= 1 {
        return Ok(());
    }
    // Snapshot original set of segments (before any copies).
    let originals: Vec<Segment> = segs.clone();
    check_segment_budget(
        segs.len(),
        originals.len() as u64 * u64::from(gr.occurrences - 1),
        "GR",
    )?;
    let step_deg = 360.0 / f64::from(gr.occurrences);
    for copy_idx in 1..gr.occurrences {
        // Loop-invariant, and the multiply is where the overflow actually happens.
        let step =
            gr.tag_increment
                .checked_mul(copy_idx)
                .ok_or_else(|| GeometryError::TagOverflow {
                    card: "GR",
                    detail: format!("tag increment {} times copy {copy_idx}", gr.tag_increment),
                })?;
        let total_angle_deg = step_deg * copy_idx as f64;
        let cos_a = total_angle_deg.to_radians().cos();
        let sin_a = total_angle_deg.to_radians().sin();
        for orig in &originals {
            let mut seg = orig.clone();
            seg.tag = copy_tag(seg.tag, step, "GR")?;
            seg.start = rotate_z(seg.start, cos_a, sin_a);
            seg.end = rotate_z(seg.end, cos_a, sin_a);
            seg.midpoint = rotate_z(seg.midpoint, cos_a, sin_a);
            seg.direction = recompute_direction(seg.start, seg.end);
            segs.push(seg);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// Apply GM rotation (Rx Ry Rz) then translation to a point.
///
/// Rotation order follows NEC convention: Rx, then Ry, then Rz.
fn transform_point(p: [f64; 3], gm: &GmCard) -> [f64; 3] {
    let p = rotate_x(
        p,
        gm.rot_x_deg.to_radians().cos(),
        gm.rot_x_deg.to_radians().sin(),
    );
    let p = rotate_y(
        p,
        gm.rot_y_deg.to_radians().cos(),
        gm.rot_y_deg.to_radians().sin(),
    );
    let p = rotate_z(
        p,
        gm.rot_z_deg.to_radians().cos(),
        gm.rot_z_deg.to_radians().sin(),
    );
    [
        p[0] + gm.translate_x,
        p[1] + gm.translate_y,
        p[2] + gm.translate_z,
    ]
}

fn rotate_x(p: [f64; 3], c: f64, s: f64) -> [f64; 3] {
    [p[0], c * p[1] - s * p[2], s * p[1] + c * p[2]]
}

fn rotate_y(p: [f64; 3], c: f64, s: f64) -> [f64; 3] {
    [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
}

fn rotate_z(p: [f64; 3], c: f64, s: f64) -> [f64; 3] {
    [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]]
}

fn recompute_direction(start: [f64; 3], end: [f64; 3]) -> [f64; 3] {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let dz = end[2] - start[2];
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len == 0.0 {
        [0.0, 0.0, 1.0] // degenerate — direction undefined
    } else {
        [dx / len, dy / len, dz / len]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, GmCard, GnCard, GrCard, GwCard};
    use nec_model::deck::NecDeck;

    fn deck_with_gw(tag: u32, segs: u32, start: [f64; 3], end: [f64; 3], r: f64) -> NecDeck {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag,
            segments: segs,
            start,
            end,
            radius: r,
        }));
        deck
    }

    /// Dipole along Z: 11-segment half-wave dipole at 28 MHz.
    /// Total length = 5.354 m; each segment ≈ 0.4867 m.
    #[test]
    fn dipole_segment_count_and_length() {
        let deck = deck_with_gw(1, 11, [0.0, 0.0, -2.677], [0.0, 0.0, 2.677], 0.001);
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).expect("should succeed");

        assert_eq!(segs.len(), 11);

        let expected_len = 5.354 / 11.0;
        for s in &segs {
            let diff = (s.length - expected_len).abs();
            assert!(diff < 1e-10, "segment length off: {}", s.length);
        }
    }

    #[test]
    fn direction_is_unit_vector() {
        let deck = deck_with_gw(1, 5, [0.0, 0.0, -1.0], [0.0, 0.0, 1.0], 0.001);
        let segs = build_geometry(&deck).unwrap();
        for s in &segs {
            let [dx, dy, dz] = s.direction;
            let mag = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!((mag - 1.0).abs() < 1e-12, "direction not unit: {mag}");
        }
    }

    #[test]
    fn midpoint_is_centre_of_segment() {
        let deck = deck_with_gw(1, 3, [0.0, 0.0, 0.0], [3.0, 0.0, 0.0], 0.001);
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        // Segments at x=[0,1], [1,2], [2,3]; midpoints at x=0.5, 1.5, 2.5
        let expected_midpoints = [0.5_f64, 1.5, 2.5];
        for (s, &ex) in segs.iter().zip(expected_midpoints.iter()) {
            assert!((s.midpoint[0] - ex).abs() < 1e-12);
        }
    }

    #[test]
    fn tag_and_indices_are_correct() {
        let deck = deck_with_gw(7, 4, [0.0, 0.0, 0.0], [4.0, 0.0, 0.0], 0.001);
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        for (i, s) in segs.iter().enumerate() {
            assert_eq!(s.tag, 7);
            assert_eq!(s.tag_index, i as u32 + 1);
            assert_eq!(s.global_index, i);
        }
    }

    #[test]
    fn two_wires_global_indices_are_contiguous() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, 0.0],
            end: [3.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 2,
            start: [0.0, 1.0, 0.0],
            end: [2.0, 1.0, 0.0],
            radius: 0.001,
        }));
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 5);
        for (i, s) in segs.iter().enumerate() {
            assert_eq!(s.global_index, i);
        }
    }

    #[test]
    fn zero_segments_is_error() {
        let deck = deck_with_gw(1, 0, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.001);
        assert!(matches!(
            build_geometry(&deck),
            Err(GeometryError::ZeroSegments { tag: 1 })
        ));
    }

    #[test]
    fn zero_length_wire_is_error() {
        let deck = deck_with_gw(1, 3, [1.0, 1.0, 1.0], [1.0, 1.0, 1.0], 0.001);
        assert!(matches!(
            build_geometry(&deck),
            Err(GeometryError::ZeroLengthWire { tag: 1 })
        ));
    }

    #[test]
    fn no_wires_is_error() {
        let deck = NecDeck::new();
        assert!(matches!(build_geometry(&deck), Err(GeometryError::NoWires)));
    }

    #[test]
    fn ground_model_defaults_to_free_space_without_gn() {
        let deck = NecDeck::new();
        assert_eq!(ground_model_from_deck(&deck), GroundModel::FreeSpace);
    }

    #[test]
    fn ground_model_detects_perfect_conductor_gn1() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 1,
            eps_r: None,
            sigma: None,
            radials: 0,
            screen: None,
            medium2: None,
        }));
        assert_eq!(ground_model_from_deck(&deck), GroundModel::PerfectConductor);
    }

    #[test]
    fn ground_model_maps_gn0_to_simple_finite_ground() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 0,
            eps_r: None,
            sigma: None,
            radials: 0,
            screen: None,
            medium2: None,
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 13.0,
                sigma: 0.005,
            }
        );
    }

    #[test]
    fn ground_model_gn0_uses_explicit_medium_params() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 0,
            eps_r: Some(5.0),
            sigma: Some(0.001),
            radials: 0,
            screen: None,
            medium2: None,
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 5.0,
                sigma: 0.001,
            }
        );
    }

    #[test]
    fn ground_model_maps_gn2_to_simple_finite_ground() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 2,
            eps_r: None,
            sigma: None,
            radials: 0,
            screen: None,
            medium2: None,
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 13.0,
                sigma: 0.005,
            }
        );
    }

    #[test]
    fn ground_model_gn_negative1_returns_free_space() {
        // GN -1 is the NEC "null ground" card — it cancels a previous GN
        // statement and should produce FreeSpace with no warning.
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: -1,
            eps_r: None,
            sigma: None,
            radials: 0,
            screen: None,
            medium2: None,
        }));
        assert_eq!(ground_model_from_deck(&deck), GroundModel::FreeSpace);
    }

    #[test]
    fn ground_model_gn2_uses_explicit_medium_params() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gn(GnCard {
            ground_type: 2,
            eps_r: Some(13.0),
            sigma: Some(0.005),
            radials: 0,
            screen: None,
            medium2: None,
        }));
        assert_eq!(
            ground_model_from_deck(&deck),
            GroundModel::SimpleFiniteGround {
                eps_r: 13.0,
                sigma: 0.005,
            }
        );
    }

    // -------------------------------------------------------------------------
    // GM / GR tests
    // -------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn make_gm(
        tag_increment: u32,
        repeat_count: u32,
        start_tag: u32,
        rx: f64,
        ry: f64,
        rz: f64,
        tx: f64,
        ty: f64,
        tz: f64,
    ) -> GmCard {
        GmCard {
            tag_increment,
            repeat_count,
            start_tag,
            rot_x_deg: rx,
            rot_y_deg: ry,
            rot_z_deg: rz,
            translate_x: tx,
            translate_y: ty,
            translate_z: tz,
        }
    }

    /// GM with NRPT=0 translates segments in place (ITGI=0 here, so no retag).
    #[test]
    fn gm_inplace_translate() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }));
        // Translate +2 m along z
        deck.cards
            .push(Card::Gm(make_gm(0, 0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0)));
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 1);
        assert!((segs[0].start[2] - 2.0).abs() < 1e-12);
        assert!((segs[0].end[2] - 2.0).abs() < 1e-12);
    }

    // -------------------------------------------------------------------------
    // FND-125 — geometry must refuse before it allocates
    // -------------------------------------------------------------------------

    fn one_segment_wire(tag: u32) -> GwCard {
        GwCard {
            tag,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }
    }

    /// The largest hole, and the one needing no GM or GR at all: a single GW
    /// card with a u32 segment count is a ~512 GB allocation from a one-line
    /// deck. Measured before the fix under `ulimit -v 2000000`: SIGABRT, exit
    /// 134, "memory allocation of 2147483648 bytes failed" — still growing when
    /// it hit the ceiling.
    #[test]
    fn a_single_gw_card_cannot_ask_for_more_segments_than_the_cap() {
        let mut deck = NecDeck::new();
        // 100 000, not 4e9: over the cap by 10x, but only ~13 MB if the guard is
        // removed. A cap test whose sabotage run is itself an unbounded
        // allocation cannot be sabotage-verified safely — the u32-overflow half
        // of this guard is proven below against `check_segment_budget` directly,
        // where nothing is allocated at all.
        deck.cards.push(Card::Gw(GwCard {
            segments: 100_000,
            ..one_segment_wire(1)
        }));
        let err = build_geometry(&deck).expect_err("100k segments must be refused");
        assert!(
            matches!(err, GeometryError::SegmentBudgetExceeded { card: "GW", .. }),
            "{err:?}"
        );
    }

    /// The GR form: the count multiplies the whole structure, so the projection
    /// must be computed in u64 — `originals * count` overflows u32 for exactly
    /// the input being guarded against.
    #[test]
    fn a_gr_card_cannot_replicate_past_the_cap() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(one_segment_wire(1)));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            occurrences: 100_001,
        }));
        let err = build_geometry(&deck).expect_err("100k copies must be refused");
        assert!(
            matches!(err, GeometryError::SegmentBudgetExceeded { card: "GR", .. }),
            "{err:?}"
        );
    }

    /// The reason the budget is a RUNNING total rather than a per-card limit.
    /// Each GM copy card doubles the structure, and every one of these cards is
    /// individually innocent — a per-card check passes all of them.
    #[test]
    fn gm_copy_cards_cannot_compose_their_way_past_the_cap() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(one_segment_wire(1)));
        // 2^15 = 32 768 segments if nothing is watching the total: past the cap,
        // but ~4 MB rather than a memory bomb when the guard is sabotaged away.
        for i in 0..15 {
            deck.cards
                .push(Card::Gm(make_gm(i + 1, 1, 0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0)));
        }
        let err = build_geometry(&deck).expect_err("15 doublings must be refused");
        assert!(
            matches!(err, GeometryError::SegmentBudgetExceeded { card: "GM", .. }),
            "{err:?}"
        );
    }

    /// The projection arithmetic must not wrap. `originals * count` and
    /// `have + adding` both overflow u32 for exactly the inputs this guard
    /// exists to catch, so a projection computed in the deck's own type would
    /// wrap into a small number and pass. Asserted directly against the helper,
    /// because reaching this through `build_geometry` would mean actually
    /// allocating the thing being refused.
    #[test]
    fn the_budget_projection_does_not_wrap_at_u32() {
        // 4e9 + 4e9 wraps to ~3.7e9 in u32 — still over the cap, so use the case
        // that wraps to a PASSING value: 2^32 exactly.
        let err = check_segment_budget(0, u64::from(u32::MAX) + 1, "GW")
            .expect_err("2^32 segments must be refused, not wrapped to 0");
        assert!(
            matches!(
                err,
                GeometryError::SegmentBudgetExceeded {
                    card: "GW",
                    projected: 4_294_967_296
                }
            ),
            "{err:?}"
        );
        // And a GR-shaped product that overflows u32: 100 000 * 100 000 = 1e10.
        let err = check_segment_budget(100_000, 100_000u64 * 100_000u64, "GR")
            .expect_err("1e10 segments must be refused");
        assert!(
            matches!(
                err,
                GeometryError::SegmentBudgetExceeded {
                    card: "GR",
                    projected: 10_000_100_000
                }
            ),
            "{err:?}"
        );
    }

    /// Negative control, and the reason the cap sits where it does: the largest
    /// deck in `corpus/` is 255 segments (`yagi-5elm-51seg.nec`). A cap that
    /// refused real decks would pass all three tests above.
    #[test]
    fn the_largest_corpus_deck_is_far_inside_the_cap() {
        let mut deck = NecDeck::new();
        // Five parallel elements 0.2 m apart, as a Yagi's are: five coincident
        // wires would meet at both ends, a degenerate deck no Yagi is (and one the
        // Hallén geometry now refuses as a junction, FND-251).
        for tag in 1..=5u32 {
            let y = 0.2 * f64::from(tag - 1);
            deck.cards.push(Card::Gw(GwCard {
                segments: 51,
                start: [0.0, y, 0.0],
                end: [1.0, y, 0.0],
                ..one_segment_wire(tag)
            }));
        }
        // The cap counts the deck's own segments; the solved mesh adds the free
        // ends' thirds (FND-227), four a wire.
        let own = build_deck_geometry(&deck).expect("its own segments").len();
        assert_eq!(own, 255);
        let segs = build_geometry(&deck).expect("a 5-element yagi is an ordinary deck");
        assert_eq!(segs.len(), 255 + 5 * 4);
        assert!(
            own * 39 < MAX_SEGMENTS,
            "the cap must clear the largest real fixture by a wide margin, \
             or it is a limit on antennas rather than on allocations"
        );
    }

    // -------------------------------------------------------------------------
    // FND-113 — tag arithmetic must not leave the u32 tag space
    // -------------------------------------------------------------------------

    /// A GM tag increment that overflows u32 is refused, not panicked on.
    ///
    /// Unchecked this was `attempt to add with overflow` in debug and a wrapped
    /// tag in release. In `fnec worker --stdio` the panic took the process down
    /// with no result line at all, which a controller cannot distinguish from a
    /// dead worker — so the deck-driven failure looked like an infrastructure
    /// one.
    #[test]
    fn gm_tag_increment_overflow_is_refused_not_panicked() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 7,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gm(make_gm(
            u32::MAX,
            0,
            0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        )));
        let err = build_geometry(&deck).expect_err("tag 7 + increment u32::MAX overflows");
        assert!(
            matches!(err, GeometryError::TagOverflow { card: "GM", .. }),
            "{err:?}"
        );
    }

    /// The GR overflow is in the MULTIPLY, one copy before the add, so a test
    /// that only exercises the addition would miss it.
    #[test]
    fn gr_tag_increment_multiply_overflow_is_refused_not_panicked() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }));
        // increment * copy_idx overflows on the second copy: 3e9 * 2 > u32::MAX.
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 3_000_000_000,
            occurrences: 3,
        }));
        let err = build_geometry(&deck).expect_err("3e9 * 2 overflows u32");
        assert!(
            matches!(err, GeometryError::TagOverflow { card: "GR", .. }),
            "{err:?}"
        );
    }

    /// A GR copy of a tag-0 segment keeps tag 0, as nec2c's rotation branch keeps
    /// it and as GM always did; GR alone retagged it, before `copy_tag` made the
    /// three copying cards share one rule. A tagged wire's copy is still retagged.
    #[test]
    fn a_gr_copy_of_tag_zero_keeps_tag_zero() {
        let mut deck = NecDeck::new();
        for (tag, x) in [(0, 1.0), (7, 3.0)] {
            deck.cards.push(Card::Gw(GwCard {
                tag,
                segments: 1,
                start: [x, 0.0, -1.0],
                end: [x, 0.0, 1.0],
                radius: 0.001,
            }));
        }
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 5,
            occurrences: 2,
        }));
        let segs = build_geometry(&deck).expect("builds");
        let tags: Vec<u32> = segs
            .iter()
            .filter(|s| s.is_deck_segment())
            .map(|s| s.tag)
            .collect();
        assert_eq!(tags, vec![0, 7, 0, 12], "{tags:?}");
    }

    /// Negative control: the ordinary GR replication a real deck uses still
    /// works. Without this, a fix that refused every GR card would pass the two
    /// tests above.
    #[test]
    fn ordinary_gr_replication_is_unaffected() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            occurrences: 4,
        }));
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).expect("a 4-fold turnstile is an ordinary deck");
        assert_eq!(segs.len(), 4);
        assert_eq!(
            segs.iter().map(|s| s.tag).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }

    /// GM with NRPT>=1 creates a copy, tagged `tag + ITGI`.
    ///
    /// It is NRPT and not ITGI that decides copy-versus-in-place: this card was
    /// `make_gm(1, 0, ..)` until FND-119, which NEC-2 reads as an in-place move
    /// that retags, not a copy.
    #[test]
    fn gm_copy_increments_tag() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
            radius: 0.001,
        }));
        // Copy with tag_increment=1, translate +1 m along y
        deck.cards
            .push(Card::Gm(make_gm(1, 1, 0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0)));
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 2);
        // Original (tag=1) unchanged
        assert_eq!(segs[0].tag, 1);
        assert!((segs[0].start[1]).abs() < 1e-12);
        // Copy (tag=2) translated
        assert_eq!(segs[1].tag, 2);
        assert!((segs[1].start[1] - 1.0).abs() < 1e-12);
    }

    /// GR with count=3 and 90-degree steps produces 4 wires (original + 3 copies).
    #[test]
    fn gr_repeat_produces_correct_count() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [1.0, 0.0, 0.0],
            end: [2.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            occurrences: 4,
        }));
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        // 1 original + 3 copies = 4 segments (each wire has 1 segment)
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].tag, 1);
        assert_eq!(segs[1].tag, 2);
        assert_eq!(segs[2].tag, 3);
        assert_eq!(segs[3].tag, 4);
    }

    /// GR 1 4 (four occurrences, 90° apart) moves (1,0,0) to (0,1,0) first.
    #[test]
    fn gr_rotation_is_correct() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 1,
            start: [1.0, 0.0, 0.0],
            end: [2.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            occurrences: 4,
        }));
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 4, "GR 1 4: four occurrences in all");
        // The first copy is a quarter turn on: y=1..2, x≈0
        assert!((segs[1].start[0]).abs() < 1e-12, "x should be ~0");
        assert!((segs[1].start[1] - 1.0).abs() < 1e-12, "y should be ~1");
        assert!((segs[1].end[0]).abs() < 1e-12);
        assert!((segs[1].end[1] - 2.0).abs() < 1e-12);
    }

    /// global_index values are contiguous 0..N-1 after GR expansion.
    #[test]
    fn gr_global_indices_are_contiguous() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [1.0, 0.0, 0.0],
            end: [2.0, 0.0, 0.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gr(GrCard {
            tag_increment: 1,
            occurrences: 3,
        }));
        // The deck's expansion; the free-end refinement is tested separately.
        let segs = build_deck_geometry(&deck).unwrap();
        assert_eq!(segs.len(), 9); // 3 wires × 3 segments
        for (i, s) in segs.iter().enumerate() {
            assert_eq!(s.global_index, i);
        }
    }

    // -----------------------------------------------------------------------
    // Free-end refinement (FND-227)
    // -----------------------------------------------------------------------

    fn parsed(text: &str) -> NecDeck {
        nec_parser::parse(text).expect("parses").deck
    }

    /// GA / GH segment tables against nec2c 1.3.1's printed SEGMENTATION DATA
    /// (centre x, y, z and length, to its 1e-4 print), captured 2026-10-14. The
    /// arc's angle convention, the helix's defaults, taper, cone and handedness all
    /// show here — nec2c's impedance cannot see handedness (a circular helix reads
    /// the same for HL = ±0.5), so this table is the handedness test.
    #[test]
    fn ga_and_gh_segments_match_nec2cs_segmentation_table() {
        let cases: [(&str, &[[f64; 4]]); 7] = [
            (
                "GA 1 6 0.5 0 180 .001",
                &[
                    [0.4665, 0.0, 0.1250, 0.2588],
                    [0.3415, 0.0, 0.3415, 0.2588],
                    [0.1250, 0.0, 0.4665, 0.2588],
                    [-0.1250, 0.0, 0.4665, 0.2588],
                    [-0.3415, 0.0, 0.3415, 0.2588],
                    [-0.4665, 0.0, 0.1250, 0.2588],
                ],
            ),
            (
                "GA 1 8 0.5 0 360 .001",
                &[
                    [0.4268, 0.0, 0.1768, 0.3827],
                    [0.1768, 0.0, 0.4268, 0.3827],
                    [-0.1768, 0.0, 0.4268, 0.3827],
                    [-0.4268, 0.0, 0.1768, 0.3827],
                    [-0.4268, 0.0, -0.1768, 0.3827],
                    [-0.1768, 0.0, -0.4268, 0.3827],
                    [0.1768, 0.0, -0.4268, 0.3827],
                    [0.4268, 0.0, -0.1768, 0.3827],
                ],
            ),
            (
                "GA 1 4 0.5 90 -30 .001",
                &[
                    [0.1250, 0.0, 0.4665, 0.2588],
                    [0.3415, 0.0, 0.3415, 0.2588],
                    [0.4665, 0.0, 0.1250, 0.2588],
                    [0.4665, 0.0, -0.1250, 0.2588],
                ],
            ),
            (
                "GH 1 8 0.2 0.4 0.1 0.1 0.1 0.1 .0005",
                &[
                    [0.0500, 0.0500, 0.0250, 0.1500],
                    [-0.0500, 0.0500, 0.0750, 0.1500],
                    [-0.0500, -0.0500, 0.1250, 0.1500],
                    [0.0500, -0.0500, 0.1750, 0.1500],
                    [0.0500, 0.0500, 0.2250, 0.1500],
                    [-0.0500, 0.0500, 0.2750, 0.1500],
                    [-0.0500, -0.0500, 0.3250, 0.1500],
                    [0.0500, -0.0500, 0.3750, 0.1500],
                ],
            ),
            (
                "GH 1 8 0.2 0.4 0.1 0.1 0.05 0.05 .0005",
                &[
                    [0.0500, 0.0469, 0.0250, 0.1459],
                    [-0.0438, 0.0469, 0.0750, 0.1376],
                    [-0.0437, -0.0406, 0.1250, 0.1295],
                    [0.0375, -0.0406, 0.1750, 0.1214],
                    [0.0375, 0.0344, 0.2250, 0.1134],
                    [-0.0313, 0.0344, 0.2750, 0.1055],
                    [-0.0312, -0.0281, 0.3250, 0.0978],
                    [0.0250, -0.0281, 0.3750, 0.0904],
                ],
            ),
            (
                "GH 1 8 0.2 -0.4 0.1 0.15 0.1 0.15 .0005",
                &[
                    [0.0750, 0.0500, 0.0250, 0.1871],
                    [0.0750, -0.0500, 0.0750, 0.1871],
                    [-0.0750, -0.0500, 0.1250, 0.1871],
                    [-0.0750, 0.0500, 0.1750, 0.1871],
                    [0.0750, 0.0500, 0.2250, 0.1871],
                    [0.0750, -0.0500, 0.2750, 0.1871],
                    [-0.0750, -0.0500, 0.3250, 0.1871],
                    [-0.0750, 0.0500, 0.3750, 0.1871],
                ],
            ),
            (
                "GH 1 8 0.2 0.4 0 0 0.1 0.1 .0005",
                &[
                    [0.0, 0.0063, 0.0250, 0.0515],
                    [-0.0125, 0.0062, 0.0750, 0.0573],
                    [-0.0125, -0.0188, 0.1250, 0.0673],
                    [0.0250, -0.0187, 0.1750, 0.0800],
                    [0.0250, 0.0313, 0.2250, 0.0944],
                    [-0.0375, 0.0312, 0.2750, 0.1097],
                    [-0.0375, -0.0438, 0.3250, 0.1256],
                    [0.0500, -0.0437, 0.3750, 0.1420],
                ],
            ),
        ];
        // B1 = 0 in a uniform helix reads as A1 (nec2c's default): the same table
        // as B1 = A1, which nec2c prints for it too.
        let defaulted = ("GH 1 8 0.2 0.4 0.1 0 0.1 0 .0005", cases[3].1);
        for (card, table) in cases.into_iter().chain([defaulted]) {
            let segs =
                build_deck_geometry(&parsed(&format!("{card}\nGE 0\nEN\n"))).expect("builds");
            assert_eq!(segs.len(), table.len(), "{card}");
            for (i, (s, row)) in segs.iter().zip(table).enumerate() {
                let got = [s.midpoint[0], s.midpoint[1], s.midpoint[2], s.length];
                for k in 0..4 {
                    assert!(
                        (got[k] - row[k]).abs() <= 5.1e-5,
                        "{card} segment {}: {got:?} vs nec2c {row:?}",
                        i + 1
                    );
                }
                // Each segment starts where the last ended, exactly (shared ends).
                if i > 0 {
                    assert_eq!(s.start, segs[i - 1].end, "{card} segment {}", i + 1);
                }
            }
        }
    }

    /// A `GC` taper as nec2c builds it: its segmentation table for a slanted
    /// wire tapered by length (RDEL 1.2) and radius (0.1 → 0.3, large so its
    /// 4-decimal radius column resolves the steps), nec2c 1.3.1, 2026-10-15. Ends
    /// shared, the last at P2 exactly.
    #[test]
    fn a_gc_taper_matches_nec2cs_segmentation_table() {
        let nec2c: [[f64; 5]; 5] = [
            [0.1202, -0.1664, -0.8320, 0.3450, 0.1000],
            [0.1645, -0.0925, -0.4625, 0.4140, 0.1316],
            [0.2177, -0.0038, -0.0190, 0.4968, 0.1732],
            [0.2816, 0.1026, 0.5131, 0.5961, 0.2280],
            [0.3582, 0.2303, 1.1517, 0.7153, 0.3000],
        ];
        let deck = "GW 3 5 0.1 -0.2 -1 0.4 0.3 1.5 0\nGC 0 0 1.2 .1 .3\nGE 0\nEN\n";
        let segs = build_deck_geometry(&parsed(deck)).expect("builds");
        assert_eq!(segs.len(), 5);
        for (i, (s, row)) in segs.iter().zip(&nec2c).enumerate() {
            let got = [
                s.midpoint[0],
                s.midpoint[1],
                s.midpoint[2],
                s.length,
                s.radius,
            ];
            for k in 0..5 {
                assert!(
                    (got[k] - row[k]).abs() <= 5.1e-5,
                    "segment {}: {got:?} vs nec2c {row:?}",
                    i + 1
                );
            }
            assert_eq!(s.tag, 3);
            if i > 0 {
                assert_eq!(s.start, segs[i - 1].end, "segment {}", i + 1);
            }
        }
        assert_eq!(segs[4].end, [0.4, 0.3, 1.5]);
        // RDEL 1 with equal radii is the plain GW, to round-off.
        let tapered = build_deck_geometry(&parsed(
            "GW 1 7 0 0 -1 0.3 0 1 0\nGC 0 0 1 .002 .002\nGE 0\nEN\n",
        ))
        .expect("builds");
        let plain =
            build_deck_geometry(&parsed("GW 1 7 0 0 -1 0.3 0 1 .002\nGE 0\nEN\n")).expect("builds");
        for (t, p) in tapered.iter().zip(&plain) {
            for k in 0..3 {
                assert!(
                    (t.midpoint[k] - p.midpoint[k]).abs() < 1e-15,
                    "{t:?} vs {p:?}"
                );
            }
            assert_eq!(t.radius, p.radius);
        }
    }

    /// The pairing is the builder's rule as well as the parser's: a deck built
    /// in code with a radius-0 GW and no GC, or a GC and no GW, is refused, not
    /// built with the wire vanished or the taper given to another card.
    #[test]
    fn the_builder_refuses_a_broken_taper_pairing() {
        let mut deck = parsed("GW 1 5 0 0 -1 0 0 1 .001\nGE 0\nEN\n");
        let Card::Gw(gw) = &mut deck.cards[0] else {
            unreachable!()
        };
        gw.radius = 0.0;
        assert!(matches!(
            build_deck_geometry(&deck),
            Err(GeometryError::TaperPairing(
                nec_model::card::TaperPairingError::WireWithoutTaper { index: 0, tag: 1 }
            ))
        ));
        let mut deck = parsed("GW 1 5 0 0 -1 0 0 1 .001\nGE 0\nEN\n");
        deck.cards.insert(
            1,
            Card::Gc(nec_model::card::GcCard {
                length_ratio: 1.0,
                radius_start: 0.001,
                radius_end: 0.002,
            }),
        );
        assert!(matches!(
            build_geometry(&deck),
            Err(GeometryError::TaperPairing(
                nec_model::card::TaperPairingError::TaperWithoutWire { index: 1 }
            ))
        ));
    }

    /// A taper may not make a segment shorter than two radii (the thin-wire
    /// limit: nec2c and fnec read RDEL 0.6 19 Ω apart in X, fed far from its end) or
    /// three junction tolerances — judged on the final structure, so a taper
    /// drawn small and scaled up by GS builds. Each refusal beside one that builds.
    #[test]
    fn a_taper_too_short_for_the_kernel_or_the_junction_tolerance_is_refused() {
        let build = |cards: &str| build_deck_geometry(&parsed(&format!("{cards}\nGE 0\nEN\n")));
        // Refused by the bound whose words are `says`.
        let short = |r: Result<Vec<Segment>, GeometryError>, says: &str| match r {
            Err(e @ GeometryError::TaperTooShort { .. }) => e.to_string().contains(says),
            _ => false,
        };
        // Δ/a alone: RDEL 0.6 on 21 segments of a 5 m wire ends at 73 µm, over the
        // absolute bound and 0.07 radii; 0.8 ends at 12 mm.
        assert!(short(
            build("GW 1 21 0 0 -2.5 0 0 2.5 0\nGC 0 0 0.6 .001 .001"),
            "2 radii"
        ));
        assert!(build("GW 1 21 0 0 -2.5 0 0 2.5 0\nGC 0 0 0.8 .001 .001").is_ok());
        // Absolute: 2.5 µm segments of a 1 µm wire pass Δ/a but not 3 µm.
        assert!(short(
            build("GW 1 3 0 0 0 0 0 7.5e-6 0\nGC 0 0 1 1e-6 1e-6"),
            "three junction tolerances"
        ));
        assert!(build("GW 1 3 0 0 0 0 0 1.2e-5 0\nGC 0 0 1 1e-6 1e-6").is_ok());
        assert!(build("GW 1 3 0 0 0 0 0 7.5e-6 0\nGC 0 0 1 1e-6 1e-6\nGS 0 0 1000").is_ok());
    }

    /// The Hallén geometry refuses GA/GH; the deck geometry (the MPIE's, and the
    /// viewport's) builds them. The text names no remedy: each frontend joins its
    /// own on, and only where the MPIE takes the deck — not this one, which has
    /// no feed (FND-271).
    #[test]
    fn the_hallen_geometry_refuses_arcs_and_helices() {
        for card in [
            "GA 1 10 0.5 0 180 .001",
            "GH 1 8 0.2 0.4 0.1 0.1 0.1 0.1 .0005",
        ] {
            let deck = parsed(&format!("{card}\nGE 0\nEN\n"));
            let e = build_geometry(&deck).expect_err(card);
            assert!(
                matches!(e, GeometryError::CurvedForHallen { .. }),
                "{card}: {e:?}"
            );
            assert!(!e.to_string().contains("--solver"), "{e}");
            assert!(!e.mpie_solves_it(), "{card}: no feed, so no MPIE solve");
            assert_eq!(e.with_mpie_remedy("REMEDY"), e.to_string());
            assert!(build_deck_geometry(&deck).is_ok(), "{card}");
            let fed = |seg: u32| {
                parsed(&format!(
                    "{card}\nGE 0\nEX 0 1 {seg} 0 1 0\nFR 0 1 0 0 30 0\nEN\n"
                ))
            };
            let e = build_geometry(&fed(2)).expect_err(card);
            assert!(e.mpie_solves_it(), "{card}");
            assert_eq!(e.with_mpie_remedy("REMEDY").matches("REMEDY").count(), 1);
            // Fed at its end the MPIE refuses it too (FND-228): no remedy (FND-276).
            let e = build_geometry(&fed(1)).expect_err(card);
            assert!(!e.mpie_solves_it(), "{card}: an end feed");
            assert_eq!(e.with_mpie_remedy("REMEDY"), e.to_string());
        }
        assert!(build_geometry(&parsed("GW 1 3 0 0 0 0 0 1 .001\nGE 0\nEN\n")).is_ok());
    }

    /// Exactly the two tips of a dipole are free; inside the wire nothing is.
    #[test]
    fn a_dipole_has_two_free_end_segments() {
        let segs = build_deck_geometry(&parsed(
            "CE\nGW 1 11 0 0 -2.5 0 0 2.5 .001\nGE 0\nEX 0 1 6 0 1 0\nFR 0 1 0 0 30 0\nEN\n",
        ))
        .unwrap();
        let free = free_end_segments(&segs, false);
        let marked: Vec<usize> = (0..segs.len()).filter(|&i| free[i]).collect();
        assert_eq!(marked, vec![0, 10]);
    }

    /// At a T the tips are free and the segments at the node are not; an end on
    /// the ground plane is joined to its image, so it is not free over a ground
    /// and is in free space.
    #[test]
    fn junctions_and_grounded_ends_are_not_free() {
        let tee = build_deck_geometry(&parsed(
            "CE\nGW 1 5 0 0 1 0 0 3 .001\nGW 2 5 0 0 3 2 0 3 .001\nGW 3 5 0 0 3 -2 0 3 .001\n\
             GE 0\nEX 0 1 2 0 1 0\nFR 0 1 0 0 30 0\nEN\n",
        ))
        .unwrap();
        let free = free_end_segments(&tee, false);
        let marked: Vec<usize> = (0..tee.len()).filter(|&i| free[i]).collect();
        // The stem's foot, and each arm's tip; nothing at the node.
        assert_eq!(marked, vec![0, 9, 14]);

        let mono = build_deck_geometry(&parsed(
            "CE\nGW 1 5 0 0 0 0 0 2.5 .001\nGE 1\nGN 1\nEX 0 1 1 0 1 0\nFR 0 1 0 0 30 0\nEN\n",
        ))
        .unwrap();
        assert_eq!(
            free_end_segments(&mono, true),
            [false, false, false, false, true]
        );
        assert_eq!(
            free_end_segments(&mono, false),
            [true, false, false, false, true]
        );
    }

    /// A touch in the middle of a segment is no junction, as in NEC: that end
    /// stays free.
    #[test]
    fn an_end_touching_a_segment_middle_is_free() {
        let segs = build_deck_geometry(&parsed(
            "CE\nGW 1 3 -1 0 0 1 0 0 .001\nGW 2 4 0 0 0 0 0 2 .001\nGE 0\n\
             EX 0 1 2 0 1 0\nFR 0 1 0 0 30 0\nEN\n",
        ))
        .unwrap();
        // GW 2 starts at the midpoint of GW 1's middle segment.
        assert!(free_end_segments(&segs, false)[3]);
    }

    /// The deck's numbering survives: deck segments keep their numbers in
    /// order, flanks are 0 and name their deck segment, the centre's midpoint is
    /// the deck segment's exactly, and each third is a third.
    #[test]
    fn refinement_keeps_the_decks_numbering_and_midpoints() {
        let text =
            "CE\nGW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 30 0\nEN\n";
        let deck_segs = build_deck_geometry(&parsed(text)).unwrap();
        let segs = build_geometry(&parsed(text)).unwrap();
        assert_eq!(segs.len(), 25);
        let numbers: Vec<u32> = segs
            .iter()
            .filter(|s| s.is_deck_segment())
            .map(|s| s.tag_index)
            .collect();
        assert_eq!(numbers, (1..=21).collect::<Vec<_>>());
        let flanks: Vec<(usize, SegmentPart)> = segs
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_deck_segment())
            .map(|(i, s)| (i, s.part))
            .collect();
        assert_eq!(
            flanks,
            vec![
                (0, SegmentPart::Flank { parent: 1 }),
                (2, SegmentPart::Flank { parent: 1 }),
                (22, SegmentPart::Flank { parent: 21 }),
                (24, SegmentPart::Flank { parent: 21 }),
            ]
        );
        let flank_numbers: Vec<u32> = segs
            .iter()
            .filter(|s| !s.is_deck_segment())
            .map(|s| s.tag_index)
            .collect();
        assert_eq!(flank_numbers, [0, 0, 0, 0]);
        for (i, s) in segs.iter().enumerate() {
            assert_eq!(s.global_index, i);
        }
        let (c, d) = (&segs[1], &deck_segs[0]);
        assert_eq!(c.part, SegmentPart::Centre);
        assert_eq!(
            c.midpoint, d.midpoint,
            "the centre stands at the deck segment's midpoint"
        );
        assert!((c.length - d.length / 3.0).abs() < 1e-15);
        assert!((c.deck_length() - d.length).abs() < 1e-12);
        assert_eq!(segs[0].start, d.start);
        assert_eq!(segs[2].end, d.end);
        // A reference resolves to the centre; segment 0 names nothing.
        assert_eq!(find_deck_segment(&segs, 1, 1), Some(1));
        assert_eq!(find_deck_segment(&segs, 1, 21), Some(23));
        assert_eq!(find_deck_segment(&segs, 1, 0), None);
        // The absolute numbering skips the flanks too (FND-282).
        let absolute: Vec<u32> = segs.iter().map(|s| s.abs_index).collect();
        assert_eq!(
            absolute,
            [vec![0, 1, 0], (2..=20).collect(), vec![0, 21, 0]].concat()
        );
        assert_eq!(find_deck_segment(&segs, 0, 1), Some(1));
        assert_eq!(find_deck_segment(&segs, 0, 21), Some(23));
        assert_eq!(find_deck_segment(&segs, 0, 0), None);
        // Values at the deck's segments come back in the deck's order.
        let idx: Vec<usize> = (0..segs.len()).collect();
        assert_eq!(
            deck_values(&segs, &idx),
            [vec![1], (3..=21).collect(), vec![23]].concat()
        );
    }

    /// A one-segment wire with two free ends is split once, not twice.
    #[test]
    fn a_one_segment_wire_is_split_once() {
        let segs = build_geometry(&parsed(
            "CE\nGW 1 1 0 0 -0.5 0 0 0.5 .001\nGE 0\nEX 0 1 1 0 1 0\nFR 0 1 0 0 30 0\nEN\n",
        ))
        .unwrap();
        let parts: Vec<SegmentPart> = segs.iter().map(|s| s.part).collect();
        assert_eq!(
            parts,
            [
                SegmentPart::Flank { parent: 1 },
                SegmentPart::Centre,
                SegmentPart::Flank { parent: 1 }
            ]
        );
    }

    /// Refinement runs on the final structure: a GM copy's free ends are refined
    /// too, and its numbering is the copy's.
    #[test]
    fn a_gm_copy_is_refined_as_the_original() {
        let segs = build_geometry(&parsed(
            "CE\nGW 1 5 0 0 -1 0 0 1 .001\nGM 1 1 0 0 0 2 0 0 0\nGE 0\nEX 0 1 3 0 1 0\n\
             FR 0 1 0 0 30 0\nEN\n",
        ))
        .unwrap();
        assert_eq!(segs.len(), 18, "two 5-segment wires, four free ends");
        for tag in [1, 2] {
            let numbers: Vec<u32> = segs
                .iter()
                .filter(|s| s.tag == tag && s.is_deck_segment())
                .map(|s| s.tag_index)
                .collect();
            assert_eq!(numbers, vec![1, 2, 3, 4, 5], "tag {tag}");
        }
    }

    // -------------------------------------------------------------------------
    // FND-233 — the chain order the plain merged basis solves in
    // -------------------------------------------------------------------------

    /// Three collinear wires along z (tags 1..3 bottom to top), written in `order`.
    fn chain_in_order(order: [usize; 3]) -> Vec<Segment> {
        let wires = [(1, 4, -1.0, 0.0), (2, 3, 0.0, 0.5), (3, 5, 0.5, 1.5)];
        let mut deck = NecDeck::new();
        for i in order {
            let (tag, segments, z0, z1) = wires[i];
            deck.cards.push(Card::Gw(GwCard {
                tag,
                segments,
                start: [0.0, 0.0, z0],
                end: [0.0, 0.0, z1],
                radius: 0.001,
            }));
        }
        build_deck_geometry(&deck).unwrap()
    }

    #[test]
    fn segment_order_is_the_identity_on_a_chain_already_in_order() {
        assert!(SegmentOrder::chains(&chain_in_order([0, 1, 2])).is_identity());
    }

    #[test]
    fn segment_order_walks_a_scrambled_chain_bottom_to_top_and_restores_it() {
        for order in [[1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            let segs = chain_in_order(order);
            let so = SegmentOrder::chains(&segs);
            assert!(!so.is_identity(), "{order:?}");
            let solved = so.segments(&segs);
            // Contiguous and forward: what the merge welds into one conductor.
            assert_eq!(
                merge_collinear_wire_endpoints(&solved),
                vec![(0, 11)],
                "{order:?}"
            );
            let positions: Vec<usize> = solved.iter().map(|s| s.global_index).collect();
            assert_eq!(positions, (0..12).collect::<Vec<_>>(), "{order:?}");
            // A per-segment vector survives the round trip, element for element.
            let tagged: Vec<(u32, u32)> = segs.iter().map(|s| (s.tag, s.tag_index)).collect();
            assert_eq!(so.restore(&so.apply(&tagged)), tagged, "{order:?}");
        }
    }

    #[test]
    fn segment_order_leaves_a_junction_in_deck_order() {
        let mut deck = deck_with_gw(1, 5, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.001);
        for (tag, x) in [(2, -1.0), (3, 1.0)] {
            deck.cards.push(Card::Gw(GwCard {
                tag,
                segments: 3,
                start: [0.0, 0.0, 1.0],
                end: [x, 0.0, 1.0],
                radius: 0.001,
            }));
        }
        let segs = build_deck_geometry(&deck).unwrap();
        assert!(SegmentOrder::chains(&segs).is_identity());
    }

    /// FND-249: a chain with a wire written backwards (a start-to-start split) is
    /// one forward conductor once that wire is flipped, and only it: the chain is
    /// walked so its lowest deck segment runs forward. A flip negates what is
    /// measured along the segment, and nothing else.
    #[test]
    fn segment_order_flips_a_reversed_wire_and_only_it() {
        let mut deck = NecDeck::new();
        for (tag, segments, z0, z1) in [(1, 4, -1.0, 0.0), (2, 3, 0.5, 0.0), (3, 5, 0.5, 1.5)] {
            deck.cards.push(Card::Gw(GwCard {
                tag,
                segments,
                start: [0.0, 0.0, z0],
                end: [0.0, 0.0, z1],
                radius: 0.001,
            }));
        }
        let segs = build_deck_geometry(&deck).unwrap();
        let so = SegmentOrder::chains(&segs);
        let solved = so.segments(&segs);
        assert_eq!(merge_collinear_wire_endpoints(&solved), vec![(0, 11)]);
        assert!(solved.iter().all(|s| s.direction == [0.0, 0.0, 1.0]));
        let flipped: Vec<u32> = (0..12)
            .filter(|&i| so.is_flipped(i))
            .map(|i| solved[i].tag)
            .collect();
        assert_eq!(flipped, vec![2, 2, 2], "exactly the reversed wire");
        for (s, d) in solved.iter().zip(so.apply(&segs)) {
            assert_eq!((s.midpoint, s.length), (d.midpoint, d.length));
        }
        // Signed quantities come back in their own sense; unsigned ones as they were.
        let v: Vec<num_complex::Complex64> = (0..12)
            .map(|i| num_complex::Complex64::new(i as f64 + 1.0, 0.5))
            .collect();
        assert_eq!(so.restore_currents(&so.apply_signed(&v)), v);
        let in_solve = so.apply_signed(&v);
        for (i, (s, d)) in in_solve.iter().zip(so.apply(&v)).enumerate() {
            assert_eq!(*s, if so.is_flipped(i) { -d } else { d });
        }

        // The `GX` shape: a half and its mirror meeting start-to-start. The walk
        // starts at the first wire's far end, so it crosses that wire backwards;
        // the orientation rule turns the walk round, and the mirror is flipped.
        let mut deck = NecDeck::new();
        for (tag, z1) in [(1, 1.0), (2, -1.0)] {
            deck.cards.push(Card::Gw(GwCard {
                tag,
                segments: 3,
                start: [0.0, 0.0, 0.0],
                end: [0.0, 0.0, z1],
                radius: 0.001,
            }));
        }
        let segs = build_deck_geometry(&deck).unwrap();
        let so = SegmentOrder::chains(&segs);
        let solved = so.segments(&segs);
        let flipped: Vec<u32> = (0..6)
            .filter(|&i| so.is_flipped(i))
            .map(|i| solved[i].tag)
            .collect();
        assert_eq!(
            flipped,
            vec![2, 2, 2],
            "the mirror, not the first-written half"
        );
    }

    // -----------------------------------------------------------------------
    // snap_junctions (FND-246). The answers against nec2c are in
    // apps/nec-cli/tests/junction_band_nec2c.rs; these hold what an impedance at
    // 1e-4 cannot see.

    /// The T with its stem top 50 µm short of the bar's joint at y = 0.
    const NEAR_MISS_TEE: &str = "CE\nGW 1 24 0 -3 4 0 3 4 .001\nGW 2 16 0 0 0 0 0 3.99995 .001\n\
                                 GE 0\nEX 0 2 8 0 1 0\nFR 0 1 0 0 7.0 0\nEN\n";

    /// A moved segment is re-derived from its new ends: a stale midpoint is 25 µm
    /// off on a 0.25 m segment, about 1e-5 in the impedance, under any Z gate.
    #[test]
    fn a_snapped_segment_is_rederived_from_its_ends() {
        let segs = build_deck_geometry(&parsed(NEAR_MISS_TEE)).unwrap();
        let top = segs.iter().rfind(|s| s.tag == 2).unwrap();
        assert_eq!(
            top.end,
            [0.0, 0.0, 4.0],
            "the stem top moves onto the bar's joint"
        );
        for s in &segs {
            let d = [
                s.end[0] - s.start[0],
                s.end[1] - s.start[1],
                s.end[2] - s.start[2],
            ];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            assert!((s.length - len).abs() < 1e-15, "{s:?}");
            for (k, dk) in d.iter().enumerate() {
                assert!(
                    (s.midpoint[k] - 0.5 * (s.start[k] + s.end[k])).abs() < 1e-15,
                    "{s:?}"
                );
                assert!((s.direction[k] * len - dk).abs() < 1e-14, "{s:?}");
            }
        }
    }

    /// An exactly drawn deck comes out bitwise as its cards expand: the snap moves
    /// nothing that already meets (a centroid or "latest member" anchor would).
    #[test]
    fn an_exactly_drawn_junction_is_not_moved() {
        let deck = parsed(&NEAR_MISS_TEE.replace("3.99995", "4"));
        let mut expanded = Vec::new();
        for c in &deck.cards {
            if let Card::Gw(gw) = c {
                expand_wire(gw, &mut expanded).unwrap();
            }
        }
        assert_eq!(build_unnumbered(&deck).unwrap(), expanded);
    }

    /// The group moves onto its earliest end: the bar (tag 1) stays where its
    /// card put it, and the stem's end goes to it.
    #[test]
    fn the_earliest_end_anchors_the_group() {
        let segs = build_deck_geometry(&parsed(NEAR_MISS_TEE)).unwrap();
        let mut bar = Vec::new();
        expand_wire(
            &GwCard {
                tag: 1,
                segments: 24,
                start: [0.0, -3.0, 4.0],
                end: [0.0, 3.0, 4.0],
                radius: 0.001,
            },
            &mut bar,
        )
        .unwrap();
        for (a, b) in segs.iter().filter(|s| s.tag == 1).zip(&bar) {
            assert_eq!((a.start, a.end), (b.start, b.end));
        }
    }

    /// Two ends within the band on either side of a grid-cell boundary — at
    /// x = 1 cell and across x = 0, where the floor changes sign — still join: the
    /// search looks in the neighbouring cells. 1 m segments, so the cell is 1 mm.
    #[test]
    fn ends_straddling_a_grid_cell_boundary_join() {
        for (xa, xb) in [(0.99e-3, 1.01e-3), (-10e-6, 10e-6)] {
            let deck = parsed(&format!(
                "CE\nGW 1 1 {xa} 0 -1 {xa} 0 0 .001\nGW 2 1 {xb} 0 0 {xb} 0 1 .001\nGE 0\n\
                 EX 0 1 1 0 1 0\nFR 0 1 0 0 100 0\nEN\n"
            ));
            let segs = build_unnumbered(&deck).unwrap();
            assert_eq!(segs[1].start, segs[0].end, "{xa} / {xb}");
        }
    }

    // -----------------------------------------------------------------------
    // FND-250: overlapping segments, refused at the builder every solver uses.

    /// A dipole (11 × 0.48 m along z, 1 mm) and `second`, a card or two.
    fn with_dipole(second: &str) -> NecDeck {
        parsed(&format!(
            "CE\nGW 1 11 0 0 -2.64 0 0 2.64 .001\n{second}\nGE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n"
        ))
    }

    fn overlap_of(deck: &NecDeck) -> Option<(u32, u32, u32, u32, bool)> {
        // Both builders: the Hallén geometry and the MPIE's deck geometry.
        let (h, m) = (build_geometry(deck), build_deck_geometry(deck));
        match (h, m) {
            (
                Err(GeometryError::OverlappingSegments {
                    a, b, duplicate, ..
                }),
                Err(GeometryError::OverlappingSegments { .. }),
            ) => Some((a.0, a.1, b.0, b.1, duplicate)),
            (Err(GeometryError::OverlappingSegments { .. }), other)
            | (other, Err(GeometryError::OverlappingSegments { .. })) => {
                panic!("one builder refused the overlap, the other gave {other:?}")
            }
            _ => None,
        }
    }

    #[test]
    fn segments_drawn_on_top_of_each_other_are_refused_by_name() {
        let cases = [
            (
                "the same wire twice",
                "GW 2 11 0 0 -2.64 0 0 2.64 .001",
                (1, 1, 2, 1, true),
            ),
            (
                "a wire along the upper half and beyond",
                "GW 2 6 0 0 0 0 0 3.5 .001",
                (1, 6, 2, 1, false),
            ),
            (
                "the same, 100 um off the axis",
                "GW 2 6 0.0001 0 0 0.0001 0 3.5 .001",
                (1, 6, 2, 1, false),
            ),
            (
                "the same, 0.6 mm off, inside the radius",
                "GW 2 6 0.0006 0 0 0.0006 0 3.5 .001",
                (1, 6, 2, 1, false),
            ),
            (
                "a copy tilted inside the radius",
                "GW 2 11 0 0 -2.64 0.00001 0 2.64 .001",
                (1, 1, 2, 1, true),
            ),
            (
                "a GM that moves nothing, same tag",
                "GM 0 1 0 0 0 0 0 0",
                (1, 1, 1, 12, true),
            ),
        ];
        for (name, second, want) in cases {
            assert_eq!(overlap_of(&with_dipole(second)), Some(want), "{name}");
        }
        // A GX reflecting a wire that crosses its plane onto itself, reversed.
        let gx = parsed("CE\nGW 1 10 0 0 -2.64 0 0 2.64 .001\nGX 2 001\nGE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n");
        assert!(
            overlap_of(&gx).is_some_and(|o| o.4),
            "the GX copy is a duplicate"
        );
        // A GR whose wire lies on the rotation axis: every copy on top of it.
        let gr = parsed("CE\nGW 1 10 0 0 0.1 0 0 5.1 .001\nGW 2 5 0 0 0.1 2 0 0.1 .001\nGR 0 4\nGE 0\nEX 0 1 1 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n");
        assert!(overlap_of(&gr).is_some(), "a GR copy on the axis");
        let msg = build_geometry(&with_dipole("GW 2 6 0 0 0 0 0 3.5 .001"))
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("segment 6 of tag 1 and segment 1 of tag 2")
                && msg.contains("end the second wire where the first ends"),
            "{msg}"
        );
    }

    #[test]
    fn wires_that_only_meet_or_run_beside_each_other_are_not_overlaps() {
        let cases = [
            (
                "a parallel wire 10 mm away",
                "GW 2 11 0.01 0 -2.64 0.01 0 2.64 .001",
            ),
            // Surfaces touching or just clear: a thin-wire validity question, not
            // this refusal. Pinned either side of the two radii, not on them.
            (
                "axes 2.001 mm apart",
                "GW 2 11 0.002001 0 -2.64 0.002001 0 2.64 .001",
            ),
            ("a T onto the dipole's interior", "GW 2 5 0 0 0 2 0 0 .001"),
            (
                "a crossing at right angles",
                "GW 2 5 -1 0 0.24 1 0 0.24 .001",
            ),
            (
                "a wire carrying on from the top",
                "GW 2 5 0 0 2.64 0 0 4 .001",
            ),
            // Near the axis at one end of the overlap only: it leaves the
            // dipole's interior at 30°, unconnected, as in nec2c.
            (
                "a wire leaving the interior at 30°",
                "GW 2 5 0 0 0 1 0 1.732 .001",
            ),
        ];
        for (name, second) in cases {
            assert_eq!(overlap_of(&with_dipole(second)), None, "{name}");
        }
        assert!(
            overlap_of(&with_dipole(
                "GW 2 11 0.001999 0 -2.64 0.001999 0 2.64 .001"
            ))
            .is_some(),
            "axes 1.999 mm apart overlap"
        );
        // A second wire starting 0.1 mm back inside the first's end: within the
        // junction band, so the snap joins them end to end first (FND-246).
        let touch = parsed("CE\nGW 1 5 0 0 -2.64 0 0 0 .001\nGW 2 5 0 0 -0.0001 0 0 2.64 .001\nGE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 28.4 0\nEN\n");
        assert_eq!(overlap_of(&touch), None);
    }

    /// Tag 0 names an absolute segment number, as nec2c's `isegno` reads it
    /// (FND-282): on `GW 1 21` + `GW 2 21`, `(0, 32)` is tag 2's 11th; with a wire
    /// tagged 0 present it is still the 11th segment built, not the tag-0 wire's.
    /// The range resolver reads `LD`/`PT` selections the same way.
    #[test]
    fn tag_zero_names_an_absolute_segment() {
        let two =
            parsed("CE\nGW 1 21 0 0 -2.5 0 0 2.5 .001\nGW 2 21 1 0 -2.5 1 0 2.5 .001\nGE 0\nEN\n");
        let segs = build_deck_geometry(&two).unwrap();
        assert_eq!(
            find_deck_segment(&segs, 0, 32),
            find_deck_segment(&segs, 2, 11)
        );
        assert_eq!(find_deck_segment(&segs, 0, 43), None);
        let zero =
            parsed("CE\nGW 1 21 0 0 -2.5 0 0 2.5 .001\nGW 0 21 1 0 -2.5 1 0 2.5 .001\nGE 0\nEN\n");
        let segs0 = build_deck_geometry(&zero).unwrap();
        assert_eq!(find_deck_segment(&segs0, 0, 11), Some(10));
        assert_eq!(segs0[10].tag, 1, "absolute 11 is on the first wire");
        assert_eq!(find_deck_segment(&segs0, 0, 32), Some(31));
        assert_eq!(segs0[31].tag, 0);
        // Ranges: a tag's own numbering, or absolute with tag 0; `0 0 0` is all.
        let sel = |tag, first, last| -> Vec<u32> {
            segs.iter()
                .filter(|s| deck_segment_in_range(s, tag, first, last))
                .map(|s| s.abs_index)
                .collect()
        };
        assert_eq!(sel(2, 1, 3), [22, 23, 24]);
        assert_eq!(sel(2, 11, 0), [32]);
        assert_eq!(sel(2, 0, 5), (22..=42).collect::<Vec<_>>(), "last unread");
        assert_eq!(sel(0, 20, 24), [20, 21, 22, 23, 24]);
        assert_eq!(sel(0, 32, 0), [32]);
        assert_eq!(sel(0, 0, 0), (1..=42).collect::<Vec<_>>());
        // `0 0 k` reads as "all" here; `ld_card_problem` refuses the card.
        assert_eq!(sel(0, 0, 5), (1..=42).collect::<Vec<_>>());
        // A refined deck: flanks are never selected, the centre stands for the
        // deck segment, and the absolute range crosses the wires.
        let refined = build_geometry(&two).unwrap();
        let abs: Vec<u32> = refined
            .iter()
            .filter(|s| deck_segment_in_range(s, 0, 21, 22))
            .map(|s| s.abs_index)
            .collect();
        assert_eq!(abs, [21, 22]);
        assert!(refined
            .iter()
            .filter(|s| !s.is_deck_segment())
            .all(|f| !deck_segment_in_range(f, 0, 0, 0)));
    }

    /// The missing-reference message says what tag 0 means (FND-282).
    #[test]
    fn a_missing_tag_zero_reference_is_explained() {
        let segs =
            build_deck_geometry(&parsed("CE\nGW 1 21 0 0 -2.5 0 0 2.5 .001\nGE 0\nEN\n")).unwrap();
        let m = missing_reference("EX", 0, 32, &segs);
        assert!(
            m.contains("no segment numbered 32") && m.contains("the deck has 21"),
            "{m}"
        );
        let m = missing_reference("TL 0 0 2 11", 0, 0, &segs);
        assert!(m.contains("segment 0 names none"), "{m}");
        assert_eq!(
            missing_reference("EX", 3, 4, &segs),
            "EX: no segment with tag 3, index 4"
        );
    }
}
