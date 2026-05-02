// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Application state machine — no iced dependency, fully testable in headless
//! environments.
//!
//! The iced binary wraps [`AppState`] and calls [`AppState::apply`] from its
//! `update` function.  Integration tests call it directly without a display.

use crate::solve::SolveResult;

/// Current phase of the solver pipeline for a loaded deck.
#[derive(Debug, Default, Clone, PartialEq)]
pub enum SolvePhase {
    /// No solve has been attempted yet (or deck path was just changed).
    #[default]
    Idle,
    /// A solve is running asynchronously.
    Solving,
    /// Solve finished successfully; result is available.
    Done(SolveResult),
    /// Solve finished with an error.
    Failed(String),
}

/// Top-level application state.
#[derive(Debug, Default)]
pub struct AppState {
    /// Path to the NEC deck file as entered by the user.
    pub deck_path: String,
    /// Current solver phase.
    pub phase: SolvePhase,
}

/// Messages sent to the application update loop.
#[derive(Debug, Clone)]
pub enum Message {
    /// User typed a new deck path.
    DeckPathChanged(String),
    /// User clicked the Solve button.
    Solve,
    /// Background solve task completed.
    SolveComplete(Result<SolveResult, String>),
}

impl AppState {
    /// Apply a message to the state machine.
    ///
    /// This is a pure function of the state — no I/O, no iced dependency.
    pub fn apply(&mut self, msg: &Message) {
        match msg {
            Message::DeckPathChanged(p) => {
                self.deck_path = p.clone();
                // Clear any previous error when the user changes the path.
                if matches!(self.phase, SolvePhase::Failed(_)) {
                    self.phase = SolvePhase::Idle;
                }
            }
            Message::Solve => {
                self.phase = SolvePhase::Solving;
            }
            Message::SolveComplete(Ok(r)) => {
                self.phase = SolvePhase::Done(r.clone());
            }
            Message::SolveComplete(Err(e)) => {
                self.phase = SolvePhase::Failed(e.clone());
            }
        }
    }

    /// Returns `true` when the Solve button should be enabled.
    pub fn can_solve(&self) -> bool {
        !self.deck_path.is_empty() && !matches!(self.phase, SolvePhase::Solving)
    }

    /// Human-readable status line shown below the path input.
    pub fn status_text(&self) -> String {
        match &self.phase {
            SolvePhase::Idle => String::from("Ready"),
            SolvePhase::Solving => String::from("Solving…"),
            SolvePhase::Done(r) => format!(
                "Done — {:.3} MHz | Z = {:.2} + j{:.2} Ω",
                r.freq_mhz, r.z_re, r.z_im
            ),
            SolvePhase::Failed(e) => format!("Error: {e}"),
        }
    }
}
