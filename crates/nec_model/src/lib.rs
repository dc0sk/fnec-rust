// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

pub mod card;
pub mod deck;

/// **Extension point EP-1 — deck post-processor.**
///
/// Implementors receive a mutable reference to the parsed [`deck::NecDeck`]
/// immediately after parsing and before geometry resolution.  This hook is
/// the safe boundary for deck-level transformations such as tag renaming,
/// card injection, or validation annotation.
///
/// # Safety model
///
/// Implementations are plain in-process Rust.  No network, filesystem, or
/// FFI access is reachable through the arguments supplied by this interface;
/// the caller enforces this by construction (no I/O capability is granted to
/// the trait).
///
/// # Example
///
/// ```
/// use nec_model::DeckPostProcessor;
/// use nec_model::deck::NecDeck;
///
/// struct CountCards(usize);
///
/// impl DeckPostProcessor for CountCards {
///     fn process(&mut self, deck: &mut NecDeck) {
///         self.0 = deck.cards.len();
///     }
/// }
///
/// let mut deck = NecDeck::new();
/// let mut counter = CountCards(0);
/// counter.process(&mut deck);
/// assert_eq!(counter.0, 0);
/// ```
pub trait DeckPostProcessor {
    /// Called once with the fully parsed deck.  Implementors may mutate
    /// `deck` in any way that preserves its structural invariants (card
    /// ordering, EN card at end when present).
    fn process(&mut self, deck: &mut deck::NecDeck);
}
