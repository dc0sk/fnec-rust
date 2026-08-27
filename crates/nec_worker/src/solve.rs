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
        let deck = "CM malformed NT: 8 fields, expected 10\nCE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nNT 1 10 1 40 0.0 -0.002 0.0 0.004\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
        let r = solve_deck_at_frequency(deck, 14.2e6, "hallen").expect("deck still solves");
        assert!(
            r.warnings
                .iter()
                .any(|w| w.contains("NT card has 8 fields")),
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
    /// solve to 74.243 + j13.900 Ω. The distributed path rejected a deck the
    /// other three frontends handle.
    #[test]
    fn a_type_5_voltage_source_is_a_feedpoint_here_as_it_is_everywhere_else() {
        let r = solve_deck_at_frequency(DIPOLE_EX5, 14.2e6, "hallen")
            .expect("EX 5 is a voltage source fnec models as a delta gap");
        // The corpus reference for this deck, which the CLI already matches.
        assert!(
            (r.impedance_re - 74.23).abs() < 0.1,
            "R = {} Ω",
            r.impedance_re
        );
        assert!(
            (r.impedance_im - 13.9).abs() < 0.1,
            "X = {} Ω",
            r.impedance_im
        );
    }

    /// A current source *is* a feedpoint, but pricing it needs the solved port
    /// voltage, which only the CLI's Hallén path computes. Saying that beats the
    /// old "no feedpoint", which was false.
    #[test]
    fn a_current_source_deck_is_refused_by_name_not_called_feedpointless() {
        let err = solve_deck_at_frequency(DIPOLE_EX4, 14.2e6, "hallen").unwrap_err();
        match err {
            SolveError::UnsupportedConfig(m) => {
                assert!(m.contains("current source"), "{m}");
                assert!(m.contains("--hosts"), "{m}");
            }
            other => panic!("expected UnsupportedConfig, got {other:?}"),
        }
    }

    /// A plane wave has no feedpoint at all: its tag/segment fields carry
    /// NTHETA/NPHI. It must not be read as one.
    #[test]
    fn a_plane_wave_deck_has_no_feedpoint() {
        let err = solve_deck_at_frequency(DIPOLE_EX1, 14.2e6, "hallen").unwrap_err();
        // Pinned to the exact variant. Accepting `UnsupportedConfig` too would let
        // FND-035's spurious source-risk rejection — raised by the same
        // `geometry_error` this function calls earlier — keep this test green
        // while the deck failed for an entirely different and wrong reason.
        assert!(matches!(err, SolveError::NoFeedpoint), "{err:?}");
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

    #[test]
    fn solve_no_feedpoint_returns_error() {
        // Deck with geometry but no EX card.
        let deck = "CM test\nGW 0 1 0 0 0 0 0 1 0.001\nGE 0\nFR 0 1 0 0 14.175 0\nEN\n";
        let err = solve_deck_at_frequency(deck, 14.175e6, "hallen").unwrap_err();
        assert!(matches!(err, SolveError::NoFeedpoint));
    }

    #[test]
    fn solve_error_display() {
        let err = SolveError::NoFeedpoint;
        assert_eq!(
            err.to_string(),
            "no driven feedpoint (EX voltage source) found in deck"
        );

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
    assemble_z_matrix_with_ground, build_geometry, build_hallen_rhs, detect_wire_junctions,
    ground_model_from_deck, solve_hallen, wire_endpoints_from_segs, GroundModel,
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

/// Minimum segment count before a worker attempts the GPU-resident solve.
const MIN_GPU_RESIDENT_SEGS: usize = 16;

/// Errors from the worker solve path.
#[derive(Debug, Clone)]
pub enum SolveError {
    ParseError(String),
    GeometryError(String),
    SingularMatrix(String),
    UnsupportedConfig(String),
    NoFeedpoint,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::ParseError(m) => write!(f, "parse error: {m}"),
            SolveError::GeometryError(m) => write!(f, "geometry error: {m}"),
            SolveError::SingularMatrix(m) => write!(f, "singular matrix: {m}"),
            SolveError::UnsupportedConfig(m) => write!(f, "unsupported config: {m}"),
            SolveError::NoFeedpoint => {
                write!(f, "no driven feedpoint (EX voltage source) found in deck")
            }
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
/// segments), the worker dispatches `nec_accel::solve_hallen_gpu_resident`
/// (PH7-CHK-003) and reports `exec_used = "gpu"`. Otherwise — out of class, or no
/// wgpu adapter — it falls back to the f64 CPU `solve_hallen`
/// (`exec_used = "cpu"`). PH7-CHK-004.
pub fn solve_deck_at_frequency_with_exec(
    deck_str: &str,
    freq_hz: f64,
    basis: &str,
    exec: &str,
) -> Result<FeedpointResult, SolveError> {
    if basis != "hallen" {
        return Err(SolveError::UnsupportedConfig(format!(
            "basis '{basis}' not supported in worker; only 'hallen' is implemented"
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
    let parse_warnings: Vec<String> = parse_result
        .warnings
        .iter()
        .map(ToString::to_string)
        .collect();

    // 2. Build geometry
    let segs = build_geometry(&deck).map_err(|e| SolveError::GeometryError(e.to_string()))?;
    let wire_endpoints = wire_endpoints_from_segs(&segs);
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

    // 4. Assemble Z-matrix and apply loads / TL stamps
    let stamps = nec_solver::build_deck_stamps(&deck, &segs, freq_hz);
    let mut z_mat = assemble_z_matrix_with_ground(&segs, freq_hz, &ground);
    stamps.apply(&mut z_mat);

    // 5. Wire-junction constraints
    let junctions = detect_wire_junctions(&segs, &wire_endpoints, 1e-6);
    let junc_constraints: Vec<(usize, usize, f64)> = junctions
        .iter()
        .map(|j| (j.seg_a, j.seg_b, j.sign))
        .collect();

    // 6. Solve — GPU-resident (PH7-CHK-003) for the supported class when
    // requested, else the f64 CPU solve.
    let gpu_eligible = exec == "gpu"
        && segs.len() >= MIN_GPU_RESIDENT_SEGS
        && matches!(
            ground,
            GroundModel::FreeSpace | GroundModel::Deferred { .. }
        )
        // The device re-solves from raw segment inputs, discarding host-side
        // stamps. This gate was already value-based rather than a card-type list,
        // which is why it never had the CLI's NT hole (FND-023) — it now asks the
        // same question through the shared seam.
        && stamps.is_identity();

    let (currents, exec_used) = if gpu_eligible {
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
            &wire_endpoints,
            &junc_constraints,
            freq_hz,
        )) {
            Some(x) if x.len() >= segs.len() => (x[..segs.len()].to_vec(), "gpu"),
            // No adapter (or short result) — fall back to CPU.
            _ => (
                cpu_currents(&z_mat, &hallen_rhs, &wire_endpoints, &junc_constraints)?,
                "cpu",
            ),
        }
    } else {
        (
            cpu_currents(&z_mat, &hallen_rhs, &wire_endpoints, &junc_constraints)?,
            "cpu",
        )
    };

    // 7. Extract the feedpoint, through the shared seam.
    //
    // This loop used to filter `excitation_type != 0` by hand, which contradicted
    // the physics it had just run: `build_hallen_rhs` drives a type-5 card as a
    // delta gap, so the worker solved such a deck and then refused to read the
    // answer, reporting "no EX type-0 card found" for a deck the CLI, the GUI and
    // the Python bindings all solve to the digit (FND-031).
    //
    // A current source is excluded here on purpose rather than by omission: it is
    // a real feedpoint, but pricing it needs the solved port voltage, which only
    // the CLI's Hallén path computes. Saying so beats returning "no feedpoint".
    if let Some(msg) =
        nec_solver::validate::unpriceable_feedpoint_error(&deck, "run without --hosts")
    {
        return Err(SolveError::UnsupportedConfig(msg));
    }
    if let Some(ex) = nec_solver::first_delta_gap_feedpoint(&deck) {
        // A feedpoint naming a segment the geometry does not contain is a bad
        // deck, not "no feedpoint" — but the distinction is the caller's, and
        // `NoFeedpoint` is what this returned before.
        let Some(idx) = segs
            .iter()
            .position(|s| s.tag == ex.tag && s.tag_index == ex.segment)
        else {
            return Err(SolveError::NoFeedpoint);
        };
        let current = currents[idx];
        let v_source = Complex64::new(ex.voltage_real, ex.voltage_imag);
        let z_in = if current.norm() > 1e-60 {
            v_source / current
        } else {
            v_source
        };
        return Ok(FeedpointResult {
            // Parse caveats first: they describe the deck the rest was derived
            // from, so they read before the matrix-fill ones.
            warnings: parse_warnings
                .iter()
                .chain(stamps.warnings.iter())
                .cloned()
                .collect(),
            impedance_re: z_in.re,
            impedance_im: z_in.im,
            current_mag: current.norm(),
            current_phase_deg: current.im.atan2(current.re).to_degrees(),
            exec_used: exec_used.to_string(),
        });
    }

    Err(SolveError::NoFeedpoint)
}

/// CPU Hallén solve returning just the current vector.
fn cpu_currents(
    z_mat: &nec_solver::ZMatrix,
    hallen_rhs: &nec_solver::HallenRhs,
    wire_endpoints: &[(usize, usize)],
    junc_constraints: &[(usize, usize, f64)],
) -> Result<Vec<Complex64>, SolveError> {
    let solution = solve_hallen(
        z_mat,
        &hallen_rhs.rhs,
        &hallen_rhs.cos_vec,
        wire_endpoints,
        junc_constraints,
    )
    .map_err(|e| SolveError::SingularMatrix(e.to_string()))?;
    Ok(solution.currents)
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
