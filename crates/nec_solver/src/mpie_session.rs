// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Driving the MPIE solver from a deck.
//!
//! Separate from `mpie.rs`, which holds the numerics, because this is the
//! *routing*: which excitation feeds the geometry, which node it drives, and
//! which decks the MPIE must refuse. That decision lived in the CLI and made the
//! CLI the only frontend that could solve a deck needing the MPIE (FND-007).
//!
//! The refusals travel **with** the solve, in [`MpieSessionError::Unsupported`].
//! They are not advisory: the MPIE's triangle basis has nowhere to stamp a load,
//! and its delta-gap feed cannot represent an incident field, so a deck carrying
//! either would be solved with the offending card silently ignored. Sharing the
//! solve without its refusals is how one frontend's wrong answer becomes four —
//! the mistake FND-048 caught on the current-source path before it shipped.

use num_complex::Complex64;

use crate::excitation::first_delta_gap_feedpoint;
use crate::geometry::Segment;
use crate::mpie::{
    feed_node_for_segment, feed_reference_sign, geometry_from_segments, segment_currents,
    solve_mpie, solve_mpie_ground, MpieError,
};
use crate::GroundModel;
use nec_model::card::Card;
use nec_model::deck::NecDeck;

/// A deck the MPIE cannot solve, and why.
///
/// Typed rather than `String` so each frontend phrases the refusal in its own
/// terms: the CLI names the flag that would work, a GUI names its solver picker,
/// and a Python caller gets an exception — three audiences, one decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MpieUnsupported {
    /// An incident plane wave (`EX` types 1-3). The MPIE drives a delta gap; it
    /// has no incident-field right-hand side.
    PlaneWave,
    /// An impressed current source (`EX` type 4), which the Hallén path solves by
    /// forcing a current and recovering the port voltage.
    CurrentSource,
    /// A card whose effect the MPIE's basis cannot stamp: `LD`, `TL`, or `NT`.
    UnstampableCard(&'static str),
}

impl MpieUnsupported {
    /// What the deck carries, for a frontend composing its own sentence.
    pub fn subject(&self) -> &'static str {
        match self {
            Self::PlaneWave => "incident plane-wave excitation",
            Self::CurrentSource => "current-source excitation",
            Self::UnstampableCard(card) => card,
        }
    }
}

impl std::fmt::Display for MpieUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaneWave | Self::CurrentSource => {
                write!(
                    f,
                    "EX: {} is not supported by the MPIE solver",
                    self.subject()
                )
            }
            Self::UnstampableCard(card) => {
                write!(f, "{card}: not supported by the MPIE solver")
            }
        }
    }
}

/// Why an MPIE solve could not run.
#[derive(Debug)]
pub enum MpieSessionError {
    /// The deck carries something the MPIE cannot represent.
    Unsupported(MpieUnsupported),
    /// No delta-gap source (`EX` type 0 or 5) to drive.
    NoVoltageSource,
    /// The `EX` card names a segment the geometry does not contain.
    DrivenSegmentNotFound { tag: u32, segment: u32 },
    /// The feed segment has no interior (degree-2) node; only such a node can
    /// host the MPIE's delta gap.
    NoInteriorNode,
    /// The numerics failed.
    Solve(MpieError),
    /// The solve returned, but not with numbers (FND-126).
    NonFiniteCurrents(crate::NonFiniteCurrents),
}

impl std::fmt::Display for MpieSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(u) => write!(f, "{u}"),
            Self::NoVoltageSource => write!(
                f,
                "the MPIE solver requires a delta-gap voltage source (EX type 0 or 5)"
            ),
            Self::DrivenSegmentNotFound { tag, segment } => {
                write!(f, "EX: driven segment {tag}/{segment} not found")
            }
            Self::NoInteriorNode => write!(
                f,
                "the feed segment has no interior (degree-2) node to drive"
            ),
            Self::Solve(e) => write!(f, "{e}"),
            Self::NonFiniteCurrents(e) => write!(f, "{e}"),
        }
    }
}

