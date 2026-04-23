// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

pub mod basis;
pub mod excitation;
pub mod geometry;
pub mod linear;
pub mod matrix;

pub use basis::{ContinuityTransform, SinusoidalTransform};
pub use excitation::{
    build_excitation, build_hallen_rhs, scale_excitation_for_pulse_rhs, ExcitationError, HallenRhs,
};
pub use geometry::{build_geometry, GeometryError, Segment};
pub use linear::{
    solve, solve_hallen, solve_with_continuity_basis, solve_with_sinusoidal_basis, HallenSolution,
    SolveError,
};
pub use matrix::{assemble_pocklington_matrix, assemble_z_matrix, ZMatrix};
