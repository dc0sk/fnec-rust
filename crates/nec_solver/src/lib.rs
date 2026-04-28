// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

pub mod basis;
pub mod excitation;
pub mod farfield;
pub mod geometry;
pub mod linear;
pub mod loads;
pub mod matrix;
pub mod tl;

pub use basis::{ContinuityTransform, SinusoidalTransform};
pub use excitation::{
    build_excitation, build_excitation_with_options, build_hallen_rhs,
    build_hallen_rhs_with_options, build_hallen_rhs_with_runtime_options,
    scale_excitation_for_pulse_rhs, Ex3NormalizationMode, ExcitationError, HallenRhs,
};
pub use farfield::{compute_radiation_pattern, rp_card_points, FarFieldPoint, FarFieldResult};
pub use geometry::{
    build_geometry, ground_model_from_deck, wire_endpoints_from_segs, GeometryError, GroundModel,
    Segment,
};
pub use linear::{
    solve, solve_hallen, solve_with_continuity_basis, solve_with_continuity_basis_per_wire,
    solve_with_sinusoidal_basis, solve_with_sinusoidal_basis_per_wire, HallenSolution, SolveError,
};
pub use loads::{build_loads, LoadWarning};
pub use matrix::{
    assemble_pocklington_matrix, assemble_z_matrix, assemble_z_matrix_with_ground, ZMatrix,
};
pub use tl::{build_tl_stamps, TlStamp, TlWarning};
