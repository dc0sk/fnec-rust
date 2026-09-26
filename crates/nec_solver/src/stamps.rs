// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The deck's contribution to the impedance matrix, built once and applied
//! wherever a frontend assembles a Hallén solve.
//!
//! `LD` loads, `TL` lines and `NT` networks each add deltas to the matrix, and
//! every frontend used to repeat that assembly for itself — six sites, plus one
//! that rebuilt the stamps purely to harvest their warnings. They drifted, as
//! duplicated assembly does: only the CLI ever applied `NT`, so the same deck
//! solved to a different impedance depending on which frontend you asked
//! (70.633 + j14.009 Ω on the CLI against 74.243 + j13.900 Ω elsewhere).
//!
//! **Build is separate from apply on purpose.** Two callers need to know whether a
//! deck produced any stamps *before* deciding what to do with the matrix — the
//! GPU-resident paths re-solve on the device and discard host-side stamps, so they
//! must decline a deck that has any. They used to answer that question with a
//! hand-listed set of card types, and the two lists disagreed: the CLI's omitted
//! `NT` entirely, so `--exec gpu` silently returned an un-stamped answer.
//! [`DeckStamps::is_identity`] replaces both lists with the only question that
//! actually matters.
//!
//! Ordering among the three is not load-bearing: every one is a `+=` delta
//! ([`ZMatrix::add_to_diagonal`], [`ZMatrix::add_to_entry`]), so they commute even
//! where a `TL` and an `NT` touch the same entry. What *is* order-sensitive is
//! anything destructive that follows — `replace_row` for pulse current-source
//! constraints must run after [`DeckStamps::apply`], never before.
//!
//! Every solver except MPIE: the Hallén family (plain, conductor-path, sinusoidal)
//! and the Pocklington pulse/continuity pair, each with its own load form (see
//! [`stamp_hallen_load_columns`], [`pocklington_load_diagonal`]). `--solver mpie`
//! rejects `LD`/`TL`/`NT` outright,
//! so the seam must not be wired into an MPIE path: the matrix it would stamp is
//! never read there.

use num_complex::Complex64;

use crate::geometry::Segment;
use crate::matrix::ZMatrix;
use nec_model::deck::NecDeck;

/// Everything a deck contributes to a solve beyond its geometry and sources,
/// plus what went wrong building it.
#[derive(Debug, Clone, Default)]
pub struct DeckStamps {
    /// Per-segment series impedance from `LD` cards. Never added to a matrix as
    /// it stands: how a load enters depends on the basis that runs — columns for
    /// the Hallén family ([`stamp_hallen_load_columns`]), a scaled diagonal for
    /// the Pocklington solvers ([`pocklington_load_diagonal`]).
    pub diagonal: Vec<Complex64>,
    /// Whether the deck has `TL` or `NT` networks. They are not matrix stamps at
    /// all: a two-port couples port voltages, so the Hallén session solves them
    /// by superposition ([`crate::network`], FND-123). Only whether they exist
    /// matters here.
    pub has_networks: bool,
    /// Informational notes about the cards (segment-0 shorthand, skipped load
    /// types). Deduplicated: the same card produces one message however many
    /// frontends render it.
    pub warnings: Vec<String>,
}

impl DeckStamps {
    /// Whether the deck contributes nothing beyond geometry and sources.
    ///
    /// The question the GPU-resident paths need: they re-fill and solve on the
    /// device from raw segment inputs, so any load or network would be silently
    /// lost there. Loads are asked by *value*, so an `LD` that stamps exactly zero
    /// does not force the CPU path. Networks are asked by *presence*: even a
    /// zero-admittance `NT` changes the answer, because it inserts itself into the
    /// port gaps (FND-123) — and a deck whose networks moved out of the matrix
    /// must not start looking like an empty one (FND-023 in reverse).
    pub fn is_identity(&self) -> bool {
        !self.has_networks && self.diagonal.iter().all(|z| *z == Complex64::new(0.0, 0.0))
    }
}

