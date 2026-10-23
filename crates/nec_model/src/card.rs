// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Typed representations of NEC deck cards (input lines).
//!
//! Only the cards required for a minimal Phase-1 dipole simulation are
//! defined here.  Each variant maps 1-to-1 to a NEC card mnemonic.

mod field_disposition;
pub use field_disposition::{
    deck_with, field_name, perturbed_line, Disposition, FieldCase, Probe, FIELD_DISPOSITION,
    GEOMETRY_CARDS,
};

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
    /// `GE I1`: whether wire ends on the ground plane are joined to their
    /// images (`1`) or left open (`0`, `-1`), as nec2c's `conect` reads it. It
    /// does not select a ground; the last `GN` card does. `0` when omitted; the
    /// parser refuses values outside `-1..=1` (FND-241).
    pub ground_reflection_flag: i32,
}

/// NEC2-aligned classification of an `EX` excitation type (I1 field).
///
/// fnec follows the canonical NEC2/nec2c numbering (see
/// `docs/ph8-chk-002-plane-wave-excitation.md`): types 1/2/3 are incident
/// plane waves, and the current source lives on type 4. Only [`Self::VoltageSource`]
/// is solved today; the others are recognized (so real 4nec2 decks are not
/// misclassified) but fail fast until their runtime semantics land in Phase 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcitationKind {
    /// Type 0 — applied-field / series voltage source.
    VoltageSource,
    /// Type 1 — incident plane wave, linear polarization.
    PlaneWaveLinear,
    /// Type 2 — incident plane wave, right-hand elliptic polarization.
    PlaneWaveRightElliptic,
    /// Type 3 — incident plane wave, left-hand elliptic polarization.
    PlaneWaveLeftElliptic,
    /// Type 4 — current source (elementary current source / applied current).
    CurrentSource,
    /// Type 5 — voltage source with current-slope discontinuity.
    VoltageSourceCurrentSlope,
    /// Any I1 value outside the canonical NEC2 0–5 range.
    Unknown(u32),
}

/// What an `EX` card means when the question is "which segment is the feedpoint".
///
/// See [`ExcitationKind::feedpoint_role`] for why this is a classification rather
/// than a predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedpointRole {
    /// Types 0 and 5 — a driven voltage source fnec models as an applied-field
    /// delta gap. `Z = V / I` at the named segment.
    DeltaGap,
    /// Type 4 — an applied current source. Still a feedpoint, but priced from the
    /// solved port voltage rather than as `V / I`, so a caller without that
    /// machinery must decline it rather than treat it as a delta gap.
    CurrentSource,
    /// Types 1-3 — an incident plane wave. **Never** a feedpoint: its tag and
    /// segment fields carry NTHETA and NPHI, not a driven segment, so a caller
    /// that reads them as one reports grid dimensions as an antenna location.
    PlaneWave,
    /// Any I1 outside the canonical 0-5 range.
    ///
    /// Callers that report a solved result do not currently see one, because the
    /// solver's excitation builders reject such a deck before any current exists
    /// — but that is a fact about those callers, not a guarantee this crate can
    /// make or enforce. A *pre*-solve caller would see it, and must decide.
    Unknown,
}

impl ExcitationKind {
    /// Classify a raw EX I1 value per canonical NEC2 numbering.
    pub fn from_type(excitation_type: u32) -> Self {
        match excitation_type {
            0 => ExcitationKind::VoltageSource,
            1 => ExcitationKind::PlaneWaveLinear,
            2 => ExcitationKind::PlaneWaveRightElliptic,
            3 => ExcitationKind::PlaneWaveLeftElliptic,
            4 => ExcitationKind::CurrentSource,
            5 => ExcitationKind::VoltageSourceCurrentSlope,
            other => ExcitationKind::Unknown(other),
        }
    }

