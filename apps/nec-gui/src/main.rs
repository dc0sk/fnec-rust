// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! fnec-gui — iced desktop frontend for the fnec antenna-modelling engine.
//!
//! Run with:
//! ```text
//! cargo run -p nec-gui
//! ```

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};
use nec_gui::app_state::{
    ActiveTab, AppState, CurrentsPhase, Message, PatternPhase, SolvePhase, SweepPhase, SweepSortCol,
};
use nec_gui::solve::{
    current_distribution_deck_path, pattern_slice_deck_path, solve_deck_path, sweep_deck_path,
    SolveResult, SweepPoint,
};
use std::path::PathBuf;

fn main() -> iced::Result {
    iced::application("fnec-gui — Antenna Modeler", FnecGui::update, FnecGui::view)
        .theme(|_| Theme::Dark)
        .run()
}

/// Root application struct wrapping the headless [`AppState`].
#[derive(Debug, Default)]
struct FnecGui {
    state: AppState,
}

impl FnecGui {
    fn update(&mut self, message: Message) -> Task<Message> {
        let spawn_solve = matches!(message, Message::Solve);
        let spawn_sweep = matches!(message, Message::RunSweep);
        let spawn_pattern = matches!(message, Message::RunPattern);
        let spawn_currents = matches!(message, Message::RunCurrents);
        self.state.apply(&message);

        if spawn_solve {
            let path = PathBuf::from(self.state.deck_path.clone());
            let vars: Option<String> = if self.state.vars_path.is_empty() {
                None
            } else {
                Some(self.state.vars_path.clone())
            };
            Task::perform(
                async move { solve_deck_path(&path, vars.as_deref()) },
                Message::SolveComplete,
            )
        } else if spawn_sweep {
            // Parse parameters (validated in apply; if invalid, sweep_phase becomes
            // SweepPhase::Running but we guard here to surface the error correctly).
            match self.state.sweep_params() {
                Ok((start, end, step)) => {
                    let path = PathBuf::from(self.state.deck_path.clone());
                    let vars: Option<String> = if self.state.vars_path.is_empty() {
                        None
                    } else {
                        Some(self.state.vars_path.clone())
                    };
                    Task::perform(
                        async move { sweep_deck_path(&path, vars.as_deref(), start, end, step) },
                        Message::SweepComplete,
                    )
                }
                Err(e) => {
                    // Surface parameter error as a completed sweep failure.
                    self.state.apply(&Message::SweepComplete(Err(e)));
                    Task::none()
                }
            }
        } else if spawn_pattern {
            match self.state.pattern_phi() {
                Ok(phi_deg) => {
                    let path = PathBuf::from(self.state.deck_path.clone());
                    let vars: Option<String> = if self.state.vars_path.is_empty() {
                        None
                    } else {
                        Some(self.state.vars_path.clone())
                    };
                    Task::perform(
                        async move { pattern_slice_deck_path(&path, vars.as_deref(), phi_deg) },
                        Message::PatternComplete,
                    )
                }
                Err(e) => {
                    self.state.apply(&Message::PatternComplete(Err(e)));
                    Task::none()
                }
            }
        } else if spawn_currents {
            let path = PathBuf::from(self.state.deck_path.clone());
            let vars: Option<String> = if self.state.vars_path.is_empty() {
                None
            } else {
                Some(self.state.vars_path.clone())
            };
            Task::perform(
                async move { current_distribution_deck_path(&path, vars.as_deref()) },
                Message::CurrentsComplete,
            )
        } else {
            Task::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // ── Tab bar ──────────────────────────────────────────────────────
        let tab_solve = if self.state.active_tab == ActiveTab::Solve {
            button("[ Solve ]").on_press(Message::TabSelected(ActiveTab::Solve))
        } else {
            button("  Solve  ").on_press(Message::TabSelected(ActiveTab::Solve))
        };
        let tab_sweep = if self.state.active_tab == ActiveTab::Sweep {
            button("[ Sweep ]").on_press(Message::TabSelected(ActiveTab::Sweep))
        } else {
            button("  Sweep  ").on_press(Message::TabSelected(ActiveTab::Sweep))
        };
        let tab_pattern = if self.state.active_tab == ActiveTab::Pattern {
            button("[ Pattern ]").on_press(Message::TabSelected(ActiveTab::Pattern))
        } else {
            button("  Pattern  ").on_press(Message::TabSelected(ActiveTab::Pattern))
        };
        let tab_currents = if self.state.active_tab == ActiveTab::Currents {
            button("[ Currents ]").on_press(Message::TabSelected(ActiveTab::Currents))
        } else {
            button("  Currents  ").on_press(Message::TabSelected(ActiveTab::Currents))
        };
        let tab_bar = row![tab_solve, tab_sweep, tab_pattern, tab_currents].spacing(4);

        // ── Shared deck-path row ─────────────────────────────────────────
        let path_row = row![
            text("Deck file:").width(Length::Fixed(80.0)),
            text_input("Path to .nec file…", &self.state.deck_path)
                .on_input(Message::DeckPathChanged)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // ── Optional variable-substitution file row ──────────────────────
        let vars_row = row![
            text("Vars file:").width(Length::Fixed(80.0)),
            text_input(
                "Optional: path to .toml or .json vars file (for $VAR decks)…",
                &self.state.vars_path,
            )
            .on_input(Message::VarsPathChanged)
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // ── Active tab content ───────────────────────────────────────────
        let tab_content: Element<Message> = match self.state.active_tab {
            ActiveTab::Solve => self.solve_view(),
            ActiveTab::Sweep => self.sweep_view(),
            ActiveTab::Pattern => self.pattern_view(),
            ActiveTab::Currents => self.currents_view(),
        };

        let content = column![tab_bar, path_row, vars_row, tab_content]
            .spacing(12)
            .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    // ── Single-frequency solve view ──────────────────────────────────────
    fn solve_view(&self) -> Element<'_, Message> {
        let solve_btn = if self.state.can_solve() {
            button("Solve").on_press(Message::Solve)
        } else {
            button("Solve")
        };
        let status = text(self.state.status_text());
        let result_section: Element<Message> = match &self.state.phase {
            SolvePhase::Done(r) => impedance_view(r),
            _ => text("").into(),
        };
        column![solve_btn, status, result_section].spacing(8).into()
    }

    // ── Sweep view ───────────────────────────────────────────────────────
    fn sweep_view(&self) -> Element<'_, Message> {
        let freq_inputs = row![
            text("Start (MHz):").width(Length::Fixed(90.0)),
            text_input("e.g. 14.0", &self.state.sweep_start)
                .on_input(Message::SweepStartChanged)
                .width(Length::Fixed(90.0)),
            text("  End (MHz):").width(Length::Fixed(90.0)),
            text_input("e.g. 18.0", &self.state.sweep_end)
                .on_input(Message::SweepEndChanged)
                .width(Length::Fixed(90.0)),
            text("  Step (MHz):").width(Length::Fixed(90.0)),
            text_input("e.g. 0.5", &self.state.sweep_step)
                .on_input(Message::SweepStepChanged)
                .width(Length::Fixed(80.0)),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);

        let run_btn = if self.state.can_sweep() {
            button("Run Sweep").on_press(Message::RunSweep)
        } else {
            button("Run Sweep")
        };

        let status = text(self.state.sweep_status_text());

        let result_section: Element<Message> = match &self.state.sweep_phase {
            SweepPhase::Done(_) => sweep_result_table(self),
            _ => text("").into(),
        };

        column![freq_inputs, run_btn, status, result_section]
            .spacing(8)
            .into()
    }
}

// ── Stand-alone helper widgets ────────────────────────────────────────────────

/// Small impedance result widget for the single-frequency tab.
fn impedance_view(r: &SolveResult) -> Element<'_, Message> {
    column![
        text("─── Result ───"),
        text(format!("Frequency  : {:.3} MHz", r.freq_mhz)),
        text(format!("Z_re       : {:.3} Ω", r.z_re)),
        text(format!("Z_im       : {:+.3} Ω", r.z_im)),
        text(format!(
            "|Z|        : {:.3} Ω",
            (r.z_re * r.z_re + r.z_im * r.z_im).sqrt()
        )),
    ]
    .spacing(4)
    .into()
}

/// Sortable sweep result table.  Borrows the full `FnecGui` to access sort state.
fn sweep_result_table(app: &FnecGui) -> Element<'_, Message> {
    let rows = app.state.sorted_sweep_rows();

