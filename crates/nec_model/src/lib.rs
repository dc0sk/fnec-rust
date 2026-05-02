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

/// **Extension point EP-4 — deck validator.**
///
/// Implementors receive a shared reference to the parsed [`deck::NecDeck`]
/// immediately before geometry resolution.  They return a list of
/// [`ValidationDiagnostic`] messages; an empty list means the deck passed
/// validation.
///
/// # Safety model
///
/// Same as EP-1: implementations are plain in-process Rust.  The interface
/// grants read-only access to the deck; no I/O capability is provided.
///
/// # Example
///
/// ```
/// use nec_model::{DeckValidator, ValidationDiagnostic};
/// use nec_model::deck::NecDeck;
///
/// struct RequireExCard;
///
/// impl DeckValidator for RequireExCard {
///     fn validate(&self, deck: &NecDeck) -> Vec<ValidationDiagnostic> {
///         use nec_model::card::Card;
///         let has_ex = deck.cards.iter().any(|c| matches!(c, Card::Ex(_)));
///         if has_ex {
///             vec![]
///         } else {
///             vec![ValidationDiagnostic::error("deck has no EX card; no feedpoint will be solved")]
///         }
///     }
/// }
///
/// let deck = NecDeck::new();
/// let v = RequireExCard;
/// let diags = v.validate(&deck);
/// assert_eq!(diags.len(), 1);
/// assert!(diags[0].message.contains("no EX card"));
/// ```
pub trait DeckValidator {
    /// Validate `deck` and return a (possibly empty) list of diagnostics.
    ///
    /// Returning an empty vec means the deck passed this validator.
    fn validate(&self, deck: &deck::NecDeck) -> Vec<ValidationDiagnostic>;
}

/// A diagnostic message produced by a [`DeckValidator`].
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationDiagnostic {
    /// Human-readable description of the issue.
    pub message: String,
    /// Severity level.
    pub level: DiagnosticLevel,
}

impl ValidationDiagnostic {
    /// Create an error-level diagnostic.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: DiagnosticLevel::Error,
        }
    }

    /// Create a warning-level diagnostic.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: DiagnosticLevel::Warning,
        }
    }
}

/// Severity level for a [`ValidationDiagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// Fatal: the caller should abort the solve.
    Error,
    /// Non-fatal: the caller may continue but should inform the user.
    Warning,
}

/// Run a list of validators against `deck` and collect all diagnostics.
///
/// Validators are run in the order given; all are run regardless of earlier
/// failures (so callers receive the complete diagnostic picture in one pass).
pub fn run_validators(
    deck: &deck::NecDeck,
    validators: &[&dyn DeckValidator],
) -> Vec<ValidationDiagnostic> {
    validators.iter().flat_map(|v| v.validate(deck)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Card, ExCard, FrCard};
    use crate::deck::NecDeck;

    struct RequireExCard;
    impl DeckValidator for RequireExCard {
        fn validate(&self, deck: &NecDeck) -> Vec<ValidationDiagnostic> {
            let has_ex = deck.cards.iter().any(|c| matches!(c, Card::Ex(_)));
            if has_ex {
                vec![]
            } else {
                vec![ValidationDiagnostic::error("deck has no EX card")]
            }
        }
    }

    struct RequireFrCard;
    impl DeckValidator for RequireFrCard {
        fn validate(&self, deck: &NecDeck) -> Vec<ValidationDiagnostic> {
            let has_fr = deck.cards.iter().any(|c| matches!(c, Card::Fr(_)));
            if has_fr {
                vec![]
            } else {
                vec![ValidationDiagnostic::error("deck has no FR card")]
            }
        }
    }

    fn make_ex_card() -> Card {
        Card::Ex(ExCard {
            excitation_type: 0,
            tag: 1,
            segment: 1,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
        })
    }

    fn make_fr_card() -> Card {
        Card::Fr(FrCard {
            step_type: 0,
            steps: 1,
            frequency_mhz: 14.0,
            step_mhz: 0.0,
        })
    }

    #[test]
    fn validator_returns_empty_when_deck_passes() {
        let mut deck = NecDeck::new();
        deck.cards.push(make_ex_card());
        let v = RequireExCard;
        let diags = v.validate(&deck);
        assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
    }

    #[test]
    fn validator_returns_error_when_deck_fails() {
        let deck = NecDeck::new(); // no EX card
        let v = RequireExCard;
        let diags = v.validate(&deck);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, DiagnosticLevel::Error);
        assert!(diags[0].message.contains("no EX card"));
    }

    #[test]
    fn run_validators_aggregates_all_diagnostics() {
        let deck = NecDeck::new(); // no EX and no FR
        let ex_v = RequireExCard;
        let fr_v = RequireFrCard;
        let diags = run_validators(&deck, &[&ex_v, &fr_v]);
        assert_eq!(diags.len(), 2, "expected two diagnostics, got: {diags:?}");
    }

    #[test]
    fn run_validators_continues_after_first_failure() {
        // Both validators fail; both should appear in result (no short-circuit).
        let deck = NecDeck::new();
        let ex_v = RequireExCard;
        let fr_v = RequireFrCard;
        let diags = run_validators(&deck, &[&ex_v, &fr_v]);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("EX")));
        assert!(messages.iter().any(|m| m.contains("FR")));
    }

    #[test]
    fn run_validators_returns_empty_when_all_pass() {
        let mut deck = NecDeck::new();
        deck.cards.push(make_ex_card());
        deck.cards.push(make_fr_card());
        let ex_v = RequireExCard;
        let fr_v = RequireFrCard;
        let diags = run_validators(&deck, &[&ex_v, &fr_v]);
        assert!(diags.is_empty());
    }

    #[test]
    fn warning_level_diagnostic_is_distinct_from_error() {
        let d_err = ValidationDiagnostic::error("bad");
        let d_warn = ValidationDiagnostic::warning("suspicious");
        assert_eq!(d_err.level, DiagnosticLevel::Error);
        assert_eq!(d_warn.level, DiagnosticLevel::Warning);
        assert_ne!(d_err.level, d_warn.level);
    }

    #[test]
    fn run_validators_with_empty_validator_list_returns_empty() {
        let deck = NecDeck::new();
        let diags = run_validators(&deck, &[]);
        assert!(diags.is_empty());
    }
}
