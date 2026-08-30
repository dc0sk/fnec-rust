// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Excitation vector builder.
//!
//! Converts `EX` cards from the parsed deck into a complex right-hand-side
//! vector V, where V[i] is the impressed voltage on segment i (0 elsewhere).
//!
//! Only excitation type 0 (series voltage source) is implemented in Phase 1.

use num_complex::Complex64;

use nec_model::card::{Card, ExCard, FeedpointRole};
use nec_model::deck::NecDeck;

use crate::geometry::{merge_collinear_wire_endpoints, ConductorPath, Segment};

const C0: f64 = 299_792_458.0; // m/s
const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7; // H/m
const ETA0: f64 = MU0 * C0; // free-space wave impedance

/// Every `EX` card that names a feedpoint, in deck order, with what kind it is.
///
/// The single answer to "which `EX` is the feedpoint" (FND-031). Seven call sites
/// used to decide this for themselves with five different filters, and the
/// differences were load-bearing: the worker refused a type-5 deck it had already
/// solved, while the GUI and the Python bindings would report a plane wave's
/// NTHETA/NPHI as a feedpoint tag and segment.
///
/// Plane waves and unrecognised types are excluded — a plane wave has no
/// feedpoint at all, and an unknown type never reaches a reporting caller because
/// [`build_excitation`] rejects it first. What *is* included is deliberately both
/// classes of driven source, because callers differ on which they can price: the
/// CLI reports a current-source feedpoint as `Z = V_port / i0`, and a caller that
/// filtered on "voltage source" alone would silently delete a corpus-pinned row.
/// Callers match on the role rather than re-deriving it.
pub fn feedpoints(deck: &NecDeck) -> impl Iterator<Item = (&ExCard, FeedpointRole)> + '_ {
    deck.cards.iter().filter_map(|card| {
        let Card::Ex(ex) = card else { return None };
        match ex.kind().feedpoint_role() {
            role @ (FeedpointRole::DeltaGap | FeedpointRole::CurrentSource) => Some((ex, role)),
            FeedpointRole::PlaneWave | FeedpointRole::Unknown => None,
        }
    })
}

/// The first delta-gap feedpoint, for callers that report a single impedance and
/// cannot price a current source.
///
/// The worker, the GUI, the Python bindings and the CLI's distributed diagnostic
/// all want exactly this. Sharing it is what makes the diagnostic's tag/segment
/// agree with the impedance's *by construction* rather than by a comment asking
/// two files to be kept in step.
pub fn first_delta_gap_feedpoint(deck: &NecDeck) -> Option<&ExCard> {
    feedpoints(deck)
        .find(|(_, role)| *role == FeedpointRole::DeltaGap)
        .map(|(ex, _)| ex)
}

/// Right-hand side data for Hallén's integral equation.
#[derive(Debug)]
pub struct HallenRhs {
    /// Hallén RHS vector b.
    pub rhs: Vec<Complex64>,
    /// cos(k·s_m) samples for the homogeneous-term column.
    pub cos_vec: Vec<f64>,
}

/// Error from the excitation builder.
#[derive(Debug, Clone, PartialEq)]
pub enum ExcitationError {
    /// An EX card referenced a (tag, segment) pair not present in the geometry.
    SegmentNotFound { tag: u32, segment: u32 },
    /// An EX card uses an excitation type not yet supported.
    UnsupportedType {
        ex_type: u32,
        tag: u32,
        segment: u32,
        i4: u32,
    },
}

impl std::fmt::Display for ExcitationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExcitationError::SegmentNotFound { tag, segment } => {
                write!(f, "EX: no segment with tag {tag}, index {segment}")
            }
            ExcitationError::UnsupportedType {
                ex_type,
                tag,
                segment,
                i4,
            } => {
                // Name the NEC2 category so real 4nec2 decks get an accurate
                // diagnostic. The "is not yet supported" substring is a stable
                // contract asserted by the corpus and ex_cards tests.
                let kind = nec_model::card::ExcitationKind::from_type(*ex_type);
                write!(
                    f,
                    "EX: {} (I1={ex_type}, tag {tag}, segment {segment}, I4={i4}) is not yet supported",
                    kind.describe()
                )
            }
        }
    }
}

impl std::error::Error for ExcitationError {}

/// Build the complex excitation (RHS) vector V from `EX` cards in `deck`.
///
/// `segs` is the flat segment list produced by [`crate::geometry::build_geometry`].
/// The returned vector has length `segs.len()`.
pub fn build_excitation(
    deck: &NecDeck,
    segs: &[Segment],
) -> Result<Vec<Complex64>, ExcitationError> {
    let mut v = vec![Complex64::new(0.0, 0.0); segs.len()];

    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        apply_ex(ex, segs, &mut v)?;
    }

    Ok(v)
}

/// Scale a wire-voltage excitation vector for NEC-2 style pulse EFIE solves.
///
/// NEC-2 applies impressed voltage sources as E = -V/(dl*lambda) in the
/// matrix RHS for wire equations. `build_excitation()` provides the V/dl part;
/// this helper applies the 1/lambda factor.
pub fn scale_excitation_for_pulse_rhs(v: &[Complex64], freq_hz: f64) -> Vec<Complex64> {
    let lambda = C0 / freq_hz;
    v.iter().map(|vi| -*vi / lambda).collect()
}