/// Build the deck's contribution: `LD` loads, and whether it has `TL`/`NT`
/// networks.
///
/// Frequency-dependent (`LD` needs it), so a sweep rebuilds per point.
pub fn build_deck_stamps(deck: &NecDeck, segs: &[Segment], freq_hz: f64) -> DeckStamps {
    let mut warnings: Vec<String> = Vec::new();

    let (diagonal, load_warnings) = crate::loads::build_loads(deck, segs, freq_hz);
    warnings.extend(load_warnings.into_iter().map(|w| w.to_string()));

    // The network notes (segment-0 shorthand) belong with the other card notes.
    // A network that cannot be built is not a note: `pre_solve_error` refuses the
    // deck and the Hallén session refuses the solve.
    if let Ok((_, notes)) = crate::network::build_networks(deck, segs, freq_hz) {
        warnings.extend(notes);
    }

    let mut seen = std::collections::HashSet::new();
    warnings.retain(|w| seen.insert(w.clone()));

    DeckStamps {
        diagonal,
        has_networks: deck.cards.iter().any(|c| {
            matches!(
                c,
                nec_model::card::Card::Tl(_) | nec_model::card::Card::Nt(_)
            )
        }),
        warnings,
    }
}

/// Stamp lumped series loads into a Hallén-family matrix, as the columns
/// [`crate::excitation::hallen_load_columns`] derives.
///
/// One function for every Hallén-matrix solve, so the routed session and the
/// sinusoidal basis cannot stamp differently. The sinusoidal basis needs nothing
/// more: it solves `Tᵀ·Z·T·a` with `I = T·a`, and a column update
/// `(Z + c·eₚᵀ)·T·a = Z·T·a + c·I_p` still multiplies the load's own current
/// sample. The stamp must go into `Z` BEFORE the projection; stamped into
/// `Tᵀ·Z·T` it would multiply a basis coefficient instead, which is the objection
/// FND-124 recorded against reusing the Hallén derivation there.
///
/// `paths` must be the conductor paths of the solve that will run, or `None` for
/// the merged-conductor basis. These are deltas: call once per matrix.
pub fn stamp_hallen_load_columns(
    z: &mut ZMatrix,
    segs: &[Segment],
    freq_hz: f64,
    loads: &[Complex64],
    paths: Option<&[crate::geometry::ConductorPath]>,
) {
    if loads.iter().all(|l| *l == Complex64::new(0.0, 0.0)) {
        return;
    }
    for (col, column) in crate::excitation::hallen_load_columns(segs, freq_hz, loads, paths) {
        for (row, delta) in column.iter().enumerate() {
            if *delta != Complex64::new(0.0, 0.0) {
                z.add_to_entry(row, col, *delta);
            }
        }
    }
}