    /// What this excitation is, for the purpose of deciding whether it names a
    /// feedpoint — the one question seven call sites used to answer for
    /// themselves, in five different ways (FND-031).
    ///
    /// The divergence was not cosmetic. The worker skipped every non-type-0 card,
    /// so a type-5 deck it had just *driven* as a delta gap through
    /// `build_hallen_rhs` was then refused at extraction with "no EX type-0 card
    /// found" — a deck three other frontends solve to the digit. The GUI and the
    /// Python bindings filtered nothing at all, so a plane wave's NTHETA/NPHI
    /// could be reported as a feedpoint tag and segment.
    ///
    /// Deliberately a *classification* rather than a predicate: no single boolean
    /// serves every caller. A current source (type 4) is not a delta gap, but it
    /// *is* a feedpoint the CLI prices as `Z = V_port / i0` — corpus-pinned
    /// under PH8-CHK-001 (`dipole-ex4-freesp-51seg`) — so a caller keyed on a
    /// voltage-source predicate would silently delete that row. That predicate,
    /// `is_voltage_source`, existed with no caller and was removed rather than
    /// left for one to find (FND-074).
    ///
    /// The match is exhaustive with no wildcard arm on purpose: a new
    /// `ExcitationKind` must force this decision rather than defaulting into
    /// whatever the last arm happened to be.
    pub fn feedpoint_role(self) -> FeedpointRole {
        match self {
            ExcitationKind::VoltageSource | ExcitationKind::VoltageSourceCurrentSlope => {
                FeedpointRole::DeltaGap
            }
            ExcitationKind::CurrentSource => FeedpointRole::CurrentSource,
            ExcitationKind::PlaneWaveLinear
            | ExcitationKind::PlaneWaveRightElliptic
            | ExcitationKind::PlaneWaveLeftElliptic => FeedpointRole::PlaneWave,
            ExcitationKind::Unknown(_) => FeedpointRole::Unknown,
        }
    }

    /// True for the incident-plane-wave types (1, 2, 3).
    pub fn is_plane_wave(self) -> bool {
        matches!(
            self,
            ExcitationKind::PlaneWaveLinear
                | ExcitationKind::PlaneWaveRightElliptic
                | ExcitationKind::PlaneWaveLeftElliptic
        )
    }

    /// Human-readable NEC2 description, used in diagnostics.
    pub fn describe(self) -> &'static str {
        match self {
            ExcitationKind::VoltageSource => "applied-field voltage source (type 0)",
            ExcitationKind::PlaneWaveLinear => "incident plane wave, linear polarization (type 1)",
            ExcitationKind::PlaneWaveRightElliptic => {
                "incident plane wave, right-hand elliptic polarization (type 2)"
            }
            ExcitationKind::PlaneWaveLeftElliptic => {
                "incident plane wave, left-hand elliptic polarization (type 3)"
            }
            ExcitationKind::CurrentSource => "current source (type 4)",
            ExcitationKind::VoltageSourceCurrentSlope => {
                "voltage source with current slope (type 5)"
            }
            ExcitationKind::Unknown(_) => "unknown excitation type",
        }
    }
}

/// EX — Excitation card.
///
/// Field layout follows canonical NEC2. The interpretation of the numeric
/// fields depends on the excitation type ([`ExCard::kind`]):
///
/// - **Voltage / current source** (types 0, 4, 5): `tag`/`segment` locate the
///   driven segment, and `voltage_real`/`voltage_imag` carry the complex source
///   value (volts, or amperes for the current source).
/// - **Incident plane wave** (types 1, 2, 3): the same positional fields carry
///   the NEC2 plane-wave parameters instead — `tag` = NTHETA, `segment` = NPHI,
///   `voltage_real` = θ (deg, incidence), `voltage_imag` = φ (deg), and
///   [`ExCard::polarization_deg`] = η, the polarization angle (F3).
///
/// Only type 0 is solved today; other types are recognized via [`ExCard::kind`]
/// and fail fast with a NEC2-accurate diagnostic until Phase 8 implements them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExCard {
    /// Excitation type (I1). See [`ExCard::kind`] for the NEC2 mapping.
    pub excitation_type: u32,
    /// Source segment tag (source types) or NTHETA (plane-wave types).
    pub tag: u32,
    /// Source segment index (source types) or NPHI (plane-wave types).
    pub segment: u32,
    /// Integer EX field I4.
    ///
    /// For EX type 0 this is typically unused and often set to 0. For other
    /// source types NEC uses this field for additional source metadata.
    pub i4: u32,
    /// Complex source real part (volts/amps) or plane-wave θ incidence (deg).
    pub voltage_real: f64,
    /// Complex source imag part (volts/amps) or plane-wave φ azimuth (deg).
    pub voltage_imag: f64,
    /// EX field F3 — plane-wave polarization angle η (deg); unused for source
    /// types. Defaults to 0.0 when the card omits it.
    pub polarization_deg: f64,
    /// EX field F6 — plane-wave axial ratio (minor/major axis of the
    /// polarization ellipse) for the elliptic types 2/3. 0.0 = linear; unused
    /// for source types. Defaults to 0.0 when the card omits it.
    pub polarization_ratio: f64,
    /// EX field F4 — plane-wave θ incidence increment (deg) for an incidence-angle
    /// sweep (`tag` = NTHETA steps). 0.0 for a single incidence; unused for source
    /// types. See PH9-CHK-001.
    pub theta_inc: f64,
    /// EX field F5 — plane-wave φ azimuth increment (deg) for an incidence-angle
    /// sweep (`segment` = NPHI steps). 0.0 for a single incidence; unused for
    /// source types.
    pub phi_inc: f64,
}

