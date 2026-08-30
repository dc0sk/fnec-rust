use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;

#[test]
// VERIFIES: FR-009 (early geometry diagnostics with actionable messages)
fn crossing_wires_fail_fast_with_actionable_error() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-geometry-crossing-{now}.nec"));

    // Two wires crossing at interior points (origin) are currently unsupported
    // and should fail before solve with an actionable geometry error.
    let deck = "GW 1 11 -1.0 0.0 0.0 1.0 0.0 0.0 0.001\nGW 2 11 0.0 -1.0 0.0 0.0 1.0 0.0 0.001\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary crossing-wires deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--allow-noncollinear-hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for crossing-wires test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "crossing-wire deck should fail, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: unsupported intersecting-wire geometry"),
        "expected intersection geometry error in stderr, got:\n{stderr}"
    );
}

#[test]
fn endpoint_wire_junction_is_not_rejected_as_intersection() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-geometry-endpoint-{now}.nec"));

    // Endpoint junction (shared wire endpoint) is allowed by current geometry
    // diagnostics (not an intersecting-wire error). However, since the two
    // wires are non-collinear and --allow-noncollinear-hallen is silently
    // ignored in Phase-1, the Hallen solver will still reject this geometry
    // with a non-collinear topology error.
    let deck = "GW 1 11 0.0 0.0 0.0 1.0 0.0 0.0 0.001\nGW 2 11 0.0 0.0 0.0 0.0 1.0 0.0 0.001\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary endpoint-junction deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg("--allow-noncollinear-hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for endpoint-junction test: {e}"));

    let _ = fs::remove_file(&deck_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // The geometry-diagnostics intersection check should NOT flag this as an
    // intersecting-wire error (the wires meet only at a shared endpoint).
    assert!(
        !stderr.contains("unsupported intersecting-wire geometry"),
        "did not expect intersection geometry error for endpoint join, got:\n{stderr}"
    );
    // Phase-2: non-collinear topologies are fully supported, so the command
    // should succeed (endpoint junction treated as KCL constraint).
    assert!(
        output.status.success(),
        "expected endpoint-junction non-collinear Hallen to succeed; stderr:\n{stderr}"
    );
}

#[test]
fn tiny_source_segment_fails_fast_with_actionable_error() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let deck_path = std::env::temp_dir().join(format!("fnec-geometry-tiny-source-{now}.nec"));

    // Very short source segment (length/radius < 2) is currently deferred and
    // should fail early with an actionable source-risk geometry diagnostic.
    let deck =
        "GW 1 1 0.0 0.0 0.0 0.000001 0.0 0.0 0.001\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n";
    fs::write(&deck_path, deck).expect("failed to write temporary tiny-source deck");

    let output = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--solver")
        .arg("hallen")
        .arg(&deck_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec for tiny-source test: {e}"));

    let _ = fs::remove_file(&deck_path);

    assert!(
        !output.status.success(),
        "tiny-source deck should fail, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: unsupported source-risk geometry: EX on tiny segment"),
        "expected source-risk geometry error in stderr, got:\n{stderr}"
    );
}

/// FND-036, at the production entry point.
///
/// The unit test pins `validate::mixed_excitation_error`; this pins that the CLI
/// actually calls it. Dropping the check from `pre_solve_error` leaves every unit
/// test green while the CLI happily prints the wrong number again — measured, not
/// assumed.
#[test]
fn a_deck_driven_by_two_kinds_of_source_is_refused() {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/dipole-mixed-sources-51seg.nec"
        ))
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a deck whose feedpoint would be meaningless must not solve:\n{stderr}"
    );
    assert!(
        stderr.contains("both a voltage source") && stderr.contains("current source"),
        "the refusal must name what is fighting:\n{stderr}"
    );
}

/// FND-093 / FND-094, at the production entry point.
///
/// The unit tests pin `validate::grid_budget_error`; this pins that the check is
/// in the `pre_solve_error` chain at all. Dropping that one line leaves every
/// unit test green while the CLI goes back to asking the allocator for
/// **68 717 379 600 bytes** and aborting — measured, not assumed.
///
/// Which FRONTEND reaches the chain is already gated per-frontend by the FND-129
/// tests (CLI, GUI, worker and bindings each have one), so this does not repeat
/// that; it gates membership of the chain, which is the new thing.
#[test]
fn an_oversized_grid_card_is_refused() {
    let dir = std::env::temp_dir().join(format!("fnec-grid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let deck = dir.join("huge-rp.nec");
    std::fs::write(
        &deck,
        "CE\nGW 1 21 0 0 -5 0 0 5 0.001\nGE\nEX 0 1 11 0 1.0 0.0\n\
         FR 0 1 0 0 14.2 0\nRP 0 65535 65535 1000 0 0 1 1\nEN\n",
    )
    .expect("write deck");

    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&deck)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an oversized grid must be refused, not attempted:\n{stderr}"
    );
    assert!(
        stderr.contains("RP") && stderr.contains("the limit is"),
        "the refusal must name the card and the limit:\n{stderr}"
    );
}

