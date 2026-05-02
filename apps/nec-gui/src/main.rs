// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! fnec-gui — iced desktop frontend for the fnec antenna-modelling engine.
//!
//! Run with:
//! ```text
//! cargo run -p nec-gui
//! ```

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length, Task, Theme};
use nec_gui::app_state::{AppState, Message, SolvePhase};
use nec_gui::solve::{solve_deck_path, SolveResult};
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
        self.state.apply(&message);

        if spawn_solve {
            let path = PathBuf::from(self.state.deck_path.clone());
            // solve_deck_path is synchronous; run it in the task future.
            // For typical antenna decks (<100 segments) this takes < 50 ms,
            // which is acceptable for a desktop application.
            Task::perform(
                async move { solve_deck_path(&path) },
                Message::SolveComplete,
            )
        } else {
            Task::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let path_row = row![
            text("Deck file:").width(Length::Fixed(80.0)),
            text_input("Path to .nec file…", &self.state.deck_path)
                .on_input(Message::DeckPathChanged)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

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

        let content = column![path_row, solve_btn, status, result_section,]
            .spacing(12)
            .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

/// Build a small impedance result widget.
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
