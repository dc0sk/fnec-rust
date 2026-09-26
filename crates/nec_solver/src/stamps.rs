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
    /// Add only the two-port couplings (`TL`, `NT`), leaving loads to the caller.
    ///
    /// Loads are never stamped here, because how a load enters depends on the
    /// basis that will run: columns for the Hallén family
    /// ([`stamp_hallen_load_columns`]), a scaled diagonal for the Pocklington
    /// solvers ([`pocklington_load_diagonal`]). [`Self::diagonal`] is the data both
    /// start from.
    ///
    /// The couplings carry the same units error as the loads did — they are
    /// impedances added to a dimensionless matrix — and are not fixed here: a
    /// two-port network couples port *voltages*, which are not unknowns in the
    /// Hallén system, so a correct treatment needs extra unknowns and equations
    /// rather than a different stamp. They earn a caveat, not a silent answer.
    pub fn apply_couplings(&self, z: &mut ZMatrix) {
        for &(row, col, delta) in &self.entries {
            z.add_to_entry(row, col, delta);
        }
    }

    /// Whether this deck stamps any two-port coupling whose model is unvalidated.
    pub fn has_couplings(&self) -> bool {
        !self.entries.is_empty()
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