impl ExCard {
    /// Classify this card's excitation type per canonical NEC2 numbering.
    pub fn kind(&self) -> ExcitationKind {
        ExcitationKind::from_type(self.excitation_type)
    }
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
    /// Normalized-gain output requested by the `XNDA` field's `N` (hundreds)
    /// digit, as nec2c splits XNDA (FND-298; it was read from the thousands digit,
    /// which only picks nec2c's column labels, so every 4nec2 `RP … 1000 …` asked
    /// for it). When set, a `NORMALIZED_PATTERN` section (total gain relative to
    /// the pattern peak) is emitted. PH9-CHK-004.
    pub normalize: bool,
    /// Directive gain requested by the `XNDA` field's `D` (tens) digit (FND-298):
    /// this card's rows report the directivity, the power gain without the
    /// radiation efficiency, as nec2c's "DIRECTIVE GAINS".
    pub directive_gain: bool,
    /// Average-power-gain output requested by the `XNDA` field's `A` (units) digit.
    /// When set, an `AVERAGE_POWER_GAIN` line (the solid-angle-weighted mean gain
    /// over the pattern region, = radiation efficiency over the full sphere) is
    /// emitted, matching nec2c's "AVERAGE POWER GAIN". PH9-CHK-004.
    pub avg_power_gain: bool,
}

/// GN — Ground definition card.
///
/// Specifies the electrical ground model below the antenna geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct GnCard {
    /// Ground type:
    ///   -1 = null ground (equivalent to no GN card)
    ///    0 = reflection coefficient method (approximate; deferred Phase 2)
    ///    1 = perfect electric conductor (image method)
    ///    2 = finite conductivity Sommerfeld/Norton (deferred Phase 2)
    pub ground_type: i32,
    /// Relative dielectric constant (EPSE field), if specified on the card.
    ///
    /// `Some` when F1 or F2 is nonzero (fields are read by position, a missing
    /// one zero); `None` for a card that gives no medium, such as `GN 1` or
    /// `GN 0`, where fnec's finite ground takes its average-ground default.
    pub eps_r: Option<f64>,
    /// Conductivity in S/m (SIG field, F2), `Some` alongside `eps_r`.
    pub sigma: Option<f64>,
    /// I2, NRADL: the number of radial wires of a ground screen, as read. The
    /// parser refuses a screen over a finite ground (FND-283), so a solved deck
    /// carries a nonzero count only over `GN 1`/`GN -1`, where nec2c ignores it.
    pub radials: i32,
    /// F3/F4 with `radials ≠ 0`: the screen's radius and wire radius (m), as read.
    pub screen: Option<(f64, f64)>,
    /// F3–F6 with `radials = 0`, when any is nonzero: the second medium or cliff
    /// (ε, σ, distance, height), as read. Only nec2c's RP modes 2, 3, 5 and 6 read
    /// it, which fnec refuses, so it never enters a solve.
    pub medium2: Option<[f64; 4]>,
}