    // ── Column headers with sort buttons ─────────────────────────────────
    let sort_indicator = |col: SweepSortCol| -> &'static str {
        if app.state.sweep_sort_col == col {
            if app.state.sweep_sort_asc {
                " ▲"
            } else {
                " ▼"
            }
        } else {
            ""
        }
    };

    let hdr_freq = button(text(format!(
        "Freq (MHz){}",
        sort_indicator(SweepSortCol::FreqMhz)
    )))
    .on_press(Message::SweepSortBy(SweepSortCol::FreqMhz))
    .width(Length::Fixed(110.0));
    let hdr_zre = button(text(format!(
        "Z_re (Ω){}",
        sort_indicator(SweepSortCol::ZRe)
    )))
    .on_press(Message::SweepSortBy(SweepSortCol::ZRe))
    .width(Length::Fixed(110.0));
    let hdr_zim = button(text(format!(
        "Z_im (Ω){}",
        sort_indicator(SweepSortCol::ZIm)
    )))
    .on_press(Message::SweepSortBy(SweepSortCol::ZIm))
    .width(Length::Fixed(110.0));
    let hdr_zmag = button(text(format!(
        "|Z| (Ω){}",
        sort_indicator(SweepSortCol::ZMag)
    )))
    .on_press(Message::SweepSortBy(SweepSortCol::ZMag))
    .width(Length::Fixed(110.0));

    let header_row = row![hdr_freq, hdr_zre, hdr_zim, hdr_zmag].spacing(4);

    // ── Data rows ─────────────────────────────────────────────────────────
    let mut data_col = column![header_row].spacing(2);
    for pt in rows.into_iter() {
        data_col = data_col.push(sweep_row(pt));
    }

    scrollable(data_col).height(Length::Fixed(280.0)).into()
}

