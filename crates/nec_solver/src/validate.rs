// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Frontend-agnostic validation and diagnostics.
//!
//! Almost everything here is **pre-solve**: a pure function of
//! `(&NecDeck, &[Segment], &GroundModel, freq_hz)` that answers whether a deck may
//! be solved and what caveats its result will carry. The one exception is
//! [`negative_resistance_warning`], which reads a solved impedance; it lives here
//! because the reasoning it needs already does, and it is documented as the
//! exception rather than quietly widening the module's contract.
//!
//! These checks used to live inside the CLI binary, where the GUI and the Python
//! bindings could not reach them — so a deck that the CLI refused outright (wires
//! crossing mid-span, a source on a degenerate segment, a wire buried in an active
//! ground) solved silently and wrongly on the other two frontends. They are pure
//! functions of `(&NecDeck, &[Segment], &GroundModel, freq_hz)`, the tuple every
//! frontend already has, and they *return* diagnostics rather than printing them,
//! so a CLI can write them to stderr and a GUI can render them in a panel.
//!
//! Severity follows [`nec_model::DiagnosticLevel`]: an `Error` means the geometry
//! is outside the solver's supported class and the solve must not run;
//! a `Warning` means the solve runs but its result carries a caveat.

use nec_model::card::{Card, FeedpointRole};
use nec_model::deck::NecDeck;
use nec_model::{DiagnosticLevel, ValidationDiagnostic};

use crate::geometry::{
    build_conductor_paths, classify_unsupported_topology, detect_wire_junctions,
    merge_collinear_wire_endpoints, UnsupportedTopology,
};
use crate::{GroundModel, Segment};

/// Speed of light in vacuum (m/s), for the wavelength-relative height check.
const C0: f64 = 299_792_458.0;

pub fn points_close(a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt() <= eps
}

pub fn segment_intersection_error(segs: &[Segment]) -> Option<String> {
    const TOUCH_EPS: f64 = 1.0e-9;
    const CROSS_EPS: f64 = 1.0e-7;
    const INTERIOR_EPS: f64 = 1.0e-6;

    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            let a = &segs[i];
            let b = &segs[j];

            // Ignore same-wire neighboring segments and endpoint junctions.
            if a.tag == b.tag {
                continue;
            }
            if segments_share_endpoint(a, b, TOUCH_EPS) {
                continue;
            }

            let (dist, s, t) = segment_closest_distance_and_params(a.start, a.end, b.start, b.end);
            let a_interior = s > INTERIOR_EPS && s < 1.0 - INTERIOR_EPS;
            let b_interior = t > INTERIOR_EPS && t < 1.0 - INTERIOR_EPS;
            if dist <= CROSS_EPS && a_interior && b_interior {
                return Some(format!(
                    "unsupported intersecting-wire geometry between tag {} seg {} and tag {} seg {}; only endpoint junctions are currently supported",
                    a.tag, a.tag_index, b.tag, b.tag_index
                ));
            }
        }
    }

    None
}

pub fn source_risk_geometry_error(deck: &NecDeck, segs: &[Segment]) -> Option<String> {
    const MIN_SOURCE_LENGTH_TO_RADIUS_RATIO: f64 = 2.0;

    // Only a DRIVEN source sits on a segment and carries this risk (FND-035).
    // This used to iterate every `EX` card with no type filter, so a plane wave —
    // whose tag/segment fields carry NTHETA and NPHI, not a driven segment —
    // could collide with a short fat segment's `(tag, index)` and produce a hard
    // rejection of a valid receive deck, on every frontend, complaining about a
    // source that is not there.
    for (ex, _role) in crate::excitation::feedpoints(deck) {
        let Some(seg) = segs
            .iter()
            .find(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        else {
            continue;
        };

        if seg.radius <= 0.0 {
            continue;
        }

        let length_to_radius = seg.length / seg.radius;
        if length_to_radius < MIN_SOURCE_LENGTH_TO_RADIUS_RATIO {
            return Some(format!(
                "unsupported source-risk geometry: EX on tiny segment tag {} seg {} (length={:.6e} m, radius={:.6e} m, L/r={:.3}). Increase segment length or reduce wire radius; tiny-loop/source-risk classes are deferred",
                ex.tag,
                ex.segment,
                seg.length,
                seg.radius,
                length_to_radius,
            ));
        }
    }

    None
}

pub fn buried_wire_geometry_error(segs: &[Segment], ground: &GroundModel) -> Option<String> {
    const BURIED_Z_EPS: f64 = 1.0e-9;

    // PH2-CHK-002 guardrail: buried-wire handling is not yet supported for
    // active image/finite-ground paths. Keep deferred/free-space behavior
    // unchanged so existing deferred contracts remain stable.
    if !matches!(
        ground,
        GroundModel::PerfectConductor | GroundModel::SimpleFiniteGround { .. }
    ) {
        return None;
    }

    for seg in segs {
        let min_z = seg.start[2].min(seg.end[2]);
        if min_z <= BURIED_Z_EPS {
            let detail = match ground {
                GroundModel::SimpleFiniteGround { eps_r, sigma } => {
                    format!("finite ground eps_r={:.3}, sigma={:.6}", eps_r, sigma)
                }
                GroundModel::PerfectConductor => "PEC ground".to_string(),
                _ => "active ground".to_string(),
            };
            return Some(format!(
                "unsupported buried-wire geometry for active ground model on tag {} seg {} (min z = {:.6e} m, {}). Use free-space or move geometry strictly above the ground interface (z > 0); buried/ground-contact classes are deferred",
                seg.tag, seg.tag_index, min_z, detail,
            ));
        }
    }

    None
}

fn segments_share_endpoint(a: &Segment, b: &Segment, eps: f64) -> bool {
    points_close(a.start, b.start, eps)
        || points_close(a.start, b.end, eps)
        || points_close(a.end, b.start, eps)
        || points_close(a.end, b.end, eps)
}

fn segment_closest_distance_and_params(
    p1: [f64; 3],
    q1: [f64; 3],
    p2: [f64; 3],
    q2: [f64; 3],
) -> (f64, f64, f64) {
    const SMALL_NUM: f64 = 1.0e-12;

    let u = [q1[0] - p1[0], q1[1] - p1[1], q1[2] - p1[2]];
    let v = [q2[0] - p2[0], q2[1] - p2[1], q2[2] - p2[2]];
    let w = [p1[0] - p2[0], p1[1] - p2[1], p1[2] - p2[2]];

    let a = dot3(u, u);
    let b = dot3(u, v);
    let c = dot3(v, v);
    let d = dot3(u, w);
    let e = dot3(v, w);
    let mut s_d = a * c - b * b;
    let mut t_d = s_d;

    let mut s_n;
    let mut t_n;

    if s_d < SMALL_NUM {
        s_n = 0.0;
        s_d = 1.0;
        t_n = e;
        t_d = c;
    } else {
        s_n = b * e - c * d;
        t_n = a * e - b * d;

        if s_n < 0.0 {
            s_n = 0.0;
            t_n = e;
            t_d = c;
        } else if s_n > s_d {
            s_n = s_d;
            t_n = e + b;
            t_d = c;
        }
    }

    if t_n < 0.0 {
        t_n = 0.0;
        if -d < 0.0 {
            s_n = 0.0;
        } else if -d > a {
            s_n = s_d;
        } else {
            s_n = -d;
            s_d = a;
        }
    } else if t_n > t_d {
        t_n = t_d;
        if -d + b < 0.0 {
            s_n = 0.0;
        } else if -d + b > a {
            s_n = s_d;
        } else {
            s_n = -d + b;
            s_d = a;
        }
    }

    let s_c = if s_n.abs() < SMALL_NUM {
        0.0
    } else {
        s_n / s_d
    };
    let t_c = if t_n.abs() < SMALL_NUM {
        0.0
    } else {
        t_n / t_d
    };

    let dx = w[0] + s_c * u[0] - t_c * v[0];
    let dy = w[1] + s_c * u[1] - t_c * v[1];
    let dz = w[2] + s_c * u[2] - t_c * v[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    (dist, s_c, t_c)
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ---------------------------------------------------------------------------
// Solver context
// ---------------------------------------------------------------------------

/// Which solver a frontend is about to run.
///
/// Two variants, not the CLI's five: the pulse, continuity and sinusoidal modes
/// are experimental and known-inaccurate for thin-wire antennas, and a diagnostic
/// that branched on them would be inventing advice for solvers no frontend should
/// offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverKind {
    /// Hallén's integral equation — the default everywhere.
    Hallen,
    /// The mixed-potential EFIE: degree-3 junctions, closed loops, near-ground
    /// currents.
    Mpie,
}

impl SolverKind {
    /// The solvers a frontend may offer, in the order a picker should list them.
    ///
    /// Hallén first because it is the default and the right answer for most
    /// decks; the MPIE is the opt-in for the topologies Hallén cannot reach.
    pub const ALL: [SolverKind; 2] = [SolverKind::Hallen, SolverKind::Mpie];

    /// A short human label, for a menu entry or a report line.
    pub fn label(self) -> &'static str {
        match self {
            SolverKind::Hallen => "Hallén",
            SolverKind::Mpie => "MPIE",
        }
    }
}

impl std::fmt::Display for SolverKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// What a frontend must tell the diagnostics about itself.
///
/// `mpie_remedy` exists because the advice "use the MPIE" is *reached* differently
/// in each frontend, and a shared string cannot name a CLI flag to a GUI user who
/// has a picker in front of them. The precedent is
/// [`unpriceable_feedpoint_error`], which takes its remedy the same way.
#[derive(Debug, Clone, Copy)]
pub struct SolverContext<'a> {
    pub kind: SolverKind,
    /// How to reach the MPIE from here — "re-run with `--solver mpie`" for the
    /// CLI, "switch the solver to MPIE" for a GUI with a picker.
    pub mpie_remedy: &'a str,
}

