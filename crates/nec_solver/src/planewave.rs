// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Incident plane-wave excitation (NEC2 EX types 1/2/3).
//!
//! Builds the Hallén right-hand side for a receiving antenna illuminated by an
//! incident plane wave, instead of a delta-gap voltage source. The physics and
//! NEC2 conventions are documented in
//! `docs/ph8-chk-002-plane-wave-excitation.md`.
//!
//! Convention (matched to `nec2c`): the wave arrives **from** direction
//! (θ, φ), so its propagation vector is `k̂ = −r̂(θ, φ)` and the incident field is
//! `E(r) = ê · E₀ · exp(−j k k̂·r) = ê · E₀ · exp(+j k r̂·r)`. The linear
//! polarization angle η orients **E** in the (θ̂, φ̂) plane: η = 0 → E along θ̂.
//!
//! The Hallén forcing uses the same normalization as the delta-gap builder in
//! [`crate::excitation`]: `rhs(sₘ) = −j·(2π/η₀)·∫ E_t(s′)·sin(k|sₘ − s′|) ds′`,
//! where `E_t` is the tangential incident field along the wire. For the
//! delta-gap that integral collapses (via the source δ) to `V·sin(k|s|)`; here
//! it is evaluated as a segment sum over the distributed incident field.
//!
//! This first increment supports the **single straight wire** class (the
//! reference dipole). Multi-wire and elliptic-polarization breadth are later
//! increments.

use num_complex::Complex64;

use nec_model::card::Card;
use nec_model::deck::NecDeck;

use crate::geometry::Segment;

const C0: f64 = 299_792_458.0; // m/s
const MU0: f64 = 4.0 * std::f64::consts::PI * 1e-7; // H/m
const ETA0: f64 = MU0 * C0; // free-space wave impedance

/// An incident plane wave parsed from an EX type 1/2/3 card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IncidentPlaneWave {
    /// Incidence polar angle θ (degrees) — the wave arrives from this direction.
    pub theta_deg: f64,
    /// Incidence azimuth angle φ (degrees).
    pub phi_deg: f64,
    /// Polarization angle η (degrees); the major-axis tilt from θ̂ (η = 0 → θ̂).
    pub eta_deg: f64,
    /// Axial ratio (minor/major axis of the polarization ellipse). 0 = linear.
    pub axial_ratio: f64,
    /// Handedness sense of the ellipse: +1 (right, EX type 2), −1 (left, type 3),
    /// unused when `axial_ratio == 0` (linear, type 1).
    pub sense: f64,
    /// Incident electric-field magnitude E₀ (V/m). Defaults to 1.0.
    pub e0: f64,
}

impl IncidentPlaneWave {
    /// Build from a plane-wave EX card. Field layout (NEC2): I2/I3 = NTHETA/NPHI
    /// (unused here — single incidence angle), F1 = θ, F2 = φ, F3 = η, F6 = axial
    /// ratio. The excitation type sets the handedness: type 1 linear, type 2
    /// right-hand elliptic, type 3 left-hand elliptic.
    pub fn from_ex_card(ex: &nec_model::card::ExCard) -> Self {
        use nec_model::card::ExcitationKind;
        let (axial_ratio, sense) = match ex.kind() {
            ExcitationKind::PlaneWaveLinear => (0.0, 0.0),
            ExcitationKind::PlaneWaveRightElliptic => (ex.polarization_ratio, 1.0),
            ExcitationKind::PlaneWaveLeftElliptic => (ex.polarization_ratio, -1.0),
            _ => (0.0, 0.0),
        };
        IncidentPlaneWave {
            theta_deg: ex.voltage_real,
            phi_deg: ex.voltage_imag,
            eta_deg: ex.polarization_deg,
            axial_ratio,
            sense,
            e0: 1.0,
        }
    }

    /// Unit propagation direction `k̂ = −r̂(θ, φ)`.
    fn r_hat(&self) -> [f64; 3] {
        let (t, p) = (self.theta_deg.to_radians(), self.phi_deg.to_radians());
        [t.sin() * p.cos(), t.sin() * p.sin(), t.cos()]
    }

    /// Complex polarization vector.
    ///
    /// The ellipse's major axis is tilted by η from θ̂; the minor axis is 90° out
    /// of phase (the `j` factor) and scaled by the axial ratio, with the sign set
    /// by the handedness:
    /// `ê = û_maj + j·sense·AR·û_minor`, where
    /// `û_maj = cos η·θ̂ + sin η·φ̂` and `û_minor = −sin η·θ̂ + cos η·φ̂`.
    /// For `AR = 0` this reduces to the real linear vector `cos η·θ̂ + sin η·φ̂`.
    fn pol_hat(&self) -> [Complex64; 3] {
        let (t, p) = (self.theta_deg.to_radians(), self.phi_deg.to_radians());
        let eta = self.eta_deg.to_radians();
        let theta_hat = [t.cos() * p.cos(), t.cos() * p.sin(), -t.sin()];
        let phi_hat = [-p.sin(), p.cos(), 0.0];
        let (ce, se) = (eta.cos(), eta.sin());
        let jminor = Complex64::new(0.0, self.sense * self.axial_ratio);
        std::array::from_fn(|i| {
            let major = ce * theta_hat[i] + se * phi_hat[i];
            let minor = -se * theta_hat[i] + ce * phi_hat[i];
            Complex64::new(major, 0.0) + jminor * minor
        })
    }
}

