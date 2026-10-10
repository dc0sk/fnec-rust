// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Minimal Hallén solve path used by the distributed worker.
//!
//! Only `basis = "hallen"` is supported.  Other basis values return
//! [`SolveError::UnsupportedConfig`].  This is sufficient for PH6-CHK-006;
//! additional bases are added in subsequent milestones.

#[cfg(test)]
mod tests {
    use super::*;

    const DIPOLE: &str = include_str!("../../../corpus/dipole-freesp-51seg.nec");
    const DIPOLE_EX5: &str = include_str!("../../../corpus/dipole-ex5-freesp-51seg.nec");
    const DIPOLE_EX4: &str = include_str!("../../../corpus/dipole-ex4-freesp-51seg.nec");
    const DIPOLE_EX1: &str = include_str!("../../../corpus/dipole-ex1-freesp-51seg.nec");

    /// FND-021: an `EX` naming a segment the geometry does not contain is a
    /// semantic error, not a syntax one. It crossed the wire as `parse_error`,
    /// sending the reader to hunt for a typo in a deck that parsed cleanly.
    #[test]
    fn a_bad_ex_reference_is_not_reported_as_a_parse_error() {
        let deck = "CM EX names a segment the wire does not have\nCE\nGW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 99 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let err = solve_deck_at_frequency(deck, 14.2e6, "hallen").unwrap_err();
        match &err {
            SolveError::UnsupportedConfig(m) => {
                assert!(m.contains("segment"), "{m}");
            }
            other => panic!("expected UnsupportedConfig, got {other:?}"),
        }
        assert!(
            !err.to_string().starts_with("parse error"),
            "the deck parsed; blaming its syntax misdirects: {err}"
        );
    }

    /// The negative control: a deck that genuinely does not parse must still be a
    /// parse error, or the fix above would have traded one mislabel for another.
    ///
    /// The obvious fixture does not work. Free text — "this is not a NEC deck" —
    /// parses cleanly and fails later as `GeometryError("deck contains no GW
    /// cards")`, which `solve_rejects_garbage_input` already pins. A real parse
    /// failure needs a card the parser recognises carrying a field it cannot read.
    #[test]
    fn genuinely_unparseable_input_is_still_a_parse_error() {
        let deck = "CE\nGW 1 abc 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEN\n";
        let err = solve_deck_at_frequency(deck, 14.2e6, "hallen").unwrap_err();
        assert!(
            matches!(err, SolveError::ParseError(_)),
            "expected ParseError, got {err:?}"
        );
        assert!(err.to_string().starts_with("parse error"), "{err}");
    }

    /// FND-026: the worker built these warnings and threw them away, so a
    /// distributed run was the one frontend that silently ignored a malformed
    /// card. Unlike the result-shape checks the controller does for itself, these
    /// have to travel on the wire — the controller never parses the deck's stamps,
    /// so if the worker drops them nobody ever learns the card was skipped.
    #[test]
    fn a_malformed_card_the_worker_skips_is_reported_not_swallowed() {
        // A card note that accompanies a successful solve: segment 0 read as the
        // tag's centre. (This used a malformed NT, then an unsupported LD; both are
        // refused now rather than skipped, FND-123 and FND-161.)
        let deck = "CM NT given by segment 0\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nNT 1 0 1 0 0.001 0 0 0 0 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let r = solve_deck_at_frequency(deck, 14.2e6, "hallen").expect("deck still solves");
        assert!(
            r.warnings
                .iter()
                .any(|w| w.contains("interpreting segment 0 as center segment 26")),
            "the skipped card must be reported: {:?}",
            r.warnings
        );
    }

    /// FND-041: the worker parsed the deck, produced caveats, and dropped them on
    /// the next line (`let deck = parse_result.deck;`). Masked for the CLI, which
    /// parses the identical bytes locally and prints its own — so the loss was
    /// invisible for exactly one caller and total for every other, including
    /// anything driving the public `run_worker_stdio`.
    #[test]
    fn a_parse_caveat_reaches_the_caller_instead_of_being_dropped() {
        // An unrecognised card is a *warning*, not an error: the deck still
        // solves, and the user still needs to know a line was ignored.
        let deck = "CM unknown card\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\n\
                    ZZ 1 2 3\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let r = solve_deck_at_frequency(deck, 14.2e6, "hallen").expect("deck still solves");
        assert!(
            r.warnings.iter().any(|w| w.contains("ZZ")),
            "the ignored card must reach the caller: {:?}",
            r.warnings
        );
    }

