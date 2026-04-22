// SPDX-License-Identifier: GPL-2.0-only
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
}

impl NecDeck {
    pub fn new() -> Self {
        Self::default()
    }
}
