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
    SolveError,
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

/// The conductor paths, but only when at least one is non-trivial — a single
/// straight wire decomposes into trivial paths that the plain basis already
/// handles exactly, and routing it through the path solver would change a
/// settled answer for no reason.
fn nontrivial_paths(segs: &[Segment]) -> Option<Vec<ConductorPath>> {
    build_conductor_paths(segs).filter(|ps| ps.iter().any(|p| !p.is_trivial()))
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
}

impl std::fmt::Display for HallenSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Excitation(m) | Self::PlaneWave(m) => write!(f, "{m}"),
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
    if loads.iter().any(|z| *z != Complex64::new(0.0, 0.0)) {
        for (col, column) in crate::excitation::hallen_load_columns(
            segs,
            freq_hz,
            loads,
            nontrivial_paths(segs).as_deref(),
        ) {
            for (row, delta) in column.iter().enumerate() {
                if *delta != Complex64::new(0.0, 0.0) {
                    z_mat.add_to_entry(row, col, *delta);
                }
            }
        }
    }

    // Path grouping, built once and shared by every arm below.
    let grouped = paths.as_ref().map(|ps| {
        let mut path_of = vec![0usize; segs.len()];
        let mut free_ends = Vec::with_capacity(ps.len() * 2);
        for (pi, p) in ps.iter().enumerate() {
            for &m in &p.segs {
                path_of[m] = pi;
            }
            free_ends.push(p.free_ends.0);
            free_ends.push(p.free_ends.1);
        }
        (path_of, free_ends)
    });

    match route.drive {
        HallenDrive::PlaneWave => {
            let pw = match (&paths, &grouped) {
                (Some(ps), Some(_)) => build_planewave_hallen_paths(deck, segs, freq_hz, ps),
                _ => build_planewave_hallen(deck, segs, freq_hz),
            }
            .map_err(|e| HallenSessionError::PlaneWave(e.to_string()))?;
            let currents = match &grouped {
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
            .map_err(HallenSessionError::Solve)?;
            Ok(HallenRouted {
                currents,
                port_voltage: None,
                route,
                residual_inputs: None,
            })
        }
        HallenDrive::CurrentSource => {
            solve_current_source(deck, segs, z_mat, freq_hz, route, &grouped)
        }
        HallenDrive::DeltaGap => {
            solve_delta_gap(deck, segs, z_mat, freq_hz, route, &paths, &grouped)
        }
    }
}

fn solve_delta_gap(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    freq_hz: f64,
    route: HallenRoute,
    paths: &Option<Vec<ConductorPath>>,
    grouped: &Option<(Vec<usize>, Vec<usize>)>,
) -> Result<HallenRouted, HallenSessionError> {
    let rhs = match paths {
        Some(ps) => build_hallen_rhs_paths(deck, segs, freq_hz, ps),
        None => build_hallen_rhs(deck, segs, freq_hz),
    }
    .map_err(|e| HallenSessionError::Excitation(e.to_string()))?;

    let (sol, grouping) = match grouped {
        Some((path_of, free_ends)) => {
            let sol = solve_hallen_paths(z_mat, &rhs.rhs, &rhs.cos_vec, path_of, free_ends)
                .map_err(HallenSessionError::Solve)?;
            (sol, Err(path_of.clone()))
        }
        None => {
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
            let sol = solve_hallen(z_mat, &rhs.rhs, &rhs.cos_vec, &endpoints, &junctions)
                .map_err(HallenSessionError::Solve)?;
            (sol, Ok(endpoints))
        }
    };

    Ok(HallenRouted {
        currents: sol.currents,
        port_voltage: None,
        route,
        residual_inputs: Some(ResidualInputs {
            c_hom: sol.c_hom_per_wire,
            cos_vec: rhs.cos_vec,
            rhs: rhs.rhs,
            grouping,
        }),
    })
}

fn solve_current_source(
    deck: &NecDeck,
    segs: &[Segment],
    z_mat: &ZMatrix,
    freq_hz: f64,
    route: HallenRoute,
    grouped: &Option<(Vec<usize>, Vec<usize>)>,
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

        // nec2c answers this deck 268.56 + j452.26.
        assert!(
            (z_paths.re - 264.88).abs() < 1.0 && (z_paths.im - 410.86).abs() < 1.0,
            "path basis should track the oracle, got {z_paths}"
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
        stamps.apply_couplings(&mut z);
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
