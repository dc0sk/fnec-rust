// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// Integration tests for EP-4 DeckValidator CLI integration (PH4-CHK-005).

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

fn write_temp_deck(prefix: &str, body: &str) -> common::TempDeck {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    common::TempDeck::new(&format!("fnec-{prefix}-{now}.nec"), body)
}

const DECK_NO_EX: &str = "\
CM Deck without EX card
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
FR 0 1 0 0 14.0 0.0
EN
";

const DECK_WITH_EX: &str = "\
CM Normal dipole
CE
GW 1 51 0.0 0.0 -5.0 0.0 0.0 5.0 0.001
GE 0
EX 0 1 26 0 1.0 0.0
FR 0 1 0 0 14.0 0.0
EN
";

/// Renamed from `validator_warning_emitted_for_deck_without_ex_card`: it is an
/// **error** now, not a warning, and a test whose name says "warning" is one a
/// reader trusts to have checked that.
#[test]
fn validator_error_emitted_for_deck_without_ex_card() {
    let path = write_temp_deck("no-ex", DECK_NO_EX);
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("failed to run fnec");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[validator]"),
        "expected [validator] tag in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("error: [validator]"),
        "an undriven deck must be refused, not merely remarked upon: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("no ex card"),
        "expected 'no EX card' in stderr: {stderr}"
    );
}

#[test]
fn no_validator_warning_for_well_formed_deck() {
    let path = write_temp_deck("with-ex", DECK_WITH_EX);
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("failed to run fnec");
    assert!(
        output.status.success(),
        "fnec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[validator]"),
        "unexpected [validator] warning for valid deck: {stderr}"
    );
}

/// Every `--solver` value the binary advertises, taken **from the binary**.
///
/// `fnec --solver` with no value prints the usage line to stderr, and that line
/// carries the closed list `--solver <pulse|hallen|continuity|sinusoidal|mpie>`.
/// Parsing it here rather than typing the five names is the point: a sixth mode
/// is swept the day it appears in the usage string, and a removed one cannot
/// leave a stale row behind.
///
/// There is no enum to sweep instead, which is itself worth recording. The CLI's
/// `SolverMode` lives in the binary crate as `pub(super)` with no `ALL`, and
/// `SolverKind::ALL` — the one enumeration that does exist — has two members
/// rather than five, because it names the two solver *bases* and not the CLI's
/// five modes. An integration test can see neither.
fn advertised_solver_modes() -> Vec<String> {
    let usage = String::from_utf8_lossy(
        &Command::new(env!("CARGO_BIN_EXE_fnec"))
            .arg("--solver")
            .output()
            .expect("failed to run fnec")
            .stderr,
    )
    .into_owned();
    let start = usage
        .find("--solver <")
        .expect("usage must advertise the --solver alternation");
    let open = start + "--solver <".len();
    let close = usage[open..]
        .find('>')
        .expect("the --solver alternation must be closed");
    usage[open..open + close]
        .split('|')
        .map(str::to_string)
        .collect()
}

/// An undriven deck is refused on **every** advertised solver mode and output
/// format, and stdout carries nothing.
///
/// This replaces `validator_warning_does_not_prevent_exit_success_for_warning_level`,
/// whose premise no longer exists: the CLI has no warning-level validator left to
/// exercise. Its comment claimed "fnec emits an empty report and exits 0", which
/// had not been true for some time — the report was 2469 bytes of `0.000000e0`
/// current rows.
///
/// Recorded rather than glossed: with the only validator promoted to error level,
/// the `DiagnosticLevel::Warning` arm in `main.rs` is now reached by nothing and
/// covered by nothing. `nec_model`'s unit tests cover `run_validators`'
/// *aggregation*, not the CLI's warning-vs-error dispatch, so deleting that arm
/// would break no test. Noted in FND-145.
///
/// **Both axes, because "every path" is a product and not a list.** The refusal
/// lives in `pre_solve_error`, above the `--solver` dispatch. The obvious place
/// to have put it — the Hallén RHS builder, where the zero vector is actually
/// born — sits *below* that dispatch, and would have left `pulse` and
/// `continuity` printing zeros at exit 0 while all four frontends looked covered.
/// Sweeping frontends alone would not have caught it; neither would a single row
/// here.
///
/// What this test does NOT discriminate, recorded so nobody reads it as more than
/// it is: the CLI reaches the refusal by two independent routes — this validator
/// and `pre_solve_error` — so deleting the shared check alone leaves every test
/// in this file green. Measured: that sabotage fails the GUI, worker and
/// `nec_solver` suites and none of these. The CLI is therefore the wrong frontend
/// to ask about the shared seam, and the per-frontend tests exist for that
/// reason. Both routes have to go before this file notices, which is the sabotage
/// that was actually run.
///
/// The stdout assertion is the other load-bearing half.
/// `docs/json-output-schema.md` documented `[]` and exit 0 for this deck — under
/// "Absence of feedpoint data", not under the file's "Stability guarantee"
/// section, which covers the field set. The behaviour was documented rather than
/// guaranteed; the binary honoured it either way. That sentence is withdrawn in
/// the same change and this pins what replaced it, because checking only the exit
/// code would let a future change re-introduce a zeros table beside the failure.
#[test]
fn an_undriven_deck_is_refused_on_every_solver_mode_and_writes_nothing_to_stdout() {
    let path = write_temp_deck("no-ex-exit", DECK_NO_EX);
    let modes = advertised_solver_modes();
    // Floor: a parse that silently produced nothing would satisfy every assertion
    // in the loop below by never entering it.
    assert!(
        modes.len() >= 5,
        "expected at least the five documented solver modes, parsed {modes:?}"
    );

    for mode in &modes {
        for format in ["text", "json"] {
            let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
                .args(["--solver", mode, "--output-format", format])
                .arg(&path)
                .output()
                .expect("failed to run fnec");
            let stderr = String::from_utf8_lossy(&output.stderr);
            let case = format!("--solver {mode} --output-format {format}");
            assert!(
                !output.status.success(),
                "a deck nothing drives must not report success ({case}): {stderr}"
            );
            assert!(
                output.stdout.is_empty(),
                "refusal must not also emit a report ({case}): {} byte(s): {}",
                output.stdout.len(),
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(
                stderr.contains("no EX card"),
                "the refusal must name its reason ({case}): {stderr}"
            );
        }
    }
}

/// The control for the test above: the refusal must key on the missing drive, not
/// on something incidental to the fixture. Same geometry, same `FR`, one `EX 0`
/// added — and it solves.
#[test]
fn the_same_deck_with_a_drive_still_solves() {
    let path = write_temp_deck("no-ex-control", DECK_WITH_EX);
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("failed to run fnec");
    assert!(
        output.status.success(),
        "control deck must still solve: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("FEEDPOINTS"),
        "control deck must still produce a report: {stdout}"
    );
}

#[test]
fn corpus_deck_does_not_trigger_validator_warning() {
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deck_path = corpus_root.join("corpus/dipole-freesp-51seg.nec");
    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&deck_path)
        .output()
        .expect("failed to run fnec");
    assert!(
        output.status.success(),
        "fnec failed on corpus deck: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[validator]"),
        "unexpected [validator] tag for corpus deck: {stderr}"
    );
}