/// EN — End-of-data card.  Signals the end of a NEC deck.
/// LD — Load card.
///
/// Applies a lumped or distributed impedance load to one or more segments.
///
/// NEC load types:
///
/// | I1 | Type                       | F1       | F2        | F3          |
/// |----|----------------------------|----------|-----------|-------------|
/// |  0 | Series RLC (lumped)        | R (Ω)    | L (H)     | C (F)       |
/// |  1 | Parallel RLC (lumped)      | R (Ω)    | L (H)     | C (F)       |
/// |  2 | Series RLC per metre       | R (Ω/m)  | L (H/m)   | C (F/m)     |
/// |  3 | Parallel RLC per metre     | R (Ω/m)  | L (H/m)   | C (F/m)     |
/// |  4 | Series impedance Z=R+jX   | R (Ω)    | X (Ω)     | —           |
/// |  5 | Wire conductivity (dist.)  | σ (S/m)  | —         | —           |
///
/// Types 2 and 3 are applied as types 0 and 1 with each value times the
/// segment length. All other load types are stored verbatim and refused at
/// solve time.
#[derive(Debug, Clone, PartialEq)]
pub struct LdCard {
    /// Load type (0–5; see table above).
    pub load_type: i32,
    /// Wire tag number. 0 makes `seg_first`/`seg_last` absolute segment numbers
    /// (NEC-2's reading, nec2c's `load()`; FND-282): `LD t 0 0 0` loads every
    /// segment, `LD t 0 a b` segments `a..=b` whatever their tags.
    pub tag: u32,
    /// First segment number within the tag. 0 = all segments of the tag (with tag
    /// 0: all segments; `LD t 0 0 b` with `b ≠ 0` is refused — NEC-2 has no
    /// segment 0).
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
/// Rotates and/or translates part of the structure, optionally generating
/// `repeat_count` new copies of it.
///
/// NEC field mapping:
///   I1 = ITGI, tag number increment applied per copy
///   I2 = NRPT, the NUMBER OF NEW STRUCTURES to generate (0 = move in place)
///   F1–F3 = rotation angles about x, y, z axes (degrees)
///   F4–F6 = translation (metres) in x, y, z
///   F7    = ITS, the tag whose FIRST segment begins the affected suffix
///           (0 = the whole structure)
///
/// **Two of these fields used to mean something else.** fnec read I2 as a *last
/// tag* and F7 as a *first tag*, giving a tag-range filter that NEC-2 does not
/// define, and it generated at most one copy because it had no NRPT concept at
/// all. A standard deck therefore lost wires silently: `GM 10 2 0 0 0 0 0 1. 0`
/// is 9 segments tagged 1/11/21 in nec2c and was 6 tagged 1/11 in fnec
/// (FND-119). The range-selection capability is not discarded — it is recorded
/// as requirement FR-011, to return in syntax NEC does not define.
///
/// `ITS` selects a **suffix in definition order**, not a range and not "every
/// tag ≥ ITS": it resolves to the first segment carrying that tag, and
/// everything from there to the end of the structure moves. Verified against
/// nec2c with wires defined in the order 5, 2, 3 — moving from tag 2 moves
/// wires 2 **and** 3 and leaves 5 alone.
#[derive(Debug, Clone, PartialEq)]
pub struct GmCard {
    /// ITGI — tag increment applied per generated copy, and to the moved
    /// segments themselves when `repeat_count == 0`.
    pub tag_increment: u32,
    /// NRPT — how many new copies of the affected suffix to generate.
    ///
    /// `0` means move in place rather than copy. Note this is NEC's rule; fnec
    /// previously keyed in-place off `tag_increment == 0`, which is a different
    /// condition and produced a different structure.
    pub repeat_count: u32,
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
    /// ITS — the tag whose first segment begins the affected suffix.
    ///
    /// `0` means the whole structure. A tag no wire carries is a hard error:
    /// nec2c exits 255 with "NO SEGMENT HAS AN ITAG OF n", where fnec used to
    /// match nothing and solve on regardless.
    pub start_tag: u32,
}

/// GR — Geometry repeat card, as NEC-2 reads it (FND-247).
///
/// `GR I1 I2`: the structure so far occurs `I2` times in all, the original
/// included, each occurrence rotated 360°/`I2` about z from the one before, with
/// tags `+ I1` per occurrence (tag 0 kept). NEC-2's card has no angle field;
/// fnec read `I2` as additional copies at an `F1` angle until FND-247.
#[derive(Debug, Clone, PartialEq)]
pub struct GrCard {
    /// Tag increment from one occurrence to the next (`I1`).
    pub tag_increment: u32,
    /// Occurrences in all, the original included (`I2`, at least 1).
    pub occurrences: u32,
}

/// GX — Reflect the structure in coordinate planes.
///
/// `GX I1 I2`: `I2` read as digits `XYZ` (a nonzero digit selects that plane's
/// normal: X reflects in the yz-plane, and so on). As in nec2c's `reflc`, the
/// reflections run z, then y, then x; each copies every segment present at that
/// point, tagging the copies `tag + increment` (tag 0 stays 0), and the
/// increment doubles after each pass.
#[derive(Debug, Clone, PartialEq)]
pub struct GxCard {
    /// Tag increment for the first pass's copies (`I1`).
    pub tag_increment: u32,
    /// Reflect in the yz-plane (x → −x).
    pub x: bool,
    /// Reflect in the xz-plane (y → −y).
    pub y: bool,
    /// Reflect in the xy-plane (z → −z).
    pub z: bool,
}

/// GA — A wire arc, as nec2c's `arc` builds it.
///
/// `GA ITG NS RADA ANG1 ANG2 RAD`: `NS` segments on a circle of radius `RADA`
/// centred at the origin in the plane y = 0, from `ANG1` to `ANG2` degrees
/// measured from +x toward +z, each segment starting where the last ended. Move
/// it with `GM`. Fields are kept as written, so a deck round-trips.
#[derive(Debug, Clone, PartialEq)]
pub struct GaCard {
    pub tag: u32,
    pub segments: u32,
    /// The arc's radius (m), `RADA`.
    pub arc_radius: f64,
    /// First angle (degrees), `ANG1`.
    pub ang1_deg: f64,
    /// Second angle (degrees), `ANG2`.
    pub ang2_deg: f64,
    /// The wire radius (m), `RAD`.
    pub radius: f64,
}

/// GH — A helix or spiral, as nec2c's `helix` builds it.
///
/// `GH ITG NS S HL A1 B1 A2 B2 RAD`: `NS` segments rising from z = 0 by |`HL`|/`NS`
/// each, `x = a(z)·cos(2πz/S)`, `y = b(z)·sin(2πz/S)`, with `a`, `b` constant
/// (`A1`, `B1`; `B1 = 0` reads as `A1`) when `A2 = A1`, else linear from
/// (`A1`, `B1`) to (`A2`, `B2`; `B2 = 0` reads as `A2`); `HL < 0` swaps x and y
/// (the other handedness). Fields are kept as written; the geometry applies
/// nec2c's defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct GhCard {
    pub tag: u32,
    pub segments: u32,
    /// Spacing between turns (m), `S`.
    pub turn_spacing: f64,
    /// Axial length (m), `HL`; negative for the other handedness.
    pub length: f64,
    pub a1: f64,
    pub b1: f64,
    pub a2: f64,
    pub b2: f64,
    /// The wire radius (m), `RAD`.
    pub radius: f64,
}

