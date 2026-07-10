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

use iced::advanced::Shell;
use iced::event::Status;
use iced::mouse::{self, Button, ScrollDelta};
use iced::widget::shader;
use iced::{Point, Rectangle};
use nec_gui::app_state::{Message, ViewportMsg};
use nec_gui::camera::Camera;
use nec_gui::mesh::MeshData;

/// Radians of orbit per pixel dragged.
const ORBIT_RAD_PER_PX: f32 = 0.008;

/// Which drag gesture is in progress (widget-local; not in `AppState`).
#[derive(Debug, Clone, Copy)]
enum DragMode {
    Orbit,
    Pan,
}

/// Transient drag bookkeeping held in the shader widget's `Program::State`.
#[derive(Debug, Default)]
pub struct DragState {
    mode: Option<DragMode>,
    last: Option<Point>,
}

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
    type State = DragState;
    type Primitive = primitive::ScenePrimitive;

    fn update(
        &self,
        state: &mut DragState,
        event: shader::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        _shell: &mut Shell<'_, Message>,
    ) -> (Status, Option<Message>) {
        let shader::Event::Mouse(mouse_event) = event else {
            return (Status::Ignored, None);
        };
        match mouse_event {
            mouse::Event::ButtonPressed(button) if cursor.is_over(bounds) => {
                let mode = match button {
                    Button::Left => Some(DragMode::Orbit),
                    Button::Middle | Button::Right => Some(DragMode::Pan),
                    _ => None,
                };
                if let Some(mode) = mode {
                    state.mode = Some(mode);
                    state.last = cursor.position();
                    return (Status::Captured, None);
                }
                (Status::Ignored, None)
            }
            mouse::Event::ButtonReleased(_) => {
                state.mode = None;
                state.last = None;
                (Status::Ignored, None)
            }
            mouse::Event::CursorMoved { position } => {
                if let (Some(mode), Some(last)) = (state.mode, state.last) {
                    let (dx, dy) = (position.x - last.x, position.y - last.y);
                    state.last = Some(position);
                    let msg = match mode {
                        DragMode::Orbit => ViewportMsg::Orbit {
                            d_yaw: -dx * ORBIT_RAD_PER_PX,
                            d_pitch: -dy * ORBIT_RAD_PER_PX,
                        },
                        DragMode::Pan => ViewportMsg::Pan {
                            dx: dx / bounds.width.max(1.0),
                            dy: dy / bounds.height.max(1.0),
                        },
                    };
                    return (Status::Captured, Some(Message::Viewport(msg)));
                }
                (Status::Ignored, None)
            }
            mouse::Event::WheelScrolled { delta } if cursor.is_over(bounds) => {
                let steps = match delta {
                    ScrollDelta::Lines { y, .. } => y,
                    ScrollDelta::Pixels { y, .. } => y / 40.0,
                };
                (
                    Status::Captured,
                    Some(Message::Viewport(ViewportMsg::Zoom(steps))),
                )
            }
            _ => (Status::Ignored, None),
        }
    }

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