impl SolverContext<'static> {
    /// The Hallén path as a command-line tool describes it.
    pub const fn cli_hallen() -> Self {
        Self {
            kind: SolverKind::Hallen,
            mpie_remedy: "re-run with `--solver mpie`",
        }
    }
}

// ---------------------------------------------------------------------------
// Deck capability
// ---------------------------------------------------------------------------

/// Whether a deck can be solved on the MPIE path: driven by at least one voltage
/// source (`EX` type 0), with no loads / transmission lines / networks and no
/// plane-wave or current-source excitation (all unsupported by the MPIE).
///
/// Used to decide whether a diagnostic may recommend the MPIE as a remedy.
pub fn mpie_compatible_deck(deck: &NecDeck) -> bool {
    if crate::mpie_session::mpie_unsupported(deck).is_some() {
        return false;
    }
    // A delta gap, not merely "not a plane wave": an unrecognised `EX` type is
    // not a voltage source, and counting it as one recommended the MPIE for a
    // deck it cannot drive (FND-037's cluster).
    crate::excitation::first_delta_gap_feedpoint(deck).is_some()
}

// ---------------------------------------------------------------------------
// Warnings
// ---------------------------------------------------------------------------

/// The MPIE's reduced thin-wire kernel uses a **single** wire radius (the first
/// segment's) for the whole geometry, so a deck mixing radii is solved
/// approximately for every differently-sized wire.
///
/// Shared rather than printed by one frontend: this caveat lived in the CLI, so
/// the same deck solved elsewhere reported an approximate impedance with nothing
/// said (the shape of FND-020). Returns `None` when every radius agrees, which is
/// the common case.
pub fn mpie_mixed_radius_caveat(segs: &[Segment]) -> Option<String> {
    let first = segs.first()?;
    let r0 = first.radius;
    let mixed = segs
        .iter()
        .any(|s| (s.radius - r0).abs() > 1e-9 * r0.max(s.radius));
    if !mixed {
        return None;
    }
    Some(format!(
        "the MPIE solver models a single wire radius; this deck mixes radii, so all \
segments are solved with the first wire's radius ({r0} m) — impedance for the \
differently-sized wires will be approximate"
    ))
}

/// A `GN` type outside the supported set is treated as free space; say so, and
/// echo the parsed medium parameters so the user can see what was read.
pub fn deferred_ground_warning(ground: &GroundModel) -> Option<String> {
    let GroundModel::Deferred {
        gn_type,
        eps_r,
        sigma,
    } = ground
    else {
        return None;
    };
    let params = match (eps_r, sigma) {
        (Some(e), Some(s)) => format!(" [parsed: EPSE={e}, SIG={s} S/m]"),
        (Some(e), None) => format!(" [parsed: EPSE={e}]"),
        (None, Some(s)) => format!(" [parsed: SIG={s} S/m]"),
        (None, None) => String::new(),
    };
    Some(format!(
        "GN type {gn_type} is not yet supported; treating this deck as free-space{params}"
    ))
}

/// `GE I1` selects the ground-reflection treatment. `0` (free space) and `1` (PEC
/// image) are handled; anything else falls back to free space and is reported.
pub fn ge_ground_reflection_warning(deck: &NecDeck) -> Option<String> {
    let flag = deck.cards.iter().find_map(|c| {
        if let Card::Ge(ge) = c {
            Some(ge.ground_reflection_flag)
        } else {
            None
        }
    })?;
    match flag {
        0 | 1 => None,
        -1 => Some(
            "GE I1=-1 requests below-ground wire handling (no image method); \
             treating as free-space"
                .to_string(),
        ),
        _ => Some(format!(
            "GE I1={flag} is not a recognised ground-reflection flag \
             (valid values: 0=free-space, 1=PEC image, -1=below-ground); \
             treating as free-space"
        )),
    }
}

/// PH9-CHK-002 / PH9-CHK-005: a **closed loop** or a **degree-3+ (T/Y) junction**
/// is outside the conductor-path Hallén solve. For these classes fnec falls back to
/// the per-wire basis, which enforces neither the loop's periodic closure nor the
/// Kirchhoff current split at a branching node, so the reported impedance, currents
/// and pattern are unreliable for the *whole* geometry — not only a junction-fed
/// segment. A 1 λ square loop, for instance, reports ≈ 20 − j1210 Ω against the true
/// ≈ 111 − j146 Ω.
///
/// The remedy depends on the deck: the MPIE second solver handles both topologies,
/// so a deck it can take is pointed at it ([`mpie_compatible_deck`]); anything else
/// is told support is deferred.
///
/// This catches the loop case that [`feedpoint_at_junction_warnings`] misses,
/// because a loop's feed need not sit on the junction.
pub fn unsupported_topology_warning(
    deck: &NecDeck,
    segs: &[Segment],
    mpie_remedy: &str,
) -> Option<String> {
    let kind = match classify_unsupported_topology(segs)? {
        UnsupportedTopology::ClosedLoop => {
            "a closed loop (a conductor with no free end); the Hallén solve does not model \
             the periodic loop closure"
        }
        UnsupportedTopology::HighDegreeJunction => {
            "a junction where three or more wires meet (a T/Y junction); the Hallén solve does \
             not model the Kirchhoff current split there"
        }
    };
    let remedy = if mpie_compatible_deck(deck) {
        format!(
            "so the reported impedance, currents, and pattern are unreliable — {mpie_remedy}, \
             which solves this geometry correctly (PH9-CHK-007)"
        )
    } else {
        "so the reported impedance, currents, and pattern for this geometry are unreliable \
         (support for this combination is deferred — see PH9-CHK-002)"
            .to_string()
    };
    Some(format!("geometry contains {kind}, {remedy}"))
}