/// Hallén system data for a plane-wave excitation: the forcing RHS plus the two
/// homogeneous columns (`cos`/`sin`) consumed by
/// [`crate::solve_hallen_planewave`].
#[derive(Debug)]
pub struct PlaneWaveHallen {
    /// Hallén forcing vector.
    pub rhs: Vec<Complex64>,
    /// `cos(k·s_local)` homogeneous column.
    pub cos_vec: Vec<f64>,
    /// `sin(k·s_local)` homogeneous column.
    pub sin_vec: Vec<f64>,
    /// The incident wave used to build the forcing.
    pub wave: IncidentPlaneWave,
}

/// Error building the plane-wave excitation.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaneWaveError {
    /// No plane-wave (EX type 1/2/3) card present in the deck.
    NoPlaneWaveCard,
    /// The geometry contains a wire junction; only straight, non-junctioned wires
    /// (one or more) are supported.
    JunctionedGeometryNotSupported,
}

impl std::fmt::Display for PlaneWaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaneWaveError::NoPlaneWaveCard => {
                write!(f, "EX: no incident-plane-wave (type 1/2/3) card found")
            }
            PlaneWaveError::JunctionedGeometryNotSupported => write!(
                f,
                "EX: incident plane wave is supported on straight, non-junctioned wires; \
                 junctioned geometry is not yet supported"
            ),
        }
    }
}

impl std::error::Error for PlaneWaveError {}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Build the Hallén forcing + homogeneous columns for the first incident
/// plane-wave EX card in `deck`.
///
/// Supports one or more **straight, non-junctioned** wires (e.g. a parallel
/// dipole array). Each wire carries its own Hallén particular solution: the
/// tangential field uses that wire's axis, the along-wire coordinate is measured
/// from that wire's midpoint, and the `sin(k|sₘ−s_p|)` kernel sums only over
/// segments on the same wire. Junctioned geometry is rejected (its continuity
/// constraints are not modelled by [`crate::solve_hallen_planewave`]).
pub fn build_planewave_hallen(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
) -> Result<PlaneWaveHallen, PlaneWaveError> {
    let wave = deck
        .cards
        .iter()
        .find_map(|card| match card {
            Card::Ex(ex) if ex.kind().is_plane_wave() => Some(IncidentPlaneWave::from_ex_card(ex)),
            _ => None,
        })
        .ok_or(PlaneWaveError::NoPlaneWaveCard)?;

    let n = segs.len();
    let wire_endpoints = crate::geometry::wire_endpoints_from_segs(segs);
    if !crate::geometry::detect_wire_junctions(segs, &wire_endpoints, 1e-6).is_empty() {
        return Err(PlaneWaveError::JunctionedGeometryNotSupported);
    }

    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let r_hat = wave.r_hat();
    let pol_hat = wave.pol_hat();
    let scale = 2.0 * std::f64::consts::PI / ETA0;

    // Map each segment to its wire index (from the endpoint ranges) and cache
    // per-wire axis + geometric midpoint.
    let mut wire_of = vec![0usize; n];
    for (wi, &(first, last)) in wire_endpoints.iter().enumerate() {
        for w in wire_of.iter_mut().take(last + 1).skip(first) {
            *w = wi;
        }
    }
    let wire_dir: Vec<[f64; 3]> = wire_endpoints
        .iter()
        .map(|&(first, _)| segs[first].direction)
        .collect();
    let wire_mid: Vec<[f64; 3]> = wire_endpoints
        .iter()
        .map(|&(first, last)| {
            let (a, b) = (segs[first].midpoint, segs[last].midpoint);
            [
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                (a[2] + b[2]) / 2.0,
            ]
        })
        .collect();

    // Per-segment along-wire coordinate s (from its wire's midpoint) and the
    // complex tangential incident field E_t(s) = (ê·û)·E₀·exp(+j k r̂·r).
    let mut s_coord = vec![0.0f64; n];
    let mut e_tan = vec![Complex64::new(0.0, 0.0); n];
    for (i, seg) in segs.iter().enumerate() {
        let w = wire_of[i];
        let dir = wire_dir[w];
        let d = [
            seg.midpoint[0] - wire_mid[w][0],
            seg.midpoint[1] - wire_mid[w][1],
            seg.midpoint[2] - wire_mid[w][2],
        ];
        s_coord[i] = dot(d, dir);
        let coupling: Complex64 = (0..3).map(|c| pol_hat[c] * dir[c]).sum::<Complex64>() * wave.e0;
        let phase = k * dot(r_hat, seg.midpoint); // +j k r̂·r
        e_tan[i] = coupling * Complex64::from_polar(1.0, phase);
    }

    // Hallén forcing per wire: rhs(sₘ) = −j·(2π/η₀)·Σ_{p∈wire(m)} E_t(s_p)·
    // sin(k|sₘ − s_p|)·Δl_p. The homogeneous cos/sin columns use the per-wire s.
    let mut rhs = vec![Complex64::new(0.0, 0.0); n];
    let mut cos_vec = vec![0.0f64; n];
    let mut sin_vec = vec![0.0f64; n];
    for m in 0..n {
        let mut acc = Complex64::new(0.0, 0.0);
        for p in 0..n {
            if wire_of[p] != wire_of[m] {
                continue;
            }
            let kernel = (k * (s_coord[m] - s_coord[p]).abs()).sin();
            acc += e_tan[p] * kernel * segs[p].length;
        }
        rhs[m] = Complex64::new(0.0, -scale) * acc;
        cos_vec[m] = (k * s_coord[m]).cos();
        sin_vec[m] = (k * s_coord[m]).sin();
    }

    Ok(PlaneWaveHallen {
        rhs,
        cos_vec,
        sin_vec,
        wave,
    })
}