/// Build Hallén RHS data (b and cos(k·s)) for the current geometry.
///
/// For the **driven wire** (the one containing the first type-0 EX source):
/// - `s` is measured along the wire's axis with s=0 at the feed segment midpoint.
/// - `b_m = -j * (2π/η₀) * V_source * sin(k * |s_m|)`
/// - `cos_vec[m] = cos(k * s_m)`
///
/// For **non-driven wires** (no EX source on the wire):
/// - They are coupled only through the Z-matrix; no incident driving field.
/// - `b_m = 0`
/// - `cos_vec[m] = cos(k * s_local_m)` where `s_local` is measured along
///   that wire's own axis with s=0 at the wire's midpoint.
///
/// This formulation supports non-collinear and junctioned multi-wire geometries.
/// The cos_vec column in the Hallén augmented system is shared across all
/// segments; per-wire C constants are handled in `solve_hallen` via the
/// `wire_endpoints` argument (one C column per wire).
pub fn build_hallen_rhs(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
) -> Result<HallenRhs, ExcitationError> {
    let mut first_ex: Option<&ExCard> = None;
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        // One classification, expressed once (FND-031). Plane waves and current
        // sources are not delta-gap sources — they have dedicated paths
        // (`crate::planewave`, `solve_hallen_current_source`) and contribute
        // nothing here — while an unrecognised type is a hard error, which is why
        // this loop cannot use the reporting seam: building an RHS needs an error
        // channel that naming a feedpoint does not.
        //
        // No wildcard arm: a new `ExcitationKind` must be decided here rather than
        // falling into whichever branch happened to be last.
        match ex.kind().feedpoint_role() {
            FeedpointRole::DeltaGap => {}
            FeedpointRole::PlaneWave | FeedpointRole::CurrentSource => continue,
            FeedpointRole::Unknown => {
                return Err(ExcitationError::UnsupportedType {
                    ex_type: ex.excitation_type,
                    tag: ex.tag,
                    segment: ex.segment,
                    i4: ex.i4,
                })
            }
        }
        if first_ex.is_none() {
            first_ex = Some(ex);
        }
    }

    // Only decides whether the deck has any delta gap at all; the sources
    // themselves are collected below.
    let Some(_ex) = first_ex else {
        return Ok(HallenRhs {
            rhs: vec![Complex64::new(0.0, 0.0); segs.len()],
            cos_vec: vec![0.0; segs.len()],
        });
    };

    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let scale = 2.0 * std::f64::consts::PI / ETA0;

    // Every delta-gap card is a source, in deck order.
    //
    // This was a `BTreeMap` keyed by wire tag, which can hold only ONE source per
    // wire — so a second gap on the same tag was dropped in silence, and the
    // `ex2.tag == driven_tag` guard dropped it before the map could even try. The
    // superposition below was already written for several sources; it was the
    // COLLECTION that deduplicated (FND-120).
    //
    // The conductor-path sibling, `build_hallen_rhs_paths`, iterates the cards
    // directly and has always superposed correctly. Measured on a start-to-start
    // split, which routes there: adding a second same-tag source changes the
    // CURRENTS block. On a straight wire, which routes here, it did not — the two
    // blocks were byte-identical, so the card contributed exactly nothing while
    // the report still printed a feedpoint row for it.
    //
    // A missing segment is now an error for EVERY card, not just the first. The
    // first card's was already a hard error and every card's is one in the path
    // sibling, so silently skipping later ones was the odd case out — and a
    // silently skipped card is the shape of FND-026.
    let mut sources: Vec<(usize, Complex64)> = Vec::new();
    for card in &deck.cards {
        let Card::Ex(ex2) = card else { continue };
        if ex2.kind().feedpoint_role() != FeedpointRole::DeltaGap {
            continue;
        }
        let idx = segs
            .iter()
            .position(|s| s.tag == ex2.tag && s.tag_index == ex2.segment)
            .ok_or(ExcitationError::SegmentNotFound {
                tag: ex2.tag,
                segment: ex2.segment,
            })?;
        sources.push((idx, Complex64::new(ex2.voltage_real, ex2.voltage_imag)));
    }

    // Collinear-connected `GW` wires form one logical conductor for the Hallén
    // homogeneous solution (PH9-CHK-002): the `cos(k·s)` coordinate and the source
    // term use the merged conductor's shared axis/origin, not each wire's own — so
    // a straight wire split across several `GW` cards is treated as one wire, as it
    // physically is. For geometry without collinear splits (single wires, parallel
    // arrays, bends, T/Y junctions) the merge is a no-op and this reproduces the
    // per-wire behaviour exactly.
    let components = merge_collinear_wire_endpoints(segs);
    let mut comp_of_seg = vec![0usize; segs.len()];
    for (ci, &(first, last)) in components.iter().enumerate() {
        for slot in comp_of_seg.iter_mut().take(last + 1).skip(first) {
            *slot = ci;
        }
    }

    let mut rhs = vec![Complex64::new(0.0, 0.0); segs.len()];
    let mut cos_vec = vec![0.0; segs.len()];

    // cos_vec: cos(k·s_local), with s_local measured from the merged conductor's
    // midpoint along its axis.
    for (m, seg) in segs.iter().enumerate() {
        let (first, last) = components[comp_of_seg[m]];
        let wire_dir = segs[first].direction;
        let wire_mid = [
            (segs[first].midpoint[0] + segs[last].midpoint[0]) / 2.0,
            (segs[first].midpoint[1] + segs[last].midpoint[1]) / 2.0,
            (segs[first].midpoint[2] + segs[last].midpoint[2]) / 2.0,
        ];
        let dl = [
            seg.midpoint[0] - wire_mid[0],
            seg.midpoint[1] - wire_mid[1],
            seg.midpoint[2] - wire_mid[2],
        ];
        let s_local = dl[0] * wire_dir[0] + dl[1] * wire_dir[1] + dl[2] * wire_dir[2];
        cos_vec[m] = (k * s_local).cos();
    }

    // rhs: each voltage source drives its whole merged conductor; `s` is measured
    // from the source segment along its axis. (Superposes for multi-source decks.)
    for &(fi, vsrc) in &sources {
        let (cf, cl) = components[comp_of_seg[fi]];
        let src_mid = segs[fi].midpoint;
        let src_dir = segs[fi].direction;
        for m in cf..=cl {
            let d = [
                segs[m].midpoint[0] - src_mid[0],
                segs[m].midpoint[1] - src_mid[1],
                segs[m].midpoint[2] - src_mid[2],
            ];
            let s = d[0] * src_dir[0] + d[1] * src_dir[1] + d[2] * src_dir[2];
            rhs[m] += Complex64::new(0.0, -scale * (k * s.abs()).sin()) * vsrc;
        }
    }

    Ok(HallenRhs { rhs, cos_vec })
}

