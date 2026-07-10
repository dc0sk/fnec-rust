// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! fnec-gui — iced desktop frontend for the fnec antenna-modelling engine.
//!
//! Run with:
//! ```text
//! cargo run -p nec-gui
//! ```

mod sweep_plot;
mod viewport;

use iced::keyboard::{Key, Modifiers};
use iced::widget::pane_grid::{Axis, Split};
use iced::widget::{
    button, canvas, checkbox, column, container, pane_grid, pick_list, progress_bar, row,
    scrollable, shader, slider, text, text_input,
};
use iced::{Border, Element, Length, Subscription, Task, Theme};
use nec_gui::app_state::{
    ActiveTab, AppState, CurrentsPhase, Message, PatternPhase, SolvePhase, SweepPhase,
    SweepSortCol, ViewportMsg,
};
use nec_gui::model_doc::{ControlEdit, ControlKind, PostSlot, WireField, WireRow};
use nec_gui::plot::PlotMetric;
use nec_gui::session::Session;
use nec_gui::solve::{
    current_distribution_deck_path, load_currents_path, load_geometry_path, load_model_doc_path,
    pattern_grid_path, pattern_slice_deck_path, read_deck_text, solve_deck_path, solve_deck_str,
    SolveResult, SweepJob, SweepPoint,
};
use std::path::PathBuf;

fn main() -> iced::Result {
    iced::application("fnec-gui — Antenna Modeler", FnecGui::update, FnecGui::view)
        .subscription(FnecGui::subscription)
        .theme(|_| Theme::Dark)
        .run()
}

/// Map a Ctrl/Cmd-modified key press to an editor undo/redo message.
///
/// `Ctrl+Z` undoes, `Ctrl+Shift+Z` / `Ctrl+Y` redoes. Only wired while the Edit
/// tab is active (see [`FnecGui::subscription`]), so it never shadows typing in
/// the other tabs.
fn editor_hotkey(key: Key, mods: Modifiers) -> Option<Message> {
    if !mods.command() {
        return None;
    }
    match key.as_ref() {
        Key::Character(c) if c.eq_ignore_ascii_case("z") => Some(if mods.shift() {
            Message::EditRedo
        } else {
            Message::EditUndo
        }),
        Key::Character(c) if c.eq_ignore_ascii_case("y") => Some(Message::EditRedo),
        _ => None,
    }
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
        // Restore the last session (deck/vars paths, sweep range, camera, view
        // options) if one was saved; otherwise start fresh.
        let mut state = AppState::default();
        if let Some(session) = Session::load() {
            session.apply_to(&mut state);
        }
        Self {
            state,
            panes,
            main_split: split,
        }
    }
}