/// FND-129, at the production entry point.
///
/// Same reason as the test above: the unit test pins
/// `validate::multiple_current_sources_error`, and this pins that the CLI
/// actually reaches it. Dropping the check out of `pre_solve_error` leaves every
/// unit test green while the CLI prints the wrong feedpoint again.
#[test]
fn a_deck_with_two_current_sources_is_refused() {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/dipole-two-current-sources-freesp.nec"
        ))
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a deck whose second feedpoint would be meaningless must not solve:\n{stderr}"
    );
    assert!(
        stderr.contains("2 current sources") && stderr.contains("tag 2 segment 11"),
        "the refusal must name how many and which one is dropped:\n{stderr}"
    );
    // The old behaviour, pinned so it cannot come back quietly: a FEEDPOINTS
    // block for the undriven source.
    assert!(
        !stdout.contains("FEEDPOINTS"),
        "no feedpoint may be reported for a deck that was refused:\n{stdout}"
    );
}

/// FND-035: a receive-only deck was hard-rejected for a source it does not have.
///
/// The source-risk check read every `EX` card with no type filter, so a plane
/// wave's NTHETA/NPHI — which live in the fields a driven source uses for tag and
/// segment — could match a short fat segment and refuse the deck outright. On
/// every frontend, since the check reaches `geometry_error` and so `diagnose`.
///
/// End-to-end rather than a unit test because this changes which decks are
/// *refused*, and the production entry point is where that has to be true.
#[test]
fn a_receive_only_deck_is_not_refused_for_a_source_it_does_not_have() {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/receive-planewave-fat-segment.nec"
        ))
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("source-risk"),
        "a deck with no driven source cannot have a source at risk:\n{stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "receive deck must solve:\n{stderr}"
    );

    // Exit 0 alone is a weak gate: a deck cross-polarized to every wire also
    // completes, with all currents exactly zero and the -999.99 dB sentinel for a
    // response. That would pass while the plane-wave path did nothing at all. The
    // fixture is broadside so the wave genuinely couples; assert the result is
    // real, not merely present.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let response = stdout
        .lines()
        .skip_while(|l| !l.starts_with("THETA PHI RESPONSE_DB"))
        .nth(1)
        .and_then(|l| l.split_whitespace().nth(2))
        .and_then(|v| v.parse::<f64>().ok())
        .expect("a receive-pattern response row");
    assert!(
        response > -999.0,
        "receive response is the null sentinel — the wave coupled to nothing: {response}"
    );
}

// ---------------------------------------------------------------------------
// The distributed path must refuse what the local path refuses (FND-013)
// ---------------------------------------------------------------------------

/// `--hosts` used to return from `main()` before the validation block, so a deck
/// the CLI refuses locally was dispatched to every worker and solved.
///
/// The assertion that matters is the *second* one: the run must fail on the
/// geometry, **before** touching the host list. Validation placed inside
/// `run_distributed_solve` could not satisfy that — `WorkerPool` spawns an SSH
/// process per host the moment it is constructed, so the pool error would come
/// first. Hoisting the check above the `--hosts` branch is what makes this pass.
/// RFC 5737 TEST-NET-3: reserved for documentation and never globally routed, so
/// a passing run never dials it and a failing one is bounded by the pool's 5 s
/// SSH connect timeout.
const UNREACHABLE_HOST: &str = "203.0.113.1";