/// Build Hallén RHS data over **conductor paths** — the general-junction delta-gap
/// path (PH9-CHK-002), used by [`crate::solve_hallen_paths`].
///
/// This is the path-aware counterpart of [`build_hallen_rhs`]. Instead of a
/// per-`GW` straight-axis coordinate, the homogeneous basis uses the **signed
/// arc-length** `s` along each [`ConductorPath`] with the traversal **sign**, so
/// `cos(k·s)` stays continuous across a bent or reversed (start-to-start) junction:
///
/// - `cos_vec[m] = sign[m]·cos(k·s_m)` where `s_m` is the segment's signed
///   arc-length on its path.
/// - Each voltage source drives its whole path; the source term is
///   `sign[m]·(−j·(2π/η)·V·sin(k·|s_m − s_src|))`, with `s` measured as arc-length
///   distance along the path.
///
/// The sign factor is what carries the fix: the current on segment `m` in its own
/// NEC direction is `sign[m]·I_path(s_m)`, so both the homogeneous and the driving
/// term pick up `sign[m]`. For a single straight wire (`sign = +1`, arc-length =
/// straight-axis coordinate) this reduces exactly to [`build_hallen_rhs`].
pub fn build_hallen_rhs_paths(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    paths: &[ConductorPath],
) -> Result<HallenRhs, ExcitationError> {
    let n = segs.len();
    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let scale = 2.0 * std::f64::consts::PI / ETA0;

    // Per-segment path index, sign, and signed arc-length.
    let mut path_of = vec![0usize; n];
    let mut sign_of = vec![1.0f64; n];
    let mut s_of = vec![0.0f64; n];
    for (pi, p) in paths.iter().enumerate() {
        for (j, &m) in p.segs.iter().enumerate() {
            path_of[m] = pi;
            sign_of[m] = p.signs[j];
            s_of[m] = p.s_mid[j];
        }
    }

    let mut cos_vec = vec![0.0; n];
    for m in 0..n {
        cos_vec[m] = sign_of[m] * (k * s_of[m]).cos();
    }

    // Collect voltage-source segments (type 0/5), superposing across the deck.
    let mut rhs = vec![Complex64::new(0.0, 0.0); n];
    let mut any_source = false;
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        // Same classification as above; see the comment there for why this loop
        // keeps its own error channel rather than adopting the reporting seam.
        match ex.kind().feedpoint_role() {
            FeedpointRole::DeltaGap => {}
            FeedpointRole::PlaneWave | FeedpointRole::CurrentSource => continue,
            FeedpointRole::Unknown => {
                return Err(ExcitationError::UnsupportedType {
                    ex_type: ex.excitation_type,
                    tag: ex.tag,
                    segment: ex.segment,
                    i4: ex.i4,
                })
            }
        }
        let fi = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
            .ok_or(ExcitationError::SegmentNotFound {
                tag: ex.tag,
                segment: ex.segment,
            })?;
        any_source = true;
        let vsrc = Complex64::new(ex.voltage_real, ex.voltage_imag);
        let src_path = path_of[fi];
        let s_src = s_of[fi];
        // The EX voltage is applied in the feed segment's own NEC direction, which
        // is `sign_of[fi]` relative to the path traversal. Referencing the driving
        // term to the feed sign keeps V/I[feed] positive regardless of which arm of
        // a start-to-start junction the feed lands on.
        let feed_sign = sign_of[fi];
        for m in 0..n {
            if path_of[m] != src_path {
                continue;
            }
            let ds = (s_of[m] - s_src).abs();
            rhs[m] +=
                Complex64::new(0.0, -scale * (k * ds).sin()) * vsrc * (sign_of[m] * feed_sign);
        }
    }

    if !any_source {
        return Ok(HallenRhs {
            rhs: vec![Complex64::new(0.0, 0.0); n],
            cos_vec,
        });
    }

    Ok(HallenRhs { rhs, cos_vec })
}

/// Build the unit-voltage Hallén source shape `g` at a given segment, for the
/// current-source (EX type 4) solve.
///
/// Returns `(source_shape, cos_vec, src_global_index)` where `source_shape` is
/// [`build_hallen_rhs`]'s RHS for a `V = 1` delta-gap at `(src_tag, src_segment)`
/// — i.e. the coefficient of the (unknown) port voltage in the current-source
/// system. Other EX cards are dropped from the synthesized geometry.
pub fn build_current_source_shape(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    src_tag: u32,
    src_segment: u32,
) -> Result<(Vec<Complex64>, Vec<f64>, usize), ExcitationError> {
    let src_seg = segs
        .iter()
        .position(|s| s.tag == src_tag && s.tag_index == src_segment)
        .ok_or(ExcitationError::SegmentNotFound {
            tag: src_tag,
            segment: src_segment,
        })?;

    // Synthesize a V=1 delta-gap at the source segment; drop other EX cards so
    // build_hallen_rhs sees a single ordinary voltage source.
    let mut synth = NecDeck::new();
    let mut placed = false;
    for card in &deck.cards {
        match card {
            Card::Ex(_) if !placed => {
                synth.cards.push(Card::Ex(ExCard {
                    excitation_type: 0,
                    tag: src_tag,
                    segment: src_segment,
                    i4: 0,
                    voltage_real: 1.0,
                    voltage_imag: 0.0,
                    polarization_deg: 0.0,
                    polarization_ratio: 0.0,
                    theta_inc: 0.0,
                    phi_inc: 0.0,
                }));
                placed = true;
            }
            Card::Ex(_) => {}
            other => synth.cards.push(other.clone()),
        }
    }

    let h = build_hallen_rhs(&synth, segs, freq_hz)?;
    Ok((h.rhs, h.cos_vec, src_seg))
}

