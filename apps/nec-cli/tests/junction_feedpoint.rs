// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Junction-fed feedpoint behavior across the PH9-CHK-002 / PH9-CHK-005 boundary.
//
// PH9-CHK-002 (general junction basis) now solves **degree-2** conductor chains
// on a continuous Hallén path — collinear splits, start-to-start splits, bends,
// and inverted-V apex feeds all give a physical impedance and emit no warning.
//
// FND-162 stages 2+3 solve **degree-3+** (T/Y) junctions and closed loops on a
// section graph. What that solve refuses — a straight run one segment long, whose
// one row cannot fix a section's two constants — the Hallén solve refuses outright
// (FND-251): the per-wire fallback it used to warn and solve on was 2 to 18 times
// off nec2c.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(deck: &str, name: &str) -> (String, String, std::process::ExitStatus) {
    run_on(deck, name, "hallen")
}

fn run_on(deck: &str, name: &str, solver: &str) -> (String, String, std::process::ExitStatus) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-{name}-{solver}-{now}.nec"));
    fs::write(&path, deck).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", solver, "--exec", "cpu"])
        .arg(&path)
        .current_dir(&root)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status,
    )
}

/// Parse the first feedpoint `Z_RE` from the FNEC report on stdout.
fn feedpoint_r(stdout: &str) -> f64 {
    let mut in_feed = false;
    for line in stdout.lines() {
        if line.starts_with("FEEDPOINTS") {
            in_feed = true;
            continue;
        }
        if in_feed {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
            if cols.len() >= 8 {
                if let Ok(zre) = cols[6].parse::<f64>() {
                    return zre;
                }
            }
        }
    }
    panic!("no feedpoint row found in:\n{stdout}");
}

// A straight half-wave dipole split into two wires that both START at the origin
// (start-to-start), fed at that junction. Physically the ~78.8 Ω single-wire
// dipole; PH9-CHK-002 now recovers it on a continuous conductor path.
const SPLIT_DIPOLE_JUNCTION_FED: &str =
    "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Same two wires, fed on wire 1 segment 13 — away from the junction.
const SPLIT_DIPOLE_FED_AWAY: &str =
    "GW 1 26 0 0 0 0 0 5.282 0.001\nGW 2 26 0 0 0 0 0 -5.282 0.001\nGE 0\nEX 0 1 13 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Ordinary single-wire dipole: no junction at all.
const SINGLE_WIRE_DIPOLE: &str =
    "GW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// A dipole modeled as two arms bent 15° at the feed, fed on one arm AWAY from the
// bend (segment 7) — a degree-2 chain. Before PH9-CHK-002 this mis-solved to a
// negative resistance; now it solves to a physical positive R.
const BENT_DIPOLE_FED_AWAY: &str =
    "GW 1 26 0 0 0 1.367 0 5.104 0.001\nGW 2 26 0 0 0 -1.367 0 -5.104 0.001\nGE 0\nEX 0 1 7 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// Three wires meeting at the origin (degree-3 T/Y), fed at the node. The section
// graph solves it: nec2c 1.3.1 gives 45.46 + j13.77 Ω at this mesh, fnec
// 46.39 + j7.95 (12.4 %), converging to 7.0 % at 25 and 4.1 % at 49 per wire.
//
// FND-256: nec2c does not settle here geometrically; it drifts by a near-constant
// step, the slow (logarithmic) convergence of a NEC-2 junction. At 1 / 3 / 9 /
// 27 × (`scripts/nec2c-converged-series.py`, captured 2026-10-16): R 45.459,
// 44.660, 44.735, 44.852, X 13.773, 13.553, 13.782, 13.999 — the last two R steps
// 0.08 and 0.12 Ω. No tail can be priced for a drift like that, so the gate is
// against the 27× point with three last steps (0.35 Ω) taken out of its 2 Ω.
const TEE_JUNCTION_FED: &str =
    "GW 1 13 0 0 0 5.282 0 0 0.001\nGW 2 13 0 0 0 -5.282 0 0 0.001\nGW 3 13 0 0 0 0 0 5.282 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// The same T with a stem one segment long, which the section graph refuses, so the
