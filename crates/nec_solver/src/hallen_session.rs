// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Which Hallén solve a deck gets — decided once, for every frontend.
//!
//! The Hallén family has four members and each has a conductor-path twin:
//! plane-wave receive, current-source drive, and delta-gap drive, the last
//! solved either on merged straight conductors or on continuous conductor paths
//! (PH9-CHK-002). Choosing between them used to be a `match` inside the CLI, so
//! `fnec` answered a bent or split geometry on paths while the GUI, `fnec_py`
//! and the worker answered the same deck on the plain basis — a split inverted-V
//! fed mid-wire came back 264.88 + j410.86 from the CLI and 9.15 − j767.60 from
//! the worker, `status: ok`, `warnings: []`, against nec2c's 268.56 + j452.26
//! (FND-121).
//!
//! The decision now lives here. Frontends ask [`hallen_route`] what a deck needs
//! and call [`solve_hallen_routed`] to get it; nobody re-derives it.
//!
//! One consequence worth stating, because it was invisible while the decision
//! was a `match`: the CLI's current-source arm sat *above* its paths arm, so a
//! current-source deck on bent geometry never reached the paths solve in any
//! frontend at all — `solve_hallen_current_source_paths` had no production
//! caller despite existing. Routing on the excitation and the topology
//! independently, as below, is what makes that reachable.

use num_complex::Complex64;

use crate::current_source::CurrentSourceError;
use crate::excitation::{
    build_current_source_shape_paths, build_hallen_rhs, build_hallen_rhs_paths,
};
use crate::geometry::{
    build_conductor_paths, detect_wire_junctions, merge_collinear_wire_endpoints, ConductorPath,
    Segment,
};
use crate::linear::{
    solve_hallen, solve_hallen_paths, solve_hallen_planewave, solve_hallen_planewave_paths,
    ConstraintRow, SolveError,
};
use crate::matrix::ZMatrix;
use crate::planewave::{build_planewave_hallen, build_planewave_hallen_paths};
use nec_model::card::{Card, ExcitationKind};
use nec_model::deck::NecDeck;

/// Tolerance for deciding that two wire ends meet, in metres.
///
/// One name for a number that decides which solver runs, and therefore decides
/// the physics. It was a bare `1e-6` at twelve call sites with no provenance
/// anywhere (FND-084); this is the definition those call sites should import.
pub const JUNCTION_TOL_M: f64 = 1e-6;

/// How a deck's excitation is driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HallenDrive {
    /// `EX 1`/`2`/`3` — receive, no driven feedpoint.
    PlaneWave,
    /// `EX 4` — a forced current rather than a forced voltage.
    CurrentSource,
    /// `EX 0`/`5` — the ordinary delta gap.
    DeltaGap,
}

/// The solve a deck needs: its drive, and whether the geometry requires the
/// conductor-path basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HallenRoute {
    pub drive: HallenDrive,
    /// True when the geometry contains a bend, a start-to-start or end-to-end
    /// split, or an apex feed that the collinear merge cannot express, so the
    /// homogeneous basis must follow signed arc length along a path.
    pub paths: bool,
}

impl HallenRoute {
    /// The `SOLVER_MODE` label this route reports.
    pub fn mode_label(&self) -> &'static str {
        match self.drive {
            HallenDrive::PlaneWave => "hallen-planewave",
            HallenDrive::CurrentSource => "hallen-current-source",
            HallenDrive::DeltaGap => "hallen",
        }
    }
}

/// True if the deck carries an incident-plane-wave `EX` card (NEC-2 types 1/2/3).
pub fn deck_has_plane_wave(deck: &NecDeck) -> bool {
    deck.cards
        .iter()
        .any(|c| matches!(c, Card::Ex(ex) if ex.kind().is_plane_wave()))
}

/// True if the deck carries a current-source `EX` card (NEC-2 type 4).
pub fn deck_has_current_source(deck: &NecDeck) -> bool {
    deck.cards
        .iter()
        .any(|c| matches!(c, Card::Ex(ex) if ex.kind() == ExcitationKind::CurrentSource))
}

/// The first current-source card's tag, segment and forced current.
fn first_current_source(deck: &NecDeck) -> Option<(u32, u32, Complex64)> {
    deck.cards.iter().find_map(|c| match c {
        Card::Ex(ex) if ex.kind() == ExcitationKind::CurrentSource => Some((
            ex.tag,
            ex.segment,
            Complex64::new(ex.voltage_real, ex.voltage_imag),
        )),
        _ => None,
    })
}

