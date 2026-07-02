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
    /// Linear polarization angle η (degrees); η = 0 orients E along θ̂.
    pub eta_deg: f64,
    /// Incident electric-field magnitude E₀ (V/m). Defaults to 1.0.
    pub e0: f64,
}

impl IncidentPlaneWave {
    /// Build from a plane-wave EX card. Field layout (NEC2): I2/I3 = NTHETA/NPHI
    /// (unused here — single incidence angle), F1 = θ, F2 = φ, F3 = η.
    pub fn from_ex_card(ex: &nec_model::card::ExCard) -> Self {
        IncidentPlaneWave {
            theta_deg: ex.voltage_real,
            phi_deg: ex.voltage_imag,
            eta_deg: ex.polarization_deg,
            e0: 1.0,
        }
    }

    /// Unit propagation direction `k̂ = −r̂(θ, φ)`.
    fn r_hat(&self) -> [f64; 3] {
        let (t, p) = (self.theta_deg.to_radians(), self.phi_deg.to_radians());
        [t.sin() * p.cos(), t.sin() * p.sin(), t.cos()]
    }

    /// Unit polarization vector `ê = cos(η)·θ̂ + sin(η)·φ̂`.
    fn pol_hat(&self) -> [f64; 3] {
        let (t, p) = (self.theta_deg.to_radians(), self.phi_deg.to_radians());
        let eta = self.eta_deg.to_radians();
        let theta_hat = [t.cos() * p.cos(), t.cos() * p.sin(), -t.sin()];
        let phi_hat = [-p.sin(), p.cos(), 0.0];
        [
            eta.cos() * theta_hat[0] + eta.sin() * phi_hat[0],
            eta.cos() * theta_hat[1] + eta.sin() * phi_hat[1],
            eta.cos() * theta_hat[2] + eta.sin() * phi_hat[2],
        ]
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
    /// The deck has more than one wire; only the single straight-wire class is
    /// supported in this increment.
    MultiWireNotSupported { wire_count: usize },
}

impl std::fmt::Display for PlaneWaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaneWaveError::NoPlaneWaveCard => {
                write!(f, "EX: no incident-plane-wave (type 1/2/3) card found")
            }
            PlaneWaveError::MultiWireNotSupported { wire_count } => write!(
                f,
                "EX: incident plane wave is only supported on a single straight wire \
                 (found {wire_count} wires); multi-wire plane-wave is not yet supported"
            ),
        }
    }
}

impl std::error::Error for PlaneWaveError {}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Build the Hallén forcing + homogeneous columns for the first incident
/// plane-wave EX card in `deck`, over the single straight wire in `segs`.
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

    // Single straight wire only in this increment.
    let mut tags: Vec<u32> = segs.iter().map(|s| s.tag).collect();
    tags.dedup();
    let unique_tags = {
        let mut t = tags.clone();
        t.sort_unstable();
        t.dedup();
        t.len()
    };
    if unique_tags > 1 {
        return Err(PlaneWaveError::MultiWireNotSupported {
            wire_count: unique_tags,
        });
    }

    let n = segs.len();
    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let r_hat = wave.r_hat();
    let pol_hat = wave.pol_hat();

    // Wire axis and geometric midpoint (single straight wire).
    let wire_dir = segs[0].direction;
    let wire_mid = {
        let first = segs[0].midpoint;
        let last = segs[n - 1].midpoint;
        [
            (first[0] + last[0]) / 2.0,
            (first[1] + last[1]) / 2.0,
            (first[2] + last[2]) / 2.0,
        ]
    };

    // Per-segment: along-wire coordinate s (from wire midpoint) and the complex
    // tangential incident field E_t(s) = (ê·û)·E₀·exp(+j k r̂·r).
    let mut s_coord = vec![0.0f64; n];
    let mut e_tan = vec![Complex64::new(0.0, 0.0); n];
    let tangential_coupling = dot(pol_hat, wire_dir);
    for (i, seg) in segs.iter().enumerate() {
        let d = [
            seg.midpoint[0] - wire_mid[0],
            seg.midpoint[1] - wire_mid[1],
            seg.midpoint[2] - wire_mid[2],
        ];
        s_coord[i] = dot(d, wire_dir);
        let phase = k * dot(r_hat, seg.midpoint); // +j k r̂·r
        e_tan[i] = Complex64::from_polar(wave.e0 * tangential_coupling, phase);
    }

    // Hallén forcing: rhs(sₘ) = −j·(2π/η₀)·Σ_p E_t(s_p)·sin(k|sₘ − s_p|)·Δl_p.
    let scale = 2.0 * std::f64::consts::PI / ETA0;
    let mut rhs = vec![Complex64::new(0.0, 0.0); n];
    let mut cos_vec = vec![0.0f64; n];
    let mut sin_vec = vec![0.0f64; n];
    for m in 0..n {
        let mut acc = Complex64::new(0.0, 0.0);
        for p in 0..n {
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