impl FnecGui {
    /// Listen for undo/redo hotkeys, but only while the wire editor is open.
    fn subscription(&self) -> Subscription<Message> {
        if self.state.active_tab == ActiveTab::Editor {
            iced::keyboard::on_key_press(editor_hotkey)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let spawn_solve = matches!(message, Message::Solve);
        let spawn_sweep = matches!(message, Message::RunSweep);
        let spawn_pattern = matches!(message, Message::RunPattern);
        let spawn_currents = matches!(message, Message::RunCurrents);
        let spawn_geometry = matches!(message, Message::LoadGeometry);
        let spawn_currents_3d = matches!(message, Message::LoadCurrents);
        let spawn_pattern_3d = matches!(message, Message::LoadPattern3d);
        let spawn_edit_load = matches!(message, Message::EditDeckLoad);
        let spawn_save = matches!(message, Message::SaveDeck);
        let spawn_apply_solve = matches!(message, Message::EditApplySolve);
        // Settings changes worth persisting to the session file.
        let persist = matches!(
            message,
            Message::DeckPathChanged(_)
                | Message::VarsPathChanged(_)
                | Message::SweepStartChanged(_)
                | Message::SweepEndChanged(_)
                | Message::SweepStepChanged(_)
                | Message::SweepMetricSelected(_)
                | Message::ToggleAxes(_)
                | Message::ToggleGrid(_)
                | Message::Viewport(ViewportMsg::ResetView)
        );
        // Pane resize is an iced-layout concern handled here (not in AppState).
        if let Message::PaneResized(ratio) = message {
            self.panes.resize(self.main_split, ratio);
        }
        self.state.apply(&message);

        if persist {
            // Fails soft: a missing config dir just skips persistence.
            let _ = Session::from_state(&self.state).save();
        }

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
                    // Read + substitute now (fast, surfaces file errors early), then
                    // stream the per-frequency solves so the chart fills in live.
                    match read_deck_text(&path, vars.as_deref()) {
                        Ok(deck_text) => Task::run(
                            iced::stream::channel(64, move |mut output| async move {
                                use iced::futures::SinkExt;
                                match SweepJob::prepare(&deck_text, start, end, step) {
                                    Ok(job) => {
                                        for &f in job.freqs_mhz() {
                                            match job.solve_at(f) {
                                                Ok(pt) => {
                                                    let _ = output
                                                        .send(Message::SweepPointComputed(pt))
                                                        .await;
                                                }
                                                Err(e) => {
                                                    let _ = output
                                                        .send(Message::SweepComplete(Err(e)))
                                                        .await;
                                                    return;
                                                }
                                            }
                                        }
                                        let _ = output.send(Message::SweepStreamDone).await;
                                    }
                                    Err(e) => {
                                        let _ = output.send(Message::SweepComplete(Err(e))).await;
                                    }
                                }
                            }),
                            |m| m,
                        ),
                        Err(e) => {
                            self.state.apply(&Message::SweepComplete(Err(e)));
                            Task::none()
                        }
                    }
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
        } else if spawn_edit_load {
            let path = PathBuf::from(self.state.deck_path.clone());
            let vars: Option<String> = if self.state.vars_path.is_empty() {
                None
            } else {
                Some(self.state.vars_path.clone())
            };
            Task::perform(
                async move { load_model_doc_path(&path, vars.as_deref()) },
                Message::EditDeckLoaded,
            )
        } else if spawn_save {
            // Render the edited deck and write it back over the loaded path.
            let path = self.state.deck_path.clone();
            match self.state.editor.doc.to_deck_string() {
                Ok(text) => Task::perform(
                    async move {
                        match std::fs::write(&path, &text) {
                            Ok(()) => Ok(path),
                            Err(e) => Err(e.to_string()),
                        }
                    },
                    Message::DeckSaved,
                ),
                Err(msg) => {
                    self.state.apply(&Message::DeckSaved(Err(msg)));
                    Task::none()
                }
            }
        } else if spawn_apply_solve {
            // Solve the edited in-memory deck (apply() already validated it and set
            // the Solving phase; on an invalid deck it recorded the error instead).
            match self.state.editor.doc.to_deck_string() {
                Ok(text) => {
                    Task::perform(async move { solve_deck_str(&text) }, Message::SolveComplete)
                }
                Err(_) => Task::none(),
            }
        } else if matches!(message, Message::BrowseDeck) {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("NEC deck", &["nec", "txt"])
                .pick_file()
            {
                self.state
                    .apply(&Message::DeckPathChanged(p.to_string_lossy().into_owned()));
                let _ = Session::from_state(&self.state).save();
            }
            Task::none()
        } else if matches!(message, Message::BrowseVars) {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Vars file", &["toml", "json"])
                .pick_file()
            {
                self.state
                    .apply(&Message::VarsPathChanged(p.to_string_lossy().into_owned()));
                let _ = Session::from_state(&self.state).save();
            }
            Task::none()
        } else if matches!(message, Message::BrowseSaveDeck) {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("NEC deck", &["nec", "txt"])
                .set_file_name("antenna.nec")
                .save_file()
            {
                let path = p.to_string_lossy().into_owned();
                let saved = match self.state.editor.doc.to_deck_string() {
                    Ok(text) => std::fs::write(&path, text)
                        .map(|()| path)
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                };
                self.state.apply(&Message::DeckSaved(saved));
            }
            Task::none()
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
            // A bordered, filled container makes the inter-pane gap read as a
            // visible divider (the pane_grid split itself only highlights on hover).
            pane_grid::Content::new(container(body).padding(6).style(pane_container_style))
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
            tab("Edit", ActiveTab::Editor),
        ]
        .spacing(4);

        let path_row = row![
            text("Deck file:").width(Length::Fixed(80.0)),
            text_input("Path to .nec file…", &self.state.deck_path)
                .on_input(Message::DeckPathChanged)
                .width(Length::Fill),
            button(text("Browse…")).on_press(Message::BrowseDeck),
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
            button(text("Browse…")).on_press(Message::BrowseVars),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let tab_content: Element<Message> = match self.state.active_tab {
            ActiveTab::Solve => self.solve_view(),
            ActiveTab::Sweep => self.sweep_view(),
            ActiveTab::Pattern => self.pattern_view(),
            ActiveTab::Currents => self.currents_view(),
            ActiveTab::Editor => self.editor_view(),
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
        let axes_toggle = checkbox("Axes", self.state.viewport.scene_opts.show_axes)
            .on_toggle(Message::ToggleAxes);
        let grid_toggle = checkbox("Grid", self.state.viewport.scene_opts.show_grid)
            .on_toggle(Message::ToggleGrid);
        // Two shorter control rows instead of one long one — a single row of every
        // button + the status text is wider than the pane and forces the whole
        // window to overflow (iced rows do not wrap).
        let geo_controls = row![load_btn, currents_btn, currents_toggle]
            .spacing(10)
            .align_y(iced::Alignment::Center);
        let view_controls = row![
            pattern_btn,
            pattern_toggle,
            axes_toggle,
            grid_toggle,
            reset_btn
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        // Long free-form text must be Fill-width so it wraps to the pane instead of
        // widening it (text defaults to Shrink = single unbroken line).
        let status = status.width(Length::Fill);
        let hint: Element<Message> = match self.state.viewport.current_range_ma() {
            Some((lo, hi)) if self.state.viewport.show_currents => text(format!(
                "|I| legend: cold {lo:.2} mA → hot {hi:.2} mA   · drag orbit · wheel zoom · middle-drag pan"
            ))
            .width(Length::Fill)
            .into(),
            _ => text("· drag = orbit · wheel = zoom · middle/right-drag = pan")
                .width(Length::Fill)
                .into(),
        };
        let scene = shader(viewport::Scene::new(&self.state.viewport))
            .width(Length::Fill)
            .height(Length::Fill);
        column![
            geo_controls,
            view_controls,
            status,
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
            SweepPhase::Streaming(_) | SweepPhase::Done(_) => self.sweep_chart_and_table(),
            _ => text("").into(),
        };

        column![freq_inputs, run_btn, status, result_section]
            .spacing(8)
            .into()
    }

    /// The sweep chart (SWR/|Z| canvas + metric picker + frequency cursor) above
    /// the sortable result table (GUI-CHK-009).
    fn sweep_chart_and_table(&self) -> Element<'_, Message> {
        let metric_row = row![
            text("Chart:"),
            pick_list(
                &PlotMetric::ALL[..],
                Some(self.state.sweep_metric),
                Message::SweepMetricSelected,
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let chart = canvas(sweep_plot::SweepPlot {
            points: self.state.sweep_points().to_vec(),
            metric: self.state.sweep_metric,
            cursor_frac: self.state.sweep_cursor,
        })
        .width(Length::Fill)
        .height(Length::Fixed(220.0));

        let cursor_readout = match self.state.sweep_cursor_point() {
            Some(p) => {
                let swr = nec_gui::plot::swr(p.z_re, p.z_im, 50.0);
                let zmag = nec_gui::plot::z_mag(p.z_re, p.z_im);
                text(format!(
                    "cursor: {:.3} MHz   Z = {:.2} {} j{:.2} Ω   |Z| = {:.1} Ω   SWR = {}",
                    p.freq_mhz,
                    p.z_re,
                    if p.z_im < 0.0 { "-" } else { "+" },
                    p.z_im.abs(),
                    zmag,
                    if swr.is_finite() {
                        format!("{swr:.2}")
                    } else {
                        "∞".to_string()
                    }
                ))
                .width(Length::Fill)
            }
            None => text("").width(Length::Fill),
        };

        let cursor_slider = slider(
            0.0..=1.0,
            self.state.sweep_cursor,
            Message::SweepCursorChanged,
        )
        .step(0.001_f32);

        column![
            metric_row,
            chart,
            cursor_readout,
            cursor_slider,
            sweep_result_table(self),
        ]
        .spacing(8)
        .into()
    }
}

// ── Styling ───────────────────────────────────────────────────────────────────

/// Pane background + border. The 1-px border on each pane, combined with the
/// pane_grid spacing, gives a divider that is always visible (the split handle
/// itself only highlights while the cursor rests on it).
fn pane_container_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(palette.background.weak.color.into()),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

// ── Wire-editor row widgets (GUI-CHK-007) ─────────────────────────────────────

/// Column width for the integer fields (tag, segments).
const W_INT: Length = Length::Fixed(52.0);
/// Column width for the coordinate/radius fields.
const W_COORD: Length = Length::Fixed(74.0);
/// Column width for the delete button.
const W_DEL: Length = Length::Fixed(44.0);

/// One field text box in a wire row. Borrows the field string, so the returned
/// element's lifetime is tied to the [`WireRow`] (the row can't be `'static`).
fn wire_cell(row: usize, field: WireField, value: &str, width: Length) -> Element<'_, Message> {
    text_input("", value)
        .on_input(move |v| Message::EditWireField {
            row,
            field,
            value: v,
        })
        .width(width)
        .into()
}

/// One editable wire row: nine text boxes + a delete button.
fn wire_edit_row(row: usize, w: &WireRow) -> Element<'_, Message> {
    iced::widget::row![
        wire_cell(row, WireField::Tag, &w.tag, W_INT),
        wire_cell(row, WireField::Segments, &w.segments, W_INT),
        wire_cell(row, WireField::X1, &w.x1, W_COORD),
        wire_cell(row, WireField::Y1, &w.y1, W_COORD),
        wire_cell(row, WireField::Z1, &w.z1, W_COORD),
        wire_cell(row, WireField::X2, &w.x2, W_COORD),
        wire_cell(row, WireField::Y2, &w.y2, W_COORD),
        wire_cell(row, WireField::Z2, &w.z2, W_COORD),
        wire_cell(row, WireField::Radius, &w.radius, W_COORD),
        button(text("Del"))
            .on_press(Message::EditWireDelete(row))
            .width(W_DEL),
    ]
    .spacing(4)
    .into()
}

// ── Control-card editors (GUI-CHK-008) ────────────────────────────────────────

/// Width for the mnemonic label at the start of a control row.
const W_MNEMONIC: Length = Length::Fixed(34.0);

/// A control-card text field; `make` is the [`ControlEdit`] tuple-variant
/// constructor for this field (e.g. `ControlEdit::ExVr`).
fn ctrl_cell(
    slot: usize,
    value: &str,
    width: Length,
    make: fn(String) -> ControlEdit,
) -> Element<'_, Message> {
    text_input("", value)
        .on_input(move |v| Message::EditControl {
            slot,
            edit: make(v),
        })
        .width(width)
        .into()
}

/// A labelled control-card text field (`label: [box]`).
fn ctrl_field<'a>(
    slot: usize,
    label: &'static str,
    value: &'a str,
    width: Length,
    make: fn(String) -> ControlEdit,
) -> Element<'a, Message> {
    row![text(label), ctrl_cell(slot, value, width, make)]
        .spacing(3)
        .align_y(iced::Alignment::Center)
        .into()
}