/// Decide which Hallén solve this deck needs. The one copy of that decision.
pub fn hallen_route(deck: &NecDeck, segs: &[Segment]) -> HallenRoute {
    let drive = if deck_has_plane_wave(deck) {
        HallenDrive::PlaneWave
    } else if deck_has_current_source(deck) {
        HallenDrive::CurrentSource
    } else {
        HallenDrive::DeltaGap
    };
    HallenRoute {
        drive,
        paths: nontrivial_paths(segs).is_some(),
    }
}

/// What the conductor-path decomposition says about a geometry.
///
/// Three outcomes, kept apart on purpose. [`nontrivial_paths`] collapses the
/// last two into `None`, which is right for the arms that only need to know
/// whether to use the path basis — and wrong for anyone who must also refuse an
/// out-of-scope topology, because "every path is trivial" and "there is no valid
/// decomposition" are opposite answers to that question. `current_source.rs`
/// distinguished them with a nested `if`/`else if` on `build_conductor_paths`;
/// this type is that distinction with a name, so the next caller cannot flatten
/// it by accident.
#[derive(Debug)]
pub(crate) enum PathRoute {
    /// At least one path is non-trivial (bent, or a reversed split): the path
    /// basis is required.
    NonTrivial(Vec<ConductorPath>),
    /// The decomposition succeeded and every path is trivial — a single straight
    /// wire, a collinear chain, a parallel array. The plain basis handles these
    /// exactly, and routing them through the path solver would change a settled
    /// answer for no reason.
    Reducible,
    /// `build_conductor_paths` refused: degree-3+ T/Y, or a closed loop.
    Unsupported,
}

/// Classify a geometry for routing. The one copy of that decision.
pub(crate) fn classify_paths(segs: &[Segment]) -> PathRoute {
    match build_conductor_paths(segs) {
        Some(ps) if ps.iter().any(|p| !p.is_trivial()) => PathRoute::NonTrivial(ps),
        Some(_) => PathRoute::Reducible,
        None => PathRoute::Unsupported,
    }
}

/// The conductor paths, but only when at least one is non-trivial.
///
/// A `None` here means "the plain basis will do", and deliberately does not say
/// why — see [`PathRoute`] for the caller that must know.
fn nontrivial_paths(segs: &[Segment]) -> Option<Vec<ConductorPath>> {
    match classify_paths(segs) {
        PathRoute::NonTrivial(ps) => Some(ps),
        PathRoute::Reducible | PathRoute::Unsupported => None,
    }
}

/// The free-end boundary rows of every conductor path, two per path, in path order
/// and then start-before-end — the order the path solvers impose them in.
///
/// Each row extrapolates the PATH current to the physical end of the chain
/// ([`crate::linear::free_end_row`]), so it needs three things only this function
/// has together: the inner neighbour along the traversal (`segs[1]` and
/// `segs[len−2]`, which are adjacent even where a wire is walked in reverse — the
/// walk pushes that wire's segments reversed), the relative sign of end and
/// neighbour, and both segment lengths, since the neighbour may lie on a different
/// `GW` card from the end.
///
/// Public so the tests that drive the path solvers directly build their rows here
/// rather than by hand. A hand-built list was the one thing standing between those
/// tests and the old `I = 0` rows.
pub fn path_end_rows(segs: &[Segment], paths: &[ConductorPath]) -> Vec<ConstraintRow> {
    let mut rows = Vec::with_capacity(paths.len() * 2);
    for p in paths {
        let k = p.segs.len();
        debug_assert_eq!(p.free_ends, (p.segs[0], p.segs[k - 1]));
        for (e, nb) in [(0, 1), (k - 1, k.wrapping_sub(2))] {
            let end = p.segs[e];
            let inner = (k > 1).then(|| (p.segs[nb], p.signs[e] * p.signs[nb]));
            let h_inner = inner.map_or(segs[end].length, |(nb, _)| segs[nb].length);
            rows.push(crate::linear::free_end_row(
                end,
                inner,
                segs[end].length,
                h_inner,
            ));
        }
    }
    rows
}

/// The per-segment path index and the free-end rows every path arm needs.
///
/// Three copies of this loop existed — here, in the CLI's receive sweep, and in
/// `current_source.rs` — guarding a convention (which end of a path is free, in
/// what order) that only agrees while nobody edits it.
pub(crate) fn group_paths(
    segs: &[Segment],
    paths: &[ConductorPath],
) -> (Vec<usize>, Vec<ConstraintRow>) {
    let mut path_of = vec![0usize; segs.len()];
    for (pi, p) in paths.iter().enumerate() {
        for &m in &p.segs {
            path_of[m] = pi;
        }
    }
    (path_of, path_end_rows(segs, paths))
}