fn sweep_row(pt: SweepPoint) -> Element<'static, Message> {
    let zmag = (pt.z_re * pt.z_re + pt.z_im * pt.z_im).sqrt();
    row![
        text(format!("{:.3}", pt.freq_mhz)).width(Length::Fixed(110.0)),
        text(format!("{:.3}", pt.z_re)).width(Length::Fixed(110.0)),
        text(format!("{:+.3}", pt.z_im)).width(Length::Fixed(110.0)),
        text(format!("{:.3}", zmag)).width(Length::Fixed(110.0)),
    ]
    .spacing(4)
    .into()
}

// ── FnecGui methods for the new tabs (PH3-CHK-011) ───────────────────────────

impl FnecGui {
    // ── Pattern view ─────────────────────────────────────────────────────
    fn pattern_view(&self) -> Element<'_, Message> {
        let phi_row = row![
            text("Azimuth φ (°):").width(Length::Fixed(110.0)),
            text_input("e.g. 0", &self.state.pattern_phi_deg)
                .on_input(Message::PatternPhiChanged)
                .width(Length::Fixed(80.0)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let run_btn = if self.state.can_run_pattern() {
            button("Run Pattern").on_press(Message::RunPattern)
        } else {
            button("Run Pattern")
        };

        let status = text(self.state.pattern_status_text());

        let result_section: Element<Message> = match &self.state.pattern_phase {
            PatternPhase::Done(_) => pattern_table(self),
            _ => text("").into(),
        };

        column![phi_row, run_btn, status, result_section]
            .spacing(8)
            .into()
    }

    // ── Currents view ─────────────────────────────────────────────────────
    fn currents_view(&self) -> Element<'_, Message> {
        let run_btn = if self.state.can_run_currents() {
            button("Run Currents").on_press(Message::RunCurrents)
        } else {
            button("Run Currents")
        };

        let status = text(self.state.currents_status_text());

        let result_section: Element<Message> = match &self.state.currents_phase {
            CurrentsPhase::Done(_) => currents_bars(self),
            _ => text("").into(),
        };

        column![run_btn, status, result_section].spacing(8).into()
    }
}

/// Pattern slice result table: θ column + gain dBi column + text bar.
fn pattern_table(app: &FnecGui) -> Element<'_, Message> {
    let rows = app.state.pattern_display_rows();

    let header = row![
        text("θ (°)").width(Length::Fixed(70.0)),
        text("Gain (dBi)").width(Length::Fixed(90.0)),
        text("Pattern").width(Length::Fill),
    ]
    .spacing(4);

    let mut col = column![header].spacing(2);
    for r in rows.into_iter() {
        col = col.push(pattern_row(r));
    }

    scrollable(col).height(Length::Fixed(300.0)).into()
}

fn pattern_row(r: nec_gui::app_state::PatternDisplayRow) -> Element<'static, Message> {
    // Bar: '█' chars scaled to 0..40 columns.
    let bar_len = (r.bar_width_frac * 40.0).round() as usize;
    let bar: String = "█".repeat(bar_len);
    row![
        text(format!("{:5.1}", r.theta_deg)).width(Length::Fixed(70.0)),
        text(format!("{:+7.2}", r.gain_dbi)).width(Length::Fixed(90.0)),
        text(bar).width(Length::Fill),
    ]
    .spacing(4)
    .into()
}

/// Current-distribution bar chart.
fn currents_bars(app: &FnecGui) -> Element<'_, Message> {
    let bars = app.state.current_display_bars();

    let header = row![
        text("Seg").width(Length::Fixed(50.0)),
        text("|I| (mA)").width(Length::Fixed(90.0)),
        text("Magnitude").width(Length::Fill),
    ]
    .spacing(4);

    let mut col = column![header].spacing(2);
    for b in bars.into_iter() {
        col = col.push(current_bar_row(b));
    }

    scrollable(col).height(Length::Fixed(300.0)).into()
}

fn current_bar_row(b: nec_gui::app_state::CurrentDisplayBar) -> Element<'static, Message> {
    let bar_len = (b.bar_width_frac * 40.0).round() as usize;
    let bar: String = "█".repeat(bar_len);
    row![
        text(format!("{:4}", b.seg_idx)).width(Length::Fixed(50.0)),
        text(format!("{:8.4}", b.current_mag_ma)).width(Length::Fixed(90.0)),
        text(bar).width(Length::Fill),
    ]
    .spacing(4)
    .into()
}
