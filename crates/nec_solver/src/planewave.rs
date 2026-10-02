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

use crate::geometry::{ConductorPath, GroundModel, Segment};

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

/// The incident field of a plane wave at any point: the direct wave plus, over a
/// ground, the ground-reflected wave — nec2c's `etmns` (FND-170).
///
/// The reflected wave is evaluated at the point's mirror image `(x, y, −z)`, and
/// its field vector is the mirrored polarization with the vertical part scaled by
/// `rrv` and the horizontal (φ̂) part by `rrh`:
/// `c = rrv·M p + (p·φ̂)(rrh − rrv)·φ̂`, `M = diag(1, 1, −1)`.
/// Perfect ground: `rrv = rrh = −1`, so the tangential field vanishes at `z = 0`.
/// Finite ground: nec2c's coefficients with `zrati = 1/√ε_c` — `rrv` is minus, and
/// `rrh` equal to, the standard Fresnel Γ_v, Γ_h of [`crate::farfield`].
///
/// The ground is a parameter, never read from the deck: the PEC-contact solve's
/// image deck has its `GN` card stripped, and a field keyed on it would be the
/// direct wave alone on a doubled wire — wrong and unrefused.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IncidentField {
    k: f64,
    r_hat: [f64; 3],
    pol: [Complex64; 3],
    /// The reflected wave's field vector `c`, absent without a reflecting ground.
    reflected: Option<[Complex64; 3]>,
}

impl IncidentField {
    pub(crate) fn new(wave: &IncidentPlaneWave, ground: &GroundModel, freq_hz: f64) -> Self {
        let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
        let pol = wave.pol_hat().map(|c| c * wave.e0);
        let reflected = reflection_coefficients(wave, ground, freq_hz).map(|(rrv, rrh)| {
            let p = wave.phi_deg.to_radians();
            let phi_hat = [-p.sin(), p.cos(), 0.0];
            let p_phi: Complex64 = (0..3).map(|c| pol[c] * phi_hat[c]).sum();
            let mirrored = [pol[0], pol[1], -pol[2]];
            std::array::from_fn(|c| rrv * mirrored[c] + p_phi * (rrh - rrv) * phi_hat[c])
        });
        IncidentField {
            k,
            r_hat: wave.r_hat(),
            pol,
            reflected,
        }
    }

    /// The incident electric field vector at `r`.
    pub(crate) fn at(&self, r: [f64; 3]) -> [Complex64; 3] {
        let direct = Complex64::from_polar(1.0, self.k * dot(self.r_hat, r));
        let mut e = self.pol.map(|c| c * direct);
        if let Some(c) = self.reflected {
            let image = Complex64::from_polar(1.0, self.k * dot(self.r_hat, [r[0], r[1], -r[2]]));
            for i in 0..3 {
                e[i] += c[i] * image;
            }
        }
        e
    }

    /// The incident field along unit direction `d` at `r`.
    pub(crate) fn tangential(&self, r: [f64; 3], d: [f64; 3]) -> Complex64 {
        let e = self.at(r);
        (0..3).map(|c| e[c] * d[c]).sum()
    }
}