/// Solve an incident plane wave on a matrix that is **already stamped**.
///
/// Split out of [`solve_hallen_routed`] because the receive-pattern sweep solves
/// the same geometry once per incidence direction and cannot use that entry
/// point: it takes `&mut ZMatrix` and adds the load columns as one-shot deltas,
/// so a per-direction call would stamp them N times. This one borrows the matrix
/// and never touches it, which is exactly what a sweep needs.
///
/// Before this existed the sweep carried its own copy of the arm below —
/// predicate, grouping, builder choice and solver dispatch — which is how
/// FND-121 was built the first time: not by anyone writing a different rule, but
/// by two copies of the same rule drifting apart later (FND-128).
///
/// The returned currents are checked finite here, so a caller needs no guard of
/// its own — see the body for why that is the seam's job and not the caller's.
pub fn solve_hallen_planewave_routed(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    freq_hz: f64,
) -> Result<Vec<Complex64>, HallenSessionError> {
    let paths = nontrivial_paths(segs);
    let grouped = paths.as_ref().map(|ps| group_paths(segs, ps));

    let pw = match &paths {
        Some(ps) => build_planewave_hallen_paths(deck, segs, freq_hz, ps),
        None => build_planewave_hallen(deck, segs, freq_hz),
    }
    .map_err(|e| HallenSessionError::PlaneWave(e.to_string()))?;

    match &grouped {
        Some((path_of, free_ends)) => solve_hallen_planewave_paths(
            z_mat,
            &pw.rhs,
            &pw.cos_vec,
            &pw.sin_vec,
            path_of,
            free_ends,
        ),
        None => solve_hallen_planewave(
            z_mat,
            &pw.rhs,
            &pw.cos_vec,
            &pw.sin_vec,
            &merge_collinear_wire_endpoints(segs),
        ),
    }
    .map_err(HallenSessionError::Solve)
    .and_then(|currents| {
        // The guard belongs to the seam, not to each caller. Both of today's
        // callers happen to check — `solve_hallen_routed` at its single exit,
        // the CLI sweep before its peak reduction — and that is exactly the
        // shape FND-126 was: a per-caller guard is one missing check away from a
        // silent NaN, and this function is `pub`, so the next caller is not in
        // this repo. The outer re-check is harmless.
        crate::check_currents_finite(&currents).map_err(HallenSessionError::NonFiniteCurrents)?;
        Ok(currents)
    })
}

/// Everything the caller needs to compute a residual for a delta-gap solve.
///
/// Present only for [`HallenDrive::DeltaGap`]: the plane-wave and current-source
/// solves do not produce a per-group homogeneous constant to subtract.
#[derive(Debug, Clone)]
pub struct ResidualInputs {
    pub c_hom: Vec<Complex64>,
    pub cos_vec: Vec<f64>,
    pub rhs: Vec<Complex64>,
    /// `Ok` groups rows by contiguous wire range, `Err` by conductor path.
    pub grouping: Result<Vec<(usize, usize)>, Vec<usize>>,
}

/// The result of a routed Hallén solve.
#[derive(Debug, Clone)]
pub struct HallenRouted {
    pub currents: Vec<Complex64>,
    /// The solved port voltage, for a current-source deck whose feedpoint is
    /// priced from it. `None` for every other drive.
    pub port_voltage: Option<Complex64>,
    pub route: HallenRoute,
    pub residual_inputs: Option<ResidualInputs>,
    /// `(segment, current)` into a `TL`/`NT` network at each driven segment that
    /// is also a network port. Empty without networks.
    ///
    /// At such a port the source is in parallel with the network, so the SOURCE
    /// current is the segment current plus this branch. Use
    /// [`HallenRouted::source_current`] for a feedpoint's impedance and power;
    /// `currents` stays the wire current, which is what radiates and what the
    /// current table prints (as nec2c does).
    pub network_branch: Vec<(usize, Complex64)>,
}

impl HallenRouted {
    /// The current the source at segment `idx` delivers: the wire current plus
    /// any network branch in parallel with it (FND-123).
    pub fn source_current(&self, idx: usize) -> Complex64 {
        self.currents[idx]
            + self
                .network_branch
                .iter()
                .filter(|(s, _)| *s == idx)
                .map(|(_, i)| *i)
                .sum::<Complex64>()
    }

    /// [`Self::currents`] with every driven segment's network branch added — the
    /// vector a frontend hands to its feedpoint impedance and input-power code.
    pub fn source_currents(&self) -> Vec<Complex64> {
        let mut out = self.currents.clone();
        for (s, i) in &self.network_branch {
            out[*s] += i;
        }
        out
    }
}

/// Errors a routed solve can raise, in the caller's terms.
#[derive(Debug)]
pub enum HallenSessionError {
    Excitation(String),
    PlaneWave(String),
    CurrentSource(CurrentSourceError),
    Solve(SolveError),
    /// The solve returned, but not with numbers (FND-126).
    NonFiniteCurrents(crate::NonFiniteCurrents),
    /// A `TL`/`NT` network could not be built or solved, or is combined with a
    /// drive the network solve does not support yet (FND-123).
    Network(String),
}