/// What in this deck the MPIE cannot represent, if anything.
///
/// Exposed separately from the solve so a frontend can refuse *before* offering
/// the MPIE — greying out a picker entry reads better than solving and failing.
/// `LD` is reported ahead of `TL`/`NT` only by card order; the first blocker wins.
///
/// Laplace loads are deliberately absent: they arrive through the CLI's
/// `--loads-config`, never through the deck, so no deck-only predicate can see
/// them. That guard stays with the frontend that owns the input.
pub fn mpie_unsupported(deck: &NecDeck) -> Option<MpieUnsupported> {
    for card in &deck.cards {
        let found = match card {
            Card::Ld(_) => MpieUnsupported::UnstampableCard("LD (loads)"),
            Card::Tl(_) => MpieUnsupported::UnstampableCard("TL (transmission line)"),
            Card::Nt(_) => MpieUnsupported::UnstampableCard("NT (network)"),
            Card::Ex(ex) if ex.kind().is_plane_wave() => MpieUnsupported::PlaneWave,
            Card::Ex(ex) if ex.kind() == nec_model::card::ExcitationKind::CurrentSource => {
                MpieUnsupported::CurrentSource
            }
            _ => continue,
        };
        return Some(found);
    }
    None
}

/// Solve a deck on the MPIE path, returning per-segment currents aligned to
/// `segs` so the reporting machinery consumes them unchanged.
///
/// Note the feed model differs from the Hallén path: NEC's `EX` drives a segment
/// gap, while the MPIE drives the nearest interior node — a half-segment offset
/// that vanishes under refinement. The MPIE's value is the topologies the Hallén
/// basis cannot reach (degree-3 junctions, closed loops, near-ground currents).
pub fn solve_mpie_session(
    deck: &NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
    freq_hz: f64,
) -> Result<Vec<Complex64>, MpieSessionError> {
    if let Some(u) = mpie_unsupported(deck) {
        return Err(MpieSessionError::Unsupported(u));
    }
    // Delta gaps only (`EX` types 0 and 5). The older `!is_plane_wave()` test
    // admitted a type-4 current source and any unrecognised type, and was safe
    // only because callers happened to reject those first (FND-037) — a shared
    // function may not depend on what its callers checked.
    let ex = first_delta_gap_feedpoint(deck).ok_or(MpieSessionError::NoVoltageSource)?;
    let driven_idx = segs
        .iter()
        .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        .ok_or(MpieSessionError::DrivenSegmentNotFound {
            tag: ex.tag,
            segment: ex.segment,
        })?;

    let geom = geometry_from_segments(segs);
    let feed_node =
        feed_node_for_segment(&geom, driven_idx).ok_or(MpieSessionError::NoInteriorNode)?;

    let has_ground = !matches!(
        ground,
        GroundModel::FreeSpace | GroundModel::Deferred { .. }
    );
    let sol = if has_ground {
        solve_mpie_ground(&geom, freq_hz, feed_node, ground)
    } else {
        solve_mpie(&geom, freq_hz, feed_node)
    }
    .map_err(MpieSessionError::Solve)?;

    // `solve_mpie` drives the feed with a unit (1 V) source; scale the resulting
    // currents by the deck's actual `EX` voltage so the reported currents are
    // physical and the feedpoint V/I (in `build_feedpoint_rows`) is independent of
    // the source voltage — MoM is linear, so I(V) = V·I(1 V).
    //
    // The unit source is applied along the *basis's* reference direction, which is
    // set by the incidence order of the fed node's two arms. `EX` instead applies
    // it along the driven segment's own `p0 → p1` tangent, and for a
    // start-to-start junction feed (both `GW` cards written outward from the
    // shared node, e.g. an apex-fed inverted-V) those oppose. Re-reference the
    // solve to the deck's source polarity, or every reported current — and hence
    // the feedpoint `V/I` — comes out negated (an unphysical negative resistance
    // for a deck that is only written differently, not built differently).
    let feed_sign = feed_reference_sign(&geom, feed_node, driven_idx).unwrap_or(1.0);
    let source_v = Complex64::new(ex.voltage_real, ex.voltage_imag) * feed_sign;
    let mut currents = segment_currents(&geom, &sol.basis_currents);
    for c in &mut currents {
        *c *= source_v;
    }
    // The MPIE had no finiteness check anywhere: a diverged solve whose feed
    // current happened to stay finite printed NaN rows for every other segment
    // (FND-126).
    crate::check_currents_finite(&currents).map_err(MpieSessionError::NonFiniteCurrents)?;
    Ok(currents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_parser::parse;

    fn deck_of(text: &str) -> NecDeck {
        parse(text).expect("deck parses").deck
    }

    const DIPOLE: &str =
        "GW 1 41 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nFR 0 1 0 0 14.2 0\nEX 0 1 21 0 1.0 0.0\nEN\n";

    #[test]
    fn a_plain_voltage_source_deck_is_supported() {
        assert_eq!(mpie_unsupported(&deck_of(DIPOLE)), None);
    }

    #[test]
    fn a_plane_wave_deck_is_refused() {
        let deck = deck_of(&DIPOLE.replace("EX 0 1 21 0 1.0 0.0", "EX 1 1 1 0 0.0 0.0"));
        assert_eq!(mpie_unsupported(&deck), Some(MpieUnsupported::PlaneWave));
    }

    #[test]
    fn a_current_source_deck_is_refused() {
        // FND-037: the old `!is_plane_wave()` feedpoint test accepted a type-4
        // source, so this deck would have been *solved* — as though the impressed
        // current were a 1 V delta gap — had the refusal stayed in the CLI.
        let deck = deck_of(&DIPOLE.replace("EX 0 1 21 0 1.0 0.0", "EX 4 1 21 0 1.0 0.0"));
        assert_eq!(
            mpie_unsupported(&deck),
            Some(MpieUnsupported::CurrentSource)
        );
    }

    #[test]
    fn unstampable_cards_are_refused_by_name() {
        for (card, name) in [
            ("LD 0 1 21 21 10.0 0.0 0.0", "LD (loads)"),
            ("TL 1 21 1 22 50.0 1.0 0 0 0 0", "TL (transmission line)"),
            ("NT 1 21 1 22 0.0 0.0 0.0 0.0 0.0 0.0", "NT (network)"),
        ] {
            let deck = deck_of(&DIPOLE.replace("GE 0", &format!("GE 0\n{card}")));
            assert_eq!(
                mpie_unsupported(&deck)
                    .as_ref()
                    .map(MpieUnsupported::subject),
                Some(name),
                "{card} must be refused by name"
            );
        }
    }

    #[test]
    fn a_type_5_source_can_drive_the_mpie() {
        // Type 5 is a delta gap everywhere else in the solver; the feedpoint-role
        // seam must agree, or the MPIE refuses a deck the Hallén path solves.
        let deck = deck_of(&DIPOLE.replace("EX 0 1 21 0 1.0 0.0", "EX 5 1 21 0 1.0 0.0"));
        assert_eq!(mpie_unsupported(&deck), None);
        assert!(first_delta_gap_feedpoint(&deck).is_some());
    }

    #[test]
    fn an_unrecognised_excitation_is_not_a_voltage_source() {
        // The old feedpoint lookup took anything that was not a plane wave, so an
        // unrecognised `EX` type was driven as a 1 V delta gap. It must now be a
        // refusal, not a silent reinterpretation.
        let deck = deck_of(&DIPOLE.replace("EX 0 1 21 0 1.0 0.0", "EX 9 1 21 0 1.0 0.0"));
        assert!(
            first_delta_gap_feedpoint(&deck).is_none(),
            "an unrecognised EX type must not be treated as a delta gap"
        );
    }

    #[test]
    fn an_unrecognised_excitation_is_refused_by_the_solve() {
        // FND-037 at the solve, not just at the lookup. `mpie_unsupported` names
        // only plane waves, current sources and unstampable cards, so an
        // unrecognised `EX` type reaches the feedpoint lookup — where the old
        // `!is_plane_wave()` test accepted it and drove the geometry with whatever
        // voltage fields the card happened to carry. Refusal is the only honest
        // answer for an excitation the solver does not recognise.
        let deck = deck_of(&DIPOLE.replace("EX 0 1 21 0 1.0 0.0", "EX 9 1 21 0 1.0 0.0"));
        assert_eq!(
            mpie_unsupported(&deck),
            None,
            "the guard does not cover this"
        );
        let segs = crate::build_geometry(&deck).expect("geometry builds");
        let err = solve_mpie_session(&deck, &segs, &GroundModel::FreeSpace, 14.2e6)
            .expect_err("an unrecognised EX type must not silently drive the MPIE");
        assert!(
            matches!(err, MpieSessionError::NoVoltageSource),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn the_refusal_travels_with_the_solve() {
        // The guard is inside `solve_mpie_session`, not merely beside it: a caller
        // that forgets `mpie_unsupported` still cannot solve a load-bearing deck.
        let deck = deck_of(&DIPOLE.replace("GE 0", "GE 0\nLD 0 1 21 21 10.0 0.0 0.0"));
        let segs = crate::build_geometry(&deck).expect("geometry builds");
        let err = solve_mpie_session(&deck, &segs, &GroundModel::FreeSpace, 14.2e6)
            .expect_err("a loaded deck must be refused by the solve itself");
        assert!(
            matches!(
                err,
                MpieSessionError::Unsupported(MpieUnsupported::UnstampableCard(_))
            ),
            "unexpected error: {err}"
        );
    }
}