#[test]
fn distributed_run_refuses_geometry_before_contacting_any_host() {
    let deck = common::TempDeck::new(
        "fnec-dist-crossing.nec",
        "CM two wires crossing mid-span\nCE\nGW 1 11 -5 0 0 5 0 0 0.001\nGW 2 11 0 -5 0 0 5 0 0.001\nGE 0\nEX 0 1 6 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    // A host that cannot be reached: if validation did not run first, the failure
    // would be about the worker pool instead of the geometry.
    let hosts = common::TempDeck::new(
        "fnec-dist-hosts.toml",
        &format!("[[worker]]\nhostname = \"{UNREACHABLE_HOST}\"\n"),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--hosts")
        .arg(&hosts)
        .arg(&deck)
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported intersecting-wire geometry"),
        "distributed run must refuse the same geometry the local run does:\n{stderr}"
    );
    // Assert on the host ADDRESS, not on the words "worker"/"SSH". `ssh` prints
    // lowercase `ssh:` and the pool gives its children an inherited stderr, so a
    // regression that moved the check back inside `run_distributed_solve` — after
    // the pool is built — produced output containing neither word and passed the
    // weaker assertion while having contacted the host. Verified: that exact
    // regression now fails here, and took 5 s doing it.
    assert!(
        !stderr.contains(UNREACHABLE_HOST),
        "must fail on the geometry BEFORE contacting any host:\n{stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "must exit 1, as the local path does"
    );
}

/// `--hosts` with `--loads-config` used to drop the loads for the whole run:
/// `run_distributed_solve` takes no Laplace parameter and the worker protocol
/// carries no field for them, so the controller returned the *unloaded*
/// impedance for a deck the user had explicitly loaded — FND-023's signature one
/// layer up. Found by fable's diff review of #393.
///
/// Same placement requirement as the geometry check above: rejecting the
/// combination must happen before `WorkerPool` is constructed, so the assertion
/// is again on the host address rather than on the word "worker".
#[test]
fn distributed_run_refuses_laplace_loads_before_contacting_any_host() {
    let deck = common::TempDeck::new(
        "fnec-dist-laplace.nec",
        "CM plain dipole\nCE\nGW 1 21 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    let hosts = common::TempDeck::new(
        "fnec-dist-laplace-hosts.toml",
        &format!("[[worker]]\nhostname = \"{UNREACHABLE_HOST}\"\n"),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--hosts")
        .arg(&hosts)
        .arg("--loads-config")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/laplace-load-rlc.toml"
        ))
        .arg(&deck)
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Laplace loads (--loads-config) are not supported with --hosts"),
        "must reject the combination rather than silently dropping the loads:\n{stderr}"
    );
    assert!(
        !stderr.contains(UNREACHABLE_HOST),
        "must fail BEFORE contacting any host:\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "must exit 1");
}

/// FND-018: the worker implements the Hallén basis only, so every task of a
/// `--solver mpie` run failed `UnsupportedConfig` — but only after the pool had
/// dialled every host, and once per frequency point. The answer was knowable
/// before the first connection.
///
/// Proven the way its siblings are: the host name must not appear in stderr. A
/// timing assertion would not do it — an unresolvable name fails DNS in
/// milliseconds, so the delay this saves only shows on a host that resolves and
/// does not answer, which a test cannot depend on.
#[test]
fn distributed_run_refuses_a_non_hallen_solver_before_contacting_any_host() {
    let hosts = common::TempDeck::new(
        "fnec-dist-solver-hosts.toml",
        &format!("[[worker]]\nhostname = \"{UNREACHABLE_HOST}\"\n"),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--hosts")
        .arg(&hosts)
        .arg("--solver")
        .arg("mpie")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/dipole-freesp-51seg.nec"
        ))
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--solver mpie is not supported with --hosts"),
        "must name the flag it is refusing:\n{stderr}"
    );
    assert!(
        !stderr.contains(UNREACHABLE_HOST),
        "must fail BEFORE contacting any host:\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "must exit 1");
}

/// The negative control: the default solver is the one the worker implements, so
/// a distributed Hallén run must still reach the hosts. Without this, refusing
/// every solver would pass the test above.
#[test]
fn a_distributed_hallen_run_still_reaches_the_hosts() {
    let hosts = common::TempDeck::new(
        "fnec-dist-hallen-hosts.toml",
        &format!("[[worker]]\nhostname = \"{UNREACHABLE_HOST}\"\n"),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--hosts")
        .arg(&hosts)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/dipole-freesp-51seg.nec"
        ))
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("is not supported with --hosts"),
        "a Hallén run must not be refused by the solver pre-check:\n{stderr}"
    );
}

