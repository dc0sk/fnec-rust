// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! fnec-gui — iced desktop frontend for the fnec antenna-modelling engine.
//!
//! Run with:
//! ```text
//! cargo run -p nec-gui
//! ```

mod viewport;

use iced::widget::pane_grid::{Axis, Split};
use iced::widget::{
    button, checkbox, column, container, pane_grid, progress_bar, row, scrollable, shader, text,
    text_input,
};
use iced::{Element, Length, Task, Theme};
use nec_gui::app_state::{
    ActiveTab, AppState, CurrentsPhase, Message, PatternPhase, SolvePhase, SweepPhase,
    SweepSortCol, ViewportMsg,
};
use nec_gui::solve::{
    current_distribution_deck_path, load_currents_path, load_geometry_path, pattern_grid_path,
    pattern_slice_deck_path, solve_deck_path, sweep_deck_path, SolveResult, SweepPoint,
};
use std::path::PathBuf;

fn main() -> iced::Result {
    iced::application("fnec-gui — Antenna Modeler", FnecGui::update, FnecGui::view)
        .theme(|_| Theme::Dark)
        .run()
}

/// Which workbench pane a `pane_grid` cell holds.
#[derive(Debug, Clone, Copy)]
enum Pane {
    /// Controls + result tables (deck path, tabs, feedpoint/sweep/pattern/currents).
    Main,
    /// The always-visible GPU 3-D viewport.
    Viewport,
}

/// Root application struct wrapping the headless [`AppState`] plus the (iced-only)
/// pane layout. The single divider's `Split` id is kept so a resize needs only
/// the new ratio (which is all the core `Message::PaneResized` carries).
#[derive(Debug)]
struct FnecGui {
    state: AppState,
    panes: pane_grid::State<Pane>,
    main_split: Split,
}

impl Default for FnecGui {
    fn default() -> Self {
        let (mut panes, main) = pane_grid::State::new(Pane::Main);
        let (_, split) = panes
            .split(Axis::Vertical, main, Pane::Viewport)
            .expect("initial pane split");
        // Give the controls pane a bit less than half by default.
        panes.resize(split, 0.42);
        Self {
            state: AppState::default(),
            panes,
            main_split: split,
        }
    }
}

impl FnecGui {
    fn update(&mut self, message: Message) -> Task<Message> {
        let spawn_solve = matches!(message, Message::Solve);
        let spawn_sweep = matches!(message, Message::RunSweep);
        let spawn_pattern = matches!(message, Message::RunPattern);
        let spawn_currents = matches!(message, Message::RunCurrents);
        let spawn_geometry = matches!(message, Message::LoadGeometry);
        let spawn_currents_3d = matches!(message, Message::LoadCurrents);
        let spawn_pattern_3d = matches!(message, Message::LoadPattern3d);
        // Pane resize is an iced-layout concern handled here (not in AppState).
        if let Message::PaneResized(ratio) = message {
            self.panes.resize(self.main_split, ratio);
        }
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
        } else if spawn_geometry {
            let path = PathBuf::from(self.state.deck_path.clone());
            let vars: Option<String> = if self.state.vars_path.is_empty() {
                None
            } else {
                Some(self.state.vars_path.clone())
            };
            Task::perform(
                async move { load_geometry_path(&path, vars.as_deref()) },
                Message::GeometryLoaded,
            )
        } else if spawn_currents_3d {
            let path = PathBuf::from(self.state.deck_path.clone());
            let vars: Option<String> = if self.state.vars_path.is_empty() {
                None
            } else {
                Some(self.state.vars_path.clone())
            };
            Task::perform(
                async move { load_currents_path(&path, vars.as_deref()) },
                Message::CurrentsSolved,
            )
        } else if spawn_pattern_3d {
            let path = PathBuf::from(self.state.deck_path.clone());
            let vars: Option<String> = if self.state.vars_path.is_empty() {
                None
            } else {
                Some(self.state.vars_path.clone())
            };
            Task::perform(
                async move { pattern_grid_path(&path, vars.as_deref()) },
                Message::Pattern3dComplete,
            )
        } else {
            Task::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // GUI-CHK-006: xnec2c-style single window — a resizable split with the
        // controls/results on the left and the always-visible 3-D viewport right.
        let grid = pane_grid::PaneGrid::new(&self.panes, |_id, kind, _maximized| {
            let body: Element<Message> = match kind {
                Pane::Main => self.main_pane(),
                Pane::Viewport => self.viewport_view(),
            };
            pane_grid::Content::new(container(body).padding(6))
        })
        .on_resize(8, |e| Message::PaneResized(e.ratio))
        .spacing(6)
        .width(Length::Fill)
        .height(Length::Fill);

        container(grid).padding(6).into()
    }

