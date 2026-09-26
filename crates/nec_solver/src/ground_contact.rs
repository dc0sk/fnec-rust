// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Wires touching a perfectly conducting ground, solved by explicit images
//! (FND-082).
//!
//! Over PEC ground the problem is exactly free space with the geometry mirrored
//! through z = 0. With each mirrored segment's direction mirrored too
//! (`t' = (tx, ty, −tz)`), the PEC image current `J' = (−Jx, −Jy, +Jz)` is
//! `−I·t'` for every orientation, so in the image segment's own frame the image
//! current is `−I`, an image delta gap is `−V`, and the image of a series load
//! is the same load.
//!
//! fnec's PEC path folds the image into the kernel and keeps only the real
//! wires as unknowns, which cannot express a wire whose base is joined to its
//! own image: the base would get a free-end row and the current would be forced
//! to vanish exactly where it is largest. Doubling the structure turns the
//! ground contact into what it physically is — a start-to-start join of the
//! wire and its image — which the conductor-path route solves with no free-end
//! row there (current flows through the base) and both homogeneous terms
//! (FND-158).
//!
//! nec2c agrees exactly: a λ/4 monopole on PEC gives the same impedance as each
//! gap of the doubled dipole driven at both central segments (39.584 + j23.206).
//!
//! Scope: PEC only. A wire touching FINITE ground is refused — nec2c answers a
//! λ/4 vertical on average ground with 179 − j261 Ω, which is not physical, and
//! fnec has no better model of a finite-ground connection.

use nec_model::card::Card;
use nec_model::deck::NecDeck;

use crate::geometry::{GroundModel, Segment};

/// A coordinate this close to z = 0 counts as touching the ground plane.
pub const GROUND_CONTACT_EPS_M: f64 = 1.0e-9;

/// The doubled, free-space problem equivalent to a PEC-ground deck with wires
/// touching the ground.
#[derive(Debug, Clone)]
pub struct ImageProblem {
    /// The deck with its ground removed and mirrored sources and loads added.
    pub deck: NecDeck,
    /// The original segments followed by their mirror images (fresh tags).
    pub segs: Vec<Segment>,
    /// How many of `segs` are the original ones; the rest are images.
    pub n_orig: usize,
}

/// Whether any segment touches the ground plane (`z ≤ ε` at an endpoint).
pub fn touches_ground(segs: &[Segment]) -> bool {
    segs.iter()
        .any(|s| s.start[2] <= GROUND_CONTACT_EPS_M || s.end[2] <= GROUND_CONTACT_EPS_M)
}

/// Build the image problem for a PEC-ground deck with ground contact.
///
/// `Ok(None)`: not PEC ground, or nothing touches it — solve as usual.
/// `Err`: the ground contact is one fnec cannot represent. Each refusal names a
/// shape that would otherwise be answered wrongly:
/// - a wire below the ground (truly buried);
/// - a segment lying in the ground plane (its image coincides with it; nec2c
///   refuses this too);
/// - a contact that, doubled, is not a set of simple conductor paths — a wire
///   grounded at both ends (a closed loop with its image), or several wires
///   meeting at one ground point. Those would fall to the junction class that
///   Hallén cannot solve (FND-162).
pub fn pec_ground_contact(
    deck: &NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
) -> Result<Option<ImageProblem>, String> {
    if !matches!(ground, GroundModel::PerfectConductor) || !touches_ground(segs) {
        return Ok(None);
    }
    for s in segs {
        let (lo, hi) = (s.start[2].min(s.end[2]), s.start[2].max(s.end[2]));
        if lo < -GROUND_CONTACT_EPS_M {
            return Err(format!(
                "tag {} seg {} lies below the ground plane (z = {lo:.6e} m); fnec does \
                 not model buried wires",
                s.tag, s.tag_index
            ));
        }
        if hi <= GROUND_CONTACT_EPS_M {
            return Err(format!(
                "tag {} seg {} lies in the ground plane; a wire touching PEC ground must \
                 leave it (its image would coincide with it)",
                s.tag, s.tag_index
            ));
        }
    }

    let n = segs.len();
    let max_tag = segs.iter().map(|s| s.tag).max().unwrap_or(0);
    let mirror = |p: [f64; 3]| [p[0], p[1], -p[2]];
    let mut doubled = segs.to_vec();
    doubled.extend(segs.iter().enumerate().map(|(i, s)| Segment {
        tag: s.tag + max_tag,
        tag_index: s.tag_index,
        global_index: n + i,
        start: mirror(s.start),
        end: mirror(s.end),
        midpoint: mirror(s.midpoint),
        direction: mirror(s.direction),
        length: s.length,
        radius: s.radius,
    }));

    if matches!(
        crate::hallen_session::classify_paths(&doubled),
        crate::hallen_session::PathRoute::Unsupported
    ) {
        return Err(
            "this ground contact makes a junction or a closed loop with its image (a wire \
             grounded at both ends, or several wires meeting at one ground point), which \
             fnec's Hallén solver cannot represent (FND-162)"
                .to_string(),
        );
    }

    // The doubled problem is free space: no GN card and no GE reflection flag.
    let mut image_deck = deck.clone();
    image_deck
        .cards
        .retain(|c| !matches!(c, Card::Gn(_) | Card::Ge(_)));
    let mut images = Vec::new();
    for card in &image_deck.cards {
        match card {
            // A delta gap's image is −V in the mirrored segment's frame.
            Card::Ex(ex)
                if ex.kind().feedpoint_role() == nec_model::card::FeedpointRole::DeltaGap =>
            {
                let mut image = ex.clone();
                image.tag += max_tag;
                image.voltage_real = -ex.voltage_real;
                image.voltage_imag = -ex.voltage_imag;
                images.push(Card::Ex(image));
            }
            // A series load's image is the same load. Tag 0 already applies to
            // every tag, the images' included, so it must not be duplicated.
            Card::Ld(ld) if ld.tag != 0 => {
                let mut image = ld.clone();
                image.tag += max_tag;
                images.push(Card::Ld(image));
            }
            _ => {}
        }
    }
    image_deck.cards.extend(images);

    Ok(Some(ImageProblem {
        deck: image_deck,
        segs: doubled,
        n_orig: n,
    }))
}