    #[test]
    fn a_clean_deck_reports_no_warnings() {
        let r = solve_deck_at_frequency(DIPOLE, 14.2e6, "hallen").expect("solve");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    /// FND-031: the worker drove a type-5 card as a delta gap through
    /// `build_hallen_rhs`, solved it, and then refused to read the answer —
    /// "no EX type-0 card found in deck" for a deck the CLI and `fnec_py` both
    /// solve to 78.834 + j42.440 Ω (74.243 + j13.900 before FND-156). The distributed path rejected a deck the
    /// other three frontends handle.
    #[test]
    fn a_type_5_voltage_source_is_a_feedpoint_here_as_it_is_everywhere_else() {
        let r = solve_deck_at_frequency(DIPOLE_EX5, 14.2e6, "hallen")
            .expect("EX 5 is a voltage source fnec models as a delta gap");
        // The corpus reference for this deck, which the CLI already matches.
        assert!(
            (r.impedance_re - 78.83).abs() < 0.1,
            "R = {} Ω",
            r.impedance_re
        );
        assert!(
            (r.impedance_im - 42.44).abs() < 0.1,
            "X = {} Ω",
            r.impedance_im
        );
    }

    /// FND-051: the worker was the last frontend refusing a current-source deck,
    /// while the CLI, the GUI and `fnec_py` all priced one. That was a scope
    /// choice, not a technical one — the machinery has been in `nec_solver` since
    /// #412, and `FeedpointResult` already carried impedance and current, so the
    /// wire format never needed the port voltage.
    ///
    /// The assertion is the CLI's corpus-pinned value, so the four frontends
    /// cannot drift apart.
    /// The GPU-resident path solves a delta-gap right-hand side from raw segment
    /// inputs, so it cannot serve a current source at all — it must stay on the
    /// CPU rather than being answered with the wrong physics.
    #[test]
    fn a_current_source_deck_never_takes_the_gpu_resident_path() {
        let r = solve_deck_at_frequency_with_exec(DIPOLE_EX4, 14.2e6, "hallen", "gpu")
            .expect("still solved");
        assert_eq!(
            r.exec_used, "cpu",
            "a current source must not go to the GPU"
        );
        // ...and the answer is the same one the CPU path gives.
        assert!(
            (r.impedance_re - 78.834228).abs() < 0.05,
            "{}",
            r.impedance_re
        );
    }

    #[test]
    fn a_current_source_deck_is_priced_and_agrees_with_the_cli() {
        let r = solve_deck_at_frequency(DIPOLE_EX4, 14.2e6, "hallen")
            .expect("the worker prices a current source now");
        assert!(
            (r.impedance_re - 78.834228).abs() < 0.05 && (r.impedance_im - 42.439515).abs() < 0.05,
            "worker gave {} + j{}, CLI gives 78.834228 + j42.439515",
            r.impedance_re,
            r.impedance_im
        );
        // The reported current is the impressed one, which is what was asked for.
        assert!((r.current_mag - 1.0).abs() < 1e-9, "{}", r.current_mag);
    }

    /// A plane wave has no feedpoint at all: its tag/segment fields carry
    /// NTHETA/NPHI. It must not be read as one, and the refusal must say where a
    /// receive deck CAN be solved (FND-108).
    #[test]
    fn a_plane_wave_deck_is_refused_as_a_receive_deck_with_a_route() {
        let err = solve_deck_at_frequency(DIPOLE_EX1, 14.2e6, "hallen").unwrap_err();
        // Pinned to the message, not only the variant: FND-035's spurious
        // source-risk rejection is also an `UnsupportedConfig`, raised by the
        // same `pre_solve_error` earlier, and must not keep this green.
        let SolveError::UnsupportedConfig(m) = &err else {
            panic!("{err:?}")
        };
        assert!(m.contains("receiving antenna has no feedpoint"), "{m}");
        assert!(m.contains("solve_currents_deck_str"), "{m}");
    }

    #[test]
    fn solve_dipole_at_resonance() {
        let result = solve_deck_at_frequency(DIPOLE, 14.175e6, "hallen").unwrap();
        // Free-space half-wave dipole at resonance: ~73 + j13 Ω
        assert!(
            result.impedance_re > 50.0 && result.impedance_re < 100.0,
            "R = {} Ω",
            result.impedance_re
        );
        assert!(
            result.impedance_im > -20.0 && result.impedance_im < 50.0,
            "X = {} Ω",
            result.impedance_im
        );
        assert!(result.current_mag > 0.0);
    }

    #[test]
    fn solve_rejects_unsupported_basis() {
        let err = solve_deck_at_frequency(DIPOLE, 14.0e6, "pulse").unwrap_err();
        assert!(matches!(err, SolveError::UnsupportedConfig(_)));
    }

    #[test]
    fn solve_rejects_empty_deck() {
        let err = solve_deck_at_frequency("", 14.0e6, "hallen").unwrap_err();
        assert!(
            matches!(err, SolveError::GeometryError(_)),
            "empty deck produced: {err}"
        );
    }

    #[test]
    fn solve_rejects_garbage_input() {
        let err = solve_deck_at_frequency("NOT A NEC DECK", 14.0e6, "hallen").unwrap_err();
        assert!(
            matches!(err, SolveError::GeometryError(_)),
            "garbage input produced: {err}"
        );
    }

    /// An undriven deck is refused for the reason it is undriven, not with the
    /// generic "no driven feedpoint".
    ///
    /// This used to expect `NoFeedpoint`, which the worker reached by solving the
    /// deck (to all zeros) and then failing to find a feedpoint to price. Both
    /// this deck and a plane-wave receive deck landed on that one variant, so the
    /// wire could not tell "nothing drives this structure" from "the drive is
    /// receive-only, which this API cannot price". `pre_solve_error` now refuses
    /// the first before any matrix is filled.
    ///
    /// `UnsupportedConfig` is an existing wire variant, so no controller breaks:
    /// adding an `ErrorCode` variant would have, since older controllers fail the
    /// whole result line on an unknown one.
    #[test]
    fn solve_undriven_deck_is_refused_by_reason() {
        // Deck with geometry but no EX card.
        let deck = "CM test\nGW 0 1 0 0 0 0 0 1 0.001\nGE 0\nFR 0 1 0 0 14.175 0\nEN\n";
        let err = solve_deck_at_frequency(deck, 14.175e6, "hallen").unwrap_err();
        match &err {
            SolveError::UnsupportedConfig(m) => assert!(
                m.contains("no EX card"),
                "the refusal must name the missing drive: {m}"
            ),
            other => panic!("expected UnsupportedConfig naming the missing EX, got {other:?}"),
        }
    }

    #[test]
    fn solve_error_display() {
        let err = SolveError::SingularMatrix("det=0".into());
        assert_eq!(err.to_string(), "singular matrix: det=0");
    }

    #[test]
    fn feedpoint_result_is_deterministic() {
        let a = solve_deck_at_frequency(DIPOLE, 14.0e6, "hallen").unwrap();
        let b = solve_deck_at_frequency(DIPOLE, 14.0e6, "hallen").unwrap();
        assert!((a.impedance_re - b.impedance_re).abs() < 1e-12);
        assert!((a.impedance_im - b.impedance_im).abs() < 1e-12);
    }
}

use nec_solver::{
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, ground_model_from_deck,
};
use num_complex::Complex64;

/// Feedpoint impedance and current at the first `EX` voltage source.
#[derive(Debug, Clone)]
pub struct FeedpointResult {
    pub impedance_re: f64,
    pub impedance_im: f64,
    pub current_mag: f64,
    pub current_phase_deg: f64,
    /// Execution path actually taken: `"cpu"` | `"gpu"` (PH7-CHK-004).
    pub exec_used: String,
    /// Caveats this solve earned: the deck's own parse warnings (FND-041),
    /// followed by those raised while filling the matrix — a malformed `LD`, `TL`
    /// or `NT` card that was skipped (FND-026). The worker is the only place the
    /// matrix-fill ones exist, so dropping them is the same as never producing
    /// them; the parse ones are merely *masked* for the CLI, which parses the
    /// same bytes locally, and lost outright for every other caller.
    pub warnings: Vec<String>,
}

// The shared floor below which the GPU-resident solve is not attempted.
use nec_accel::MIN_GPU_RESIDENT_SEGS;

/// What the worker says to a receive deck. The worker's protocol carries a
/// feedpoint impedance and nothing else, so the route is another frontend; the
/// wire format stays as it is (FND-108).
const RECEIVE_DECK_REMEDY: &str = "the distributed worker returns feedpoint impedances only; \
     solve it locally with the fnec CLI, the GUI's Currents tab, or \
     fnec_py.solve_currents_deck_str";

/// Errors from the worker solve path.
#[derive(Debug, Clone)]
pub enum SolveError {
    ParseError(String),
    GeometryError(String),
    SingularMatrix(String),
    UnsupportedConfig(String),
    /// The deck asks for more of some resource than this worker will allocate.
    ///
    /// Distinct from `UnsupportedConfig` because the remedy is different: the
    /// deck is well-formed and the geometry is supported, it is simply too big.
    /// `ErrorCode::ResourceExhausted` has been on the wire since this crate's
    /// first commit with no producer; this is what it was for (FND-125).
    ResourceExhausted(String),
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::ParseError(m) => write!(f, "parse error: {m}"),
            SolveError::GeometryError(m) => write!(f, "geometry error: {m}"),
            SolveError::SingularMatrix(m) => write!(f, "singular matrix: {m}"),
            SolveError::UnsupportedConfig(m) => write!(f, "unsupported config: {m}"),
            SolveError::ResourceExhausted(m) => write!(f, "resource exhausted: {m}"),
        }
    }
}

