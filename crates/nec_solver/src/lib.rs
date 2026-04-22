// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

pub mod excitation;
pub mod geometry;
pub mod linear;
pub mod matrix;

pub use excitation::{build_excitation, ExcitationError};
pub use geometry::{build_geometry, GeometryError, Segment};
pub use linear::{solve, SolveError};
pub use matrix::{assemble_z_matrix, ZMatrix};