/// GS — Scale the structure.
///
/// `GS I1 I2 F1`: every segment defined so far — `GM`/`GR` copies included —
/// has its ends and radius multiplied by `F1`; wires defined after it are not
/// scaled, and later `GM` translations are in the new units. `I1`/`I2` are
/// read and ignored, as nec2c ignores them (its `datagn`, card "gs").
#[derive(Debug, Clone, PartialEq)]
pub struct GsCard {
    /// The scale factor, finite and positive (the parser refuses others).
    pub scale: f64,
}

/// GC — The taper of the `GW` before it, as nec2c's `wire` builds it.
///
/// `GC I1 I2 RDEL RAD1 RAD2` follows a `GW` whose radius is 0: each segment of
/// that wire is `RDEL` times as long as the one before, and the radii run
/// geometrically from `RAD1` on the first segment to `RAD2` on the last. `I1`/`I2`
/// are read and ignored, as NEC-2 defines them and nec2c reads them. The pairing
/// is one rule, [`taper_pairing`], for every builder of a deck.
#[derive(Debug, Clone, PartialEq)]
pub struct GcCard {
    /// Each segment's length over the one before (`RDEL`), positive.
    pub length_ratio: f64,
    /// The first segment's radius (m), `RAD1`.
    pub radius_start: f64,
    /// The last segment's radius (m), `RAD2`.
    pub radius_end: f64,
}