/// A ground-type pick-list choice (`GN I1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GroundChoice(i32);
impl std::fmt::Display for GroundChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.0 {
            -1 => "-1 none",
            0 => "0 reflection",
            1 => "1 perfect",
            2 => "2 finite",
            _ => "?",
        };
        f.write_str(s)
    }
}
const GROUND_CHOICES: [GroundChoice; 4] = [
    GroundChoice(-1),
    GroundChoice(0),
    GroundChoice(1),
    GroundChoice(2),
];

/// An `LD` load-type pick-list choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoadChoice(i32);
impl std::fmt::Display for LoadChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.0 {
            0 => "0 series RLC",
            1 => "1 parallel RLC",
            2 => "2 series (per-m)",
            3 => "3 parallel (per-m)",
            4 => "4 impedance Z",
            5 => "5 wire conductivity",
            _ => "?",
        };
        f.write_str(s)
    }
}
const LOAD_CHOICES: [LoadChoice; 6] = [
    LoadChoice(0),
    LoadChoice(1),
    LoadChoice(2),
    LoadChoice(3),
    LoadChoice(4),
    LoadChoice(5),
];

/// An `FR` step-type pick-list choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StepChoice(u32);
impl std::fmt::Display for StepChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0 == 0 {
            "0 linear"
        } else {
            "1 multiplicative"
        })
    }
}
const STEP_CHOICES: [StepChoice; 2] = [StepChoice(0), StepChoice(1)];