/// nec2c's `(rrv, rrh)` for the wave's incidence angle (`etmns`, matrix.c), or
/// `None` where nothing reflects: free space, and `Deferred` ground, which solves
/// in free space and says so.
fn reflection_coefficients(
    wave: &IncidentPlaneWave,
    ground: &GroundModel,
    freq_hz: f64,
) -> Option<(Complex64, Complex64)> {
    match ground {
        GroundModel::FreeSpace | GroundModel::Deferred { .. } => None,
        GroundModel::PerfectConductor => {
            Some((Complex64::new(-1.0, 0.0), Complex64::new(-1.0, 0.0)))
        }
        GroundModel::SimpleFiniteGround { eps_r, sigma } => {
            let eps_c = crate::sommerfeld::complex_permittivity(freq_hz, *eps_r, *sigma);
            let zrati = Complex64::new(1.0, 0.0) / eps_c.sqrt();
            let t = wave.theta_deg.to_radians();
            let (cth, sth) = (t.cos(), t.sin());
            let root = (Complex64::new(1.0, 0.0) - zrati * zrati * (sth * sth)).sqrt();
            let rrh = (zrati * cth - root) / (zrati * cth + root);
            let rrv = -(cth - zrati * root) / (cth + zrati * root);
            Some((rrv, rrh))
        }
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
/// Supports one or more **straight, non-junctioned** conductors (e.g. a parallel
/// dipole array). Each conductor carries its own Hallén particular solution: the
/// tangential field uses that conductor's axis, the along-wire coordinate is
/// measured from its midpoint, and the `sin(k|sₘ−s_p|)` kernel sums only over
/// segments on the same conductor. Junctioned geometry is rejected (its continuity
/// constraints are not modelled by [`crate::solve_hallen_planewave`]).
///
/// "Conductor" rather than "`GW` card": a straight wire split across several
/// collinear `GW` cards is one conductor here, via
/// [`crate::geometry::merge_collinear_wire_endpoints`].
pub fn build_planewave_hallen(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    ground: &GroundModel,
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
    // Collinear `GW` splits are merged into one logical conductor, exactly as the
    // delta-gap sibling `crate::build_hallen_rhs` does — and as
    // `crate::solve_hallen_planewave`, which consumes what this builds, already
    // assumed: its caller hands it `merge_collinear_wire_endpoints`, so a raw
    // per-`GW` list here meant the builder and the solver disagreed about what a
    // wire is. All three uses below need the merged list, not just the junction
    // test: the `sin(k|s_m - s_p|)` sum must run over the whole conductor, and `s`
    // must be measured from the conductor's midpoint rather than reset at a split.
    //
    // On geometry with no collinear split this is a strict no-op — the merge
    // returns exactly `wire_endpoints_from_segs` there — so no deck that solved
    // before changes, and a genuine T/Y junction is still detected and refused
    // (FND-142).
    let wire_endpoints = crate::geometry::merge_collinear_wire_endpoints(segs);
    if !crate::geometry::detect_wire_junctions(segs, &wire_endpoints).is_empty() {
        return Err(PlaneWaveError::JunctionedGeometryNotSupported);
    }

    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let field = IncidentField::new(&wave, ground, freq_hz);
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
    // complex tangential incident field E_t(s) along the wire axis û — the direct
    // wave (ê·û)·E₀·exp(+j k r̂·r) plus, over ground, the reflected one.
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
        e_tan[i] = field.tangential(seg.midpoint, dir);
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

/// The incident plane wave as a delta gap on every segment, `V_p = E_t(p)·Δl_p`
/// with `E_t` the incident field along the segment's own direction — the form the
/// section-graph solve takes its forcing in (FND-162 stage 5). The per-wire
/// builders above sum the same terms against `sin(k|s_m − s_p|)`; the graph does
/// that itself, section by section.
pub(crate) fn planewave_gaps(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    ground: &GroundModel,
) -> Result<Vec<crate::section_graph::GraphFeed>, PlaneWaveError> {
    let wave = deck
        .cards
        .iter()
        .find_map(|card| match card {
            Card::Ex(ex) if ex.kind().is_plane_wave() => Some(IncidentPlaneWave::from_ex_card(ex)),
            _ => None,
        })
        .ok_or(PlaneWaveError::NoPlaneWaveCard)?;
    let field = IncidentField::new(&wave, ground, freq_hz);
    Ok(segs
        .iter()
        .enumerate()
        .map(|(seg, s)| crate::section_graph::GraphFeed {
            seg,
            volts: field.tangential(s.midpoint, s.direction) * s.length,
        })
        .collect())
}

/// Build the plane-wave Hallén forcing + homogeneous columns over **conductor
/// paths** — the general-junction receive path (PH9-CHK-002), consumed by
/// [`crate::solve_hallen_paths`] with the section layout and corner term of a bent
/// path (FND-162). The forcing is a superposition of delta-gap right-hand sides
/// (a feed at each `s_p`, weighted by `E_path(s_p)·Δl_p`), so it is smooth at every
/// bend and the driven solve's bend rows hold for it unchanged.
///
/// This is the path-aware counterpart of [`build_planewave_hallen`], mirroring how
/// [`crate::build_hallen_rhs_paths`] generalizes the delta-gap forcing. Instead of
/// a per-`GW` along-wire coordinate, the homogeneous basis and the incident-field
/// integral use the **signed arc-length** `s` along each [`ConductorPath`] with the
/// traversal **sign**, so `cos(k·s)` / `sin(k·s)` stay continuous across a bent or
/// reversed (start-to-start) junction and the `sin(k|sₘ−s_p|)` kernel sums over the
/// whole conductor path rather than resetting at each `GW` boundary:
///
/// - The current on segment `m` in its own NEC direction is `sign[m]·I_path(s_m)`.
///   The incident tangential field in the *path traversal* direction is
///   `E_path(s_p) = sign[p]·(ê·d̂_p)·E₀·exp(+j k r̂·r_p)`.
/// - `rhs[m] = sign[m]·(−j·2π/η₀)·Σ_{p∈path(m)} E_path(s_p)·sin(k|s_m−s_p|)·Δl_p`.
/// - `cos_vec[m] = sign[m]·cos(k·s_m)`, `sin_vec[m] = sign[m]·sin(k·s_m)`.
///
/// For a single straight wire (`sign = +1`, arc-length = the wire axis) this
/// reduces exactly to [`build_planewave_hallen`]. Because the paths already model
/// the junction continuity, this builder does **not** reject junctioned geometry;
/// the caller selects it for the non-trivial (bent / connected) receive class.
pub fn build_planewave_hallen_paths(
    deck: &NecDeck,
    segs: &[Segment],
    freq_hz: f64,
    paths: &[ConductorPath],
    ground: &GroundModel,
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
    let k = 2.0 * std::f64::consts::PI * freq_hz / C0;
    let field = IncidentField::new(&wave, ground, freq_hz);
    let scale = 2.0 * std::f64::consts::PI / ETA0;

    // Per-segment path index, traversal sign, and signed arc-length from the paths.
    let mut path_of = vec![0usize; n];
    let mut sign_of = vec![1.0f64; n];
    let mut s_of = vec![0.0f64; n];
    for (pi, p) in paths.iter().enumerate() {
        for (j, &m) in p.segs.iter().enumerate() {
            path_of[m] = pi;
            sign_of[m] = p.signs[j];
            s_of[m] = p.s_mid[j];
        }
    }

    // Incident tangential field in the path-traversal direction at each segment:
    // E_path(s_p) = sign[p]·E_t(r_p) along d̂_p, direct plus, over ground, reflected.
    let mut e_path = vec![Complex64::new(0.0, 0.0); n];
    for (p, seg) in segs.iter().enumerate() {
        e_path[p] = field.tangential(seg.midpoint, seg.direction) * sign_of[p];
    }

    // Hallén forcing per path: rhs(sₘ) = sign[m]·(−j·2π/η₀)·Σ_{p∈path(m)} E_path(s_p)·
    // sin(k|sₘ − s_p|)·Δl_p. The homogeneous cos/sin columns carry the same sign.
    let mut rhs = vec![Complex64::new(0.0, 0.0); n];
    let mut cos_vec = vec![0.0f64; n];
    let mut sin_vec = vec![0.0f64; n];
    for m in 0..n {
        let mut acc = Complex64::new(0.0, 0.0);
        for p in 0..n {
            if path_of[p] != path_of[m] {
                continue;
            }
            let kernel = (k * (s_of[m] - s_of[p]).abs()).sin();
            acc += e_path[p] * kernel * segs[p].length;
        }
        rhs[m] = Complex64::new(0.0, -scale) * acc * sign_of[m];
        cos_vec[m] = sign_of[m] * (k * s_of[m]).cos();
        sin_vec[m] = sign_of[m] * (k * s_of[m]).sin();
    }

    Ok(PlaneWaveHallen {
        rhs,
        cos_vec,
        sin_vec,
        wave,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excitation::build_hallen_rhs_paths;
    use crate::geometry::{build_conductor_paths, build_geometry};

    /// FND-162: the receive forcing is a superposition of delta-gap right-hand
    /// sides — a unit feed at each segment `p`, weighted by the incident field's
    /// tangential part there times `Δl_p`. That identity is why the bend rows,
    /// written for a feed, hold for a plane wave; were either builder's kernel or
    /// sign convention to drift, the receive solve would lose the bend silently.
    #[test]
    fn the_receive_forcing_is_a_sum_of_delta_gap_feeds() {
        let geo = "CE\nGW 1 7 0 0 0 0 0 5 .001\nGW 2 7 0 0 5 5 0 5 .001\nGE 0\n";
        let tail = "FR 0 1 0 0 14.2 0\nEN\n";
        let freq = 14.2e6;
        let rx = nec_parser::parse(&format!("{geo}EX 1 1 1 0 60 30 20\n{tail}"))
            .expect("parses")
            .deck;
        let segs = build_geometry(&rx).expect("geometry");
        let paths = build_conductor_paths(&segs).expect("one bent path");
        let pw = build_planewave_hallen_paths(&rx, &segs, freq, &paths, &GroundModel::FreeSpace)
            .expect("receive rhs");

        let k = 2.0 * std::f64::consts::PI * freq / C0;
        let (r_hat, pol) = (pw.wave.r_hat(), pw.wave.pol_hat());
        let mut sum = vec![Complex64::new(0.0, 0.0); segs.len()];
        for (j, &p) in paths[0].segs.iter().enumerate() {
            let sg = &segs[p];
            // The incident field along the path's traversal at `p`, times Δl_p.
            let e_tan: Complex64 = (0..3).map(|c| pol[c] * sg.direction[c]).sum::<Complex64>()
                * pw.wave.e0
                * Complex64::from_polar(1.0, k * dot(r_hat, sg.midpoint))
                * paths[0].signs[j];
            let fed = nec_parser::parse(&format!(
                "{geo}EX 0 {} {} 0 1 0\n{tail}",
                sg.tag, sg.tag_index
            ))
            .expect("parses")
            .deck;
            let dg = build_hallen_rhs_paths(&fed, &segs, freq, &paths).expect("feed rhs");
            for (m, v) in dg.rhs.iter().enumerate() {
                sum[m] += *v * e_tan * sg.length;
            }
            assert_eq!(dg.cos_vec, pw.cos_vec, "the homogeneous columns must agree");
            assert_eq!(dg.sin_vec, pw.sin_vec, "the homogeneous columns must agree");
        }
        let peak = pw.rhs.iter().map(|v| v.norm()).fold(0.0f64, f64::max);
        for (m, (a, b)) in pw.rhs.iter().zip(&sum).enumerate() {
            assert!(
                (a - b).norm() <= 1e-12 * peak,
                "row {m}: receive {a} vs sum of feeds {b}"
            );
        }
    }
}