/// How a deck breaks the `GW`–`GC` pairing: what nec2c stops on with
/// "GEOMETRY DATA CARD ERROR".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaperPairingError {
    /// The `GW` at this card index has radius 0 and the next card is not a `GC`.
    WireWithoutTaper { index: usize, tag: u32 },
    /// The `GC` at this card index does not directly follow a `GW` of radius 0.
    TaperWithoutWire { index: usize },
}

impl std::fmt::Display for TaperPairingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WireWithoutTaper { tag, .. } => write!(
                f,
                "GW tag {tag} has radius 0, NEC's tapered wire, so the next card must be the \
                 GC giving its radii; give the wire its radius, or follow it with \
                 `GC 0 0 RDEL RAD1 RAD2`"
            ),
            Self::TaperWithoutWire { .. } => f.write_str(
                "a GC tapers the GW directly before it, whose radius must be 0; \
                 write the pair as `GW … 0` then `GC 0 0 RDEL RAD1 RAD2`",
            ),
        }
    }
}

/// The `GC` tapering the card at `index`: `Some` when that card is a `GW` of
/// radius 0 and the next is a `GC`. Every consumer reads the pairing here.
pub fn taper_of(cards: &[Card], index: usize) -> Option<&GcCard> {
    match (cards.get(index), cards.get(index + 1)) {
        (Some(Card::Gw(gw)), Some(Card::Gc(gc))) if gw.radius == 0.0 => Some(gc),
        _ => None,
    }
}

/// Every radius-0 `GW` directly followed by its `GC`, and every `GC` directly
/// after one: the first card that breaks the pairing, else `Ok`.
pub fn taper_pairing(cards: &[Card]) -> Result<(), TaperPairingError> {
    for (index, card) in cards.iter().enumerate() {
        match card {
            Card::Gw(gw) if gw.radius == 0.0 && taper_of(cards, index).is_none() => {
                return Err(TaperPairingError::WireWithoutTaper { index, tag: gw.tag });
            }
            Card::Gc(_) if index == 0 || taper_of(cards, index - 1).is_none() => {
                return Err(TaperPairingError::TaperWithoutWire { index });
            }
            _ => {}
        }
    }
    Ok(())
}

/// TL — Transmission line card, in the NEC-2 layout (FND-111).
///
/// `TL I1 I2 I3 I4 F1 F2 F3 F4 F5 F6 [F7] [F8]`
///   I1–I4: the two ends, as (tag, segment) pairs. Segment 0 is fnec's shorthand
///          for the tag's centre segment.
///   F1: characteristic impedance Z0 (Ω). NEGATIVE means a crossed line: |Z0|
///       with a 180° reversal between the ends.
///   F2: line length (m). Zero or negative means the straight-line distance
///       between the two segment centres, as NEC-2 does.
///   F3–F6: shunt admittance at end 1 (real, imag) and end 2 (real, imag), in
///       siemens, added across each end.
///   F7: fnec extension — velocity factor (default 1).
///   F8: fnec extension — total matched-line loss in dB (default 0 = lossless).
///
/// fnec used to read `TL t1 s1 t2 s2 NSEG TYPE Z0 LEN VF`, its own layout, which
/// misread every standard card. That layout is refused at parse time with the
/// NEC-2 equivalent in the message, rather than reinterpreted.
#[derive(Debug, Clone, PartialEq)]
pub struct TlCard {
    /// Tag number of the first end.
    pub tag1: u32,
    /// Segment number within tag1 (0 = the tag's centre segment).
    pub segment1: u32,
    /// Tag number of the second end.
    pub tag2: u32,
    /// Segment number within tag2 (0 = the tag's centre segment).
    pub segment2: u32,
    /// Characteristic impedance (Ω); negative = crossed line.
    pub z0: f64,
    /// Line length (m); zero or negative = distance between the segment centres.
    pub length: f64,
    /// Shunt admittance across end 1, (real, imag) siemens.
    pub shunt1: (f64, f64),
    /// Shunt admittance across end 2, (real, imag) siemens.
    pub shunt2: (f64, f64),
    /// Velocity factor (fnec extension, F7); 1 when absent.
    pub velocity_factor: f64,
    /// Total matched-line loss in dB (fnec extension, F8); 0 when absent.
    pub loss_db: f64,
}

