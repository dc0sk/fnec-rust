// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! GPU 3-D viewport (GUI redesign — see `docs/gui-redesign-plan.md`).
//!
//! `Scene` is the `shader::Program`: it holds the camera and an `Arc` to the
//! built line mesh (from the headless `nec_gui::mesh`), and on each `draw()`
//! bakes the view-projection matrix (using the widget's aspect ratio) into a
//! cheap [`primitive::ScenePrimitive`]. GUI-CHK-002 renders wires + axes + grid
//! with a fixed camera; interaction lands in GUI-CHK-003.

mod primitive;

use std::sync::Arc;

use iced::mouse;
use iced::widget::shader;
use iced::Rectangle;
use nec_gui::app_state::Message;
use nec_gui::camera::Camera;
use nec_gui::mesh::MeshData;

/// The 3-D scene bound to the current viewport state.
#[derive(Debug)]
pub struct Scene {
    camera: Camera,
    mesh: Option<Arc<MeshData>>,
    rev: u64,
}

impl Scene {
    /// Build from the headless viewport state (called each `view()`).
    pub fn new(state: &nec_gui::app_state::ViewportState) -> Self {
        Self {
            camera: state.camera,
            mesh: state.scene.clone(),
            rev: state.scene_rev,
        }
    }
}

impl shader::Program<Message> for Scene {
    type State = ();
    type Primitive = primitive::ScenePrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let aspect = if bounds.height > 0.0 {
            bounds.width / bounds.height
        } else {
            1.0
        };
        primitive::ScenePrimitive {
            view_proj: self.camera.view_proj(aspect).to_cols_array_2d(),
            mesh: self.mesh.clone(),
            rev: self.rev,
        }
    }
}
