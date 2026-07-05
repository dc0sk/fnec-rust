// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

pub mod basis;
pub mod excitation;
pub mod farfield;
pub mod geometry;
pub mod linear;
pub mod loads;
pub mod matrix;
pub mod network;
pub mod planewave;
pub mod tl;

pub use basis::{ContinuityTransform, SinusoidalTransform};
pub use excitation::{
    build_current_source_shape, build_excitation, build_hallen_rhs, build_hallen_rhs_paths,
    scale_excitation_for_pulse_rhs, ExcitationError, HallenRhs,
};
pub use farfield::{
    bilinear_interp_gain, compute_radiation_pattern, integrate_radiated_power, near_e_field,
    near_h_field, radiation_efficiency, rp_card_points, FarFieldPoint, FarFieldResult, NearFieldE,
    NearFieldH, NearFieldPoint, RpGainGrid,
};
pub use geometry::{
    build_conductor_paths, build_geometry, detect_wire_junctions, ground_model_from_deck,
    merge_collinear_wire_endpoints, wire_endpoints_from_segs, ConductorPath, GeometryError,
    GroundModel, Segment, WireJunction,
};
pub use linear::{
    solve, solve_hallen, solve_hallen_current_source, solve_hallen_paths, solve_hallen_planewave,
    solve_hallen_planewave_paths, solve_hallen_sinusoidal_basis, solve_with_continuity_basis,
    solve_with_continuity_basis_per_wire, solve_with_sinusoidal_basis,
    solve_with_sinusoidal_basis_per_wire, CurrentSourceSolution, HallenSolution, SolveError,
};
pub use loads::{build_loads, LoadWarning};
pub use matrix::{
    assemble_pocklington_matrix, assemble_z_matrix, assemble_z_matrix_with_ground, ZMatrix,
};
pub use network::{build_nt_stamps, NtStamp, NtWarning};
pub use planewave::{
    build_planewave_hallen, build_planewave_hallen_paths, IncidentPlaneWave, PlaneWaveError,
    PlaneWaveHallen,
};
pub use tl::{build_tl_stamps, TlStamp, TlWarning};
