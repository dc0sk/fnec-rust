// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! What frequencies a deck asks to be solved at.
//!
//! **One derivation, because there were five readings.** The CLI expanded the
//! *first* `FR` card and treated an unrecognised `step_type` as "the start
//! frequency alone"; `fnec_py` expanded *every* card and treated it as linear;
//! the GUI read the first card's frequency in three separate places; and
//! `validate` expanded a fourth way to check for degenerate values. A two-`FR`
//! deck therefore solved at different frequencies depending on which frontend
//! you asked — the house defect, live (FND-057).
//!
//! **The reference settles it, and neither implementation was right.** Measured
//! against `nec2c` 1.3.3 on a 21-segment 10.5 m dipole with one `XQ`:
//!
//! | deck | nec2c reports |
//! |------|---------------|
//! | `FR 14.2` alone | 78.860 + j44.755 |
//! | `FR 7.1` alone | 13.792 − j914.16 |
//! | `FR 14.2` then `FR 7.1` | 13.792 − j914.16, header `FREQUENCY : 7.1000E+00 MHz` |
//! | `FR 7.1` then `FR 14.2` | 78.860 + j44.755, header `1.4200E+01 MHz` |
//!
//! So the **last** `FR` before execution governs — the CLI's "first" is wrong.
//! And `FR 2 3 0 0 14.2 0.1` runs 14.2/14.3/14.4 in nec2c, so an unrecognised
//! `step_type` is **linear** — the CLI's "start alone" is wrong there too, while
//! `fnec_py` was right.
//!
//! fnec has no execution-card sequencing: it drops `XQ` as an unknown card and
//! runs one implicit execution, so "the `FR` in force at that execution" is the
//! last one before `EN`. A deck written as `FR/XQ/FR/RP` expected both to run,
//! which fnec cannot do — so a superseded card earns a warning rather than
//! silence, via [`superseded_fr_warnings`].

use nec_model::card::{Card, FrCard};
use nec_model::deck::NecDeck;

/// Upper bound on the points one `FR` card may expand to.
///
/// `steps` is a `u32`, so `FR 0 400000000 0 0 14.0 0.0` asks for 3.2 GB of
/// `f64`. The validator was taught not to allocate that in #417; the *expander*
/// still did, and a tiny `step_mhz` keeps the extremes in range so validation
/// passes. Matches the GUI's own sweep cap, which exists for the same reason.
pub const MAX_FR_POINTS: usize = 100_000;

/// One `FR` card's sweep, as a value.
///
/// Everything that needs to know a deck's frequencies goes through this, so a
/// second reading cannot appear without deleting the first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrSweep {
    pub start_mhz: f64,
    pub step_mhz: f64,
    /// `step_type == 1`. Every other value is linear, which is what the
    /// reference does — not "the start frequency alone".
    pub multiplicative: bool,
    pub steps: u32,
}

impl FrSweep {
    pub fn from_card(fr: &FrCard) -> Self {
        Self {
            start_mhz: fr.frequency_mhz,
            step_mhz: fr.step_mhz,
            multiplicative: fr.step_type == 1,
            steps: fr.steps,
        }
    }