/// PH9-CHK-002: one warning per driven segment that sits on a genuine wire junction,
/// where the feed current splits across the joined wires so the single-segment `V/I`
/// is not the true feedpoint impedance.
///
/// Silent when the whole deck decomposes into supported degree-2 conductor paths —
/// every junction feed there (bends, start-to-start splits, an inverted-V apex) is
/// solved correctly on a continuous basis. The merged (collinear-conductor) grouping
/// is used so a straight conductor merely split across `GW` cards is not flagged.
pub fn feedpoint_at_junction_warnings(deck: &NecDeck, segs: &[Segment]) -> Vec<String> {
    if build_conductor_paths(segs).is_some() {
        return Vec::new();
    }
    let merged = merge_collinear_wire_endpoints(segs);
    let junctions = detect_wire_junctions(segs, &merged, 1e-6);
    if junctions.is_empty() {
        return Vec::new();
    }
    let mut junction_segs = std::collections::HashSet::new();
    for j in &junctions {
        junction_segs.insert(j.seg_a);
        junction_segs.insert(j.seg_b);
    }
    let mut out = Vec::new();
    // Every driven feedpoint, delta gap or current source — a current source on a
    // junction has its feed current split across the joined wires just as a
    // voltage source does. Through the seam so an unrecognised `EX` type is not
    // silently treated as a feedpoint, which the plane-wave-only skip allowed.
    for (ex, _role) in crate::excitation::feedpoints(deck) {
        if let Some((idx, _)) = segs
            .iter()
            .enumerate()
            .find(|(_, s)| s.tag == ex.tag && s.tag_index == ex.segment)
        {
            if junction_segs.contains(&idx) {
                out.push(format!(
                    "feedpoint at tag {} segment {} is on a wire junction; \
                     the feed current splits across the joined wires, so the reported \
                     impedance (V/I on one segment) is not accurate and may be unphysical \
                     (junction-fed impedance is deferred — see PH9-CHK-002)",
                    ex.tag, ex.segment
                ));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Post-solve result check
// ---------------------------------------------------------------------------

/// PH9-CHK-005: a passive antenna cannot have a negative input resistance, so a
/// negative `Re(Z)` on the Hallén path means the reported result is unreliable.
///
/// The one **post-solve** check in this module. It is here rather than in the CLI
/// because the reasoning it needs — is there a genuine junction, and can this deck
/// even be handed to the MPIE — already lives here, and because a frontend that
/// cannot run it reports a physically impossible impedance with no caveat at all
/// (FND-014: the GUI, the Python bindings and the worker all did).
///
/// Deliberately **not** part of [`diagnose`], which is contracted to be solve-free:
/// the GUI calls `diagnose` on every keystroke without a matrix, and adding a check
/// that needs an impedance would either break that or make it lie.
///
/// Returns `None` for a non-negative resistance without touching the geometry, so
/// the junction scan costs nothing on the overwhelmingly common path.
///
/// The MPIE is only offered as a cross-check when the deck can actually be solved
/// that way ([`mpie_compatible_deck`]) — the same rule
/// [`unsupported_topology_warning`] follows. Recommending `--solver mpie` for a
/// deck carrying an `LD` card sends the reader to a solver that rejects it.
///
/// The caller supplies the solver context: this is the **Hallén-basis** diagnosis,
/// and the only one any non-CLI frontend needs, since all three are Hallén-only.
/// The CLI keeps its own MPIE arm, whose message is a claim about that binary's
/// solver arsenal rather than about the deck.
pub fn negative_resistance_warning(
    z_re: f64,
    tag: usize,
    seg: usize,
    deck: &NecDeck,
    segs: &[Segment],
    ctx: SolverContext<'_>,
) -> Option<String> {
    if !is_negative_resistance(z_re) {
        return None;
    }
    Some(negative_resistance_message(
        z_re,
        tag,
        seg,
        &negative_resistance_cause(deck, segs, ctx),
    ))
}

/// The canonical sentence, with the cause supplied by the caller.
///
/// Public so the one caller that supplies its own cause — the CLI's MPIE arm,
/// whose explanation is a claim about that binary's solver arsenal rather than
/// about the deck — composes the *same* sentence rather than hand-copying it.
/// Before this existed the wording lived in two places, and nothing would have
/// caught them drifting apart.
///
/// `z_re` is expected to be negative; callers gate on [`is_negative_resistance`]
/// so that all of them agree on what "negative" means for a non-finite value.
pub fn negative_resistance_message(z_re: f64, tag: usize, seg: usize, cause: &str) -> String {
    format!(
        "feedpoint tag {tag} segment {seg} has negative resistance (Re Z = {z_re:.3} Ω), \
         which is physically impossible for a passive antenna; the result is unreliable — {cause}"
    )
}

/// The single predicate for "this resistance earns the negative-resistance
/// caveat", so a sweep counter, a solver-mode arm and the shared seam cannot
/// disagree about one value.
///
/// **`NaN` is deliberately excluded.** A `NaN` impedance is a non-converged solve,
/// not a negative resistance, and the sentence above would read "has negative
/// resistance (Re Z = NaN Ω)" — self-contradictory, and pointing at a
/// junctioned-geometry cause that is not the real one. Reporting non-convergence
/// is a separate diagnostic (FND-030); this one stays silent rather than lying,
/// which is also the behaviour the CLI had before the check was shared.
pub fn is_negative_resistance(z_re: f64) -> bool {
    z_re < 0.0
}

/// The explanation [`negative_resistance_warning`] offers, on its own.
///
/// Public because a caller that reports *many* negative points — a frequency sweep
/// over a junctioned deck goes negative at nearly every point — needs the cause
/// once rather than the whole per-point sentence repeated. The cause depends only
/// on the geometry and the deck, both fixed across a sweep.
///
/// Exposing it beats letting such a caller split the message on its punctuation:
/// two of the three causes contain an em-dash themselves, so that surgery silently
/// returns a fragment.
pub fn negative_resistance_cause(
    deck: &NecDeck,
    segs: &[Segment],
    ctx: SolverContext<'_>,
) -> String {
    // The MPIE models junctions correctly, so a junction is never the reason
    // there — this cause is a claim about the solver, not about the deck. It was
    // an arm in the CLI, which meant the GUI running an MPIE solve would have
    // recommended the solver it was already running.
    if ctx.kind == SolverKind::Mpie {
        return "please report it as a solver defect".to_string();
    }
    let mpie_remedy = ctx.mpie_remedy;
    if has_wire_junction(segs) {
        "commonly a junctioned-geometry limitation (see PH9-CHK-002)".to_string()
    } else if mpie_compatible_deck(deck) {
        // Saying "junctioned-geometry limitation" here would send the reader after
        // a cause the deck does not contain.
        format!(
            "this geometry has no wire junction, so the usual junctioned-geometry cause \
             (PH9-CHK-002) does not apply and the reason is not identified — {mpie_remedy} \
             to cross-check, and please report it if it persists"
        )
    } else {
        "this geometry has no wire junction, so the usual junctioned-geometry cause \
         (PH9-CHK-002) does not apply and the reason is not identified — the MPIE \
         cross-check is unavailable for this deck, so please report it"
            .to_string()
    }
}

/// Whether the geometry contains a genuine wire junction — a bend, a T/Y, or a
/// start-to-start split.
///
/// Uses the same merged (collinear-conductor) grouping as
/// [`feedpoint_at_junction_warnings`], so the two cannot disagree: a straight
/// conductor merely split across several `GW` cards is *not* a junction, because
/// the solver does not treat it as one.
///
/// Exists so a diagnostic can check whether a junction-based explanation applies
/// before offering one. Blaming "a junctioned-geometry limitation" on a single
/// straight wire sends the reader after a cause that is not present.
pub fn has_wire_junction(segs: &[Segment]) -> bool {
    let merged = merge_collinear_wire_endpoints(segs);
    !detect_wire_junctions(segs, &merged, 1e-6).is_empty()
}

/// PH9-CHK-006: an antenna **very low over finite ground** has only an approximate
/// feedpoint impedance.
///
/// fnec models finite ground (GN0/GN2) with a reflection-coefficient image, which
/// matches nec2c's reflection-coefficient method (GN0) and, for heights ≥ ~0.2 λ,
/// the exact Sommerfeld solution (GN2) to ~10 %. Below ~0.1 λ the two diverge
/// sharply: the Sommerfeld **surface wave** dominates and the reflection-coefficient
/// approximation becomes unreliable — for a horizontal λ/2 dipole at 0.025 λ its ΔR
/// is −24 Ω where the Sommerfeld truth is **+9 Ω**, a sign error.
///
/// `surface_wave_modelled` suppresses the warning for a caller that *did* model the
/// surface wave (the CLI's `--ground-solver sommerfeld`, once its correction actually
/// applied) — the sentence would otherwise deny the very correction the caller made.
pub fn low_finite_ground_warning(
    segs: &[Segment],
    ground: &GroundModel,
    freq_hz: f64,
    surface_wave_modelled: bool,
) -> Option<String> {
    if !matches!(ground, GroundModel::SimpleFiniteGround { .. })
        || freq_hz <= 0.0
        || surface_wave_modelled
    {
        return None;
    }
    let lambda = C0 / freq_hz;
    let min_z = segs
        .iter()
        .flat_map(|s| [s.start[2], s.end[2]])
        .fold(f64::INFINITY, f64::min);
    if !min_z.is_finite() || min_z < 0.0 {
        return None; // buried / below ground is handled by the geometry error path
    }
    if min_z >= 0.1 * lambda {
        return None;
    }
    Some(format!(
        "antenna is {:.3} λ ({:.3} m) above finite ground (below ~0.1 λ); the \
         near-ground feedpoint impedance uses a reflection-coefficient approximation and \
         does not model the Sommerfeld surface wave, so it is only approximate here \
         (finite-ground impedance is accurate to ~10% for heights ≥ ~0.2 λ — see PH9-CHK-006)",
        min_z / lambda,
        min_z
    ))
}

// ---------------------------------------------------------------------------
// Hallén-path caveats, shared by every frontend that runs one
// ---------------------------------------------------------------------------

/// Every caveat a Hallén solve of this deck earns from its *geometry and ground*,
/// in the order the CLI has always printed them.
///
/// The single producer of this set (FND-020). Before it existed, the CLI listed
/// the three calls in one place and the distributed path listed two of them in
/// another, so a deck at 0.03 λ over `GN 2` — or one fed on a junction — came back
/// through `--hosts` as bare numbers.
///
/// **Add a new deck-derived Hallén caveat here, not at a call site.** That is what
/// makes the set reach every frontend by construction rather than by whoever
/// remembers. Caveats that depend on options a particular frontend owns — a
/// declined `--ground-solver sommerfeld` request, an execution-mode fallback —
/// deliberately stay with that frontend, because no other frontend can even make
/// the request.
///
/// Caller-gated on the solver, not gated here: the MPIE models junctions, loops
/// and the surface wave correctly, so none of these apply to it. Passing that in
/// would mean this crate knowing about a CLI-private `SolverMode`.
pub fn hallen_geometry_caveats(
    deck: &NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
    freq_hz: f64,
    surface_wave_modelled: bool,
    mpie_remedy: &str,
) -> Vec<String> {
    let mut out = frequency_independent_caveats(deck, segs, mpie_remedy);
    if let Some(w) = low_finite_ground_warning(segs, ground, freq_hz, surface_wave_modelled) {
        out.push(w);
    }
    out
}

/// Every `EX` card whose type this build does not recognise.
///
/// A **warning**, not an error, and the distinction is the whole point. The deck
/// cannot be solved — `build_excitation` refuses an unrecognised type — but the
/// GUI's keystroke-time strip renders `diagnose`'s *warnings* and deliberately
/// swallows its errors, on the reasoning that a hard rejection is surfaced by the
/// action the user ran. That left a deck with an unknown `EX` type looking clean
/// while typing and failing on Solve, with nothing in between (FND-039).
///
/// So this warns that the solve will refuse, rather than claiming the deck is
/// merely imperfect. Phrased to say what will happen, not to imply it might work.
pub fn unrecognised_excitation_warnings(deck: &NecDeck) -> Vec<String> {
    deck.cards
        .iter()
        .filter_map(|c| match c {
            Card::Ex(ex) => match ex.kind() {
                nec_model::card::ExcitationKind::Unknown(t) => Some(format!(
                    "EX type {t} (tag {}, segment {}) is not a recognised excitation; \
                     NEC-2 defines types 0-5, and the solve will refuse this deck",
                    ex.tag, ex.segment
                )),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Why a deck's requested frequencies cannot be solved, if they cannot.
///
/// A frequency must be **finite and strictly positive**. Nothing checked this, and
/// the results were not merely wrong but confidently wrong (FND-056), measured at
/// `e5cc774` on a 21-segment dipole:
///
/// - `FR 0 1 0 0 0.0 0` drives the current to exactly zero, so `Z = V/I` takes
///   its zero-current branch and prints the `EX` source voltage back as an
///   impedance — `1.000000 + j0.000000`, exit 0, no warning.
/// - `FR 0 1 0 0 -14.2 0` returns `67.161824 + j32.275596`, the exact complex
///   **conjugate** of the +14.2 MHz answer. A typo'd minus sign flips the
///   reactance between capacitive and inductive and reports it as fact.
///
/// The check is over the **generated** frequencies, not the card's first field: a
/// descending sweep passes any start-value test and still walks into negative
/// frequency. `FR 0 5 0 0 10.0 -3.0` solves and reports `FREQ_MHZ -2.000000`.
///
/// Finiteness is checked in **hertz**, not megahertz, and that is not pedantry:
/// every frontend multiplies by 1e6, so a *finite* field near the top of the range
/// becomes an infinite frequency. `FR 0 1 0 0 1e303 0` — every field finite, so
/// the parser's own finiteness check passes it — produced `FREQ_MHZ inf`, `NaN`
/// currents, and `Z = 1.000000 + j0.000000`: both of the defects this function
/// exists to stop, from a deck it would otherwise have accepted.
pub fn frequency_error(deck: &NecDeck) -> Option<String> {
    // The **governing** card only. An earlier card is superseded and never runs
    // (see `frequency::governing_fr_sweep`), and nec2c answers a deck whose first
    // FR is degenerate and whose last is fine — refusing it would be a rejection
    // the reference does not make. A superseded card that is *also* degenerate is
    // reported by `superseded_frequency_warnings` instead: ignored and broken is
    // a caveat, not a refusal.
    let sweep = crate::frequency::governing_fr_sweep(deck)?;
    for f in sweep.extremes_mhz() {
        let hz = f * 1e6;
        if hz.is_finite() && f > 0.0 {
            continue;
        }
        let what = if sweep.len() > 1 {
            format!(
                "the sweep starting at {} MHz reaches {f} MHz",
                sweep.start_mhz
            )
        } else {
            format!("{f} MHz is not a usable frequency")
        };
        // One cause per case: quoting all three would leave the reader to work
        // out which applies to the deck in front of them.
        let why = if !hz.is_finite() {
            "a frequency at or beyond the limit of the number format overflows \
             to infinity once converted to hertz, and every current solved from \
             it comes out NaN"
        } else if f < 0.0 {
            "a negative frequency returns the complex conjugate of its positive \
             counterpart, which silently flips the reactance between capacitive \
             and inductive"
        } else {
            "a zero frequency drives the current to zero, so the reported \
             impedance becomes the source voltage rather than an impedance"
        };
        return Some(format!("FR: {what}; {why}. Frequencies must be > 0"));
    }
    None
}

/// Caveats about `FR` cards that do not govern the solve.
///
/// Two kinds, and both are warnings rather than errors: the card is superseded
/// (so fnec will not run it, though NEC-2 might have), and separately it may be
/// degenerate. A degenerate card that never runs is not grounds to refuse a deck
/// the reference answers — but it is almost certainly a typo, so it is said out
/// loud.
pub fn superseded_frequency_warnings(deck: &NecDeck) -> Vec<String> {
    let mut out = crate::frequency::superseded_fr_warnings(deck);
    let sweeps = crate::frequency::fr_sweeps(deck);
    if sweeps.len() > 1 {
        for (i, s) in sweeps[..sweeps.len() - 1].iter().enumerate() {
            if s.extremes_mhz()
                .iter()
                .any(|f| !(f * 1e6).is_finite() || *f <= 0.0)
            {
                out.push(format!(
                    "FR card {} is also not a usable frequency ({} MHz); it is \
                     superseded, so this refuses nothing — but it is very likely a typo",
                    i + 1,
                    s.start_mhz
                ));
            }
        }
    }
    // A card asking for more points than will be expanded: say so rather than
    // silently returning a shorter sweep than the deck requested.
    if let Some(g) = crate::frequency::governing_fr_sweep(deck) {
        if g.is_truncated() {
            out.push(format!(
                "FR asks for {} frequency points; only the first {} are solved",
                g.len(),
                crate::frequency::MAX_FR_POINTS
            ));
        }
    }
    out
}

/// Why a deck driven by two kinds of source at once cannot be solved, if it is.
///
/// A delta gap and a current source in one deck produce a **silently wrong**
/// answer, not merely an ambiguous one. The current-source solve replaces the
/// right-hand side entirely — `build_current_source_shape` drops the other `EX`
/// cards and the Hallén rows for the delta gap are zeroed — so its feedpoint is
/// priced over currents that never saw its drive. Measured on a 51-segment
/// dipole carrying both: the voltage feedpoint reports **0.678 + j0.086 Ω** where
/// the same deck without the current source gives **74.243 + j13.900**, a
/// hundredfold error at exit 0 with no warning (FND-036).
///
/// Superposition would be the physically correct answer and is real solver work.
/// Refusing is the honest interim: the numbers were never meaningful, so nothing
/// is lost by declining to print them.
///
/// Scope is deliberately the two *driven* kinds. A plane wave alongside a driven
/// source is a different mix — receive versus transmit — routed elsewhere, and it
/// has its own wrong answer recorded separately (FND-050) rather than being swept
/// in here on the way past.
pub fn mixed_excitation_error(deck: &NecDeck) -> Option<String> {
    let mut delta_gap = None;
    let mut current_source = None;
    for (ex, role) in crate::excitation::feedpoints(deck) {
        match role {
            FeedpointRole::DeltaGap if delta_gap.is_none() => delta_gap = Some(ex),
            FeedpointRole::CurrentSource if current_source.is_none() => current_source = Some(ex),
            _ => {}
        }
    }
    let (v, i) = (delta_gap?, current_source?);
    Some(format!(
        "EX: this deck is driven by both a voltage source (type {} on tag {} segment {}) \
         and a current source (type {} on tag {} segment {}). fnec solves one drive kind \
         at a time — the current-source path replaces the right-hand side, so the voltage \
         source's feedpoint would be reported from currents its own drive never produced. \
         Remove one, or solve them as separate decks",
        v.excitation_type, v.tag, v.segment, i.excitation_type, i.tag, i.segment
    ))
}

/// Why a deck has no impedance a delta-gap frontend can report, if that is the
/// case — named, rather than left to a fallthrough.
///
/// A current source **is** a feedpoint, but pricing one needs the solved port
/// voltage, which only the CLI's Hallén path computes. A frontend without that
/// machinery has to decline, and the useful question is *which* way it declines:
/// the GUI and the Python bindings fell through their feedpoint loop to
/// "deck has no EX card", which is false for a deck that plainly has one and
/// sends the reader looking for a missing card (FND-038).
///
/// `remedy` is the caller's, because the honest advice differs: the distributed
/// path says "run without `--hosts`", a GUI says "use the CLI". Everything else
/// is the same sentence, which is why it lives here rather than a third time in
/// each frontend.
pub fn unpriceable_feedpoint_error(deck: &NecDeck, remedy: &str) -> Option<String> {
    // Only when there is no delta gap at all: a deck carrying both is priced from
    // the delta gap and needs no excuse.
    if crate::excitation::first_delta_gap_feedpoint(deck).is_some() {
        return None;
    }
    let (ex, _) = crate::excitation::feedpoints(deck)
        .find(|(_, role)| *role == FeedpointRole::CurrentSource)?;
    Some(format!(
        "EX type {} (current source) on tag {} segment {}: a current-source \
         feedpoint is priced from the solved port voltage, which this path does \
         not compute; {remedy}",
        ex.excitation_type, ex.tag, ex.segment
    ))
}

/// The same set for a whole frequency sweep.
///
/// Only one of these caveats depends on frequency, and it is the reason this
/// function exists: evaluating a sweep at a single frequency reports the wrong
/// answer for every other point. The low-ground check trips below 0.1 λ, so the
/// **lowest** swept frequency is the worst case — if it does not trip there it
/// trips nowhere.
pub fn hallen_geometry_caveats_swept(
    deck: &NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
    freqs_hz: &[f64],
    surface_wave_modelled: bool,
    mpie_remedy: &str,
) -> Vec<String> {
    let mut out = frequency_independent_caveats(deck, segs, mpie_remedy);
    if let Some(w) = swept_low_ground_caveat(segs, ground, freqs_hz, surface_wave_modelled) {
        out.push(w);
    }
    out
}

/// The caveats that hold for the deck regardless of frequency.
fn frequency_independent_caveats(
    deck: &NecDeck,
    segs: &[Segment],
    mpie_remedy: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(w) = unsupported_topology_warning(deck, segs, mpie_remedy) {
        out.push(w);
    }
    out.extend(feedpoint_at_junction_warnings(deck, segs));
    out
}

/// The low-over-ground caveat for a swept range, annotated with what it applies to.
///
/// Separate from the full set because a frontend that already shows the
/// frequency-independent caveats elsewhere — the GUI's deck-caveat strip does —
/// wants only this one, and showing the same sentence twice on one screen is its
/// own defect.
///
/// The annotation is built here rather than by each caller. Two callers grew their
/// own version of it and had already diverged: one named the affected count and the
/// other did not, so the same sweep read as wholly affected in one frontend and
/// partly in the other. Both also matched the caveat by substring to find it, which
/// breaks the moment its wording changes.
pub fn swept_low_ground_caveat(
    segs: &[Segment],
    ground: &GroundModel,
    freqs_hz: &[f64],
    surface_wave_modelled: bool,
) -> Option<String> {
    let usable: Vec<f64> = freqs_hz.iter().copied().filter(|f| *f > 0.0).collect();
    let worst = usable.iter().copied().fold(f64::INFINITY, f64::min);
    if !worst.is_finite() {
        return None;
    }
    let base = low_finite_ground_warning(segs, ground, worst, surface_wave_modelled)?;
    if usable.len() <= 1 {
        return Some(base);
    }
    // The caveat quotes a height in wavelengths, which belongs to the worst case
    // alone. Without saying so, a reader takes "0.030 λ" as true of every point.
    let affected = usable
        .iter()
        .filter(|f| low_finite_ground_warning(segs, ground, **f, surface_wave_modelled).is_some())
        .count();
    Some(format!(
        "{base} (worst case, at {:.6} MHz; {affected} of {} swept frequencies are affected)",
        worst / 1e6,
        usable.len()
    ))
}

// ---------------------------------------------------------------------------
// Aggregate
// ---------------------------------------------------------------------------

/// The first hard geometry error, in the order the checks must run: the two that
/// need only the wire layout, then the one that needs the resolved ground model.
///
/// `Some(_)` means the deck is outside the supported class and the solve must not
/// run — the string is the message to show the user.
pub fn geometry_error(deck: &NecDeck, segs: &[Segment], ground: &GroundModel) -> Option<String> {
    segment_intersection_error(segs)
        .or_else(|| source_risk_geometry_error(deck, segs))
        .or_else(|| buried_wire_geometry_error(segs, ground))
}

/// Every reason a deck must not be solved at all, geometry or otherwise.
///
/// This is the gate a frontend calls before solving; [`geometry_error`] is one
/// part of it and keeps its narrower name honest. The distinction matters because
/// the first non-geometry refusal — mixed excitation (FND-036) — would otherwise
/// have been bolted into a function called `geometry_error`, where the next reader
/// looking for "why was my deck refused" would never think to look.
///
/// Errors first and one at a time: a caller that sees `Some` must not solve, and
/// which reason it reports is deliberate — geometry before excitation, because a
/// deck whose wires cross has a problem no excitation change will fix.
pub fn pre_solve_error(deck: &NecDeck, segs: &[Segment], ground: &GroundModel) -> Option<String> {
    geometry_error(deck, segs, ground)
        .or_else(|| mixed_excitation_error(deck))
        .or_else(|| frequency_error(deck))
}

/// Every frontend-independent diagnostic for a solve, errors first.
///
/// A single `Error` (if any) is the hard geometry rejection from [`geometry_error`];
/// a caller that sees one must not solve. The rest are warnings the caller should
/// surface but may otherwise ignore.
///
/// Solver-specific caveats (experimental basis modes, mixed radii on the MPIE,
/// execution-mode fallback, a declined Sommerfeld request) are *not* here — they
/// depend on options only the CLI exposes, and it emits them itself.
///
/// [`negative_resistance_warning`] is not here either, for a different and stricter
/// reason: it needs a solved impedance, and this function is contracted to be
/// **solve-free**. The GUI calls it on every keystroke with no matrix in hand
/// (#369), so a check that required one would either break that path or report on
/// a result that does not exist yet. Callers run it after the solve instead.
pub fn diagnose(
    deck: &NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
    freq_hz: f64,
    ctx: SolverContext<'_>,
) -> Vec<ValidationDiagnostic> {
    let mut out = Vec::new();
    if let Some(e) = pre_solve_error(deck, segs, ground) {
        out.push(ValidationDiagnostic::error(e));
    }
    // What the *chosen* solver cannot represent is an error, not a caveat: the
    // MPIE has nowhere to stamp a load, so offering to solve anyway would answer
    // with the card silently ignored.
    if ctx.kind == SolverKind::Mpie {
        if let Some(u) = crate::mpie_session::mpie_unsupported(deck) {
            out.push(ValidationDiagnostic::error(u.to_string()));
        }
    }

    // Solver-independent: these describe the deck and the ground model, not the
    // basis that will be used on them.
    for w in [
        ge_ground_reflection_warning(deck),
        deferred_ground_warning(ground),
    ]
    .into_iter()
    .flatten()
    {
        out.push(ValidationDiagnostic::warning(w));
    }
    // Solver-independent too: an `EX` type nothing recognises is a fact about the
    // deck, and both solvers refuse it.
    for w in unrecognised_excitation_warnings(deck) {
        out.push(ValidationDiagnostic::warning(w));
    }
    for w in superseded_frequency_warnings(deck) {
        out.push(ValidationDiagnostic::warning(w));
    }

    match ctx.kind {
        // The Hallén basis cannot model a loop closure, a Kirchhoff split at a
        // T/Y junction, or the surface wave near lossy ground.
        SolverKind::Hallen => {
            for w in [
                unsupported_topology_warning(deck, segs, ctx.mpie_remedy),
                low_finite_ground_warning(segs, ground, freq_hz, false),
            ]
            .into_iter()
            .flatten()
            {
                out.push(ValidationDiagnostic::warning(w));
            }
            for w in feedpoint_at_junction_warnings(deck, segs) {
                out.push(ValidationDiagnostic::warning(w));
            }
        }
        // The MPIE solves all three correctly, so repeating those caveats there
        // would be false — and the topology one would recommend the solver
        // already running. Its own limitation is the single-radius kernel.
        SolverKind::Mpie => {
            if let Some(w) = mpie_mixed_radius_caveat(segs) {
                out.push(ValidationDiagnostic::warning(w));
            }
        }
    }
    out
}

/// Whether `diags` contains a diagnostic that must stop the solve.
pub fn has_error(diags: &[ValidationDiagnostic]) -> bool {
    diags.iter().any(|d| d.level == DiagnosticLevel::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nec_parser::parse;

    /// Parse a deck and build its geometry — the exact pair every frontend holds.
    fn deck_and_segs(src: &str) -> (NecDeck, Vec<Segment>) {
        let parsed = parse(src).expect("deck parses");
        let segs = crate::build_geometry(&parsed.deck).expect("geometry builds");
        (parsed.deck, segs)
    }

    const CLEAN_DIPOLE: &str =
        "GW 1 21 -5.278 0 0 5.278 0 0 0.001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    #[test]
    fn a_clean_free_space_dipole_produces_no_diagnostics() {
        let (deck, segs) = deck_and_segs(CLEAN_DIPOLE);
        let diags = diagnose(
            &deck,
            &segs,
            &GroundModel::FreeSpace,
            14.2e6,
            SolverContext::cli_hallen(),
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert!(!has_error(&diags));
    }

    #[test]
    fn wires_crossing_mid_span_are_a_hard_error() {
        // Two wires crossing at the origin, neither meeting the other at an endpoint.
        let (deck, segs) = deck_and_segs(
            "GW 1 11 -5 0 0 5 0 0 0.001\nGW 2 11 0 -5 0 0 5 0 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let err = segment_intersection_error(&segs).expect("crossing wires must be rejected");
        assert!(err.contains("intersecting-wire"), "{err}");
        let diags = diagnose(
            &deck,
            &segs,
            &GroundModel::FreeSpace,
            14.2e6,
            SolverContext::cli_hallen(),
        );
        assert!(has_error(&diags), "diagnose must surface it as an Error");
    }

    #[test]
    fn an_endpoint_junction_is_not_an_intersection() {
        // Negative control for the check above: sharing an endpoint is legal.
        let (_deck, segs) = deck_and_segs(
            "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 0 5 0 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(segment_intersection_error(&segs), None);
    }

    #[test]
    fn a_source_on_a_degenerate_segment_is_a_hard_error() {
        // 201 segments over 5.278 m with a 2 cm radius: L/r = 1.31, under the 2.0 floor.
        let (deck, segs) = deck_and_segs(
            "GW 1 201 -2.639 0 0 2.639 0 0 0.02\nGE\nEX 0 1 101 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let err = source_risk_geometry_error(&deck, &segs).expect("tiny source segment");
        assert!(err.contains("source-risk"), "{err}");
        // The same wire without the EX card on it is fine — the check is about the source.
        let (bare, bare_segs) =
            deck_and_segs("GW 1 201 -2.639 0 0 2.639 0 0 0.02\nGE\nFR 0 1 0 0 14.2 0.0\nEN\n");
        assert_eq!(source_risk_geometry_error(&bare, &bare_segs), None);
    }

    #[test]
    fn a_plane_wave_does_not_trigger_a_source_risk_rejection() {
        // FND-035. A plane wave's tag/segment fields carry NTHETA and NPHI, not a
        // driven segment — so when they happened to collide with a short fat
        // segment's `(tag, index)`, a receive deck with **no driven source at all**
        // was hard-rejected on every frontend, told "EX on tiny segment" about a
        // source that is not there.
        //
        // Wire 1 is 3 segments over 0.02 m at radius 0.01 (L/r = 0.667, well under
        // the 2.0 floor) and the plane wave names tag 1 segment 2.
        let (deck, segs) = deck_and_segs(
            "GW 1 3 0 0 0 0 0 0.02 0.01\nGW 2 21 1 0 -5.282 1 0 5.282 0.001\nGE 0\nEX 1 1 2 0 0.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(
            source_risk_geometry_error(&deck, &segs),
            None,
            "a receive deck has no source to be at risk"
        );
        // Positive control on the same geometry: put a real source there and the
        // rejection must come back, or this test proves only that the check is off.
        let (driven, driven_segs) = deck_and_segs(
            "GW 1 3 0 0 0 0 0 0.02 0.01\nGW 2 21 1 0 -5.282 1 0 5.282 0.001\nGE 0\nEX 0 1 2 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert!(
            source_risk_geometry_error(&driven, &driven_segs).is_some(),
            "a driven source on that same segment must still be refused"
        );
    }

    #[test]
    fn an_unrecognised_excitation_is_not_treated_as_a_junction_feedpoint() {
        // The plane-wave-only skip let an unknown `EX` type count as a feedpoint,
        // so a deck could be warned about a junction feed it does not have.
        //
        // A degree-3 T, because a degree-2 bend is merged into one conductor path
        // and deliberately no longer warns (PH9-CHK-002) — a bend fixture here
        // would pass for the wrong reason.
        let (deck, segs) = deck_and_segs(
            "GW 1 13 0 0 0 5.282 0 0 0.001\nGW 2 13 0 0 0 -5.282 0 0 0.001\nGW 3 13 0 0 0 0 0 5.282 0.001\nGE 0\nEX 9 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert!(
            feedpoint_at_junction_warnings(&deck, &segs).is_empty(),
            "an unrecognised EX type is not a feedpoint"
        );
        // Positive control on the same geometry: a real feed there must still warn,
        // or this proves only that the check stopped firing.
        let (driven, driven_segs) = deck_and_segs(
            "GW 1 13 0 0 0 5.282 0 0 0.001\nGW 2 13 0 0 0 -5.282 0 0 0.001\nGW 3 13 0 0 0 0 0 5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert!(
            !feedpoint_at_junction_warnings(&driven, &driven_segs).is_empty(),
            "a real feed on the junction must still warn"
        );
    }

    #[test]
    fn a_wire_touching_an_active_ground_is_a_hard_error_but_only_over_active_ground() {
        let (_deck, segs) = deck_and_segs(
            "GW 1 21 0 0 0 0 0 10 0.001\nGE 1\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let pec = buried_wire_geometry_error(&segs, &GroundModel::PerfectConductor)
            .expect("a wire reaching z=0 over PEC ground must be rejected");
        assert!(pec.contains("buried-wire"), "{pec}");
        assert!(buried_wire_geometry_error(
            &segs,
            &GroundModel::SimpleFiniteGround {
                eps_r: 13.0,
                sigma: 0.005
            }
        )
        .is_some());
        // Free space has no interface to be buried in.
        assert_eq!(
            buried_wire_geometry_error(&segs, &GroundModel::FreeSpace),
            None
        );
    }

    // -----------------------------------------------------------------------
    // negative_resistance_warning
    // -----------------------------------------------------------------------

    const STRAIGHT: &str =
        "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    // An inverted-V fed away from the apex: two wires meeting at a bend, which is
    // a genuine junction. Solves to Re Z = -5.973 Ω on the Hallén path.
    const BENT: &str = "GW 1 21 -5.0 0 0.0 0.0 0 3.0 0.001\nGW 2 21 0.0 0 3.0 5.0 0 0.0 0.001\nGE 0\nEX 0 1 5 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

    #[test]
    fn a_swept_low_ground_caveat_names_the_frequency_and_the_affected_count() {
        // Antenna 0.634 m up over GN 2: 0.030 lambda at 14.2 MHz, 0.127 lambda at
        // 60 MHz. A sweep across that straddles the 0.1 lambda threshold, so the
        // quoted height is true of some points and not others — and saying which
        // is the whole job of this annotation.
        let (_deck, segs) = deck_and_segs(
            "GW 1 21 -5.282 0 0.634 5.282 0 0.634 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let gn2 = GroundModel::SimpleFiniteGround {
            eps_r: 13.0,
            sigma: 0.005,
        };
        let freqs = [14.2e6, 30.0e6, 60.0e6];
        let w = swept_low_ground_caveat(&segs, &gn2, &freqs, false).expect("caveat");
        assert!(w.contains("worst case, at 14.2"), "{w}");
        assert!(w.contains("2 of 3 swept frequencies"), "{w}");

        // A single-frequency "sweep" earns the bare caveat: there is no other point
        // for the reader to mistake it for.
        let single = swept_low_ground_caveat(&segs, &gn2, &[14.2e6], false).expect("caveat");
        assert!(!single.contains("worst case"), "{single}");

        // Descending order must not change the answer.
        let descending = [60.0e6, 30.0e6, 14.2e6];
        assert_eq!(
            swept_low_ground_caveat(&segs, &gn2, &descending, false),
            Some(w)
        );

        // Entirely above the threshold: silence.
        assert_eq!(
            swept_low_ground_caveat(&segs, &gn2, &[60.0e6, 80.0e6], false),
            None
        );
    }

    #[test]
    fn a_deck_driven_by_two_kinds_of_source_is_refused_by_name() {
        // FND-036. This reported 0.678 + j0.086 Ω for the voltage feedpoint where
        // the same deck without the current source gives 74.243 + j13.900 — a
        // hundredfold error, exit 0, no warning. The current-source solve replaces
        // the right-hand side, so the delta gap was priced over currents its own
        // drive never produced.
        let (deck, _segs) = deck_and_segs(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nEX 4 1 13 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let msg = mixed_excitation_error(&deck).expect("a refusal");
        // Both cards named: "remove one" is unactionable if the reader cannot tell
        // which two are fighting.
        assert!(msg.contains("type 0 on tag 1 segment 26"), "{msg}");
        assert!(msg.contains("type 4 on tag 1 segment 13"), "{msg}");
    }

    #[test]
    fn a_single_drive_kind_is_not_refused() {
        // Both controls matter: firing on an ordinary deck would refuse every
        // corpus case, and firing on neither would leave FND-036 open.
        for deck_src in [
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 4 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        ] {
            let (deck, _segs) = deck_and_segs(deck_src);
            assert_eq!(mixed_excitation_error(&deck), None, "{deck_src}");
        }
    }

    #[test]
    fn a_plane_wave_beside_a_driven_source_is_out_of_scope_here() {
        // A different mix — receive versus transmit — routed elsewhere, and with
        // its own wrong answer (FND-050). Sweeping it in here would also refuse a
        // corpus fixture two frontends' tests depend on, so the scope boundary is
        // deliberate rather than an oversight.
        let (deck, _segs) = deck_and_segs(
            "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 1 1 3 0 0.0 0.0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(mixed_excitation_error(&deck), None);
    }

    #[test]
    fn a_current_source_only_deck_is_declined_by_name_not_called_cardless() {
        // FND-038. The GUI and the bindings fell through their feedpoint loop to
        // "deck has no EX card" — false for a deck that plainly has one, and it
        // sends the reader looking for a missing card instead of the real reason.
        let (deck, _segs) = deck_and_segs(
            "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 4 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let msg = unpriceable_feedpoint_error(&deck, "use the fnec CLI").expect("a named reason");
        assert!(msg.contains("current source"), "{msg}");
        assert!(msg.contains("tag 1 segment 11"), "{msg}");
        assert!(msg.contains("use the fnec CLI"), "{msg}");
        assert!(
            !msg.contains("no EX card"),
            "must not blame a card the deck has: {msg}"
        );
    }

    #[test]
    fn a_deck_with_both_source_kinds_needs_no_excuse() {
        // Priced from the delta gap, so there is nothing to decline. Without this
        // the check could fire on any deck containing a current source at all.
        let (deck, _segs) = deck_and_segs(
            "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 4 1 5 0 1.0 0.0\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(unpriceable_feedpoint_error(&deck, "use the fnec CLI"), None);
    }

    #[test]
    fn a_deck_with_no_feedpoint_at_all_gets_no_current_source_excuse() {
        // A genuinely cardless deck must still fall through to the caller's own
        // message, or this would replace one wrong reason with another.
        let (deck, _segs) =
            deck_and_segs("GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nFR 0 1 0 0 14.2 0.0\nEN\n");
        assert_eq!(unpriceable_feedpoint_error(&deck, "use the fnec CLI"), None);
    }

    #[test]
    fn a_non_negative_resistance_produces_no_warning() {
        let (deck, segs) = deck_and_segs(STRAIGHT);
        assert_eq!(
            negative_resistance_warning(74.24, 1, 11, &deck, &segs, SolverContext::cli_hallen()),
            None
        );
        // Exactly zero is not negative. A passive antenna can present a very small
        // resistance; only a sign change is the impossible-result signal.
        assert_eq!(
            negative_resistance_warning(0.0, 1, 11, &deck, &segs, SolverContext::cli_hallen()),
            None
        );
    }

    #[test]
    fn a_junctioned_geometry_is_told_the_junction_cause() {
        let (deck, segs) = deck_and_segs(BENT);
        let w =
            negative_resistance_warning(-5.973, 1, 5, &deck, &segs, SolverContext::cli_hallen())
                .expect("warning");
        assert!(w.contains("negative resistance"), "{w}");
        assert!(w.contains("Re Z = -5.973"), "{w}");
        assert!(w.contains("PH9-CHK-002"), "{w}");
        assert!(
            !w.contains("no wire junction"),
            "must not deny the junction this deck has: {w}"
        );
    }

    #[test]
    fn a_junctionless_geometry_is_not_blamed_on_a_junction_it_lacks() {
        let (deck, segs) = deck_and_segs(STRAIGHT);
        let w = negative_resistance_warning(-1.0, 1, 11, &deck, &segs, SolverContext::cli_hallen())
            .expect("warning");
        assert!(w.contains("no wire junction"), "{w}");
        // This deck the MPIE can take, so the cross-check is worth offering.
        assert!(w.contains("--solver mpie"), "{w}");
    }

    #[test]
    fn a_loaded_junctionless_deck_is_not_sent_to_a_solver_that_rejects_it() {
        // The mirror of `an_unsupported_topology_with_a_load_is_told_support_is_deferred_not_to_use_mpie`.
        // The MPIE rejects `LD`, so recommending it as a cross-check here would send
        // the reader to a solver that refuses the deck — the exact failure
        // `mpie_compatible_deck` exists to prevent.
        let (deck, segs) = deck_and_segs(
            "GW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nLD 0 1 11 11 50.0 0.0 0.0\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let w = negative_resistance_warning(-1.0, 1, 11, &deck, &segs, SolverContext::cli_hallen())
            .expect("warning");
        assert!(w.contains("no wire junction"), "{w}");
        assert!(
            !w.contains("--solver mpie"),
            "must not recommend a solver that rejects this deck: {w}"
        );
        assert!(w.contains("cross-check is unavailable"), "{w}");
    }

    #[test]
    fn geometry_error_reports_the_ground_dependent_check_too() {
        // Ordering matters: this deck is clean until the ground model is known.
        let (deck, segs) = deck_and_segs(
            "GW 1 21 0 0 0 0 0 10 0.001\nGE 1\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(geometry_error(&deck, &segs, &GroundModel::FreeSpace), None);
        assert!(geometry_error(&deck, &segs, &GroundModel::PerfectConductor).is_some());
    }

    #[test]
    fn a_low_antenna_over_finite_ground_warns_unless_the_surface_wave_was_modelled() {
        // 0.05 λ up at 14.2 MHz — inside the < 0.1 λ band.
        let (_deck, segs) = deck_and_segs(
            "GW 1 21 -5.278 0 1.056 5.278 0 1.056 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let gn2 = GroundModel::SimpleFiniteGround {
            eps_r: 13.0,
            sigma: 0.005,
        };
        assert!(low_finite_ground_warning(&segs, &gn2, 14.2e6, false).is_some());
        // A caller that modelled the surface wave must not be told it did not.
        assert_eq!(low_finite_ground_warning(&segs, &gn2, 14.2e6, true), None);
        // Free space has no finite ground to be low over.
        assert_eq!(
            low_finite_ground_warning(&segs, &GroundModel::FreeSpace, 14.2e6, false),
            None
        );
        // Raising the antenna above 0.1 λ clears it.
        let (_d, high) = deck_and_segs(
            "GW 1 21 -5.278 0 5.0 5.278 0 5.0 0.001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert_eq!(low_finite_ground_warning(&high, &gn2, 14.2e6, false), None);
    }

    /// The predicate a diagnostic uses before offering a junction-based
    /// explanation. A straight wire has no junction however it is written.
    #[test]
    fn has_wire_junction_distinguishes_a_bend_from_a_straight_wire() {
        let (_d, straight) = deck_and_segs(CLEAN_DIPOLE);
        assert!(
            !has_wire_junction(&straight),
            "a single straight wire has no junction"
        );

        // A straight conductor split across two GW cards is still straight — the
        // solver merges it, so a diagnostic must not call it a junction either.
        let (_d, split) = deck_and_segs(
            "GW 1 11 -5.278 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5.278 0 0 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert!(
            !has_wire_junction(&split),
            "a collinear split is merged, so it is not a junction"
        );

        // A real bend is.
        let (_d, bent) = deck_and_segs(
            "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 0 0 5 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert!(has_wire_junction(&bent), "a bend is a junction");
    }

    #[test]
    fn a_degree_three_junction_warns_and_recommends_the_mpie() {
        let (deck, segs) = deck_and_segs(
            "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5 0 0 0.001\nGW 3 11 0 0 0 0 0 5 0.001\nGE\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let w = unsupported_topology_warning(&deck, &segs, "re-run with `--solver mpie`")
            .expect("T junction must warn");
        assert!(w.contains("three or more wires"), "{w}");
        assert!(
            w.contains("--solver mpie"),
            "an MPIE-capable deck must be pointed at it: {w}"
        );
        // A straight dipole has no such topology.
        let (d2, s2) = deck_and_segs(CLEAN_DIPOLE);
        assert_eq!(
            unsupported_topology_warning(&d2, &s2, "re-run with `--solver mpie`"),
            None
        );
    }

    #[test]
    fn an_unsupported_topology_with_a_load_is_told_support_is_deferred_not_to_use_mpie() {
        // The MPIE rejects LD loads, so recommending it here would send the user
        // to a solver that refuses the deck.
        let (deck, segs) = deck_and_segs(
            "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5 0 0 0.001\nGW 3 11 0 0 0 0 0 5 0.001\nGE\nLD 4 1 6 6 50.0 0.0\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert!(!mpie_compatible_deck(&deck));
        let w = unsupported_topology_warning(&deck, &segs, "re-run with `--solver mpie`")
            .expect("still an unsupported topology");
        assert!(
            !w.contains("--solver mpie"),
            "must not recommend a solver that rejects the deck: {w}"
        );
        assert!(w.contains("deferred"), "{w}");
    }

    /// A finite field can still become an infinite *frequency*: every frontend
    /// multiplies MHz by 1e6. Measured before this check, with every field finite
    /// so the parser passed it: `FR 0 1 0 0 1e303 0` gave `FREQ_MHZ inf`, `NaN`
    /// currents, and `Z = 1.000000 + j0.000000` — both defects this module exists
    /// to stop, from a deck it accepted.
    #[test]
    fn a_frequency_that_overflows_to_infinity_in_hertz_is_refused() {
        let deck = |fr: &str| {
            parse(&format!(
                "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\n{fr}\nEX 0 1 11 0 1.0 0.0\nEN\n"
            ))
            .expect("deck parses")
            .deck
        };
        assert!(frequency_error(&deck("FR 0 1 0 0 1e303 0")).is_some());
        // ...and when the *generated* list overflows rather than the start value.
        assert!(frequency_error(&deck("FR 0 3 0 0 1e308 1e308")).is_some());
        // Multiplicative: the start is a finite 1e300 MHz, and the fifth step
        // (1e300 x 1000^4 = 1e312) is not. `1e10 x 100^39` looked like an
        // overflow when I first wrote this and is merely 1e88 — the case has to
        // be computed, not eyeballed.
        assert!(frequency_error(&deck("FR 1 5 0 0 1e300 1000.0")).is_some());
        // Negative control: an ordinary large-but-sane frequency still passes.
        assert_eq!(frequency_error(&deck("FR 0 1 0 0 30000.0 0")), None);
    }

    /// The check must not build the list it validates. `steps` is a `u32`, and
    /// this function runs inside `pre_solve_error` — which the GUI calls on every
    /// Apply+Solve and the worker on every task — so collecting `steps` floats
    /// would let two integers stall or abort those processes from inside
    /// validation. A defect introduced by a check is still a defect.
    ///
    /// **The step count is deliberately modest, and that is the point of this
    /// comment.** Timing is the only signal that distinguishes bounded from
    /// unbounded here — the two agree on every verdict, which is why the bounded
    /// form is correct — so the test has to be *sabotaged* to prove it bites. A
    /// first version used 4e9 steps; sabotaging it asked for a 32 GB allocation
    /// and took the development machine down. 50 M keeps the sabotage at ~400 MB
    /// and still leaves three orders of magnitude between the two behaviours, so
    /// the threshold is nowhere near the noise even on a loaded machine.
    #[test]
    fn the_frequency_check_is_bounded_for_an_enormous_step_count() {
        let deck = parse(
            "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nFR 0 50000000 0 0 14.0 0.0\n\
             EX 0 1 11 0 1.0 0.0\nEN\n",
        )
        .expect("deck parses")
        .deck;
        let t = std::time::Instant::now();
        let _ = frequency_error(&deck);
        let ms = t.elapsed().as_millis();
        assert!(
            ms < 300,
            "frequency_error took {ms} ms for a 50M-step FR card; it is expanding \
             the list instead of taking its extremes"
        );
    }

    /// Each degenerate class gets its own case, because they reach the same
    /// wrong answer by different routes and a single "bad frequency" test would
    /// pass while any one of them regressed.
    /// FND-039: a deck with an unrecognised `EX` type looked clean in the GUI's
    /// keystroke-time strip and then failed on Solve, with nothing in between.
    /// Measured before the fix: `deck_warnings` returned `[]` and the solve
    /// returned `EX: unknown excitation type (I1=9, ...)`.
    #[test]
    fn an_unrecognised_excitation_type_earns_a_caveat() {
        let deck = parse(
            "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nFR 0 1 0 0 14.2 0\n\
             EX 9 1 11 0 1.0 0.0\nEN\n",
        )
        .expect("deck parses")
        .deck;
        let w = unrecognised_excitation_warnings(&deck);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("type 9") && w[0].contains("tag 1"), "{w:?}");
        // It must not imply the deck might work: the solve refuses it.
        assert!(w[0].contains("refuse"), "{w:?}");
    }

    /// It reaches `diagnose` as a **warning**, because the GUI's caveat strip
    /// renders warnings and swallows errors — an error here would leave the
    /// original defect exactly as it was.
    #[test]
    fn the_unrecognised_excitation_caveat_is_a_warning_not_an_error() {
        let (deck, segs) = deck_and_segs(
            "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nFR 0 1 0 0 14.2 0\n\
             EX 9 1 11 0 1.0 0.0\nEN\n",
        );
        let diags = diagnose(
            &deck,
            &segs,
            &GroundModel::FreeSpace,
            14.2e6,
            SolverContext::cli_hallen(),
        );
        assert!(
            diags
                .iter()
                .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("type 9")),
            "{diags:?}"
        );
        assert!(
            !has_error(&diags),
            "an error would be swallowed by the strip"
        );
    }

    /// Negative control: every canonical type 0-5 is recognised, so an ordinary
    /// deck earns no such caveat. Swept from the type list rather than spot-checked,
    /// so a new canonical type cannot be silently reported as unrecognised.
    #[test]
    fn no_canonical_excitation_type_is_called_unrecognised() {
        for t in 0..=5 {
            let deck = parse(&format!(
                "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nFR 0 1 0 0 14.2 0\n\
                 EX {t} 1 11 0 1.0 0.0\nEN\n"
            ))
            .expect("deck parses")
            .deck;
            assert!(
                unrecognised_excitation_warnings(&deck).is_empty(),
                "EX type {t} is canonical and must not be called unrecognised"
            );
        }
    }

    #[test]
    fn every_degenerate_frequency_class_is_refused() {
        let deck_with = |fr: &str| {
            parse(&format!(
                "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\n{fr}\nEX 0 1 11 0 1.0 0.0\nEN\n"
            ))
            .expect("deck parses")
            .deck
        };
        for (fr, why) in [
            ("FR 0 1 0 0 0.0 0", "zero"),
            ("FR 0 1 0 0 -14.2 0", "negative"),
            // Descending past zero: the start value is a perfectly good 10 MHz,
            // so a check on the card's first field alone would pass this. It
            // solves and reports FREQ_MHZ -2.000000 on main.
            ("FR 0 5 0 0 10.0 -3.0", "descending sweep through zero"),
            // Multiplicative (step_type is FR's *first* field) with a zero
            // ratio: every step after the first collapses to 0 MHz.
            ("FR 1 3 0 0 14.2 0.0", "multiplicative collapse to zero"),
        ] {
            assert!(
                frequency_error(&deck_with(fr)).is_some(),
                "{why} frequency must be refused: {fr}"
            );
        }
    }

    #[test]
    fn an_ordinary_deck_and_a_single_point_sweep_are_not_refused() {
        let deck_with = |fr: &str| {
            parse(&format!(
                "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\n{fr}\nEX 0 1 11 0 1.0 0.0\nEN\n"
            ))
            .expect("deck parses")
            .deck
        };
        // Every corpus deck is `FR 0 1 0 0 f 0.0` — a step of 0.0 with one step
        // must stay legal, or the whole corpus is refused.
        assert_eq!(frequency_error(&deck_with("FR 0 1 0 0 14.2 0.0")), None);
        assert_eq!(frequency_error(&deck_with("FR 0 5 0 0 14.0 0.1")), None);
        assert_eq!(frequency_error(&deck_with("FR 1 3 0 0 14.0 2.0")), None);
    }

    /// The CLI expands only the first `FR` card; `fnec_py` expands them all. The
    /// validator must cover the union, so a bad frequency in a later card cannot
    /// slip through on the frontend that reads it.
    #[test]
    fn a_degenerate_frequency_in_a_later_fr_card_is_still_refused() {
        let deck = parse(
            "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nFR 0 1 0 0 14.2 0\n\
             FR 0 1 0 0 -7.1 0\nEX 0 1 11 0 1.0 0.0\nEN\n",
        )
        .expect("deck parses")
        .deck;
        assert!(
            frequency_error(&deck).is_some(),
            "a bad frequency in the second FR card must be refused"
        );
    }

    #[test]
    fn an_mpie_deck_with_a_load_is_an_error_from_diagnose_not_a_warning() {
        // The severity is the whole point and nothing else pins it: the shared
        // `solve_mpie_session` refuses this deck anyway and with the identical
        // text, so downgrading this arm to a Warning is invisible through any
        // solve. What it changes is *when* the user learns — an Error is the
        // pre-solve refusal, while a Warning would let the caveat strip render it
        // and the solve proceed to fail later (FND-054).
        let (deck, segs) = deck_and_segs(
            "GW 1 41 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nLD 0 1 21 21 10.0 0.0 0.0\n\
             EX 0 1 21 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
        );
        let ctx = SolverContext {
            kind: SolverKind::Mpie,
            mpie_remedy: "unused on the MPIE arm",
        };
        let diags = diagnose(&deck, &segs, &GroundModel::FreeSpace, 14.2e6, ctx);
        assert!(
            has_error(&diags),
            "an LD deck on the MPIE must be an Error, not a caveat: {diags:?}"
        );
        // ...and on Hallén the same deck is fine, so this is the solver's limit.
        let hallen = diagnose(
            &deck,
            &segs,
            &GroundModel::FreeSpace,
            14.2e6,
            SolverContext::cli_hallen(),
        );
        assert!(!has_error(&hallen), "the LD deck must solve on Hallén");
    }

    #[test]
    fn an_unrecognised_ge_flag_and_a_deferred_gn_type_each_warn() {
        let (deck, _segs) = deck_and_segs(
            "GW 1 21 -5.278 0 1 5.278 0 1 0.001\nGE 7\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        let w = ge_ground_reflection_warning(&deck).expect("GE I1=7 is not recognised");
        assert!(w.contains("GE I1=7"), "{w}");
        // The supported flags are silent.
        let (ok, _) = deck_and_segs(CLEAN_DIPOLE);
        assert_eq!(ge_ground_reflection_warning(&ok), None);

        let deferred = GroundModel::Deferred {
            gn_type: 4,
            eps_r: Some(13.0),
            sigma: Some(0.005),
        };
        let w = deferred_ground_warning(&deferred).expect("a deferred GN type warns");
        assert!(w.contains("GN type 4") && w.contains("EPSE=13"), "{w}");
        assert_eq!(deferred_ground_warning(&GroundModel::FreeSpace), None);
    }
}