impl std::error::Error for SolveError {}

/// Run a Hallén solve on `deck_str` at `freq_hz` and return the feedpoint result.
///
/// The `basis` parameter must be `"hallen"`; any other value returns
/// [`SolveError::UnsupportedConfig`]. Always uses the CPU solve; the GPU-capable
/// variant is [`solve_deck_at_frequency_with_exec`].
pub fn solve_deck_at_frequency(
    deck_str: &str,
    freq_hz: f64,
    basis: &str,
) -> Result<FeedpointResult, SolveError> {
    solve_deck_at_frequency_with_exec(deck_str, freq_hz, basis, "cpu")
}

/// Run a Hallén solve, honouring an `exec` preference (`"cpu"` | `"gpu"`).
///
/// When `exec == "gpu"` and the deck is in the GPU-resident supported class
/// (free-space/deferred ground, no LD/TL stamps, ≥ [`MIN_GPU_RESIDENT_SEGS`]
/// segments), the worker dispatches `nec_accel::solve_hallen_gpu_resident` on the
/// merged-conductor grouping (PH7-CHK-003) and reports `exec_used = "gpu"`.
/// Otherwise — out of class, no wgpu adapter, or a declined or rejected device
/// answer — it runs the same routed f64 CPU solve as `exec = "cpu"`
/// (`exec_used = "cpu"`), so the two can differ only by the device's rounding
/// (FND-199). PH7-CHK-004.
pub fn solve_deck_at_frequency_with_exec(
    deck_str: &str,
    freq_hz: f64,
    basis: &str,
    exec: &str,
) -> Result<FeedpointResult, SolveError> {
    solve_deck_reporting_warnings(deck_str, freq_hz, basis, exec).0
}