/// Build the unit-voltage Hallén source shape `g` over **conductor paths** — the
/// general-junction current-source path (PH9-CHK-002), consumed by
/// [`crate::solve_hallen_current_source_paths`].
///
/// Identical to [`build_current_source_shape`] except the synthesized `V = 1`
/// delta-gap RHS is built with the path-aware [`build_hallen_rhs_paths`] (signed
/// arc-length `cos(k·s)`, source term summed along the conductor path) instead of
/// the per-`GW` [`build_hallen_rhs`], so the source shape stays continuous across a
/// degree-2 junction. Returns `(source_shape, cos_vec, src_global_index)`.
pub fn build_current_source_shape_paths(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    src_tag: u32,
    src_segment: u32,
    paths: &[ConductorPath],
) -> Result<(Vec<Complex64>, Vec<f64>, usize), ExcitationError> {
    let src_seg = segs
        .iter()
        .position(|s| s.tag == src_tag && s.tag_index == src_segment)
        .ok_or(ExcitationError::SegmentNotFound {
            tag: src_tag,
            segment: src_segment,
        })?;

    // Synthesize a V=1 delta-gap at the source segment; drop other EX cards so
    // build_hallen_rhs_paths sees a single ordinary voltage source.
    let mut synth = NecDeck::new();
    let mut placed = false;
    for card in &deck.cards {
        match card {
            Card::Ex(_) if !placed => {
                synth.cards.push(Card::Ex(ExCard {
                    excitation_type: 0,
                    tag: src_tag,
                    segment: src_segment,
                    i4: 0,
                    voltage_real: 1.0,
                    voltage_imag: 0.0,
                    polarization_deg: 0.0,
                    polarization_ratio: 0.0,
                    theta_inc: 0.0,
                    phi_inc: 0.0,
                }));
                placed = true;
            }
            Card::Ex(_) => {}
            other => synth.cards.push(other.clone()),
        }
    }

    let h = build_hallen_rhs_paths(&synth, segs, freq_hz, paths)?;
    Ok((h.rhs, h.cos_vec, src_seg))
}