    /// How many points this card asks for. At least one: NEC treats `steps = 0`
    /// as a single frequency rather than none.
    pub fn len(&self) -> usize {
        self.steps.max(1) as usize
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// The `i`-th frequency. **The single derivation** — `frequencies_mhz` and
    /// `extremes_mhz` both call this, so a validator and an expander cannot come
    /// to different answers about the same card.
    pub fn frequency_mhz(&self, i: usize) -> f64 {
        if self.multiplicative {
            // `powi` takes an i32; beyond that the magnitude is already far past
            // anything finite, so clamping cannot mask a value in range.
            self.start_mhz * self.step_mhz.powi(i.min(i32::MAX as usize) as i32)
        } else {
            self.start_mhz + self.step_mhz * (i as f64)
        }
    }

    /// The frequencies this card asks for, in order, bounded by [`MAX_FR_POINTS`].
    pub fn frequencies_mhz(&self) -> Vec<f64> {
        (0..self.len().min(MAX_FR_POINTS))
            .map(|i| self.frequency_mhz(i))
            .collect()
    }

    /// First and last only, for a check that must not build the list.
    ///
    /// Both expansions are monotone in magnitude, so the extremes bracket every
    /// value between them — which is what lets validation stay O(1) while the
    /// expander is bounded rather than free.
    pub fn extremes_mhz(&self) -> [f64; 2] {
        [self.frequency_mhz(0), self.frequency_mhz(self.len() - 1)]
    }

    /// Whether this card asks for more points than will be expanded.
    pub fn is_truncated(&self) -> bool {
        self.len() > MAX_FR_POINTS
    }
}

/// Every `FR` card in the deck, in order.
pub fn fr_sweeps(deck: &NecDeck) -> Vec<FrSweep> {
    deck.cards
        .iter()
        .filter_map(|c| match c {
            Card::Fr(fr) => Some(FrSweep::from_card(fr)),
            _ => None,
        })
        .collect()
}

/// The `FR` card that governs the solve: the **last** one.
///
/// See the module docs — measured against nec2c in both orders.
pub fn governing_fr_sweep(deck: &NecDeck) -> Option<FrSweep> {
    fr_sweeps(deck).pop()
}

/// The frequencies (Hz) the deck will actually be solved at.
pub fn frequencies_hz(deck: &NecDeck) -> Vec<f64> {
    governing_fr_sweep(deck)
        .map(|s| s.frequencies_mhz().into_iter().map(|f| f * 1e6).collect())
        .unwrap_or_default()
}

/// One warning per `FR` card the last one supersedes.
///
/// Not silence. A deck with several `FR` cards was usually written as
/// `FR/XQ/FR/RP`, where NEC-2 runs *both* — fnec has no execution-card
/// sequencing and runs one, so dropping the earlier ones without saying so
/// silently discards runs the author asked for.
pub fn superseded_fr_warnings(deck: &NecDeck) -> Vec<String> {
    let sweeps = fr_sweeps(deck);
    if sweeps.len() < 2 {
        return Vec::new();
    }
    let governing = sweeps[sweeps.len() - 1];
    sweeps[..sweeps.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "FR card {} ({} MHz) is superseded by the last FR card ({} MHz): fnec runs \
                 one execution per deck, at the frequencies the last card asks for. NEC-2 \
                 would run an earlier card too if an execution card followed it, which fnec \
                 does not model",
                i + 1,
                s.start_mhz,
                governing.start_mhz
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_parser::parse;

    fn deck_of(fr: &str) -> NecDeck {
        parse(&format!(
            "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\n{fr}\nEX 0 1 11 0 1.0 0.0\nEN\n"
        ))
        .expect("deck parses")
        .deck
    }

    /// The oracle result, pinned. `nec2c` on this deck reports
    /// `FREQUENCY : 7.1000E+00 MHz` and 13.792 - j914.16, which is the *second*
    /// card's answer; reversing the cards gives the other one. The CLI took the
    /// first card and `fnec_py` took both.
    #[test]
    fn the_last_fr_card_governs_as_nec2c_does() {
        let d = deck_of("FR 0 1 0 0 14.2 0\nFR 0 1 0 0 7.1 0");
        assert_eq!(frequencies_hz(&d), vec![7.1e6]);
        let r = deck_of("FR 0 1 0 0 7.1 0\nFR 0 1 0 0 14.2 0");
        assert_eq!(frequencies_hz(&r), vec![14.2e6]);
    }

    /// `nec2c` runs `FR 2 3 0 0 14.2 0.1` as 14.2/14.3/14.4 — linear. The CLI
    /// treated an unrecognised `step_type` as the start frequency alone.
    #[test]
    fn an_unrecognised_step_type_is_linear_as_nec2c_does() {
        let d = deck_of("FR 2 3 0 0 14.2 0.1");
        let f: Vec<f64> = frequencies_hz(&d).iter().map(|h| h / 1e6).collect();
        assert_eq!(f.len(), 3);
        assert!((f[1] - 14.3).abs() < 1e-9, "{f:?}");
        assert!((f[2] - 14.4).abs() < 1e-9, "{f:?}");
    }

    #[test]
    fn a_multiplicative_card_multiplies() {
        let d = deck_of("FR 1 3 0 0 10.0 2.0");
        let f: Vec<f64> = frequencies_hz(&d).iter().map(|h| h / 1e6).collect();
        assert_eq!(f.len(), 3);
        assert!(
            (f[1] - 20.0).abs() < 1e-9 && (f[2] - 40.0).abs() < 1e-9,
            "{f:?}"
        );
    }

    /// A superseded card is warned about, not silently dropped: the deck was
    /// probably written as `FR/XQ/FR/RP`, where NEC-2 runs both.
    #[test]
    fn a_superseded_card_is_named_rather_than_silently_dropped() {
        let w = superseded_fr_warnings(&deck_of("FR 0 1 0 0 14.2 0\nFR 0 1 0 0 7.1 0"));
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("14.2") && w[0].contains("7.1"), "{w:?}");
        // Negative control: one card supersedes nothing.
        assert!(superseded_fr_warnings(&deck_of("FR 0 1 0 0 14.2 0")).is_empty());
    }

    /// The extremes must agree with the list they summarise, or a validator and
    /// an expander built on this type could still disagree — the defect being
    /// closed, one layer down.
    #[test]
    fn the_extremes_agree_with_the_expansion() {
        for fr in [
            "FR 0 5 0 0 14.0 0.1",
            "FR 0 5 0 0 10.0 -3.0",
            "FR 1 4 0 0 10.0 2.0",
            "FR 1 4 0 0 10.0 0.5",
            "FR 0 1 0 0 14.2 0",
        ] {
            let s = governing_fr_sweep(&deck_of(fr)).expect("a card");
            let list = s.frequencies_mhz();
            let [first, last] = s.extremes_mhz();
            assert_eq!(first, list[0], "{fr}");
            assert_eq!(last, *list.last().unwrap(), "{fr}");
        }
    }

    /// `steps` is a `u32`; the expander must not allocate what it asks for.
    #[test]
    fn an_enormous_step_count_is_bounded() {
        let s = governing_fr_sweep(&deck_of("FR 0 400000000 0 0 14.0 0.0")).expect("a card");
        assert!(s.is_truncated());
        let t = std::time::Instant::now();
        let list = s.frequencies_mhz();
        assert_eq!(list.len(), MAX_FR_POINTS);
        assert!(t.elapsed().as_millis() < 500, "expansion was not bounded");
    }

    #[test]
    fn a_deck_with_no_fr_card_asks_for_no_frequencies() {
        let d = parse("GW 1 21 0 0 -1 0 0 1 0.001\nGE 0\nEN\n")
            .expect("parses")
            .deck;
        assert!(frequencies_hz(&d).is_empty());
        assert!(governing_fr_sweep(&d).is_none());
    }
}