/// The solve, plus the caveats the deck earned **whether or not it succeeded**.
///
/// A deck can be both flawed and refused — an unrecognised card *and* a bad `EX`
/// reference — and the plain `Result` shape loses the first when it reports the
/// second. On success the returned list is the same one carried in
/// [`FeedpointResult::warnings`]; on failure it is the only copy (FND-059).
pub fn solve_deck_reporting_warnings(
    deck_str: &str,
    freq_hz: f64,
    basis: &str,
    exec: &str,
) -> (Result<FeedpointResult, SolveError>, Vec<String>) {
    let mut warnings = Vec::new();
    let result = solve_inner(deck_str, freq_hz, basis, exec, &mut warnings);
    (result, warnings)
}

/// `warnings` is an out-parameter rather than part of the return type because
/// every early exit here is a `?`, and threading a second value through eight of
/// them would mean rewriting each one to say nothing new.
fn solve_inner(
    deck_str: &str,
    freq_hz: f64,
    basis: &str,
    exec: &str,
    warnings: &mut Vec<String>,
) -> Result<FeedpointResult, SolveError> {
    if basis != "hallen" {
        return Err(SolveError::UnsupportedConfig(format!(
            "basis '{basis}' not supported in worker; only 'hallen' is implemented"
        )));
    }

    // The frequency arrives on the wire, not in the deck, so `pre_solve_error`
    // below cannot see it — its `frequency_error` reads the deck's FR card and
    // takes no frequency argument at all. That left the one input the deck does
    // not carry as the one input never validated: a negative `frequency_hz`
    // returned the exact complex conjugate with `status: ok` and no warning,
    // on the very class the FR seam refuses for every deck-borne path
    // (FND-098). This end is authoritative because `run_worker_stdio` is public
    // API fed by arbitrary stdin and the controller may be a different version.
    if !nec_solver::is_usable_frequency_mhz(freq_hz / 1e6) {
        return Err(SolveError::UnsupportedConfig(format!(
            "frequency_hz {freq_hz} is not a usable frequency; \
             frequencies must be finite and > 0"
        )));
    }

    // 1. Parse
    let parse_result =
        nec_parser::parse(deck_str).map_err(|e| SolveError::ParseError(e.to_string()))?;
    let deck = parse_result.deck;
    // The parse caveats travel with the result. Discarding them here was the same
    // as never producing them for anything driving `run_worker_stdio`, which is
    // public API: the CLI happens to parse the identical bytes locally and print
    // its own, so the loss was masked for exactly one of the callers (FND-041).
    warnings.extend(parse_result.warnings.iter().map(ToString::to_string));

    // 2. Build geometry
    let segs = build_geometry(&deck).map_err(|e| match e {
        // Well-formed deck, supported geometry, simply too big — a different
        // remedy from "this config is not supported", so a different code.
        nec_solver::GeometryError::SegmentBudgetExceeded { .. } => {
            SolveError::ResourceExhausted(e.to_string())
        }
        other => SolveError::GeometryError(other.to_string()),
    })?;
    let ground = ground_model_from_deck(&deck);

    // 2b. Refuse geometry outside the solver's supported class, exactly as the CLI
    // does locally (FND-013).
    //
    // This is not a redundant second check behind the controller's: the worker is a
    // SEPARATELY INSTALLED binary, reached over SSH at whatever `binary_path` the
    // hosts file names, so it may be a different fnec version with a different
    // supported class. A controller can never speak for it. `run_worker_stdio` is
    // also public API fed by arbitrary stdin, so this is the only end that is
    // authoritative about what this build will solve.
    //
    // Reported as `UnsupportedConfig` rather than `GeometryError`: the latter falls
    // into `process_task`'s catch-all and would cross the wire labelled
    // `parse_error`, which is simply untrue and would send a user looking at their
    // deck's syntax. Adding a new `ErrorCode` variant instead would break older
    // controllers, which fail the whole result line on an unknown variant.
    if let Some(err) = nec_solver::validate::pre_solve_error(&deck, &segs, &ground) {
        return Err(SolveError::UnsupportedConfig(err));
    }

    // 2c. Stamps are computed here, before the RHS, because their *caveats* must
    // be in hand if the RHS build fails. `build_deck_stamps` needs only the deck,
    // the segments and the frequency, so nothing about the ordering is forced —
    // and with it after step 3, a deck that was both flawed (a skipped `LD`) and
    // refused (an `EX` naming a missing segment) reported the refusal with the
    // flaw missing, which is FND-059's own sentence one exit over. Applying them
    // stays at step 4, where the matrix exists.
    let stamps = nec_solver::build_deck_stamps(&deck, &segs, freq_hz);
    warnings.extend(stamps.warnings.iter().cloned());

    // 3. Build Hallén RHS
    let hallen_rhs = build_hallen_rhs(&deck, &segs, freq_hz).map_err(|e| {
        use nec_solver::ExcitationError;
        match e {
            ExcitationError::UnsupportedType { ex_type, .. } => SolveError::UnsupportedConfig(
                format!("EX type {ex_type} not supported in worker Hallén path"),
            ),
            // Not ParseError: the deck parsed. `ExcitationError` has exactly two
            // variants and `UnsupportedType` is matched above, so this arm is
            // `SegmentNotFound` and nothing else — an `EX` naming a segment the
            // geometry does not contain. Labelling that "parse error" sends the
            // reader hunting for a syntax mistake that is not there (FND-021), the
            // same mislabel class FND-013's fix avoided for geometry. It stays a
            // catch-all so a future variant lands on the safer of the two codes.
            //
            // `UnsupportedConfig` rather than a new `ErrorCode` variant: the enum
            // is serialised on the wire, so adding a variant breaks an older
            // controller's deserialisation outright — unlike the additive
            // `warnings` field of FND-026, which was compatible both ways.
            other => SolveError::UnsupportedConfig(other.to_string()),
        }
    })?;

    // 3b. A deck with nothing to price — a plane-wave receive deck — is refused
    // here, before the matrix fill it cannot use. It used to be solved in full
    // and then refused as `NoFeedpoint`, with a sentence that named no remedy.
    // After the stamps, so a flawed receive deck still reports its flaw (FND-059),
    // and after the RHS build, whose refusal of an EX type outside 0-5 names the
    // real cause — `feedpoints` skips such a card, so ahead of it this would
    // misreport the deck as a receive deck. Still before the O(N^2) fill.
    if nec_solver::feedpoints(&deck).next().is_none() {
        return Err(SolveError::UnsupportedConfig(
            nec_solver::validate::unpriceable_feedpoint_error(&deck, RECEIVE_DECK_REMEDY),
        ));
    }

    // 4. Assemble Z-matrix
    let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);
    // Loads and TL/NT networks reach the solve as data, applied by the routed
    // session in the basis that runs (FND-122, FND-123).

    // A current source is a real feedpoint, but it needs its own solve — the
    // excitation vector is all zeros, so `V/I` has nothing to divide. The routed
    // session runs it and returns the port voltage it is priced from (FND-051).
    let mut current_source_port: Option<Complex64> = None;

    // 6. Solve — GPU-resident (PH7-CHK-003) for the supported class when
    // requested, else the f64 CPU solve.
    // The route decides which member of the Hallén family this deck needs, and
    // it is the same decision every frontend makes (FND-121). The device path
    // below implements exactly one of them — the plain delta-gap solve — so it
    // must ask, rather than assume.
    let gpu_eligible = exec == "gpu"
        && segs.len() >= MIN_GPU_RESIDENT_SEGS
        // The device solves the plain delta-gap route on the merged-straight
        // basis, in free space — the one class shared with the CLI's solve and
        // its automatic pick.
        && nec_solver::gpu_resident_class(&deck, &segs, &ground).is_ok()
        // The device re-solves from raw segment inputs, discarding host-side
        // stamps. This gate was already value-based rather than a card-type list,
        // which is why it never had the CLI's NT hole (FND-023) — it now asks the
        // same question through the shared seam.
        && stamps.is_identity();

    // The device solves the merged-conductor basis the CLI's GPU-resident path
    // uses: collinear `GW` runs as one conductor, a junction row only where
    // distinct conductors meet. Handed the raw per-card grouping instead, its
    // constraint rows outnumbered its unknowns on every collinear split, so it
    // declined them all — and the fallback below was a plain `solve_hallen` on
    // that same raw grouping, without the FND-158 term: a 1+9+9+1 split dipole
    // came back 1032.2 - j3723.8 under `exec: gpu` against 76.68 + j32.87 under
    // `exec: cpu` and from the CLI, labelled `exec_used: cpu` (FND-199).
    let merged = nec_solver::merged_grouping(&segs);
    // An excitation the homogeneous constants absorb whole (FND-227) is the
    // routed CPU solve's to refuse, saying why.
    let gpu_eligible = gpu_eligible
        && !nec_solver::merged_excitation_in_homogeneous_span(
            &hallen_rhs.rhs,
            &hallen_rhs.cos_vec,
            &hallen_rhs.sin_vec,
            &merged.0,
            &merged.1,
        );
    let device_currents = if gpu_eligible {
        let (merged_endpoints, junction_tuples) = merged;
        let z_inputs: Vec<nec_accel::ZSegmentInput> = segs
            .iter()
            .map(|s| nec_accel::ZSegmentInput {
                midpoint: s.midpoint,
                direction: s.direction,
                length: s.length,
                radius: s.radius,
            })
            .collect();
        match pollster::block_on(nec_accel::solve_hallen_gpu_resident(
            &z_inputs,
            &hallen_rhs.rhs,
            &hallen_rhs.cos_vec,
            &hallen_rhs.sin_vec,
            &merged_endpoints,
            &nec_solver::sin_eligible(&merged_endpoints, &junction_tuples),
            &nec_solver::hallen_constraint_rows(
                &merged_endpoints,
                &junction_tuples,
                &segs.iter().map(|s| s.length).collect::<Vec<_>>(),
            ),
            freq_hz,
        )) {
            Ok(x) if x.len() >= segs.len() => Some(x[..segs.len()].to_vec()),
            // No adapter, a device fault, or a rejected f32 answer (or a short
            // result) — the routed CPU solve below, which `exec_used` reports.
            _ => None,
        }
    } else {
        None
    };

    let (currents, exec_used) = if let Some(x) = device_currents {
        (x, "gpu")
    } else {
        // One call for every Hallén route: plane-wave, current-source, and
        // delta-gap on either the merged-conductor or the conductor-path basis.
        // This branch used to be a plain `solve_hallen`, so a bent or split
        // geometry came back 9.15 - j767.60 where the CLI — which had the paths
        // arm — gave 264.88 + j410.86 and nec2c 268.56 + j452.26 (FND-121). The
        // device's fallback is this branch too, not a second, plainer solve.
        let routed =
            nec_solver::solve_hallen_routed(&deck, &segs, &mut z_mat, freq_hz, &stamps.diagonal)
                .map_err(|e| SolveError::UnsupportedConfig(e.to_string()))?;
        current_source_port = routed.port_voltage;
        // Only the feedpoint is read from these, so they are the SOURCE currents:
        // a feed that is also a TL/NT port includes the network branch (FND-123).
        (routed.source_currents(), "cpu")
    };

    // 7. Extract the feedpoint, through the shared seam.
    //
    // This loop used to filter `excitation_type != 0` by hand, which contradicted
    // the physics it had just run: `build_hallen_rhs` drives a type-5 card as a
    // delta gap, so the worker solved such a deck and then refused to read the
    // answer, reporting "no EX type-0 card found" for a deck the CLI, the GUI and
    // the Python bindings all solve to the digit (FND-031).
    //
    // A current source is priced here now (FND-051). It was refused while only the
    // CLI could compute the port voltage `Z = V_port/i0` needs; that machinery has
    // been in `nec_solver` since #412, and `FeedpointResult` already carries
    // impedance and current, so nothing about the wire format had to change — the
    // port voltage never crosses it.
    if let Some(v_port) = current_source_port {
        let (ex, _) = nec_solver::feedpoints(&deck)
            .find(|(_, role)| *role == nec_model::card::FeedpointRole::CurrentSource)
            .ok_or_else(|| {
                SolveError::UnsupportedConfig(
                    "a current-source solve without a current source".to_string(),
                )
            })?;
        let i0 = Complex64::new(ex.voltage_real, ex.voltage_imag);
        let z_in =
            nec_solver::feedpoint_impedance(v_port, i0, ex.tag as usize, ex.segment as usize)
                .map_err(|e| SolveError::UnsupportedConfig(e.to_string()))?;
        return Ok(FeedpointResult {
            warnings: warnings.clone(),
            impedance_re: z_in.re,
            impedance_im: z_in.im,
            current_mag: i0.norm(),
            current_phase_deg: i0.im.atan2(i0.re).to_degrees(),
            exec_used: exec_used.to_string(),
        });
    }
    if let Some(ex) = nec_solver::first_delta_gap_feedpoint(&deck) {
        // Unreachable today: `build_hallen_rhs` refuses an EX naming an absent
        // segment first. Kept defensive, saying what would be true — the deck
        // HAS a feedpoint; its segment is missing.
        let Some(idx) = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        else {
            return Err(SolveError::UnsupportedConfig(format!(
                "EX on tag {} segment {} names a segment the geometry does not contain",
                ex.tag, ex.segment
            )));
        };
        let current = currents[idx];
        let v_source = Complex64::new(ex.voltage_real, ex.voltage_imag);
        let z_in = nec_solver::feedpoint_impedance(
            v_source,
            current,
            ex.tag as usize,
            ex.segment as usize,
        )
        .map_err(|e| SolveError::UnsupportedConfig(e.to_string()))?;
        return Ok(FeedpointResult {
            // Parse caveats first: they describe the deck the rest was derived
            // from, so they read before the matrix-fill ones.
            warnings: warnings.clone(),
            impedance_re: z_in.re,
            impedance_im: z_in.im,
            current_mag: current.norm(),
            current_phase_deg: current.im.atan2(current.re).to_degrees(),
            exec_used: exec_used.to_string(),
        });
    }

    // Unreachable: step 3b refused every deck without a feedpoint, before the
    // solve. Worded apart from that refusal so a receive deck reaching here —
    // step 3b gone, and a full solve wasted — fails the tests that pin it.
    Err(SolveError::UnsupportedConfig(
        "internal: solved a deck with no feedpoint to price".to_string(),
    ))
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    /// Two wires crossing mid-span — geometry the CLI refuses locally.
    const CROSSING: &str = "\
GW 1 11 -5 0 0 5 0 0 0.001
GW 2 11 0 -5 0 0 5 0 0.001
GE 0
EX 0 1 6 0 1.0 0.0
FR 0 1 0 0 14.2 0
EN
";
    /// The same wires meeting at an endpoint instead — legal, and must still solve.
    const ENDPOINT_JUNCTION: &str = "\
GW 1 11 -5 0 0 0 0 0 0.001
GW 2 11 0 0 0 0 5 0 0.001
GE 0
EX 0 1 6 0 1.0 0.0
FR 0 1 0 0 14.2 0
EN
";

    /// The worker is a separately installed binary reached over SSH, so it cannot
    /// rely on its controller having validated anything (FND-013).
    #[test]
    fn worker_refuses_geometry_the_cli_refuses() {
        let err = solve_deck_at_frequency_with_exec(CROSSING, 14.2e6, "hallen", "cpu")
            .expect_err("crossing wires must be refused");
        match err {
            SolveError::UnsupportedConfig(m) => {
                assert!(m.contains("intersecting-wire"), "unexpected message: {m}");
            }
            // Not GeometryError: that falls into `process_task`'s catch-all and
            // would cross the wire labelled `parse_error`.
            other => panic!("expected UnsupportedConfig, got {other:?}"),
        }
    }

    /// Negative control: the guard must reject the unsupported class, not everything.
    #[test]
    fn worker_still_solves_legal_geometry() {
        let r = solve_deck_at_frequency_with_exec(ENDPOINT_JUNCTION, 14.2e6, "hallen", "cpu")
            .expect("an endpoint junction is legal and must still solve");
        assert!(
            r.impedance_re.is_finite() && r.impedance_im.is_finite(),
            "expected a finite impedance, got {} + j{}",
            r.impedance_re,
            r.impedance_im
        );
    }
}