// Hallén solve refuses the deck (FND-251). The one-segment stem sits between the
// junction and a bend: a free-end one-segment arm is refined into thirds (FND-227)
// and the graph takes it.
const TEE_ONE_SEGMENT_STEM_FED: &str =
    "GW 1 13 0 0 0 5.282 0 0 0.001\nGW 2 13 0 0 0 -5.282 0 0 0.001\nGW 3 1 0 0 0 0 0 0.5 0.001\nGW 4 5 0 0 0.5 0.5 0 0.5 0.001\nGE 0\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn start_to_start_junction_fed_now_solves() {
    // The headline PH9-CHK-002 case: fed exactly at the degree-2 junction.
    let (stdout, stderr, _status) = run(SPLIT_DIPOLE_JUNCTION_FED, "junction-fed");
    assert!(
        !stderr.contains("wire junction"),
        "a degree-2 junction feed must no longer warn; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("negative resistance"),
        "the split dipole now has physical R>0; stderr:\n{stderr}"
    );
    let r = feedpoint_r(&stdout);
    assert!(
        (r - 79.33).abs() < 2.0,
        "junction-fed split dipole must recover the single-wire ~79.3 Ω; got {r:.3}"
    );
}

#[test]
fn bent_dipole_fed_away_now_solves() {
    // Feed off the bend on a degree-2 bent dipole: previously negative R, now physical.
    let (stdout, stderr, _status) = run(BENT_DIPOLE_FED_AWAY, "bent-fed-away");
    assert!(
        !stderr.contains("negative resistance"),
        "bent degree-2 dipole now solves to positive R; stderr:\n{stderr}"
    );
    assert!(
        feedpoint_r(&stdout) > 0.0,
        "bent dipole resistance must be positive"
    );
}

#[test]
fn split_dipole_fed_away_does_not_warn() {
    // A junction exists in the geometry, but the feed is not on it.
    let (_stdout, stderr, _status) = run(SPLIT_DIPOLE_FED_AWAY, "fed-away");
    assert!(
        !stderr.contains("is on a wire junction"),
        "feed away from the junction should not warn; stderr:\n{stderr}"
    );
}

#[test]
fn single_wire_feedpoint_does_not_warn() {
    let (_stdout, stderr, _status) = run(SINGLE_WIRE_DIPOLE, "single-wire");
    assert!(
        !stderr.contains("is on a wire junction"),
        "single-wire dipole should not warn; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("negative resistance"),
        "single-wire dipole (R>0) must not trip the negative-resistance check; stderr:\n{stderr}"
    );
}

#[test]
fn degree3_tee_junction_fed_now_solves() {
    let (stdout, stderr, _status) = run(TEE_JUNCTION_FED, "tee-junction");
    for w in ["wire junction", "T/Y junction", "negative resistance"] {
        assert!(
            !stderr.contains(w),
            "a solved T must not warn {w:?}; stderr:\n{stderr}"
        );
    }
    let r = feedpoint_r(&stdout);
    let (nec2c, drift) = (44.852, 3.0 * 0.118);
    assert!(
        (r - nec2c).abs() + drift < 2.0,
        "the T must land within 2 Ω of nec2c's 27× {nec2c} Ω, less its drift; got {r:.3}"
    );
}

#[test]
fn degree3_tee_junction_the_graph_refuses_is_refused() {
    // The one-segment stem would send it back to the per-wire basis, which has no
    // reliable answer for a junction (FND-251), so the deck is refused, naming the
    // junction, the run and the edit, and the MPIE, which takes this deck. The
    // negative control is `degree3_tee_junction_fed_now_solves`: the same T with a
    // 13-segment stem solves.
    let (stdout, stderr, status) = run(TEE_ONE_SEGMENT_STEM_FED, "tee-one-seg");
    assert!(
        !status.success(),
        "a T the section graph refuses must not solve; stderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("FEEDPOINTS"),
        "a refused deck must print no result; stdout:\n{stdout}"
    );
    for want in [
        "a junction where three or more wires meet",
        "(tag 3 segment 1)",
        "at least two segments",
        "--solver mpie",
    ] {
        assert!(
            stderr.contains(want),
            "the refusal must say {want:?}; stderr:\n{stderr}"
        );
    }
    // The remedy it names is one the run can take.
    let (mpie_out, mpie_err, mpie_status) = run_on(TEE_ONE_SEGMENT_STEM_FED, "tee-one-seg", "mpie");
    assert!(
        mpie_status.success() && feedpoint_r(&mpie_out) > 0.0,
        "the MPIE the refusal names must solve the deck; stderr:\n{mpie_err}"
    );
}