/// Build an inline editor for one post slot, or `None` for a preserved card that
/// has no editor (comments, GE, RP, EN, …).
fn control_editor_row(slot: usize, s: &PostSlot) -> Option<Element<'_, Message>> {
    let r: Element<Message> = match s {
        PostSlot::Ex(ex) => row![
            text("EX").width(W_MNEMONIC),
            ctrl_field(slot, "type", &ex.kind, W_INT, ControlEdit::ExKind),
            ctrl_field(slot, "tag", &ex.tag, W_INT, ControlEdit::ExTag),
            ctrl_field(slot, "seg", &ex.segment, W_INT, ControlEdit::ExSegment),
            ctrl_field(slot, "Vr", &ex.vr, W_COORD, ControlEdit::ExVr),
            ctrl_field(slot, "Vi", &ex.vi, W_COORD, ControlEdit::ExVi),
            control_del_button(slot),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into(),
        PostSlot::Gn(gn) => row![
            text("GN").width(W_MNEMONIC),
            pick_list(
                GROUND_CHOICES,
                Some(GroundChoice(gn.ground_type)),
                move |c| {
                    Message::EditControl {
                        slot,
                        edit: ControlEdit::GnType(c.0),
                    }
                }
            ),
            ctrl_field(slot, "εr", &gn.eps_r, W_COORD, ControlEdit::GnEps),
            ctrl_field(slot, "σ", &gn.sigma, W_COORD, ControlEdit::GnSigma),
            control_del_button(slot),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into(),
        PostSlot::Ld(ld) => row![
            text("LD").width(W_MNEMONIC),
            pick_list(LOAD_CHOICES, Some(LoadChoice(ld.load_type)), move |c| {
                Message::EditControl {
                    slot,
                    edit: ControlEdit::LdType(c.0),
                }
            }),
            ctrl_field(slot, "tag", &ld.tag, W_INT, ControlEdit::LdTag),
            ctrl_field(slot, "segₐ", &ld.seg_first, W_INT, ControlEdit::LdSegFirst),
            ctrl_field(slot, "segᵦ", &ld.seg_last, W_INT, ControlEdit::LdSegLast),
            ctrl_field(slot, "F1", &ld.f1, W_COORD, ControlEdit::LdF1),
            ctrl_field(slot, "F2", &ld.f2, W_COORD, ControlEdit::LdF2),
            ctrl_field(slot, "F3", &ld.f3, W_COORD, ControlEdit::LdF3),
            control_del_button(slot),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into(),
        PostSlot::Fr(fr) => row![
            text("FR").width(W_MNEMONIC),
            pick_list(STEP_CHOICES, Some(StepChoice(fr.step_type)), move |c| {
                Message::EditControl {
                    slot,
                    edit: ControlEdit::FrStepType(c.0),
                }
            }),
            ctrl_field(slot, "steps", &fr.steps, W_INT, ControlEdit::FrSteps),
            ctrl_field(
                slot,
                "MHz",
                &fr.frequency_mhz,
                W_COORD,
                ControlEdit::FrFrequency
            ),
            ctrl_field(slot, "Δ", &fr.step_mhz, W_COORD, ControlEdit::FrStep),
            control_del_button(slot),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into(),
        PostSlot::Other(_) => return None,
    };
    Some(r)
}

/// The per-row delete button shared by all control-card editors.
fn control_del_button(slot: usize) -> Element<'static, Message> {
    button(text("Del"))
        .on_press(Message::EditDeleteControl(slot))
        .width(W_DEL)
        .into()
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

    // ── Wire editor (GUI-CHK-007) ─────────────────────────────────────────
    fn editor_view(&self) -> Element<'_, Message> {
        let load_btn = if self.state.deck_path.is_empty() {
            button("Load deck into editor")
        } else {
            button("Load deck into editor").on_press(Message::EditDeckLoad)
        };

        if !self.state.editor.loaded {
            return column![
                load_btn,
                text(
                    "Load a deck to edit its GW wires. Every valid edit previews \
                     live in the 3-D view."
                )
                .width(Length::Fill),
            ]
            .spacing(8)
            .into();
        }

        let header = row![
            text("Tag").width(W_INT),
            text("Segs").width(W_INT),
            text("x1").width(W_COORD),
            text("y1").width(W_COORD),
            text("z1").width(W_COORD),
            text("x2").width(W_COORD),
            text("y2").width(W_COORD),
            text("z2").width(W_COORD),
            text("radius").width(W_COORD),
            text("").width(W_DEL),
        ]
        .spacing(4);

        let mut table = column![header].spacing(3);
        for (i, w) in self.state.editor.doc.wires.iter().enumerate() {
            table = table.push(wire_edit_row(i, w));
        }
        let table = scrollable(table).direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        });

        let add_btn = button("+ Add wire").on_press(Message::EditWireAdd);
        let save_btn = button("Save deck").on_press(Message::SaveDeck);
        let save_as_btn = button("Save as…").on_press(Message::BrowseSaveDeck);
        let undo_btn = if self.state.editor.history.can_undo() {
            button("Undo").on_press(Message::EditUndo)
        } else {
            button("Undo")
        };
        let redo_btn = if self.state.editor.history.can_redo() {
            button("Redo").on_press(Message::EditRedo)
        } else {
            button("Redo")
        };

        let apply_btn = button("Apply + Solve").on_press(Message::EditApplySolve);

        let dirty = if self.state.editor.doc.dirty {
            "  •unsaved"
        } else {
            ""
        };
        let status: Element<Message> = match &self.state.editor.error {
            Some(e) => text(format!("⚠ {e}")).width(Length::Fill).into(),
            None => text(format!(
                "Preview OK — {} wire(s){dirty}",
                self.state.editor.doc.wire_count()
            ))
            .width(Length::Fill)
            .into(),
        };
        let save_status = text(self.state.editor.save_status.clone()).width(Length::Fill);

        // ── Sources & environment (EX/GN/LD/FR editors) ──────────────────────
        let add_bar = row![
            text("Sources & environment"),
            button(text("+ Ground")).on_press(Message::EditAddControl(ControlKind::Gn)),
            button(text("+ Load")).on_press(Message::EditAddControl(ControlKind::Ld)),
            button(text("+ Source")).on_press(Message::EditAddControl(ControlKind::Ex)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        let mut controls = column![add_bar].spacing(4);
        let mut any_control = false;
        for (i, slot) in self.state.editor.doc.post_slots().iter().enumerate() {
            if let Some(editor) = control_editor_row(i, slot) {
                controls = controls.push(editor);
                any_control = true;
            }
        }
        if !any_control {
            controls = controls
                .push(text("(no EX / GN / LD / FR cards in this deck)").width(Length::Fill));
        }
        let controls = scrollable(controls)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .width(Length::Fill);

        let solve_line = text(self.state.status_text()).width(Length::Fill);

        column![
            row![
                load_btn,
                add_btn,
                undo_btn,
                redo_btn,
                save_btn,
                save_as_btn,
                apply_btn
            ]
            .spacing(8),
            table,
            status,
            controls,
            solve_line,
            save_status,
        ]
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
