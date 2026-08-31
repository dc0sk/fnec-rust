// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

pub mod basis;
pub mod current_source;
pub mod excitation;
pub mod farfield;
pub mod feedpoint;
pub mod frequency;
pub mod geometry;
pub mod hallen_session;
pub mod linear;
pub mod loads;
pub mod matrix;
pub mod mpie;
pub mod mpie_session;
pub mod network;
pub mod planewave;
pub mod sommerfeld;
pub mod stamps;
pub mod taper;
pub mod tl;
pub mod validate;

pub use basis::{ContinuityTransform, SinusoidalTransform};
pub use current_source::{solve_current_source_hallen, CurrentSourceError, CurrentSourceFeedpoint};
pub use excitation::{
    build_current_source_shape, build_current_source_shape_paths, build_excitation,
    build_hallen_rhs, build_hallen_rhs_paths, feedpoints, first_delta_gap_feedpoint,
    scale_excitation_for_pulse_rhs, ExcitationError, HallenRhs,
};
pub use farfield::{
    bilinear_interp_gain, compute_radiation_pattern, feedpoint_input_power, gain_correction_db,
    integrate_radiated_power, near_e_field, near_h_field, radiation_efficiency, rp_card_points,
    FarFieldPoint, FarFieldResult, NearFieldE, NearFieldH, NearFieldPoint, RpGainGrid,
};
pub use feedpoint::{
    check_currents_finite, feedpoint_drive_voltage, feedpoint_impedance, FeedpointError,
    NonFiniteCurrents, MIN_FEEDPOINT_CURRENT,
};
pub use frequency::{
    fr_sweeps, frequencies_hz, governing_fr_sweep, is_usable_frequency_mhz, superseded_fr_warnings,
    FrSweep, MAX_FR_POINTS,
};
pub use geometry::{
    build_conductor_paths, build_geometry, classify_unsupported_topology, detect_wire_junctions,
    ground_model_from_deck, merge_collinear_wire_endpoints, wire_endpoints_from_segs,
    ConductorPath, GeometryError, GroundModel, Segment, UnsupportedTopology, WireJunction,
    MAX_SEGMENTS,
};
pub use hallen_session::{
    classify_paths, deck_has_current_source, deck_has_plane_wave, group_paths, hallen_route,
    solve_hallen_planewave_routed, solve_hallen_routed, HallenDrive, HallenRoute, HallenRouted,
    HallenSessionError, PathRoute, ResidualInputs, JUNCTION_TOL_M,
};
pub use linear::{
    solve, solve_hallen, solve_hallen_paths, solve_hallen_planewave, solve_hallen_planewave_paths,
    solve_hallen_sinusoidal_basis, solve_with_continuity_basis,
    solve_with_continuity_basis_per_wire, solve_with_sinusoidal_basis,
    solve_with_sinusoidal_basis_per_wire, CurrentSourceSolution, HallenSolution, SolveError,
};
pub use loads::{add_laplace_loads, build_loads, laplace_impedance, LaplaceLoad, LoadWarning};
pub use matrix::{
    assemble_pocklington_matrix, assemble_z_matrix, assemble_z_matrix_with_ground, ZMatrix,
};
pub use mpie::{
    assemble_free_space_z, assemble_with_ground, feed_node_for_segment, feed_reference_sign,
    geometry_from_segments, segment_currents, segments_for_farfield, solve_mpie,
    solve_mpie_free_space, solve_mpie_ground, straight_wire, MpieError, MpieGeometry, MpieSolution,
    MpieWire,
};
pub use mpie_session::{mpie_unsupported, solve_mpie_session, MpieSessionError, MpieUnsupported};
pub use network::{build_nt_stamps, NtStamp, NtWarning};
pub use planewave::{
    build_planewave_hallen, build_planewave_hallen_paths, IncidentPlaneWave, PlaneWaveError,
    PlaneWaveHallen,
};
pub use stamps::{build_deck_stamps, DeckStamps};
pub use taper::{leeson_equivalent_element, EquivalentElement, TaperSection};
pub use tl::{build_tl_stamps, TlStamp, TlWarning};