// A 1λ square loop (perimeter ≈ λ at 14.2 MHz), fed mid-wire — away from every
// corner junction. The per-wire basis reported ≈20 − j1210 Ω; the section graph
// solves the loop as a cycle, 109.09 − j143.50 against nec2c's 111.01 − j146.27
// on this mesh (1.8 %; 1.1 % at 21 and 0.6 % at 41 per side). The gate is against
// nec2c's converged value, 108.97 − j146.10 (41 a side at 27×, `EK 0`; this 11 a
// side deck at 27× gives 109.13 − j146.14), not the 11-a-side 111.01, which moves
// 63 % of the 3 Ω gate under nec2c's own refinement (FND-257).
const SQUARE_LOOP_FED_MIDWIRE: &str =
    "GW 1 11 -2.639 0 0 2.639 0 0 0.001\nGW 2 11 2.639 0 0 2.639 0 5.278 0.001\nGW 3 11 2.639 0 5.278 -2.639 0 5.278 0.001\nGW 4 11 -2.639 0 5.278 -2.639 0 0 0.001\nGE 0\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

// The same loop with one side one segment long, which the section graph refuses,
// so the Hallén solve refuses the deck (FND-251).
const SQUARE_LOOP_ONE_SEGMENT_SIDE: &str =
    "GW 1 11 -2.639 0 0 2.639 0 0 0.001\nGW 2 1 2.639 0 0 2.639 0 5.278 0.001\nGW 3 11 2.639 0 5.278 -2.639 0 5.278 0.001\nGW 4 11 -2.639 0 5.278 -2.639 0 0 0.001\nGE 0\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";

#[test]
fn closed_loop_now_solves() {
    let (stdout, stderr, _status) = run(SQUARE_LOOP_FED_MIDWIRE, "square-loop");
    assert!(
        !stderr.contains("closed loop") && !stderr.contains("negative resistance"),
        "a solved loop must not warn; stderr:\n{stderr}"
    );
    let r = feedpoint_r(&stdout);
    assert!(
        (r - 108.97).abs() < 3.0,
        "the loop must land near nec2c's converged 108.97 Ω; got {r:.3}"
    );
}

#[test]
fn closed_loop_the_graph_refuses_is_refused() {
    // A loop the section graph refuses is refused, even though the feed is mid-wire
    // and on no junction (FND-251). Its stderr alone cannot tell a refusal from the
    // warning it replaced — both name the loop and `--solver mpie` — so the exit
    // status and the missing report decide. Negative control: `closed_loop_now_solves`.
    let (stdout, stderr, status) = run(SQUARE_LOOP_ONE_SEGMENT_SIDE, "square-loop-one-seg");
    assert!(
        !status.success(),
        "a loop the section graph refuses must not solve; stderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("FEEDPOINTS"),
        "a refused deck must print no result; stdout:\n{stdout}"
    );
    for want in [
        "a closed loop",
        "(tag 2 segment 1)",
        "at least two segments",
        "--solver mpie",
    ] {
        assert!(
            stderr.contains(want),
            "the refusal must say {want:?}; stderr:\n{stderr}"
        );
    }
    // The remedy it names is one the run can take.
    let (mpie_out, mpie_err, mpie_status) =
        run_on(SQUARE_LOOP_ONE_SEGMENT_SIDE, "square-loop-one-seg", "mpie");
    assert!(
        mpie_status.success() && feedpoint_r(&mpie_out) > 0.0,
        "the MPIE the refusal names must solve the deck; stderr:\n{mpie_err}"
    );
}

// ---------------------------------------------------------------------------
// The negative-resistance warning must not blame a cause the deck cannot have
// ---------------------------------------------------------------------------

/// A single straight wire, badly under-segmented (2 segments over 3.3 λ), which
/// the Hallén solve returns a negative resistance for (Re Z = −333.6 Ω). There is
/// no junction anywhere in it. It was 3 segments over ~1.9 λ until FND-227 refined
/// the free ends, which that wire then solved to a positive resistance.
const STRAIGHT_NEGATIVE_R: &str = "\
CM one straight wire, no junction
CE
GW 1 2 0 0 0 0 0 10 0.01
GE 0
EX 0 1 1 0 1.0 0.0
FR 0 1 0 0 100 0
EN
";

