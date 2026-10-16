// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The top-level container for a parsed NEC deck.

use crate::card::Card;

/// A fully parsed NEC deck.
///
/// Cards are stored in source order.  The `EN` card, when present, is
/// included as the last element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NecDeck {
    pub cards: Vec<Card>,
    /// Every program card as NEC reads its fields, in source order, for the
    /// NEC-2D card echo (PAR-011).
    pub program_cards: Vec<ProgramCard>,
}

impl NecDeck {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A program card — any card after the geometry that is not a comment — with its
/// fields as NEC reads them: the first four as integers and the next six as
/// reals, a missing field read as zero.
///
/// The NEC-2D output echoes every one of these as
/// `***** DATA CARD NO.  n   XX  I1 I2 I3 I4  F1 … F6`, and 4nec2 reads the
/// parameters of its pattern request from that echo (PAR-011, the removal
/// ablation in `docs/par011-dropin-evidence-memo.md`). So the record keeps the
/// cards fnec itself ignores, such as `XQ` and `EK`, and the fields fnec's typed
/// cards drop, such as `GN`'s I2–I4 and the XNDA digits of `RP`.
///
/// The record holds what the card says. The echo prints an `EN` with the
/// previous card's fields instead of its own, and reads the end of the file as an
/// `EN` the deck may not have; those are rules of the echo, applied by its
/// writer (`apps/nec-cli/tests/par011_card_echo.rs` pins both against the kernel).
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramCard {
    /// The 1-based line of the card in the deck.
    pub line: usize,
    /// The mnemonic, in upper case.
    pub mnemonic: String,
    /// `(I1..I4, F1..F6)`, or `None` when a field is not a number NEC could read
    /// (possible only on a card fnec ignores, whose fields it never checks).
    pub fields: Option<([i32; 4], [f64; 6])>,
}
