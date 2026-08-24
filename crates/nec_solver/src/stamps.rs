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
//! This is the Hallén family only. `--solver mpie` rejects `LD`/`TL`/`NT` outright,
//! so the seam must not be wired into an MPIE path: the matrix it would stamp is
//! never read there.

use num_complex::Complex64;

use crate::geometry::Segment;
use crate::matrix::ZMatrix;
use nec_model::deck::NecDeck;

/// Everything a deck contributes to the impedance matrix, plus what went wrong
/// building it.
#[derive(Debug, Clone, Default)]
pub struct DeckStamps {
    /// Per-segment series impedance from `LD` cards, added to the diagonal.
    pub diagonal: Vec<Complex64>,
    /// Off-diagonal `(row, col, delta)` contributions from `TL` and `NT` cards.
    pub entries: Vec<(usize, usize, Complex64)>,
    /// Cards that were skipped, and why. Deduplicated: the same malformed card
    /// produces one message however many frontends render it.
    pub warnings: Vec<String>,
}

impl DeckStamps {
    /// Add every stamp to `z`.
    ///
    /// Must run before any destructive matrix edit (`replace_row`), and must not be
    /// applied twice to the same matrix — these are deltas, not assignments.
    pub fn apply(&self, z: &mut ZMatrix) {
        if !self.diagonal.is_empty() {
            z.add_to_diagonal(&self.diagonal);
        }
        for &(row, col, delta) in &self.entries {
            z.add_to_entry(row, col, delta);
        }
    }

    /// Whether applying this would leave the matrix unchanged.
    ///
    /// The question the GPU-resident paths need: a deck that stamps nothing can be
    /// solved on the device, because there is no host-side contribution to lose.
    /// Asked of the *values* rather than of which cards are present, so a deck
    /// carrying an `LD` card that stamps zero is not needlessly refused, and a card
    /// type nobody remembered to list cannot slip through.
    pub fn is_identity(&self) -> bool {
        self.entries.is_empty() && self.diagonal.iter().all(|z| *z == Complex64::new(0.0, 0.0))
    }
}

/// Build the deck's matrix contribution: `LD` loads, `TL` lines and `NT` networks.
///
/// Frequency-dependent — `LD` and `TL` both need it — so a sweep rebuilds per point.
pub fn build_deck_stamps(deck: &NecDeck, segs: &[Segment], freq_hz: f64) -> DeckStamps {
    let mut warnings: Vec<String> = Vec::new();

    let (diagonal, load_warnings) = crate::loads::build_loads(deck, segs, freq_hz);
    warnings.extend(load_warnings.into_iter().map(|w| w.to_string()));

    let (tl_stamps, tl_warnings) = crate::tl::build_tl_stamps(deck, segs, freq_hz);
    warnings.extend(tl_warnings.into_iter().map(|w| w.to_string()));

    let (nt_stamps, nt_warnings) = crate::network::build_nt_stamps(deck, segs);
    warnings.extend(nt_warnings.into_iter().map(|w| w.to_string()));

    // One message per distinct problem. The frontends previously deduplicated
    // differently — the CLI only for `NT`, the bindings across everything, the GUI
    // not at all — so "the same warnings everywhere" was not true even where the
    // same cards were read.
    let mut seen = std::collections::HashSet::new();
    warnings.retain(|w| seen.insert(w.clone()));

    let mut entries: Vec<(usize, usize, Complex64)> = Vec::new();
    entries.extend(tl_stamps);
    entries.extend(nt_stamps);

    DeckStamps {
        diagonal,
        entries,
        warnings,
    }
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
        assert!(!stamps.entries.is_empty(), "NT should stamp off-diagonals");
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
