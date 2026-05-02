// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Application state machine — no iced dependency, fully testable in headless
//! environments.
//!
//! The iced binary wraps [`AppState`] and calls [`AppState::apply`] from its
//! `update` function.  Integration tests call it directly without a display.

use crate::solve::{SolveResult, SweepPoint};

/// Active view tab.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ActiveTab {
    /// Single-frequency solve (existing deck view).
    #[default]
    Solve,
    /// Frequency-range sweep.
    Sweep,
}

/// Current phase of the single-frequency solver pipeline.
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

/// Current phase of the sweep pipeline.
#[derive(Debug, Default, Clone, PartialEq)]
pub enum SweepPhase {
    #[default]
    Idle,
    Running,
    Done(Vec<SweepPoint>),
    Failed(String),
}

/// Top-level application state.
#[derive(Debug)]
pub struct AppState {
    /// Path to the NEC deck file as entered by the user.
    pub deck_path: String,
    /// Current active tab.
    pub active_tab: ActiveTab,
    /// Single-frequency solver phase.
    pub phase: SolvePhase,
    // ── Sweep tab state ────────────────────────────────────────────────────
    /// Sweep start frequency (MHz), as text so the input field can hold it.
    pub sweep_start: String,
    /// Sweep end frequency (MHz).
    pub sweep_end: String,
    /// Sweep step size (MHz).
    pub sweep_step: String,
    /// Sort column for the result table.
    pub sweep_sort_col: SweepSortCol,
    /// Whether to sort ascending.
    pub sweep_sort_asc: bool,
    /// Sweep pipeline phase.
    pub sweep_phase: SweepPhase,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            deck_path: String::new(),
            active_tab: ActiveTab::default(),
            phase: SolvePhase::default(),
            sweep_start: "14.0".into(),
            sweep_end: "18.0".into(),
            sweep_step: "0.5".into(),
            sweep_sort_col: SweepSortCol::FreqMhz,
            sweep_sort_asc: true,
            sweep_phase: SweepPhase::default(),
        }
    }
}

/// Which column the sweep table is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SweepSortCol {
    #[default]
    FreqMhz,
    ZRe,
    ZIm,
    ZMag,
}

/// Messages sent to the application update loop.
#[derive(Debug, Clone)]
pub enum Message {
    // ── Global ────────────────────────────────────────────────────────────
    /// User typed a new deck path.
    DeckPathChanged(String),
    /// User switched tabs.
    TabSelected(ActiveTab),
    // ── Single-frequency tab ──────────────────────────────────────────────
    /// User clicked the Solve button.
    Solve,
    /// Background single-frequency solve task completed.
    SolveComplete(Result<SolveResult, String>),
    // ── Sweep tab ────────────────────────────────────────────────────────
    /// User edited the sweep start frequency.
    SweepStartChanged(String),
    /// User edited the sweep end frequency.
    SweepEndChanged(String),
    /// User edited the sweep step size.
    SweepStepChanged(String),
    /// User clicked the Run Sweep button.
    RunSweep,
    /// Background sweep task completed.
    SweepComplete(Result<Vec<SweepPoint>, String>),
    /// User clicked a column header to sort.
    SweepSortBy(SweepSortCol),
}

impl AppState {
    /// Apply a message to the state machine.
    ///
    /// This is a pure function of the state — no I/O, no iced dependency.
    pub fn apply(&mut self, msg: &Message) {
        match msg {
            Message::DeckPathChanged(p) => {
                self.deck_path = p.clone();
                if matches!(self.phase, SolvePhase::Failed(_)) {
                    self.phase = SolvePhase::Idle;
                }
            }
            Message::TabSelected(tab) => {
                self.active_tab = tab.clone();
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
            Message::SweepStartChanged(s) => self.sweep_start = s.clone(),
            Message::SweepEndChanged(s) => self.sweep_end = s.clone(),
            Message::SweepStepChanged(s) => self.sweep_step = s.clone(),
            Message::RunSweep => {
                self.sweep_phase = SweepPhase::Running;
            }
            Message::SweepComplete(Ok(pts)) => {
                self.sweep_phase = SweepPhase::Done(pts.clone());
            }
            Message::SweepComplete(Err(e)) => {
                self.sweep_phase = SweepPhase::Failed(e.clone());
            }
            Message::SweepSortBy(col) => {
                if self.sweep_sort_col == *col {
                    self.sweep_sort_asc = !self.sweep_sort_asc;
                } else {
                    self.sweep_sort_col = *col;
                    self.sweep_sort_asc = true;
                }
            }
        }
    }

    /// Returns `true` when the single-frequency Solve button should be enabled.
    pub fn can_solve(&self) -> bool {
        !self.deck_path.is_empty() && !matches!(self.phase, SolvePhase::Solving)
    }

    /// Returns `true` when the Run Sweep button should be enabled.
    pub fn can_sweep(&self) -> bool {
        !self.deck_path.is_empty() && !matches!(self.sweep_phase, SweepPhase::Running)
    }

    /// Parse sweep parameters; returns `Err` with a diagnostic if any field
    /// is not a valid positive float.
    pub fn sweep_params(&self) -> Result<(f64, f64, f64), String> {
        let start = self
            .sweep_start
            .parse::<f64>()
            .map_err(|_| format!("invalid start frequency: '{}'", self.sweep_start))?;
        let end = self
            .sweep_end
            .parse::<f64>()
            .map_err(|_| format!("invalid end frequency: '{}'", self.sweep_end))?;
        let step = self
            .sweep_step
            .parse::<f64>()
            .map_err(|_| format!("invalid step size: '{}'", self.sweep_step))?;
        if step <= 0.0 {
            return Err(format!("step must be > 0, got {step}"));
        }
        if start >= end {
            return Err(format!("start ({start}) must be less than end ({end})"));
        }
        Ok((start, end, step))
    }

    /// Returns sorted rows for the sweep result table.
    pub fn sorted_sweep_rows(&self) -> Vec<SweepPoint> {
        let SweepPhase::Done(rows) = &self.sweep_phase else {
            return Vec::new();
        };
        let mut v = rows.clone();
        let asc = self.sweep_sort_asc;
        match self.sweep_sort_col {
            SweepSortCol::FreqMhz => v.sort_by(|a, b| cmp_f64(a.freq_mhz, b.freq_mhz, asc)),
            SweepSortCol::ZRe => v.sort_by(|a, b| cmp_f64(a.z_re, b.z_re, asc)),
            SweepSortCol::ZIm => v.sort_by(|a, b| cmp_f64(a.z_im, b.z_im, asc)),
            SweepSortCol::ZMag => v.sort_by(|a, b| {
                let ma = (a.z_re * a.z_re + a.z_im * a.z_im).sqrt();
                let mb = (b.z_re * b.z_re + b.z_im * b.z_im).sqrt();
                cmp_f64(ma, mb, asc)
            }),
        }
        v
    }

    /// Human-readable status line for the single-frequency tab.
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

    /// Human-readable status line for the sweep tab.
    pub fn sweep_status_text(&self) -> String {
        match &self.sweep_phase {
            SweepPhase::Idle => String::from("Enter a frequency range and click Run Sweep."),
            SweepPhase::Running => String::from("Sweeping…"),
            SweepPhase::Done(pts) => format!("Done — {} points", pts.len()),
            SweepPhase::Failed(e) => format!("Error: {e}"),
        }
    }
}

fn cmp_f64(a: f64, b: f64, asc: bool) -> std::cmp::Ordering {
    let ord = a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
    if asc {
        ord
    } else {
        ord.reverse()
    }
}
