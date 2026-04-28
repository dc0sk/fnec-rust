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

/// GE — Geometry end card.
///
/// Marks the end of geometry input. The optional ground-reflection integer
/// field is preserved for diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct GeCard {
    /// Optional GE ground-reflection flag.
    ///
    /// `0` disables geometry reflection. Non-zero values request reflected
    /// geometry handling that is currently deferred in Phase 1.
    pub ground_reflection_flag: i32,
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
/// LD — Load card.
///
/// Applies a lumped or distributed impedance load to one or more segments.
///
/// NEC load types implemented in Phase 1:
///
/// | I1 | Type                       | F1       | F2        | F3          |
/// |----|----------------------------|----------|-----------|-------------|
/// |  0 | Series RLC (lumped)        | R (Ω)    | L (H)     | C (F)       |
/// |  4 | Series impedance Z=R+jX   | R (Ω)    | X (Ω)     | —           |
/// |  5 | Wire conductivity (dist.)  | σ (S/m)  | —         | —           |
///
/// All other load types are stored verbatim and handled at solve time.
#[derive(Debug, Clone, PartialEq)]
pub struct LdCard {
    /// Load type (0–5; see table above).
    pub load_type: i32,
    /// Wire tag number.  0 = apply to all tags.
    pub tag: u32,
    /// First segment number within the tag.  0 = all segments.
    pub seg_first: u32,
    /// Last segment number within the tag.  0 = same as `seg_first`.
    pub seg_last: u32,
    /// F1: resistance (Ω), conductance, or conductivity depending on type.
    pub f1: f64,
    /// F2: inductance (H) or reactance (Ω) depending on type.
    pub f2: f64,
    /// F3: capacitance (F).  0.0 means no capacitor term.
    pub f3: f64,
}

/// GM — Geometry move card.
///
/// Translates and/or rotates a range of wire tags (or all wires).
/// Supports either in-place transformation (`tag_increment = 0`) or appending
/// one transformed copy with incremented tags (`tag_increment > 0`).
///
/// NEC field mapping:
///   I1 = tag number increment between copies (0 = move in place)
///   I2 = tag number of the last wire to move (0 = all wires so far)
///   F1–F3 = rotation angles about x, y, z axes (degrees)
///   F4–F6 = translation (metres) in x, y, z
///   F7    = starting tag for the range to move (0 = all)
#[derive(Debug, Clone, PartialEq)]
pub struct GmCard {
    /// Tag increment for the appended transformed copy (0 = transform in place).
    pub tag_increment: u32,
    /// Last tag in the affected range (0 = all wires defined so far).
    pub last_tag: u32,
    /// Rotation about x-axis, degrees.
    pub rot_x_deg: f64,
    /// Rotation about y-axis, degrees.
    pub rot_y_deg: f64,
    /// Rotation about z-axis, degrees.
    pub rot_z_deg: f64,
    /// Translation along x, metres.
    pub translate_x: f64,
    /// Translation along y, metres.
    pub translate_y: f64,
    /// Translation along z, metres.
    pub translate_z: f64,
    /// Starting tag for the affected range (0 = all wires defined so far).
    pub first_tag: u32,
}

/// GR — Geometry repeat card.
///
/// Repeats existing wires by successive rotation about the z-axis.
///
/// NEC field mapping:
///   I1 = tag increment between copies
///   I2 = number of times to repeat (additional copies, not including original)
///   F1 = rotation angle per step (degrees, about z-axis)
#[derive(Debug, Clone, PartialEq)]
pub struct GrCard {
    /// Tag number increment between successive copies.
    pub tag_increment: u32,
    /// Number of additional copies to create (0 = no-op).
    pub count: u32,
    /// Rotation angle per copy, degrees about z-axis.
    pub angle_deg: f64,
}

/// TL — Transmission line card.
///
/// Connects two segments with a transmission line, with optional attenuation and
/// velocity factor.  Used to model transmission lines (e.g., antenna feeds, coupled
/// elements) as circuit connections.
///
/// NEC field mapping (TL I1 I2 I3 I4 I5 I6 F1 F2 F3):
///   I1–I4: Define segment locations (tag, segment number pairs)
///   I5: Number of transmission-line segments (for stepped models; typically 1)
///   I6: Transmission-line type (0 = lossless; non-zero flags lossy/complex models)
///   F1: Characteristic impedance (Ω)
///   F2: Transmission-line length (m)
///   F3: Angle (°) for lossy models; velocity factor (ratio) for lossless
#[derive(Debug, Clone, PartialEq)]
pub struct TlCard {
    /// Tag number of the first segment to connect.
    pub tag1: u32,
    /// Segment number within tag1 (0 = all segments in tag).
    pub segment1: u32,
    /// Tag number of the second segment to connect.
    pub tag2: u32,
    /// Segment number within tag2 (0 = all segments in tag).
    pub segment2: u32,
    /// Number of transmission-line segments in the model (typically 1).
    pub num_segments: u32,
    /// Transmission-line type: 0 = lossless, non-zero = lossy/complex.
    pub tl_type: u32,
    /// Characteristic impedance (Ω).
    pub z0: f64,
    /// Transmission-line length (m).
    pub length: f64,
    /// Angle (°) for lossy models or velocity factor (ratio) for lossless.
    pub f3: f64,
}

/// PT — Transmission-line source card.
///
/// PT semantics are not yet implemented in the solver path. Phase 2 currently
/// preserves PT fields to improve deck portability and enable explicit runtime
/// deferred-support warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct PtCard {
    /// Raw PT fields captured after the mnemonic.
    pub raw_fields: Vec<String>,
}

/// NT — Network definition card.
///
/// NT semantics are not yet implemented in the solver path. Phase 2 currently
/// preserves NT fields to improve deck portability and enable explicit runtime
/// deferred-support warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct NtCard {
    /// Raw NT fields captured after the mnemonic.
    pub raw_fields: Vec<String>,
}

/// EN — End-of-data card.  Signals the end of a NEC deck.
#[derive(Debug, Clone, PartialEq)]
pub struct EnCard;

/// All supported card variants.
#[derive(Debug, Clone, PartialEq)]
pub enum Card {
    Comment(CommentCard),
    Gw(GwCard),
    Gm(GmCard),
    Gr(GrCard),
    Ge(GeCard),
    Gn(GnCard),
    Ld(LdCard),
    Tl(TlCard),
    Pt(PtCard),
    Nt(NtCard),
    Ex(ExCard),
    Fr(FrCard),
    Rp(RpCard),
    En(EnCard),
}