fn apply_ex(ex: &ExCard, segs: &[Segment], v: &mut [Complex64]) -> Result<(), ExcitationError> {
    // Same classification as `build_hallen_rhs` (FND-031): only a delta-gap
    // source impresses V/Δl on the EFIE voltage vector; plane waves and current
    // sources have dedicated paths; an unrecognised type is an error.
    match ex.kind().feedpoint_role() {
        FeedpointRole::DeltaGap => {}
        FeedpointRole::PlaneWave | FeedpointRole::CurrentSource => return Ok(()),
        FeedpointRole::Unknown => {
            return Err(ExcitationError::UnsupportedType {
                ex_type: ex.excitation_type,
                tag: ex.tag,
                segment: ex.segment,
                i4: ex.i4,
            })
        }
    }

    // Find the segment by tag + tag_index.
    let idx = segs
        .iter()
        .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        .ok_or(ExcitationError::SegmentNotFound {
            tag: ex.tag,
            segment: ex.segment,
        })?;

    // The EFIE RHS has units of V/m (electric field).  A series voltage
    // source of voltage V over a segment of length Δl impresses a tangential
    // field E = V / Δl at the midpoint of that segment.
    let delta_l = segs[idx].length;
    v[idx] += Complex64::new(ex.voltage_real, ex.voltage_imag) / delta_l;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The columns a set of lumped series loads contributes to the Hallén matrix.
///
/// A load is not a diagonal term. In Hallén's equation the unknown is the current
/// and the matrix is dimensionless (`A[m,n] = cos α · ∫G dl`), while the driving
/// term carries the `−j·2π/η₀` scale and a `sin(k|s − s'|)` shape — the particular
/// solution of `(d²/ds² + k²)A_s = −jωμε·E_s`, whose kernel is `sin(k|s−s'|)/(2k)`.
///
/// A lumped series impedance `Z_p` at segment `p` imposes
/// `E_s = Z_p·I(s_p)·δ(s − s_p)`, which is *exactly* a delta-gap source of
/// `−I_p·Z_p`. The delta integrates out, so there is no `Δl` and no factor of
/// one half. Because `I_p` is the unknown, the term moves to the left-hand side
/// as a rank-1 update of column `p`, with the same shape the source term uses:
///
/// ```text
/// A[m, p] += −j·(2π/η₀)·Z_p·sin(k·|s_m − s_p|)
/// ```
///
/// Adding `Z_p` to `A[p,p]` instead — ohms onto a dimensionless matrix — was
/// FND-122: a 100 Ω load at the feed shifted Z by +700+j135 where the port
/// identity requires exactly +100, and off-feed series resistors *lowered* the
/// feedpoint resistance.
///
/// The stamp contributes nothing to its own row, since `sin(0) = 0`. That reads
/// wrong and is right: a load's effect on the current at its own position comes
/// through the rest of the column, not through a self term.
///
/// Returns `(column_index, column)` pairs; segments with a zero load are skipped.
pub fn hallen_load_columns(
    segs: &[Segment],
    freq_hz: f64,
    loads: &[Complex64],
    paths: Option<&[ConductorPath]>,
) -> Vec<(usize, Vec<Complex64>)> {
    let n = segs.len();
    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let scale = 2.0 * std::f64::consts::PI / ETA0;
    let zero = Complex64::new(0.0, 0.0);
    let mut out = Vec::new();

    match paths {
        // Conductor-path basis: signed arc length, and the column carries the
        // product of the two signs exactly as the source term carries
        // `sign_of[m] * feed_sign`. Without it a load on a reversed arm of a
        // start-to-start split stamps with the wrong sign — invisible on every
        // straight-wire test.
        Some(paths) => {
            let mut path_of = vec![0usize; n];
            let mut sign_of = vec![1.0f64; n];
            let mut s_of = vec![0.0f64; n];
            for (pi, p) in paths.iter().enumerate() {
                for (j, &m) in p.segs.iter().enumerate() {
                    path_of[m] = pi;
                    sign_of[m] = p.signs[j];
                    s_of[m] = p.s_mid[j];
                }
            }
            for (p_idx, &z_load) in loads.iter().enumerate().take(n) {
                if z_load == zero {
                    continue;
                }
                let mut col = vec![zero; n];
                let load_sign = sign_of[p_idx];
                for (m, slot) in col.iter_mut().enumerate() {
                    if path_of[m] != path_of[p_idx] {
                        continue;
                    }
                    let ds = (s_of[m] - s_of[p_idx]).abs();
                    *slot = Complex64::new(0.0, -scale * (k * ds).sin())
                        * z_load
                        * (sign_of[m] * load_sign);
                }
                out.push((p_idx, col));
            }
        }
        // Merged-conductor basis: the straight-axis coordinate the plain source
        // term uses, measured from the loaded segment along its own direction.
        None => {
            let components = crate::geometry::merge_collinear_wire_endpoints(segs);
            let mut comp_of = vec![0usize; n];
            for (ci, &(first, last)) in components.iter().enumerate() {
                for slot in comp_of.iter_mut().take(last + 1).skip(first) {
                    *slot = ci;
                }
            }
            for (p_idx, &z_load) in loads.iter().enumerate().take(n) {
                if z_load == zero {
                    continue;
                }
                let mut col = vec![zero; n];
                let (cf, cl) = components[comp_of[p_idx]];
                let src_mid = segs[p_idx].midpoint;
                let src_dir = segs[p_idx].direction;
                for m in cf..=cl {
                    let d = [
                        segs[m].midpoint[0] - src_mid[0],
                        segs[m].midpoint[1] - src_mid[1],
                        segs[m].midpoint[2] - src_mid[2],
                    ];
                    let s = d[0] * src_dir[0] + d[1] * src_dir[1] + d[2] * src_dir[2];
                    col[m] = Complex64::new(0.0, -scale * (k * s.abs()).sin()) * z_load;
                }
                out.push((p_idx, col));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {

    // -----------------------------------------------------------------------
    // The feedpoint seam (FND-031)
    // -----------------------------------------------------------------------

    fn deck_of(src: &str) -> NecDeck {
        nec_parser::parse(src).expect("parse").deck
    }

    // -----------------------------------------------------------------------
    // FND-120 — every delta gap drives the deck, not one per wire tag
    // -----------------------------------------------------------------------

    /// A mirror-symmetric wire driven at two mirror-image segments must present
    /// the SAME impedance at both. It is a symmetry argument, so it holds
    /// whatever the basis does and cannot be satisfied by a formulation quirk.
    ///
    /// Before the fix the second source contributed exactly nothing: the RHS
    /// collection was a `BTreeMap` keyed by wire tag, so one wire could hold one
    /// source. Measured on a 51-segment dipole fed at segments 16 and 36, the two
    /// feedpoints reported 112.18 + j16.63 and 106.72 + j28.78 — visibly
    /// asymmetric on a symmetric problem — and the CURRENTS block was
    /// byte-identical to the same deck with the second `EX` card deleted.
    ///
    /// The asymmetry is the assertion. Pinning one impedance would pass if both
    /// sources were dropped.
    #[test]
    fn two_gaps_on_one_wire_drive_it_symmetrically() {
        let src = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\n\
                   EX 0 1 16 0 1.0 0.0\nEX 0 1 36 0 1.0 0.0\n\
                   FR 0 1 0 0 14.2 0.0\nEN\n";
        let deck = deck_of(src);
        let segs = crate::build_geometry(&deck).expect("geometry");
        let rhs = build_hallen_rhs(&deck, &segs, 14.2e6).expect("rhs builds");

        let idx = |tag_index: u32| {
            segs.iter()
                .position(|s| s.tag == 1 && s.tag_index == tag_index)
                .expect("segment exists")
        };
        let (a, b) = (idx(16), idx(36));

        // Both feed segments must carry the same driving term, since each is the
        // other's mirror image and both sources are 1 V.
        let (ra, rb) = (rhs.rhs[a], rhs.rhs[b]);
        assert!(
            (ra - rb).norm() < 1e-12,
            "symmetric drive must give symmetric RHS at the two feeds: \
             {ra} vs {rb}"
        );

        // And the mirror symmetry must hold across the whole wire, which fails if
        // only one source is present: with one gap at 16 the profile is
        // lopsided.
        for m in 0..segs.len() {
            let mirror = segs.len() - 1 - m;
            let d = (rhs.rhs[m] - rhs.rhs[mirror]).norm();
            assert!(
                d < 1e-9,
                "segment {m} and its mirror {mirror} differ by {d}; a deck \
                 symmetric in geometry and drive cannot have an asymmetric RHS"
            );
        }
    }

    /// The control that stops the test above passing for the wrong reason: with
    /// ONE gap, off centre, the RHS must NOT be mirror-symmetric. Without this, a
    /// change that zeroed the RHS entirely would satisfy every assertion above.
    #[test]
    fn one_off_centre_gap_is_deliberately_asymmetric() {
        let src = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\n\
                   EX 0 1 16 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
        let deck = deck_of(src);
        let segs = crate::build_geometry(&deck).expect("geometry");
        let rhs = build_hallen_rhs(&deck, &segs, 14.2e6).expect("rhs builds");
        let worst = (0..segs.len())
            .map(|m| (rhs.rhs[m] - rhs.rhs[segs.len() - 1 - m]).norm())
            .fold(0.0f64, f64::max);
        assert!(
            worst > 1e-6,
            "a single off-centre gap must produce an asymmetric RHS, got {worst}"
        );
    }

    /// A second `EX` naming a segment that does not exist was silently skipped
    /// here while the conductor-path sibling refused it. A skipped card is the
    /// FND-026 shape: the deck asked for something and nobody said no.
    #[test]
    fn a_later_gap_on_a_missing_segment_is_refused_not_skipped() {
        let src = "CE\nGW 1 21 0 0 -5.0 0 0 5.0 0.001\nGE\n\
                   EX 0 1 11 0 1.0 0.0\nEX 0 1 99 0 1.0 0.0\n\
                   FR 0 1 0 0 14.2 0.0\nEN\n";
        let deck = deck_of(src);
        let segs = crate::build_geometry(&deck).expect("geometry");
        let err = build_hallen_rhs(&deck, &segs, 14.2e6).expect_err("segment 99 does not exist");
        assert!(
            matches!(
                err,
                ExcitationError::SegmentNotFound {
                    tag: 1,
                    segment: 99
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_plane_wave_is_never_a_feedpoint() {
        // Its tag/segment fields carry NTHETA/NPHI. Reading them as a feedpoint
        // reports grid dimensions as an antenna location — which the GUI and the
        // Python bindings both did.
        let deck = deck_of(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 1 1 3 0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let ex = first_delta_gap_feedpoint(&deck).expect("the voltage source");
        assert_eq!((ex.tag, ex.segment), (1, 26));
        assert_eq!(feedpoints(&deck).count(), 1, "only the driven source");
    }

    #[test]
    fn a_type_5_source_is_a_feedpoint() {
        // The worker skipped type 5 while `build_hallen_rhs` drove it as a delta
        // gap, so it solved such a deck and then refused to read the answer.
        let deck = deck_of(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 5 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let ex = first_delta_gap_feedpoint(&deck).expect("type 5 is a delta gap");
        assert_eq!((ex.tag, ex.segment), (1, 26));
    }

    #[test]
    fn a_current_source_is_a_feedpoint_but_not_a_delta_gap() {
        // The distinction the seam exists to carry: a filter on "voltage source"
        // would delete the CLI's corpus-pinned current-source row (PH8-CHK-001),
        // and a filter on "any EX" would hand it to callers that cannot price it.
        let deck = deck_of(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 4 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(feedpoints(&deck).count(), 1);
        assert_eq!(
            feedpoints(&deck).next().map(|(_, r)| r),
            Some(FeedpointRole::CurrentSource)
        );
        assert!(first_delta_gap_feedpoint(&deck).is_none());
    }

    #[test]
    fn the_first_delta_gap_is_the_first_one_in_deck_order() {
        // Without two delta-gap cards in one deck, "first" is untested: a seam
        // returning the LAST delta gap passes every other test here. Demonstrated
        // in review, so this is the test that catches it.
        let deck = deck_of(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 10 0 1.0 0.0\nEX 5 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(
            first_delta_gap_feedpoint(&deck).map(|e| e.segment),
            Some(10),
            "must be the first delta gap in deck order, not the last"
        );
    }

    #[test]
    fn an_unrecognised_excitation_type_is_not_a_feedpoint() {
        // The seam documents that unknown types are excluded, and nothing tested
        // it: yielding `Unknown` as a `DeltaGap` passed the whole seam block, all
        // the worker tests and all the CLI bin tests. Harmless only because
        // `build_excitation` rejects such a deck first — an invariant that lives
        // in another module and could move.
        let deck = deck_of(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 9 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(feedpoints(&deck).count(), 0);
        assert!(first_delta_gap_feedpoint(&deck).is_none());
    }

    #[test]
    fn feedpoints_are_yielded_in_deck_order() {
        let deck = deck_of(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 4 1 10 0 1.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let got: Vec<_> = feedpoints(&deck).map(|(ex, r)| (ex.segment, r)).collect();
        assert_eq!(
            got,
            vec![
                (10, FeedpointRole::CurrentSource),
                (26, FeedpointRole::DeltaGap)
            ]
        );
        // "First delta gap" is not "first feedpoint".
        assert_eq!(
            first_delta_gap_feedpoint(&deck).map(|e| e.segment),
            Some(26)
        );
    }

    use super::*;
    use nec_model::card::{Card, ExCard, GwCard};
    use nec_model::deck::NecDeck;

    use crate::geometry::build_geometry;

    const TEST_FREQ_HZ: f64 = 14.2e6;

    fn dipole_deck() -> NecDeck {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 6, // centre segment (1-based)
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
            polarization_deg: 0.0,
            polarization_ratio: 0.0,
            theta_inc: 0.0,
            phi_inc: 0.0,
        }));
        deck
    }

    #[test]
    fn voltage_placed_at_correct_segment() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let v = build_excitation(&deck, &segs).unwrap();

        assert_eq!(v.len(), 11);
        // Only segment index 5 (0-based) should be excited (tag_index 6).
        // The stored value is V/Δl (V/m), not raw voltage.
        let seg_len = segs[5].length;
        for (i, vi) in v.iter().enumerate() {
            if i == 5 {
                let expected = Complex64::new(1.0, 0.0) / seg_len;
                assert!(
                    (vi - expected).norm() < 1e-12,
                    "segment 6 should have V/Δl={expected}, got {vi}"
                );
            } else {
                assert_eq!(*vi, Complex64::new(0.0, 0.0), "segment {i} should be zero");
            }
        }
    }

    #[test]
    fn complex_voltage_is_stored() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 2,
            i4: 0,
            voltage_real: 0.5,
            voltage_imag: -0.5,
            polarization_deg: 0.0,
            polarization_ratio: 0.0,
            theta_inc: 0.0,
            phi_inc: 0.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        let v = build_excitation(&deck, &segs).unwrap();
        // Stored value is V/Δl.  Segment 1 spans from z=-1/3 to z=+1/3 → length 2/3.
        let seg_len = segs[1].length;
        let expected = Complex64::new(0.5, -0.5) / seg_len;
        assert!(
            (v[1] - expected).norm() < 1e-12,
            "expected {expected}, got {}",
            v[1]
        );
    }

    #[test]
    fn unknown_ex_type_is_error() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 6, // unknown (types 0,5 are voltage sources)
            tag: 1,
            segment: 2,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
            polarization_deg: 0.0,
            polarization_ratio: 0.0,
            theta_inc: 0.0,
            phi_inc: 0.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert!(matches!(
            build_excitation(&deck, &segs),
            Err(ExcitationError::UnsupportedType {
                ex_type: 6,
                tag: 1,
                segment: 2,
                i4: 0,
            })
        ));
    }

    #[test]
    fn segment_not_found_is_error() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: 0,
            tag: 99, // no such tag
            segment: 1,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
            polarization_deg: 0.0,
            polarization_ratio: 0.0,
            theta_inc: 0.0,
            phi_inc: 0.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        assert!(matches!(
            build_excitation(&deck, &segs),
            Err(ExcitationError::SegmentNotFound { tag: 99, .. })
        ));
    }

    #[test]
    fn hallen_rhs_has_expected_shapes() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();
        assert_eq!(h.rhs.len(), segs.len());
        assert_eq!(h.cos_vec.len(), segs.len());
    }

    #[test]
    fn hallen_rhs_feedpoint_cos_is_one_and_rhs_is_zero() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();

        // EX is on segment 6 (1-based) => index 5
        let feed_idx = 5usize;
        assert!(
            (h.cos_vec[feed_idx] - 1.0).abs() < 1e-12,
            "cos(feed) expected 1, got {}",
            h.cos_vec[feed_idx]
        );
        assert!(
            h.rhs[feed_idx].norm() < 1e-12,
            "rhs(feed) expected ~0, got {}",
            h.rhs[feed_idx]
        );
    }

    #[test]
    fn hallen_rhs_uses_two_pi_over_eta0_scale() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();

        let sample_idx = 0usize;
        let scale = 2.0 * std::f64::consts::PI / ETA0;
        let k = 2.0 * std::f64::consts::PI * TEST_FREQ_HZ / C0;
        let feed_mid = segs[5].midpoint;
        let sample_mid = segs[sample_idx].midpoint;
        let s = sample_mid[2] - feed_mid[2];
        let expected = Complex64::new(0.0, -scale * (k * s.abs()).sin());

        assert!(
            (h.rhs[sample_idx] - expected).norm() < 1e-12,
            "expected {expected}, got {}",
            h.rhs[sample_idx]
        );
    }

    #[test]
    fn hallen_rhs_is_symmetric_for_symmetric_dipole() {
        let deck = dipole_deck();
        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();

        let n = segs.len();
        for i in 0..n {
            let j = n - 1 - i;
            assert!(
                (h.cos_vec[i] - h.cos_vec[j]).abs() < 1e-12,
                "cos symmetry mismatch at {i}/{j}: {} vs {}",
                h.cos_vec[i],
                h.cos_vec[j]
            );
            assert!(
                (h.rhs[i] - h.rhs[j]).norm() < 1e-12,
                "rhs symmetry mismatch at {i}/{j}: {} vs {}",
                h.rhs[i],
                h.rhs[j]
            );
        }
    }

    #[test]
    fn hallen_rhs_accepts_non_collinear_topology() {
        // Non-collinear multi-wire geometries are now supported; build_hallen_rhs
        // should succeed and return per-wire local cos_vec values.
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 9,
            start: [-0.25, 0.0, 2.677],
            end: [0.25, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 4,
            segments: 9,
            start: [0.25, 0.0, 2.677],
            end: [0.25, 0.0, 3.177],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
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
        }));

        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();
        assert_eq!(h.rhs.len(), segs.len());
        assert_eq!(h.cos_vec.len(), segs.len());
        // Non-driven segments should have zero RHS.
        for (idx, seg) in segs.iter().enumerate() {
            if seg.tag != 1 {
                assert_eq!(h.rhs[idx].re, 0.0);
                assert_eq!(h.rhs[idx].im, 0.0);
            }
        }
    }

    #[test]
    fn hallen_rhs_allows_parallel_multi_wire_topology() {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 11,
            start: [0.0, 0.0, -2.677],
            end: [0.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Gw(GwCard {
            tag: 2,
            segments: 11,
            start: [1.0, 0.0, -2.677],
            end: [1.0, 0.0, 2.677],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
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
        }));

        let segs = build_geometry(&deck).unwrap();
        let h = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ).unwrap();
        assert_eq!(h.rhs.len(), segs.len());
        assert_eq!(h.cos_vec.len(), segs.len());
    }

    // ── Proptest sweeps (BL-IMPR-005) ───────────────────────────────────

    use proptest::prelude::*;

    /// Minimal single-segment geometry for proptest use.
    fn three_seg_deck_with_ex(
        v_re: f64,
        v_im: f64,
        ex_type: u32,
        seg_idx: u32,
    ) -> (NecDeck, Vec<Segment>) {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 3,
            start: [0.0, 0.0, -1.0],
            end: [0.0, 0.0, 1.0],
            radius: 0.001,
        }));
        deck.cards.push(Card::Ex(ExCard {
            excitation_type: ex_type,
            tag: 1,
            segment: seg_idx,
            i4: 0,
            voltage_real: v_re,
            voltage_imag: v_im,
            polarization_deg: 0.0,
            polarization_ratio: 0.0,
            theta_inc: 0.0,
            phi_inc: 0.0,
        }));
        let segs = build_geometry(&deck).unwrap();
        (deck, segs)
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(512))]

        /// EX type 0: only the target segment is non-zero; all others are zero.
        #[test]
        fn proptest_ex_type0_only_target_segment_nonzero(
            v_re in -1e6_f64..=1e6_f64,
            v_im in -1e6_f64..=1e6_f64,
            seg_idx in 1_u32..=3_u32,
        ) {
            let (deck, segs) = three_seg_deck_with_ex(v_re, v_im, 0, seg_idx);
            let v = build_excitation(&deck, &segs).unwrap();
            prop_assert_eq!(v.len(), 3);
            for (i, vi) in v.iter().enumerate() {
                let one_based = (i + 1) as u32;
                if one_based == seg_idx {
                    // Must be non-zero (V / dl).
                    let dl = segs[i].length;
                    let expected = Complex64::new(v_re, v_im) / dl;
                    prop_assert!((vi - expected).norm() < 1e-12,
                        "seg {seg_idx}: expected {expected}, got {vi}");
                } else {
                    prop_assert_eq!(*vi, Complex64::new(0.0, 0.0),
                        "seg {} should be zero", one_based);
                }
            }
        }

        /// EX type 0: stored value is V / dl (V/m units).
        #[test]
        fn proptest_ex_type0_stored_value_is_v_over_dl(
            v_re in -1e6_f64..=1e6_f64,
            v_im in -1e6_f64..=1e6_f64,
        ) {
            let (deck, segs) = three_seg_deck_with_ex(v_re, v_im, 0, 2);
            let v = build_excitation(&deck, &segs).unwrap();
            let dl = segs[1].length; // segment index 1 = seg_idx 2 (1-based)
            let expected = Complex64::new(v_re, v_im) / dl;
            prop_assert!((v[1] - expected).norm() < 1e-12,
                "expected {expected}, got {}", v[1]);
        }

        /// EX type 6+ is unknown: build_excitation must return UnsupportedType
        /// error, not panic. Types 1/2/3 (plane wave) and 4 (current source) are
        /// handled by dedicated paths; types 0 and 5 are voltage sources.
        #[test]
        fn proptest_unsupported_ex_types_return_error(
            v_re in -1e6_f64..=1e6_f64,
            v_im in -1e6_f64..=1e6_f64,
        ) {
            let (deck, segs) = three_seg_deck_with_ex(v_re, v_im, 6, 2);
            let result = build_excitation(&deck, &segs);
            prop_assert!(
                matches!(result, Err(ExcitationError::UnsupportedType { ex_type: 6, .. })),
                "expected UnsupportedType(6), got {result:?}"
            );
        }

        /// EX type 6+: build_hallen_rhs must return UnsupportedType error, not panic.
        #[test]
        fn proptest_hallen_rhs_unsupported_ex_types_return_error(
            seg_idx in 1_u32..=3_u32,
        ) {
            let (deck, segs) = three_seg_deck_with_ex(1.0, 0.0, 6, seg_idx);
            let result = build_hallen_rhs(&deck, &segs, TEST_FREQ_HZ);
            prop_assert!(
                matches!(result, Err(ExcitationError::UnsupportedType { ex_type: 6, .. })),
                "expected UnsupportedType(6), got {result:?}"
            );
        }

        /// Plane-wave (1/2/3) and current-source (4) types are skipped by the
        /// delta-gap builders (handled by their dedicated paths), so
        /// build_excitation returns a zero voltage vector rather than an error.
        #[test]
        fn proptest_plane_wave_types_skipped_by_build_excitation(
            ex_type in 1_u32..=4_u32,
        ) {
            let (deck, segs) = three_seg_deck_with_ex(1.0, 0.0, ex_type, 2);
            let v = build_excitation(&deck, &segs).expect("handled types are skipped, not errored");
            prop_assert!(v.iter().all(|x| x.norm() == 0.0), "skipped-type v_vec must be zero");
        }

        /// EX type 0: scale_excitation_for_pulse_rhs is linear — doubling the
        /// voltage doubles the scaled result.
        #[test]
        fn proptest_scale_excitation_is_linear_in_voltage(
            v_re in -1e6_f64..=1e6_f64,
            v_im in -1e6_f64..=1e6_f64,
            freq in 1e4_f64..3e10_f64,
        ) {
            let v = vec![Complex64::new(v_re, v_im)];
            let v2 = vec![Complex64::new(2.0 * v_re, 2.0 * v_im)];
            let scaled = scale_excitation_for_pulse_rhs(&v, freq);
            let scaled2 = scale_excitation_for_pulse_rhs(&v2, freq);
            prop_assert!((scaled2[0] - 2.0 * scaled[0]).norm() < 1e-9,
                "scaling not linear: 2*scaled={}, scaled2={}", 2.0 * scaled[0], scaled2[0]);
        }
    }
}
