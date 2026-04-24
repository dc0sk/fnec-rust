// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Typed representations of NEC deck cards (input lines).
//!
//! Only the cards required for a minimal Phase-1 dipole simulation are
//! defined here.  Each variant maps 1-to-1 to a NEC card mnemonic.

/// Comment card (CM … CE).
///
/// In a NEC deck every `CM` line contributes one line of free text; the
/// sequence is terminated by a single `CE` line (which may itself carry text).
#[derive(Debug, Clone, PartialEq)]
pub struct CommentCard {
    /// Raw text after the mnemonic, stripped of leading/trailing whitespace.
    pub text: String,
}

/// GW — Wire geometry card.
///
/// Defines a straight-wire segment.  All coordinates are in metres.
#[derive(Debug, Clone, PartialEq)]
pub struct GwCard {
    /// Tag number (wire identifier, ≥ 1).
    pub tag: u32,
    /// Number of segments.
    pub segments: u32,
    /// Start point (x, y, z) in metres.
    pub start: [f64; 3],
    /// End point (x, y, z) in metres.
    pub end: [f64; 3],
    /// Wire radius in metres.
    pub radius: f64,
}

/// EX — Excitation card.
///
/// Phase 1 models voltage-source excitation (type 0). Additional fields are
/// preserved so later phases can implement more EX source families.
#[derive(Debug, Clone, PartialEq)]
pub struct ExCard {
    /// Excitation type (0 = voltage source).
    pub excitation_type: u32,
    /// Tag number of the segment carrying the source.
    pub tag: u32,
    /// Segment number within the tag.
    pub segment: u32,
    /// Integer EX field I4.
    ///
    /// For EX type 0 this is typically unused and often set to 0. For other
    /// source types NEC uses this field for additional source metadata.
    pub i4: u32,
    /// Real part of the voltage (volts).
    pub voltage_real: f64,
    /// Imaginary part of the voltage (volts).
    pub voltage_imag: f64,
}

/// FR — Frequency card.
///
/// Defines the frequency (or sweep) for the simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct FrCard {
    /// Step type: 0 = linear, 1 = multiplicative.
    pub step_type: u32,
    /// Number of frequency steps.
    pub steps: u32,
    /// Starting frequency in MHz.
    pub frequency_mhz: f64,
    /// Step size in MHz (linear) or multiplier (multiplicative).
    pub step_mhz: f64,
}

/// RP — Radiation-pattern request card.
#[derive(Debug, Clone, PartialEq)]
pub struct RpCard {
    /// Output mode (0 = major-lobe, 1 = minor-lobe, etc.).
    pub mode: u32,
    /// Number of theta angles.
    pub n_theta: u32,
    /// Number of phi angles.
    pub n_phi: u32,
    /// Starting theta (degrees).
    pub theta0: f64,
    /// Starting phi (degrees).
    pub phi0: f64,
    /// Theta increment (degrees).
    pub d_theta: f64,
    /// Phi increment (degrees).
    pub d_phi: f64,
}

/// GN — Ground definition card.
///
/// Specifies the electrical ground model below the antenna geometry.
/// Only the first integer field (ground type) is stored in Phase 1.
/// Future phases can extend this struct with conductivity and permittivity.
#[derive(Debug, Clone, PartialEq)]
pub struct GnCard {
    /// Ground type:
    ///   -1 = null ground (equivalent to no GN card)
    ///    0 = reflection coefficient method (approximate; deferred Phase 2)
    ///    1 = perfect electric conductor (image method)
    ///    2 = finite conductivity Sommerfeld/Norton (deferred Phase 2)
    pub ground_type: i32,
}

/// EN — End-of-data card.  Signals the end of a NEC deck.
#[derive(Debug, Clone, PartialEq)]
pub struct EnCard;

/// All supported card variants.
#[derive(Debug, Clone, PartialEq)]
pub enum Card {
    Comment(CommentCard),
    Gw(GwCard),
    Gn(GnCard),
    Ex(ExCard),
    Fr(FrCard),
    Rp(RpCard),
    En(EnCard),
}