/// A U whose middle run is one segment long: two 90° bends, which are wire
/// junctions to the negative-resistance cause (`has_wire_junction`), and which the
/// Hallén solve leaves unmodelled, solving to a negative resistance (Re Z = −2.113 Ω;
/// nec2c +2.36). It was a Y with a one-segment arm until FND-251 refused every loop
/// or junction the section graph cannot take — no deck it takes was found with a
/// negative resistance — and a stem-fed Y with three 11-segment arms before the
/// section graph solved that.
const JUNCTION_NEGATIVE_R: &str = "\
CM U with a one-segment middle
CE
GW 1 10 0 0 0.5 0 0 0 .001
GW 2 1 0 0 0 0.1 0 0 .001
GW 3 10 0.1 0 0 0.1 0 0.5 .001
GE 0
EX 0 1 5 0 1 0
FR 0 1 0 0 100 0
EN
";

fn stderr_for(deck: &str, name: &str) -> String {
    let path = std::env::temp_dir().join(format!("fnec-negr-{name}.nec"));
    std::fs::write(&path, deck).expect("write deck");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--solver", "hallen"])
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Regression: the warning offered "commonly a junctioned-geometry limitation
/// (see PH9-CHK-002)" unconditionally, so a deck containing a single straight wire
/// was sent after a cause it does not contain.
#[test]
fn negative_resistance_does_not_blame_junctions_on_a_junctionless_deck() {
    let stderr = stderr_for(STRAIGHT_NEGATIVE_R, "straight");
    assert!(
        stderr.contains("negative resistance"),
        "fixture must actually produce a negative resistance:\n{stderr}"
    );
    assert!(
        stderr.contains("no wire junction"),
        "a junctionless deck must say so:\n{stderr}"
    );
    assert!(
        !stderr.contains("commonly a junctioned-geometry limitation"),
        "must not blame junctions on a deck with none:\n{stderr}"
    );
}

/// The other side of the same contract: where a junction really is present, the
/// junction explanation is still the useful one and must be kept.
#[test]
fn negative_resistance_still_blames_junctions_where_there_is_one() {
    let stderr = stderr_for(JUNCTION_NEGATIVE_R, "junction");
    assert!(
        stderr.contains("negative resistance"),
        "fixture must actually produce a negative resistance:\n{stderr}"
    );
    assert!(
        stderr.contains("commonly a junctioned-geometry limitation"),
        "a bent deck must keep the junction explanation:\n{stderr}"
    );
    assert!(
        !stderr.contains("no wire junction"),
        "must not claim there is no junction when there is:\n{stderr}"
    );
}

/// The refusal judges the segments the solve runs on (FND-251): a T whose
/// one-segment base stands on perfect ground is solved as its doubled image
/// problem, where the base is two segments and the section graph takes it. It
/// solves, and agrees with nec2c 1.3.1 (0.89985 − j165.33 Ω; fnec 0.9038 −
/// j165.29, and 0.5 % at twice the mesh).
#[test]
fn a_tee_whose_one_segment_base_stands_on_perfect_ground_solves() {
    let deck = "CE\nGW 1 1 0 0 0 0 0 0.5 .001\nGW 2 10 0 0 0.5 2.5 0 0.5 .001\n\
                GW 3 10 0 0 0.5 -2.5 0 0.5 .001\nGE 1\nGN 1\nEX 0 1 1 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let (stdout, stderr, status) = run(deck, "grounded-tee-one-seg-base");
    assert!(
        status.success(),
        "the image problem takes it; stderr:\n{stderr}"
    );
    let row: Vec<f64> = stdout
        .lines()
        .skip_while(|l| *l != "FEEDPOINTS")
        .nth(2)
        .expect("feedpoint row")
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let (nec2c_re, nec2c_im) = (0.89985, -165.33);
    let rel = (row[6] - nec2c_re).hypot(row[7] - nec2c_im) / nec2c_re.hypot(nec2c_im);
    assert!(rel < 0.01, "{row:?} against nec2c {nec2c_re} {nec2c_im}");
}
