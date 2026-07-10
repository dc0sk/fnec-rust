// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Session persistence (GUI-CHK-010).
//!
//! A small TOML snapshot of the last GUI session — deck/vars paths, the sweep
//! range and chart metric, the camera pose, and the viewport view options — so
//! reopening the app restores where the user left off. The geometry itself is not
//! stored; the deck path is, and the user re-loads it.
//!
//! [`Session::from_state`] / [`Session::apply_to`] convert to and from
//! [`AppState`]; the round-trip is unit-tested. File IO uses the platform config
//! directory and fails soft (a missing or corrupt session is simply ignored).

use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::mesh::SceneOptions;
use crate::plot::PlotMetric;

/// A persisted GUI session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub deck_path: String,
    pub vars_path: String,
    pub sweep_start: String,
    pub sweep_end: String,
    pub sweep_step: String,
    /// Chart metric label ("SWR" or "|Z| (Ω)").
    pub sweep_metric: String,
    pub cam_target: [f32; 3],
    pub cam_distance: f32,
    pub cam_yaw: f32,
    pub cam_pitch: f32,
    pub show_axes: bool,
    pub show_grid: bool,
}

impl Default for Session {
    fn default() -> Self {
        // Mirror AppState::default so a partial/missing file fills sane values.
        Self::from_state(&AppState::default())
    }
}

impl Session {
    /// Capture the persistable slice of the current application state.
    pub fn from_state(state: &AppState) -> Self {
        let cam = &state.viewport.camera;
        Self {
            deck_path: state.deck_path.clone(),
            vars_path: state.vars_path.clone(),
            sweep_start: state.sweep_start.clone(),
            sweep_end: state.sweep_end.clone(),
            sweep_step: state.sweep_step.clone(),
            sweep_metric: state.sweep_metric.label().to_string(),
            cam_target: cam.target.to_array(),
            cam_distance: cam.distance,
            cam_yaw: cam.yaw,
            cam_pitch: cam.pitch,
            show_axes: state.viewport.scene_opts.show_axes,
            show_grid: state.viewport.scene_opts.show_grid,
        }
    }

    /// Restore this session into a fresh application state (paths, sweep inputs,
    /// chart metric, camera pose, view options). Geometry is not restored — the
    /// deck path is set and the user re-loads it.
    pub fn apply_to(&self, state: &mut AppState) {
        state.deck_path = self.deck_path.clone();
        state.vars_path = self.vars_path.clone();
        state.sweep_start = self.sweep_start.clone();
        state.sweep_end = self.sweep_end.clone();
        state.sweep_step = self.sweep_step.clone();
        state.sweep_metric = metric_from_label(&self.sweep_metric);
        let cam = &mut state.viewport.camera;
        cam.target = glam::Vec3::from(self.cam_target);
        cam.distance = self.cam_distance;
        cam.yaw = self.cam_yaw;
        cam.pitch = self.cam_pitch;
        state.viewport.scene_opts = SceneOptions {
            show_axes: self.show_axes,
            show_grid: self.show_grid,
        };
    }

    /// Serialize to TOML text.
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }

    /// Parse from TOML text.
    pub fn from_toml(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| e.to_string())
    }

    /// The on-disk session-file path: `$XDG_CONFIG_HOME/fnec-gui/session.toml`,
    /// falling back to `$HOME/.config/fnec-gui/session.toml`. `None` if neither
    /// environment variable is set.
    pub fn config_path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
            })?;
        Some(base.join("fnec-gui").join("session.toml"))
    }

    /// Write this session to [`Session::config_path`], creating the directory.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or("no config directory (HOME unset)")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, self.to_toml()?).map_err(|e| e.to_string())
    }

    /// Load the persisted session, or `None` if absent/unreadable/corrupt.
    pub fn load() -> Option<Self> {
        let path = Self::config_path()?;
        let text = std::fs::read_to_string(path).ok()?;
        Self::from_toml(&text).ok()
    }
}

/// Map a chart-metric label back to a [`PlotMetric`] (defaults to SWR).
fn metric_from_label(label: &str) -> PlotMetric {
    if label == PlotMetric::ZMag.label() {
        PlotMetric::ZMag
    } else {
        PlotMetric::Swr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::field_reassign_with_default)] // AppState has many fields; mutate a few.
    fn round_trips_through_state_and_toml() {
        let mut state = AppState::default();
        state.deck_path = "antenna.nec".into();
        state.vars_path = "vars.toml".into();
        state.sweep_start = "7.0".into();
        state.sweep_end = "7.3".into();
        state.sweep_step = "0.05".into();
        state.sweep_metric = PlotMetric::ZMag;
        state.viewport.camera.distance = 12.5;
        state.viewport.camera.yaw = 1.23;
        state.viewport.scene_opts.show_axes = false;

        let session = Session::from_state(&state);
        let toml = session.to_toml().expect("serialize");
        let parsed = Session::from_toml(&toml).expect("parse");
        assert_eq!(session, parsed);

        let mut restored = AppState::default();
        parsed.apply_to(&mut restored);
        assert_eq!(restored.deck_path, "antenna.nec");
        assert_eq!(restored.vars_path, "vars.toml");
        assert_eq!(restored.sweep_start, "7.0");
        assert_eq!(restored.sweep_end, "7.3");
        assert_eq!(restored.sweep_step, "0.05");
        assert_eq!(restored.sweep_metric, PlotMetric::ZMag);
        assert!((restored.viewport.camera.distance - 12.5).abs() < 1e-6);
        assert!((restored.viewport.camera.yaw - 1.23).abs() < 1e-6);
        assert!(!restored.viewport.scene_opts.show_axes);
    }

    #[test]
    fn metric_label_maps_both_ways() {
        assert_eq!(metric_from_label("SWR"), PlotMetric::Swr);
        assert_eq!(
            metric_from_label(PlotMetric::ZMag.label()),
            PlotMetric::ZMag
        );
        // Unknown label falls back to SWR.
        assert_eq!(metric_from_label("nonsense"), PlotMetric::Swr);
    }

    #[test]
    fn partial_toml_fills_defaults() {
        // Only a deck path present → other fields take AppState defaults.
        let session = Session::from_toml("deck_path = \"d.nec\"\n").expect("parse");
        assert_eq!(session.deck_path, "d.nec");
        assert_eq!(session.sweep_start, AppState::default().sweep_start);
    }
}
