// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Orbit camera for the 3-D viewport (GUI-CHK-002 fit + fixed view; the
//! orbit/zoom/pan interaction lands in GUI-CHK-003).
//!
//! Pure `glam` math, unit-tested headlessly. NEC geometry is **z-up**, so the
//! view matrix uses `Vec3::Z` as the up vector and antennas stand upright.

use glam::{Mat4, Vec3};

/// An orbit ("turntable") camera: it looks at `target` from `distance` away,
/// oriented by `yaw` (about z) and `pitch` (elevation from the xy-plane).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    /// Azimuth about the z-axis, radians.
    pub yaw: f32,
    /// Elevation from the xy-plane, radians (clamped to ±89°).
    pub pitch: f32,
    /// Vertical field of view, radians.
    pub fov_y: f32,
}

impl Default for Camera {
    fn default() -> Self {
        // A three-quarter isometric-ish view.
        Self {
            target: Vec3::ZERO,
            distance: 10.0,
            yaw: -0.9,
            pitch: 0.5,
            fov_y: std::f32::consts::FRAC_PI_4,
        }
    }
}

const PITCH_LIMIT: f32 = 1.552; // ~89°

impl Camera {
    /// Camera eye position in world space (z-up spherical about `target`).
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let dir = Vec3::new(cp * self.yaw.cos(), cp * self.yaw.sin(), self.pitch.sin());
        self.target + dir * self.distance
    }

    /// Combined view-projection matrix for the given viewport aspect ratio.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let far = self.distance * 10.0 + 100.0;
        let near = (self.distance * 0.01).max(0.001);
        let proj = Mat4::perspective_rh(self.fov_y, aspect.max(0.01), near, far);
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Z);
        proj * view
    }

    /// Frame a bounding box: centre the target and back off so the sphere of the
    /// given `radius` fits the vertical field of view (with a small margin).
    pub fn fit(&mut self, center: [f32; 3], radius: f32) {
        self.target = Vec3::from_array(center);
        self.distance = (radius / (self.fov_y * 0.5).sin() * 1.25).max(0.5);
    }

    /// Orbit by screen-space deltas (radians). Pitch is clamped. (GUI-CHK-003.)
    pub fn orbit(&mut self, d_yaw: f32, d_pitch: f32) {
        self.yaw += d_yaw;
        self.pitch = (self.pitch + d_pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Zoom by scroll steps (positive = closer). Distance stays positive.
    pub fn zoom(&mut self, steps: f32) {
        self.distance = (self.distance * 0.88f32.powf(steps)).clamp(0.05, 1.0e6);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_centers_target_and_backs_off() {
        let mut c = Camera::default();
        c.fit([1.0, 2.0, 3.0], 5.0);
        assert_eq!(c.target, Vec3::new(1.0, 2.0, 3.0));
        // Distance grows with radius and exceeds it.
        assert!(
            c.distance > 5.0,
            "camera should sit outside the geometry sphere"
        );
        let mut c2 = Camera::default();
        c2.fit([0.0; 3], 10.0);
        assert!(c2.distance > c.distance, "larger bbox → farther camera");
    }

    #[test]
    fn eye_is_distance_from_target() {
        let c = Camera {
            target: Vec3::new(1.0, 1.0, 1.0),
            distance: 7.0,
            ..Camera::default()
        };
        assert!((c.eye().distance(c.target) - 7.0).abs() < 1e-4);
    }

    #[test]
    fn target_projects_near_screen_center() {
        let mut c = Camera::default();
        c.fit([0.0; 3], 3.0);
        let m = c.view_proj(1.5);
        let clip = m * c.target.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        // The look-at target lands at the center of the frame.
        assert!(ndc.x.abs() < 1e-3 && ndc.y.abs() < 1e-3, "ndc={ndc:?}");
        // ...and in front of the camera (0 < depth < 1 for wgpu clip space).
        assert!((0.0..=1.0).contains(&ndc.z), "depth {} out of range", ndc.z);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut c = Camera::default();
        c.orbit(0.0, 100.0);
        assert!(c.pitch <= PITCH_LIMIT + 1e-6);
        c.orbit(0.0, -100.0);
        assert!(c.pitch >= -PITCH_LIMIT - 1e-6);
    }

    #[test]
    fn zoom_scales_distance_and_stays_positive() {
        let mut c = Camera::default();
        let d0 = c.distance;
        c.zoom(1.0);
        assert!(c.distance < d0, "zoom in should shrink distance");
        c.zoom(-1.0);
        assert!((c.distance - d0).abs() < 1e-3, "zoom out should restore");
        c.zoom(1000.0);
        assert!(c.distance > 0.0);
    }
}