/// The sibling of the test above, and the reason the class matters more than the
/// instance: `--ground-solver sommerfeld` was dropped by `--hosts` exactly as the
/// Laplace loads were. The worker derives its ground model from the deck alone,
/// so the PH9-CHK-006 surface-wave correction never reached it and the run
/// returned the uncorrected reflection-coefficient impedance — measured locally
/// on this deck as 92.266 + j13.617 Ω against 95.524 + j12.166 Ω with the
/// correction, a 3.26 Ω answer change for a flag the user passed explicitly.
///
/// Found by the strong-model review of the FND-025 fix, which pointed out that
/// FND-025 was one member of a class and asked which others were live.
#[test]
fn distributed_run_refuses_sommerfeld_before_contacting_any_host() {
    let hosts = common::TempDeck::new(
        "fnec-dist-somm-hosts.toml",
        &format!("[[worker]]\nhostname = \"{UNREACHABLE_HOST}\"\n"),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg("--hosts")
        .arg(&hosts)
        .arg("--ground-solver")
        .arg("sommerfeld")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/dipole-gn2-near-ground-51seg.nec"
        ))
        .output()
        .expect("run fnec");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--ground-solver sommerfeld is not supported with --hosts"),
        "must reject the combination rather than silently ignoring the flag:\n{stderr}"
    );
    assert!(
        !stderr.contains(UNREACHABLE_HOST),
        "must fail BEFORE contacting any host:\n{stderr}"
    );
    assert_eq!(out.status.code(), Some(1), "must exit 1");
}

// ---------------------------------------------------------------------------
// Degenerate frequencies (FND-056)
// ---------------------------------------------------------------------------

/// The production entry point, not the unit: `frequency_error` is folded into the
/// shared `pre_solve_error` aggregate precisely so every frontend refuses these,
/// and a unit test would pass while the CLI still printed a number.
#[test]
fn the_cli_refuses_every_degenerate_frequency_class() {
    const GEOM: &str = "GW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\n";
    for (fr, needle, what) in [
        ("FR 0 1 0 0 0.0 0", "zero frequency", "zero"),
        ("FR 0 1 0 0 -14.2 0", "negative frequency", "negative"),
        // The start value is a perfectly good 10 MHz — this is why the check has
        // to be over the generated list. On main it solved and reported
        // FREQ_MHZ -2.000000.
        (
            "FR 0 5 0 0 10.0 -3.0",
            "negative frequency",
            "descending through zero",
        ),
    ] {
        let deck = common::TempDeck::new(
            &format!("fnec_freq_{}.nec", what.replace(' ', "_")),
            &format!("CM {what}\nCE\n{GEOM}{fr}\nEX 0 1 11 0 1.0 0.0\nEN\n"),
        );
        let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .arg(deck.to_str().unwrap())
            .output()
            .expect("run fnec");
        assert!(
            !out.status.success(),
            "a {what} frequency must be refused, got exit 0"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(needle),
            "the refusal must name the cause for a {what} frequency: {stderr}"
        );
    }
}

/// The negative control, and the reason this gate is safe to add: every corpus
/// deck is `FR 0 1 0 0 f 0.0`, so a step of 0.0 with one step must stay legal.
#[test]
fn an_ordinary_deck_still_solves_after_the_frequency_gate() {
    let deck = common::TempDeck::new(
        "fnec_freq_control.nec",
        "CM control\nCE\nGW 1 21 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\n\
         FR 0 1 0 0 14.2 0\nEX 0 1 11 0 1.0 0.0\nEN\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(deck.to_str().unwrap())
        .output()
        .expect("run fnec");
    assert!(
        out.status.success(),
        "the control deck must still solve: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A non-finite field on any card is a parse error now, closing one of the two
/// input routes to the `NaN` feedpoint row FND-030 describes. The other — a
/// finite field that overflows to an infinite frequency once multiplied into
/// hertz — is closed by `validate::frequency_error`. Neither closes a genuinely
/// diverged CPU solve, which is why that row stays open.
#[test]
fn the_cli_refuses_a_non_finite_field() {
    let deck = common::TempDeck::new(
        "fnec_nan_coord.nec",
        "CM NaN coordinate\nCE\nGW 1 21 0 0 NaN 0 0 5.0 0.001\nGE 0\n\
         FR 0 1 0 0 14.2 0\nEX 0 1 11 0 1.0 0.0\nEN\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(deck.to_str().unwrap())
        .output()
        .expect("run fnec");
    assert!(
        !out.status.success(),
        "a NaN coordinate must be refused, got exit 0"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("NaN NaN"),
        "a NaN feedpoint row reached the report: {combined}"
    );
}
