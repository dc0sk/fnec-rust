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
use nec_gui::app_state::{ActiveTab, AppState, Message, SolvePhase, SweepPhase, SweepSortCol};
use nec_gui::solve::{solve_deck_path, sweep_deck_path, SolveResult, SweepPoint};
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
        self.state.apply(&message);

        if spawn_solve {
            let path = PathBuf::from(self.state.deck_path.clone());
            Task::perform(
                async move { solve_deck_path(&path) },
                Message::SolveComplete,
            )
        } else if spawn_sweep {
            // Parse parameters (validated in apply; if invalid, sweep_phase becomes
            // SweepPhase::Running but we guard here to surface the error correctly).
            match self.state.sweep_params() {
                Ok((start, end, step)) => {
                    let path = PathBuf::from(self.state.deck_path.clone());
                    Task::perform(
                        async move { sweep_deck_path(&path, start, end, step) },
                        Message::SweepComplete,
                    )
                }
                Err(e) => {
                    // Surface parameter error as a completed sweep failure.
                    self.state.apply(&Message::SweepComplete(Err(e)));
                    Task::none()
                }
            }
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
        let tab_bar = row![tab_solve, tab_sweep].spacing(4);

        // ── Shared deck-path row ─────────────────────────────────────────
        let path_row = row![
            text("Deck file:").width(Length::Fixed(80.0)),
            text_input("Path to .nec file…", &self.state.deck_path)
                .on_input(Message::DeckPathChanged)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // ── Active tab content ───────────────────────────────────────────
        let tab_content: Element<Message> = match self.state.active_tab {
            ActiveTab::Solve => self.solve_view(),
            ActiveTab::Sweep => self.sweep_view(),
        };

        let content = column![tab_bar, path_row, tab_content]
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
