// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Frontend-agnostic pre-solve validation and diagnostics.
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

use nec_model::card::Card;
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

pub fn source_risk_geometry_error(cards: &[Card], segs: &[Segment]) -> Option<String> {
    const MIN_SOURCE_LENGTH_TO_RADIUS_RATIO: f64 = 2.0;

    for card in cards {
        if let Card::Ex(ex) = card {
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
// Deck capability
// ---------------------------------------------------------------------------

/// Whether a deck can be solved on the MPIE path: driven by at least one voltage
/// source (`EX` type 0), with no loads / transmission lines / networks and no
/// plane-wave or current-source excitation (all unsupported by the MPIE).
///
/// Used to decide whether a diagnostic may recommend the MPIE as a remedy.
pub fn mpie_compatible_deck(deck: &NecDeck) -> bool {
    let mut has_voltage_source = false;
    for card in &deck.cards {
        match card {
            Card::Ld(_) | Card::Tl(_) | Card::Nt(_) => return false,
            Card::Ex(ex) => {
                if ex.kind().is_plane_wave()
                    || ex.kind() == nec_model::card::ExcitationKind::CurrentSource
                {
                    return false;
                }
                has_voltage_source = true;
            }
            _ => {}
        }
    }
    has_voltage_source
}

// ---------------------------------------------------------------------------
// Warnings
// ---------------------------------------------------------------------------

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
pub fn unsupported_topology_warning(deck: &NecDeck, segs: &[Segment]) -> Option<String> {
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
        "so the reported impedance, currents, and pattern are unreliable — re-run with \
         `--solver mpie`, which solves this geometry correctly (PH9-CHK-007)"
    } else {
        "so the reported impedance, currents, and pattern for this geometry are unreliable \
         (support for this combination is deferred — see PH9-CHK-002)"
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
    for card in &deck.cards {
        let Card::Ex(ex) = card else { continue };
        if ex.kind().is_plane_wave() {
            continue; // receiving antenna, no feedpoint
        }
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
// Aggregate
// ---------------------------------------------------------------------------

/// The first hard geometry error, in the order the checks must run: the two that
/// need only the wire layout, then the one that needs the resolved ground model.
///
/// `Some(_)` means the deck is outside the supported class and the solve must not
/// run — the string is the message to show the user.
pub fn geometry_error(deck: &NecDeck, segs: &[Segment], ground: &GroundModel) -> Option<String> {
    segment_intersection_error(segs)
        .or_else(|| source_risk_geometry_error(&deck.cards, segs))
        .or_else(|| buried_wire_geometry_error(segs, ground))
}

/// Every frontend-independent diagnostic for a solve, errors first.
///
/// A single `Error` (if any) is the hard geometry rejection from [`geometry_error`];
/// a caller that sees one must not solve. The rest are warnings the caller should
/// surface but may otherwise ignore.
///
/// Solver-specific caveats (experimental basis modes, mixed radii on the MPIE,
/// execution-mode fallback, negative resistance, a declined Sommerfeld request) are
/// *not* here — they depend on options only the CLI exposes, and it emits them
/// itself.
pub fn diagnose(
    deck: &NecDeck,
    segs: &[Segment],
    ground: &GroundModel,
    freq_hz: f64,
) -> Vec<ValidationDiagnostic> {
    let mut out = Vec::new();
    if let Some(e) = geometry_error(deck, segs, ground) {
        out.push(ValidationDiagnostic::error(e));
    }
    for w in [
        ge_ground_reflection_warning(deck),
        deferred_ground_warning(ground),
        unsupported_topology_warning(deck, segs),
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
        let diags = diagnose(&deck, &segs, &GroundModel::FreeSpace, 14.2e6);
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
        let diags = diagnose(&deck, &segs, &GroundModel::FreeSpace, 14.2e6);
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
        let err = source_risk_geometry_error(&deck.cards, &segs).expect("tiny source segment");
        assert!(err.contains("source-risk"), "{err}");
        // The same wire without the EX card on it is fine — the check is about the source.
        assert_eq!(source_risk_geometry_error(&[], &segs), None);
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
        let w = unsupported_topology_warning(&deck, &segs).expect("T junction must warn");
        assert!(w.contains("three or more wires"), "{w}");
        assert!(
            w.contains("--solver mpie"),
            "an MPIE-capable deck must be pointed at it: {w}"
        );
        // A straight dipole has no such topology.
        let (d2, s2) = deck_and_segs(CLEAN_DIPOLE);
        assert_eq!(unsupported_topology_warning(&d2, &s2), None);
    }

    #[test]
    fn an_unsupported_topology_with_a_load_is_told_support_is_deferred_not_to_use_mpie() {
        // The MPIE rejects LD loads, so recommending it here would send the user
        // to a solver that refuses the deck.
        let (deck, segs) = deck_and_segs(
            "GW 1 11 -5 0 0 0 0 0 0.001\nGW 2 11 0 0 0 5 0 0 0.001\nGW 3 11 0 0 0 0 0 5 0.001\nGE\nLD 4 1 6 6 50.0 0.0\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
        );
        assert!(!mpie_compatible_deck(&deck));
        let w = unsupported_topology_warning(&deck, &segs).expect("still an unsupported topology");
        assert!(
            !w.contains("--solver mpie"),
            "must not recommend a solver that rejects the deck: {w}"
        );
        assert!(w.contains("deferred"), "{w}");
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