/// PT — Print-control card.
///
/// Controls printing of the segment currents, as NEC-2 reads `PT I1 I2 I3 I4`
/// (PH9-CHK-004, FND-299): applied to the CURRENTS table.
#[derive(Debug, Clone, PartialEq)]
pub struct PtCard {
    /// I1: −1 (or less) suppresses the currents; 0 prints them; 1–3 are nec2c's
    /// receive-pattern print formats, printed by fnec as for 0.
    pub mode: i32,
    /// I2: the tag of the printed range; 0 makes I3..I4 absolute segment numbers.
    pub tag: u32,
    /// I3: the first printed segment; 0 prints every segment.
    pub seg_first: u32,
    /// I4: the last printed segment; 0 = `seg_first`.
    pub seg_last: u32,
}

/// NT — Network definition card.
///
/// `NT I1 I2 I3 I4 F1..F6`, read by position: a two-port network between two
/// segments, given by its short-circuit admittances, assembled into the MoM
/// matrix as admittance stamps (`build_nt_stamps`, PH8-CHK-004).
#[derive(Debug, Clone, PartialEq)]
pub struct NtCard {
    /// I1: the tag of port 1 (0: `segment1` is an absolute segment number).
    pub tag1: u32,
    /// I2: port 1's segment.
    pub segment1: u32,
    /// I3: the tag of port 2.
    pub tag2: u32,
    /// I4: port 2's segment.
    pub segment2: u32,
    /// F1/F2: Y11, (real, imag) siemens.
    pub y11: (f64, f64),
    /// F3/F4: Y12 = Y21.
    pub y12: (f64, f64),
    /// F5/F6: Y22.
    pub y22: (f64, f64),
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
    Gs(GsCard),
    Ga(GaCard),
    Gh(GhCard),
    Gc(GcCard),
    Gx(GxCard),
    Ge(GeCard),
    Gn(GnCard),
    Ld(LdCard),
    Tl(TlCard),
    Pt(PtCard),
    Nt(NtCard),
    Ex(ExCard),
    Fr(FrCard),
    Rp(RpCard),
    Ne(NearFieldCard),
    Nh(NearFieldCard),
    En(EnCard),
}

/// The observation grid shared by the `NE` and `NH` near-field request cards.
///
/// NEC-2 gives both cards the same field layout — `NE`/`NH I1 NX NY NZ X0 Y0 Z0
/// DX DY DZ` — and they differ only in *which* field is requested: `NE` the
/// electric field, `NH` the magnetic one. The requested quantity is carried by
/// the [`Card`] variant ([`Card::Ne`] / [`Card::Nh`]), not by this struct, which
/// describes the grid alone.
///
/// `I1` is the coordinate type (0 = rectangular, 1 = spherical), `NX/NY/NZ` are
/// the point counts along each axis, `X0/Y0/Z0` the first point, and `DX/DY/DZ`
/// the increments. For spherical (`I1=1`) the axes are reinterpreted as
/// `NX→R (m), NY→φ (deg), NZ→θ (deg)`.
#[derive(Debug, Clone, PartialEq)]
pub struct NearFieldCard {
    /// Coordinate type (I1): 0 = rectangular, 1 = spherical.
    pub coord_type: u32,
    pub nx: u32,
    pub ny: u32,
    pub nz: u32,
    pub x0: f64,
    pub y0: f64,
    pub z0: f64,
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
}