#[cfg(test)]
mod wire_frequency_gate_tests {
    use super::*;

    const DIPOLE: &str =
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

    /// The frequency arrives on the wire, so `pre_solve_error` cannot see it —
    /// `frequency_error` reads the deck's FR card and takes no frequency
    /// argument. A negative `frequency_hz` therefore returned the exact complex
    /// conjugate with `status: ok` and no warning, on the very class the FR seam
    /// refuses for every deck-borne path (FND-098).
    #[test]
    fn a_negative_wire_frequency_is_refused_not_conjugated() {
        let ok = solve_deck_at_frequency(DIPOLE, 14.2e6, "hallen").expect("control must solve");
        let err = solve_deck_at_frequency(DIPOLE, -14.2e6, "hallen")
            .expect_err("a negative wire frequency must be refused");
        assert!(
            matches!(err, SolveError::UnsupportedConfig(ref m) if m.contains("not a usable frequency")),
            "{err:?}"
        );
        // Pin what the bug actually produced, so a regression cannot pass by
        // failing for some other reason: the refused value used to come back as
        // the conjugate of the control.
        assert!(
            ok.impedance_im.abs() > 1.0,
            "control reactance should be non-trivial"
        );
    }

    #[test]
    fn a_non_finite_wire_frequency_is_refused() {
        for f in [f64::NAN, f64::INFINITY, 0.0] {
            let err = solve_deck_at_frequency(DIPOLE, f, "hallen").expect_err("must be refused");
            assert!(
                matches!(err, SolveError::UnsupportedConfig(ref m) if m.contains("not a usable frequency")),
                "{f}: {err:?}"
            );
        }
    }
}