impl std::fmt::Display for HallenSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Excitation(m) | Self::PlaneWave(m) | Self::Network(m) => write!(f, "{m}"),
            Self::CurrentSource(e) => write!(f, "{e}"),
            Self::Solve(e) => write!(f, "{e}"),
            Self::NonFiniteCurrents(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HallenSessionError {}

/// Solve a deck with whichever member of the Hallén family it needs.
///
/// This is the entry point every frontend uses. It performs the CPU solve; a
/// caller with a device fast path for one specific route (the CLI's
/// GPU-resident delta-gap fill+solve) should ask [`hallen_route`] first and take
/// its own path only for the route it actually supports, rather than repeating
/// the routing decision.
pub fn solve_hallen_routed(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &mut ZMatrix,
    freq_hz: f64,
    loads: &[Complex64],
) -> Result<HallenRouted, HallenSessionError> {
    let routed = solve_hallen_routed_inner(deck, segs, z_mat, freq_hz, loads)?;
    // One exit, guarded once. The inner function returns from three arms, and a
    // check per arm is three chances to add a fourth arm without one (FND-126).
    crate::check_currents_finite(&routed.currents)
        .map_err(HallenSessionError::NonFiniteCurrents)?;
    Ok(routed)
}

fn solve_hallen_routed_inner(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &mut ZMatrix,
    freq_hz: f64,
    loads: &[Complex64],
) -> Result<HallenRouted, HallenSessionError> {
    let route = hallen_route(deck, segs);
    let paths = if route.paths {
        nontrivial_paths(segs)
    } else {
        None
    };

    // Lumped series loads enter as matrix *columns*, not diagonal terms, and the
    // coordinate they use must match the basis that will run — which is why this
    // happens here, after the route is known, rather than in `DeckStamps::apply`
    // where the basis is not yet decided (FND-122).
    //
    // These are deltas: call once per matrix, exactly as the old `apply` was.
    crate::stamps::stamp_hallen_load_columns(z_mat, segs, freq_hz, loads, paths.as_deref());

    // Path grouping, built once and shared by every arm below.
    let grouped = paths.as_ref().map(|ps| group_paths(segs, ps));

    let (networks, _) =
        crate::network::build_networks(deck, segs, freq_hz).map_err(HallenSessionError::Network)?;
    if !networks.is_empty() && route.drive != HallenDrive::DeltaGap {
        // Not a limitation of the model, of the superposition: the plane-wave
        // solve carries a sin column the unit-gap solves do not, so the two
        // responses live in different least-squares systems and do not add
        // exactly; a current source would need its rescaling redone through the
        // network. Refused rather than approximated (FND-123).
        return Err(HallenSessionError::Network(format!(
            "TL/NT networks are supported with voltage (delta-gap) sources only; \
             this deck is driven by a {}",
            match route.drive {
                HallenDrive::PlaneWave => "plane wave",
                _ => "current source",
            }
        )));
    }

    match route.drive {
        HallenDrive::PlaneWave => {
            // The arm itself lives in `solve_hallen_planewave_routed`, because
            // the receive sweep needs it without the load stamping above.
            let currents = solve_hallen_planewave_routed(deck, segs, z_mat, freq_hz)?;
            Ok(HallenRouted {
                currents,
                port_voltage: None,
                route,
                residual_inputs: None,
                network_branch: Vec::new(),
            })
        }
        HallenDrive::CurrentSource => {
            solve_current_source(deck, segs, z_mat, freq_hz, route, &grouped)
        }
        HallenDrive::DeltaGap => solve_delta_gap(
            deck, segs, z_mat, freq_hz, route, &paths, &grouped, &networks,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn solve_delta_gap(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    freq_hz: f64,
    route: HallenRoute,
    paths: &Option<Vec<ConductorPath>>,
    grouped: &Option<(Vec<usize>, Vec<ConstraintRow>)>,
    networks: &crate::network::Networks,
) -> Result<HallenRouted, HallenSessionError> {
    let build_rhs = |d: &NecDeck| {
        match paths {
            Some(ps) => build_hallen_rhs_paths(d, segs, freq_hz, ps),
            None => build_hallen_rhs(d, segs, freq_hz),
        }
        .map_err(|e| HallenSessionError::Excitation(e.to_string()))
    };
    let rhs = build_rhs(deck)?;

    // The one solve every response goes through: same matrix (loads already
    // stamped, once), same constraints. The network superposition below is exact
    // only because every response comes from here.
    let endpoints_and_junctions = if grouped.is_some() {
        None
    } else {
        let endpoints = merge_collinear_wire_endpoints(segs);
        let mut comp_of = vec![0usize; segs.len()];
        for (ci, &(first, last)) in endpoints.iter().enumerate() {
            for slot in comp_of.iter_mut().take(last + 1).skip(first) {
                *slot = ci;
            }
        }
        let junctions: Vec<(usize, usize, f64)> =
            detect_wire_junctions(segs, &endpoints, JUNCTION_TOL_M)
                .iter()
                .filter(|j| comp_of[j.seg_a] != comp_of[j.seg_b])
                .map(|j| (j.seg_a, j.seg_b, j.sign))
                .collect();
        Some((endpoints, junctions))
    };
    let solve = |r: &crate::excitation::HallenRhs| {
        match (grouped, &endpoints_and_junctions) {
            (Some((path_of, free_ends)), _) => {
                solve_hallen_paths(z_mat, &r.rhs, &r.cos_vec, path_of, free_ends)
            }
            (None, Some((endpoints, junctions))) => {
                solve_hallen(z_mat, &r.rhs, &r.cos_vec, endpoints, junctions)
            }
            (None, None) => unreachable!("one of the two groupings is always built"),
        }
        .map_err(HallenSessionError::Solve)
    };

    let sol = solve(&rhs)?;
    let grouping = match grouped {
        Some((path_of, _)) => Err(path_of.clone()),
        None => Ok(endpoints_and_junctions
            .as_ref()
            .expect("plain grouping")
            .0
            .clone()),
    };
    let mut residual = ResidualInputs {
        c_hom: sol.c_hom_per_wire,
        cos_vec: rhs.cos_vec,
        rhs: rhs.rhs,
        grouping,
    };
    if networks.is_empty() {
        return Ok(HallenRouted {
            currents: sol.currents,
            port_voltage: None,
            route,
            residual_inputs: Some(residual),
            network_branch: Vec::new(),
        });
    }

    // Networks (FND-123): the structure's response to a unit gap at each
    // undriven port, from a deck identical but for its sources, so the gap is
    // built by the same code (and the same path signs) as a real feed.
    let driven: Vec<(usize, Complex64)> = crate::excitation::feedpoints(deck)
        .filter_map(|(ex, _)| {
            segs.iter()
                .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
                .map(|i| (i, Complex64::new(ex.voltage_real, ex.voltage_imag)))
        })
        .collect();
    let mut unit_rhs: Vec<(Vec<Complex64>, Vec<Complex64>)> = Vec::new();
    let net = crate::network::solve_with_networks(networks, &driven, &sol.currents, |q| {
        let unit = unit_gap_deck(deck, &segs[q]);
        let r = build_rhs(&unit)?;
        let s = solve(&r)?;
        unit_rhs.push((r.rhs, s.c_hom_per_wire));
        Ok::<_, HallenSessionError>(s.currents)
    })
    .map_err(|e| match e {
        crate::network::NetworkSolveError::Solve(e) => e,
        crate::network::NetworkSolveError::Singular => HallenSessionError::Network(
            "the TL/NT networks leave a port voltage undetermined (singular port system)"
                .to_string(),
        ),
    })?;

    // Superpose the right-hand side and the homogeneous constants with the same
    // weights, so the continuity diagnostic describes the system actually solved.
    for ((r, c), (_, v)) in unit_rhs.iter().zip(&net.undriven) {
        for (a, b) in residual.rhs.iter_mut().zip(r) {
            *a += v * b;
        }
        for (a, b) in residual.c_hom.iter_mut().zip(c) {
            *a += v * b;
        }
    }
    Ok(HallenRouted {
        currents: net.currents,
        port_voltage: None,
        route,
        residual_inputs: Some(residual),
        network_branch: net.driven_branch,
    })
}

/// `deck` with its sources replaced by a single 1 V delta gap at `seg`.
fn unit_gap_deck(deck: &NecDeck, seg: &Segment) -> NecDeck {
    let mut unit = deck.clone();
    unit.cards
        .retain(|c| !matches!(c, nec_model::card::Card::Ex(_)));
    unit.cards
        .push(nec_model::card::Card::Ex(nec_model::card::ExCard {
            excitation_type: 0,
            tag: seg.tag,
            segment: seg.tag_index,
            i4: 0,
            voltage_real: 1.0,
            voltage_imag: 0.0,
            polarization_deg: 0.0,
            polarization_ratio: 0.0,
            theta_inc: 0.0,
            phi_inc: 0.0,
        }));
    unit
}

fn solve_current_source(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    freq_hz: f64,
    route: HallenRoute,
    grouped: &Option<(Vec<usize>, Vec<ConstraintRow>)>,
) -> Result<HallenRouted, HallenSessionError> {
    // The plain case keeps the existing pricing helper, which finds the source
    // card, builds the shape and prices the port in one step.
    let Some((path_of, free_ends)) = grouped else {
        let fp = crate::current_source::solve_current_source_hallen(deck, segs, z_mat, freq_hz)
            .map_err(HallenSessionError::CurrentSource)?;
        return Ok(HallenRouted {
            currents: fp.currents,
            port_voltage: Some(fp.port_voltage),
            route,
            residual_inputs: None,
            network_branch: Vec::new(),
        });
    };

    let (tag, seg, i0) = first_current_source(deck)
        .ok_or_else(|| HallenSessionError::Excitation("no current source in deck".into()))?;
    let paths = nontrivial_paths(segs).expect("grouped implies non-trivial paths");
    let (shape, cos_vec, src_seg) =
        build_current_source_shape_paths(deck, segs, freq_hz, tag, seg, &paths)
            .map_err(|e| HallenSessionError::Excitation(e.to_string()))?;
    // The same scaling as the plain branch, through the same helper. This branch
    // is a SECOND copy of the current-source decision -- `solve_current_source`
    // intercepts the path case here rather than letting `current_source.rs`
    // handle it -- so fixing only that file would have left junctioned decks on
    // the old solver. Measured before this line changed: a start-to-start split
    // answered 247.935 + j384.841 under EX 4 against 264.882 + j410.856 under
    // EX 0, a 6.4% split in FREE SPACE. The defect is not confined to ground; it
    // appears wherever the augmented system is inconsistent, and a bent conductor
    // does that too (FND-118).
    let sol = solve_hallen_paths(z_mat, &shape, &cos_vec, path_of, free_ends)
        .map_err(HallenSessionError::Solve)?;
    let (currents, port_voltage) =
        crate::current_source::scale_to_impressed_current(sol.currents, src_seg, i0, tag, seg)
            .map_err(HallenSessionError::CurrentSource)?;
    Ok(HallenRouted {
        currents,
        port_voltage: Some(port_voltage),
        route,
        residual_inputs: None,
        network_branch: Vec::new(),
    })
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use crate::geometry::{build_geometry, ground_model_from_deck, merge_collinear_wire_endpoints};
    use crate::matrix::assemble_z_matrix_with_ground;

    /// A start-to-start split fed mid-wire: two `GW` cards meeting at their own
    /// start points, which the collinear merge cannot express as one straight
    /// conductor. This is the geometry class FND-121 was measured on.
    const SPLIT_V: &str = "CE\nGW 1 21 0 0 3 -5 0 0 .001\nGW 2 21 0 0 3 5 0 0 .001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    const STRAIGHT: &str =
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

    fn setup(deck_str: &str) -> (nec_model::deck::NecDeck, Vec<Segment>, ZMatrix) {
        let deck = nec_parser::parse(deck_str).expect("deck parses").deck;
        let segs = build_geometry(&deck).expect("geometry builds");
        let ground = ground_model_from_deck(&deck);
        let z = assemble_z_matrix_with_ground(&segs, 14.2e6, &ground);
        (deck, segs, z)
    }

    fn feed_current(
        deck: &nec_model::deck::NecDeck,
        segs: &[Segment],
        i: &[Complex64],
    ) -> Complex64 {
        let ex = crate::first_delta_gap_feedpoint(deck).expect("deck has a delta gap");
        let idx = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
            .expect("feed segment exists");
        i[idx]
    }

    /// The routing decision itself: a split geometry needs the path basis, a
    /// straight one does not. Everything else in this module rests on it.
    #[test]
    fn a_split_geometry_routes_to_paths_and_a_straight_one_does_not() {
        let (d, s, _) = setup(SPLIT_V);
        let r = hallen_route(&d, &s);
        assert_eq!(r.drive, HallenDrive::DeltaGap);
        assert!(
            r.paths,
            "a start-to-start split needs the conductor-path basis"
        );

        let (d, s, _) = setup(STRAIGHT);
        assert!(
            !hallen_route(&d, &s).paths,
            "a single straight wire must keep the plain basis — routing it \
             through the path solver would change a settled answer for no reason"
        );
    }

    /// The heart of FND-121. The plain basis and the path basis give materially
    /// different answers on this deck, so *which one runs* is not a detail: the
    /// CLI ran the path solve and the GUI, bindings and worker ran the plain one,
    /// and the deck came back 264.88 + j410.86 from one and 9.15 - j767.60 from
    /// the others, both with `status: ok` and no caveat.
    ///
    /// Pinning the gap rather than only the right answer is deliberate. A test
    /// that asserted the correct value alone would still pass if some future
    /// change made the two bases agree for the wrong reason.
    #[test]
    fn the_two_bases_disagree_so_the_route_decides_the_answer() {
        let (deck, segs, z) = setup(SPLIT_V);

        let mut z = z;
        let routed = solve_hallen_routed(&deck, &segs, &mut z, 14.2e6, &[]).expect("routed solve");
        assert!(routed.route.paths, "this deck must take the path route");
        let z_paths = Complex64::new(1.0, 0.0) / feed_current(&deck, &segs, &routed.currents);

        // The basis the three non-CLI frontends used to take.
        let rhs = build_hallen_rhs(&deck, &segs, 14.2e6).expect("plain rhs");
        let endpoints = merge_collinear_wire_endpoints(&segs);
        let plain = solve_hallen(&z, &rhs.rhs, &rhs.cos_vec, &endpoints, &[]).expect("plain solve");
        let z_plain = Complex64::new(1.0, 0.0) / feed_current(&deck, &segs, &plain.currents);

        // nec2c answers this deck 268.56 + j452.26. Until FND-156 the path basis
        // gave 264.88 + j410.86, which looked like tracking the oracle and was not:
        // the free-end rows sat half a segment inside each tip, and that 1/N
        // shortening happened to cancel a real offset at 21 segments per arm. Both
        // end conditions converge to the same limit (old 295.26 + j495.96, new
        // 304.40 + j522.25 at 81 per arm, heading for ~306 + j526), while nec2c
        // converges to ~279 + j459. The remaining ~10% is a Hallén-vs-nec2c
        // offset on this off-centre, near-antiresonant feed, present on a straight
        // wire too, not a discretisation error. Pin the value, not the oracle.
        assert!(
            (z_paths.re - 297.81).abs() < 1.0 && (z_paths.im - 511.45).abs() < 1.0,
            "path basis moved, got {z_paths}"
        );
        assert!(
            (z_plain.re - z_paths.re).abs() > 100.0,
            "the two bases must still disagree for this test to be meaningful; \
             plain gave {z_plain}, paths gave {z_paths}"
        );
    }
}

#[cfg(test)]
mod load_stamp_tests {
    use super::*;
    use crate::geometry::{build_geometry, ground_model_from_deck};
    use crate::matrix::assemble_z_matrix_with_ground;

    const F: f64 = 14.2e6;
    const BASE: &str = "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\n";

    /// Feedpoint Z for a deck body, with the load cards caller-supplied.
    fn z_in(load_cards: &str) -> Complex64 {
        let src = format!("{BASE}{load_cards}FR 0 1 0 0 14.2 0\nEN\n");
        let deck = nec_parser::parse(&src).expect("parses").deck;
        let segs = build_geometry(&deck).expect("geometry");
        let ground = ground_model_from_deck(&deck);
        let mut z = assemble_z_matrix_with_ground(&segs, F, &ground);
        let stamps = crate::stamps::build_deck_stamps(&deck, &segs, F);
        let routed =
            solve_hallen_routed(&deck, &segs, &mut z, F, &stamps.diagonal).expect("routed solve");
        let ex = crate::first_delta_gap_feedpoint(&deck).expect("feedpoint");
        let idx = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
            .expect("feed segment");
        Complex64::new(ex.voltage_real, ex.voltage_imag) / routed.currents[idx]
    }

    /// The port identity: a series impedance at the feed shifts the feedpoint
    /// impedance by exactly itself, in any correct method of moments. It is the
    /// one load check that needs no oracle.
    ///
    /// Tolerance, not equality. The identity is exact for an exact square solve;
    /// fnec solves an overdetermined system by regularized normal equations, and
    /// the controlled-source superposition argument does not survive least
    /// squares untouched. The measured residual is ~7e-3 Ω here. Asserting
    /// equality would be asserting something false.
    ///
    /// Before the fix this shift was +699.86 + j134.76 for a 100 Ω load: ohms
    /// added to a dimensionless matrix, over-applied by a clean linear factor of
    /// 7.0 (FND-122).
    #[test]
    fn a_load_at_the_feed_shifts_z_by_exactly_itself() {
        let unloaded = z_in("");
        for r in [1.0_f64, 10.0, 100.0, 1000.0] {
            let loaded = z_in(&format!("LD 4 1 26 26 {r} 0.0 0.0\n"));
            let shift = loaded - unloaded;
            let tol = (1e-3 * r).max(0.05);
            assert!(
                (shift.re - r).abs() < tol && shift.im.abs() < tol,
                "{r} Ω at the feed should shift Z by {r}+j0, got {shift} (tol {tol})"
            );
        }
    }

    /// A reactive load must move the reactance by itself and leave the resistance
    /// alone — the identity's other half, which a magnitude-only check would miss.
    #[test]
    fn a_reactive_load_at_the_feed_moves_only_the_reactance() {
        let unloaded = z_in("");
        // LD 4 takes R and X directly.
        let shift = z_in("LD 4 1 26 26 0.0 250.0 0.0\n") - unloaded;
        assert!(
            shift.re.abs() < 0.05 && (shift.im - 250.0).abs() < 0.25,
            "a +j250 load should shift Z by j250, got {shift}"
        );
    }

    /// The physics the port identity cannot see. If the load column were built
    /// from the same scale error as the source term, the identity above would
    /// still pass — it only proves stamp-scale equals source-scale. These two
    /// cases are checked against an oracle instead.
    ///
    /// Reference values are `/usr/bin/nec2c` on the identical decks; fnec's
    /// Hallén differs from nec2c by a systematic few percent, so the tolerance is
    /// 10%. Before the fix, off-feed series *resistors* made the feedpoint
    /// resistance go **down** by 2.29 Ω, and LD 5 conductor loss was ~20× low.
    #[test]
    fn off_feed_and_distributed_loads_track_the_oracle() {
        let unloaded = z_in("");

        let off_feed = z_in("LD 4 1 13 13 50.0 0.0 0.0\nLD 4 1 39 39 50.0 0.0 0.0\n") - unloaded;
        assert!(
            off_feed.re > 0.0,
            "series resistors must raise the feedpoint resistance, got {off_feed}"
        );
        assert!(
            (off_feed.re - 52.9).abs() / 52.9 < 0.10 && (off_feed.im + 9.0).abs() < 1.5,
            "two 50 Ω at segs 13/39: nec2c gives +52.9 - j9.0, got {off_feed}"
        );

        // LD 5 is a distributed wire conductivity. Its per-segment stamp is the
        // midpoint-rule discretisation of the same integral, so it takes the same
        // column treatment rather than a special case.
        let copper = z_in("LD 5 1 0 0 5.8e7 0.0 0.0\n") - unloaded;
        assert!(
            (copper.re - 0.939).abs() / 0.939 < 0.10,
            "copper loss: nec2c gives +0.939 Ω, got {copper}"
        );
    }
}

#[cfg(test)]
mod end_row_tests {
    use super::*;
    use crate::geometry::{build_conductor_paths, build_geometry};

    /// FND-156, the case the path rows exist for: the path ENDS on a one-segment
    /// wire walked in reverse, and its inner neighbour is on another wire walked
    /// forward, with a different segment length. Every sign and length the row
    /// needs comes from a different place here, so a mix-up in any of them shows.
    ///
    /// Wire 1 is one 0.3 m segment from the junction down to a free tip; wire 2
    /// is ten 0.5 m segments up from the same junction (start-to-start).
    #[test]
    fn a_reversed_one_segment_end_wire_extrapolates_the_path_current() {
        let deck = nec_parser::parse(
            "CE\nGW 1 1 0 0 0 0 0 -0.3 .001\nGW 2 10 0 0 0 0 0 5 .001\nGE\n\
             EX 0 2 5 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
        )
        .expect("deck parses")
        .deck;
        let segs = build_geometry(&deck).expect("geometry builds");
        let paths = build_conductor_paths(&segs).expect("a degree-2 path");
        assert_eq!(paths.len(), 1);
        let p = &paths[0];
        let rows = path_end_rows(&segs, &paths);
        assert_eq!(rows.len(), 2);

        // The end on wire 1 (segment 0) and its neighbour on wire 2.
        let (pos, row) = rows
            .iter()
            .map(|r| (p.segs.iter().position(|&m| m == r.0).unwrap(), *r))
            .find(|(_, r)| r.0 == 0)
            .expect("a row at the one-segment wire's tip");
        let nb = row.1.expect("an inner neighbour to extrapolate through");
        assert_eq!(segs[nb].tag, 2, "the neighbour is across the junction");
        assert_ne!(
            p.signs[pos],
            p.signs[p.segs.iter().position(|&m| m == nb).unwrap()],
            "this geometry must reverse one wire relative to the other"
        );

        // A PATH current zero at the tip and linear in arc length, turned into
        // the per-segment currents the solver's unknowns are (I_seg = sign·I_path).
        let mut i_seg = vec![0.0; segs.len()];
        let mut s = 0.0;
        let order: Vec<usize> = if pos == 0 {
            (0..p.segs.len()).collect()
        } else {
            (0..p.segs.len()).rev().collect()
        };
        for k in order {
            let m = p.segs[k];
            i_seg[m] = p.signs[k] * (s + segs[m].length / 2.0);
            s += segs[m].length;
        }
        let (a, b, va, vb) = row;
        let residual = va * i_seg[a] + vb * i_seg[b.unwrap()];
        assert!(
            residual.abs() < 1e-12,
            "row {row:?} leaves {residual} on a path current that is zero at the tip"
        );
    }
}