/// The diagonal a set of lumped series loads adds to a pulse-basis Pocklington
/// system, BEFORE the solver's right-hand-side scaling.
///
/// Pulse testing matches the tangential field at each segment midpoint, and a
/// delta-gap source of voltage `V` on segment `p` enters the right-hand side as
/// `V / Δl_p` ([`crate::build_excitation`]). A lumped series load `Z_p` is a
/// source of `−Z_p·I_p` in the same place, so it contributes `−Z_p·I_p / Δl_p`,
/// and moving the unknown to the left gives a diagonal term of `+Z_p / Δl_p`.
///
/// **The caller must pass the result through exactly the scaling it applies to
/// the source vector** (`--pulse-rhs nec2` multiplies by `−1/λ`). A load is a
/// source, so it takes the source's units, and since the scaling is linear the
/// port identity survives it: a load at the feed raises `Z_in` by exactly `Z_p`,
/// whatever the rest of the matrix is. Adding bare `Z_p` instead, as this basis
/// did until FND-124, gave `−4591 Ω` for a `1050 Ω` feed load: `Z_p` divided by
/// the missing `−1/(λ·Δl)`.
pub fn pocklington_load_diagonal(segs: &[Segment], loads: &[Complex64]) -> Vec<Complex64> {
    loads.iter().zip(segs).map(|(z, s)| *z / s.length).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_model::card::{Card, GwCard, LdCard, NtCard};

    const FREQ_HZ: f64 = 14.2e6;

    // `is_identity` decides whether a deck may be re-filled and solved on the
    // GPU, where every host-side stamp would be discarded. It is also the one
    // piece of this seam that CI can never exercise end to end: the runners have
    // no GPU adapter, so the device call falls back to CPU and a broken gate
    // returns the right answer anyway. These are the standing tripwire that a
    // one-time hardware check of FND-023 cannot be.
    fn dipole() -> NecDeck {
        let mut deck = NecDeck::new();
        deck.cards.push(Card::Gw(GwCard {
            tag: 1,
            segments: 21,
            start: [0.0, 0.0, -5.282],
            end: [0.0, 0.0, 5.282],
            radius: 0.001,
        }));
        deck
    }

    fn stamps_for(deck: &NecDeck) -> DeckStamps {
        let segs = crate::build_geometry(deck).expect("geometry");
        build_deck_stamps(deck, &segs, FREQ_HZ)
    }

    #[test]
    fn a_deck_with_no_stamping_cards_is_identity() {
        assert!(stamps_for(&dipole()).is_identity());
    }

    #[test]
    fn an_nt_card_makes_the_deck_non_identity() {
        // The exact case FND-023 shipped: the CLI's GPU gate listed `Ld` and
        // `Tl` and omitted `Nt`, so an NT deck was re-solved on the device and
        // the stamp silently dropped. A card-type question could miss this; a
        // question about values cannot.
        let mut deck = dipole();
        deck.cards.push(Card::Nt(NtCard {
            raw_fields: [
                "1", "6", "1", "16", // tag1 seg1 tag2 seg2
                "0.0", "-0.002", // Y11
                "0.0", "0.004", // Y12
                "0.0", "-0.002", // Y22
            ]
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        }));
        let stamps = stamps_for(&deck);
        assert!(stamps.has_networks, "an NT card is a network");
        assert!(!stamps.is_identity());
    }

    /// FND-123 moved TL/NT out of the matrix entries. A GPU gate that still asked
    /// only about entries would now wave a TL-only deck through and drop its line
    /// on the device — FND-023 again, from the other side.
    #[test]
    fn a_tl_card_makes_the_deck_non_identity() {
        let mut deck = dipole();
        deck.cards.push(Card::Tl(nec_model::card::TlCard {
            tag1: 1,
            segment1: 5,
            tag2: 1,
            segment2: 15,
            z0: 50.0,
            length: 1.0,
            shunt1: (0.0, 0.0),
            shunt2: (0.0, 0.0),
            velocity_factor: 1.0,
            loss_db: 0.0,
        }));
        let stamps = stamps_for(&deck);
        assert!(stamps.has_networks);
        assert!(!stamps.is_identity());
    }

    #[test]
    fn a_series_rlc_load_makes_the_deck_non_identity() {
        let mut deck = dipole();
        deck.cards.push(Card::Ld(LdCard {
            load_type: 0,
            tag: 1,
            seg_first: 11,
            seg_last: 11,
            f1: 50.0,
            f2: 0.0,
            f3: 0.0,
        }));
        assert!(!stamps_for(&deck).is_identity());
    }

    #[test]
    fn a_load_card_that_stamps_nothing_stays_identity() {
        // `is_identity` asks whether `apply` would change the matrix, not
        // whether a card is present. An `LD 4` with R = X = 0 parses, matches a
        // segment, and adds exactly zero — so the device path is safe for it.
        // An epsilon comparison here would err in the dangerous direction
        // (a small real stamp sent to the GPU to be discarded) rather than the
        // safe one.
        let mut deck = dipole();
        deck.cards.push(Card::Ld(LdCard {
            load_type: 4,
            tag: 1,
            seg_first: 11,
            seg_last: 11,
            f1: 0.0,
            f2: 0.0,
            f3: 0.0,
        }));
        assert!(
            stamps_for(&deck).is_identity(),
            "a zero-valued load must not force the CPU path"
        );
    }
}