#[cfg(test)]
mod frontend_parity_tests {
    use super::*;

    /// Bent and split geometry, the class FND-121 was measured on: an apex-fed
    /// inverted-V and a start-to-start split fed mid-wire. Both need the
    /// conductor-path basis; both used to be answered here on the plain one.
    const SPLIT_V: &str = "CE\nGW 1 21 0 0 3 -5 0 0 .001\nGW 2 21 0 0 3 5 0 0 .001\nGE\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    const INVERTED_V: &str = "CE\nGW 1 21 -5 0 0 0 0 3 .001\nGW 2 21 0 0 3 5 0 0 .001\nGE\nEX 0 1 21 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    const STRAIGHT: &str =
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";

    /// The worker and the library seam every other frontend calls must agree on
    /// the same deck at the same frequency.
    ///
    /// This is the gate that was missing. The worker reached its answer through
    /// its own `build_hallen_rhs` + `solve_hallen`, so it could — and did —
    /// diverge from the CLI without any test noticing: a split inverted-V came
    /// back 9.15 - j767.60 here against 264.88 + j410.86 there, `status: ok`,
    /// `warnings: []`, with nec2c at 268.56 + j452.26.
    #[test]
    fn the_worker_agrees_with_the_shared_solver_seam_on_path_geometry() {
        for (name, deck_str) in [
            ("split-V", SPLIT_V),
            ("inverted-V", INVERTED_V),
            ("straight", STRAIGHT),
        ] {
            let got = solve_deck_at_frequency(deck_str, 14.2e6, "hallen")
                .unwrap_or_else(|e| panic!("{name}: worker solve failed: {e}"));

            // The same deck through the library seam the CLI, GUI and bindings use.
            let deck = nec_parser::parse(deck_str).expect("parses").deck;
            let segs = nec_solver::build_geometry(&deck).expect("geometry");
            let ground = nec_solver::ground_model_from_deck(&deck);
            let mut z = nec_solver::assemble_z_matrix_with_ground(&segs, 14.2e6, &ground);
            let routed = nec_solver::solve_hallen_routed(&deck, &segs, &mut z, 14.2e6, &[])
                .unwrap_or_else(|e| panic!("{name}: routed solve failed: {e}"));

            let ex = nec_solver::first_delta_gap_feedpoint(&deck).expect("feedpoint");
            let idx = segs
                .iter()
                .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
                .expect("feed segment");
            let v = Complex64::new(ex.voltage_real, ex.voltage_imag);
            let want = v / routed.currents[idx];

            assert!(
                (got.impedance_re - want.re).abs() < 1e-6
                    && (got.impedance_im - want.im).abs() < 1e-6,
                "{name}: worker {} + j{} vs shared seam {} + j{}",
                got.impedance_re,
                got.impedance_im,
                want.re,
                want.im
            );
        }
    }
    /// Collinear splits, the class the device used to decline and the fallback
    /// used to solve on the raw per-card grouping. The 1+9+9+1 split came back
    /// 1032.2 - j3723.8 under `exec: gpu` against 76.68 + j32.87 under
    /// `exec: cpu` and from the CLI (nec2c 79.41 + j45.63), labelled
    /// `exec_used: cpu` (FND-199). With no adapter — CI — the fallback is what
    /// runs, which is the half that was wrong; with one, the device must land on
    /// the same answer to its single-precision rounding.
    #[test]
    fn exec_gpu_answers_a_collinear_split_as_exec_cpu_does() {
        const ONE_NINE_NINE_ONE: &str = "CE\nGW 1 1 0 0 -5.282 0 0 -4.5 .001\nGW 2 9 0 0 -4.5 0 0 0 .001\nGW 3 9 0 0 0 0 0 4.5 .001\nGW 4 1 0 0 4.5 0 0 5.282 .001\nGE\nEX 0 2 9 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        const THREE_FIFTEEN_THREE: &str = "CE\nGW 1 3 0 0 -5.282 0 0 -4 .001\nGW 2 15 0 0 -4 0 0 4 .001\nGW 3 3 0 0 4 0 0 5.282 .001\nGE\nEX 0 2 8 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        const TEN_TEN: &str = "CE\nGW 1 10 0 0 -5.282 0 0 0 .001\nGW 2 10 0 0 0 0 0 5.282 .001\nGE\nEX 0 1 10 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        for (name, deck_str) in [
            ("1+9+9+1", ONE_NINE_NINE_ONE),
            ("3+15+3", THREE_FIFTEEN_THREE),
            ("10+10", TEN_TEN),
        ] {
            let cpu = solve_deck_at_frequency_with_exec(deck_str, 14.2e6, "hallen", "cpu")
                .unwrap_or_else(|e| panic!("{name}: exec cpu failed: {e}"));
            let solve_gpu = || {
                solve_deck_at_frequency_with_exec(deck_str, 14.2e6, "hallen", "gpu")
                    .unwrap_or_else(|e| panic!("{name}: exec gpu failed: {e}"))
            };
            let mut gpu = solve_gpu();
            // With an adapter the device must TAKE the split: handed the raw
            // grouping it declined every one, and the fallback hid that. A
            // device fault on this host's driver (FND-190) earns one re-run,
            // as the hybrid lane's tests do.
            if gpu.exec_used == "cpu"
                && pollster::block_on(nec_accel::wgpu_device::hardware_adapter_present())
            {
                gpu = solve_gpu();
                assert_eq!(
                    gpu.exec_used, "gpu",
                    "{name}: a hardware adapter is present, but the device declined the split twice"
                );
            }
            let z_cpu = Complex64::new(cpu.impedance_re, cpu.impedance_im);
            let z_gpu = Complex64::new(gpu.impedance_re, gpu.impedance_im);
            // The fallback is the identical f64 solve; the device rounds in f32.
            let tol = if gpu.exec_used == "cpu" { 1e-9 } else { 1e-3 } * z_cpu.norm();
            assert!(
                (z_gpu - z_cpu).norm() <= tol,
                "{name}: exec gpu ({}) {z_gpu} vs exec cpu {z_cpu}",
                gpu.exec_used
            );
            eprintln!(
                "{name}: exec gpu ({}) {z_gpu} vs exec cpu {z_cpu}",
                gpu.exec_used
            );
        }
    }
}
