// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! GPU 3-D viewport (GUI redesign — see `docs/gui-redesign-plan.md`).
//!
//! Phase 0 (GUI-CHK-001) is a shader-widget spike: a `shader::Program` that
//! renders a hard-coded triangle into iced's own wgpu frame, proving the
//! iced-0.13 custom-wgpu integration end-to-end before any real geometry lands.

mod primitive;

use iced::mouse;
use iced::widget::shader;
use iced::Rectangle;

use nec_gui::app_state::Message;

/// The 3-D scene. For the Phase-0 spike it is stateless and draws a triangle;
/// later phases carry the camera + mesh handles here.
#[derive(Debug, Default)]
pub struct Scene;

impl shader::Program<Message> for Scene {
    type State = ();
    type Primitive = primitive::TrianglePrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        primitive::TrianglePrimitive
    }
}