    /// The left workbench pane: deck inputs, tab bar, and the active result table.
    fn main_pane(&self) -> Element<'_, Message> {
        let tab = |label: &str, on: ActiveTab| {
            let caption = if self.state.active_tab == on {
                format!("[ {label} ]")
            } else {
                format!("  {label}  ")
            };
            button(text(caption)).on_press(Message::TabSelected(on))
        };
        let tab_bar = row![
            tab("Solve", ActiveTab::Solve),
            tab("Sweep", ActiveTab::Sweep),
            tab("Pattern", ActiveTab::Pattern),
            tab("Currents", ActiveTab::Currents),
        ]
        .spacing(4);

        let path_row = row![
            text("Deck file:").width(Length::Fixed(80.0)),
            text_input("Path to .nec file…", &self.state.deck_path)
                .on_input(Message::DeckPathChanged)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

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

        let tab_content: Element<Message> = match self.state.active_tab {
            ActiveTab::Solve => self.solve_view(),
            ActiveTab::Sweep => self.sweep_view(),
            ActiveTab::Pattern => self.pattern_view(),
            ActiveTab::Currents => self.currents_view(),
            // The viewport is its own pane now; this tab is unreachable via UI.
            ActiveTab::Viewport => self.solve_view(),
        };

        scrollable(
            column![tab_bar, path_row, vars_row, tab_content]
                .spacing(12)
                .padding(12),
        )
        .into()
    }

    // ── Single-frequency solve view ──────────────────────────────────────
    /// GUI-CHK-001: the GPU 3-D viewport. Phase 0 renders a shader-widget spike
    /// (a triangle) to prove the iced-0.13 custom-wgpu integration; later phases
    /// replace it with the wire geometry, currents, and pattern lobe.
    fn viewport_view(&self) -> Element<'_, Message> {
        let load_btn = if self.state.deck_path.is_empty() {
            button("Load geometry")
        } else {
            button("Load geometry").on_press(Message::LoadGeometry)
        };
        let status = text(if self.state.viewport.status.is_empty() {
            "Load a deck's geometry to view it in 3-D (wires, axes, ground grid).".to_string()
        } else {
            self.state.viewport.status.clone()
        });
        let reset_btn = if self.state.viewport.fit_bounds.is_some() {
            button("Reset view").on_press(Message::Viewport(ViewportMsg::ResetView))
        } else {
            button("Reset view")
        };
        let currents_btn = if self.state.deck_path.is_empty() {
            button("Solve currents")
        } else {
            button("Solve currents").on_press(Message::LoadCurrents)
        };
        let currents_toggle = checkbox("Color by |I|", self.state.viewport.show_currents)
            .on_toggle(Message::ToggleCurrents);
        let pattern_btn = if self.state.deck_path.is_empty() {
            button("Solve pattern")
        } else {
            button("Solve pattern").on_press(Message::LoadPattern3d)
        };
        let pattern_toggle = checkbox("Show pattern", self.state.viewport.show_pattern)
            .on_toggle(Message::TogglePattern);
        let controls = row![
            load_btn,
            currents_btn,
            currents_toggle,
            pattern_btn,
            pattern_toggle,
            reset_btn,
            status,
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);
        let hint: Element<Message> = match self.state.viewport.current_range_ma() {
            Some((lo, hi)) if self.state.viewport.show_currents => text(format!(
                "|I| legend: cold {lo:.2} mA  →  hot {hi:.2} mA   · drag orbit · wheel zoom · middle-drag pan"
            ))
            .into(),
            _ => text("· drag = orbit · wheel = zoom · middle/right-drag = pan").into(),
        };
        let scene = shader(viewport::Scene::new(&self.state.viewport))
            .width(Length::Fill)
            .height(Length::Fill);
        column![
            controls,
            hint,
            container(scene).width(Length::Fill).height(Length::Fill)
        ]
        .spacing(8)
        .into()
    }

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
    row![
        text(format!("{:5.1}", r.theta_deg)).width(Length::Fixed(70.0)),
        text(format!("{:+7.2}", r.gain_dbi)).width(Length::Fixed(90.0)),
        magnitude_bar(r.bar_width_frac as f32),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center)
    .into()
}

/// A proportional bar drawn as a real widget (iced `progress_bar`), replacing the
/// old Unicode block-character bars that rendered as tofu in the default font.
fn magnitude_bar(frac: f32) -> Element<'static, Message> {
    container(
        progress_bar(0.0..=1.0, frac.clamp(0.0, 1.0))
            .width(Length::Fill)
            .height(Length::Fixed(14.0)),
    )
    .width(Length::Fill)
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
    row![
        text(format!("{:4}", b.seg_idx)).width(Length::Fixed(50.0)),
        text(format!("{:8.4}", b.current_mag_ma)).width(Length::Fixed(90.0)),
        magnitude_bar(b.bar_width_frac as f32),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center)
    .into()
}
